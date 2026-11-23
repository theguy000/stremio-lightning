//! Pure response parsing and retry schedule for browser-assisted Facebook/Apple OAuth polling.

use std::future::Future;
use std::time::Duration;

use super::{Credentials, PollError, Provider};

/// Pacing and budget for polling `strem.io` for the completed browser sign-in.
pub(crate) struct PollSchedule {
    pub first_interval: Duration,
    pub later_interval: Duration,
    pub fast_polls: u32,
    pub max_polls: u32,
    pub tolerated_transport_errors: u32,
}

pub(crate) const SCHEDULE: PollSchedule = PollSchedule {
    first_interval: Duration::from_secs(1),
    later_interval: Duration::from_secs(2),
    fast_polls: 10,
    max_polls: 90,
    tolerated_transport_errors: 3,
};

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Poll {
    Pending,
    Ready(Credentials),
}

/// Parses one response body from `strem.io`'s account-poll endpoint.
/// Non-JSON bodies (such as bare-text 503 while consent is in progress) mean `Poll::Pending`.
pub(crate) fn parse_account(provider: Provider, body: &str) -> Result<Poll, PollError> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Ok(Poll::Pending);
    };
    if let Some(error) = value.get("error").filter(|error| !error.is_null()) {
        return Err(PollError::Refused(
            refusal_message(error).unwrap_or_else(|| "Stremio refused the sign-in.".to_owned()),
        ));
    }
    let Some(user) = value.get("user") else {
        return Ok(Poll::Pending);
    };
    let field = |name: &str| {
        user.get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    match provider {
        Provider::Facebook => match (field("email"), field("fbLoginToken")) {
            (Some(email), Some(password)) => {
                Ok(Poll::Ready(Credentials::Password { email, password }))
            }
            _ => Ok(Poll::Pending),
        },
        Provider::Apple => match (field("token"), field("sub")) {
            (Some(token), Some(sub)) => Ok(Poll::Ready(Credentials::Apple {
                token,
                sub,
                email: field("email").unwrap_or_default(),
                name: field("name").unwrap_or_default(),
            })),
            _ => Ok(Poll::Pending),
        },
    }
}

fn refusal_message(error: &serde_json::Value) -> Option<String> {
    match error {
        serde_json::Value::String(message) => Some(message.clone()),
        serde_json::Value::Object(fields) => fields
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
    .filter(|message| !message.is_empty())
}

pub(crate) async fn wait_with<F, Fut>(
    schedule: PollSchedule,
    mut poll: F,
    cancelled: impl Fn() -> bool,
) -> Result<Credentials, PollError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<Poll, PollError>>,
{
    let mut fast_polls = schedule.fast_polls;
    let mut transport_errors = 0;
    for _ in 0..schedule.max_polls {
        if cancelled() {
            return Err(PollError::Cancelled);
        }
        match poll().await {
            Ok(Poll::Ready(credentials)) => return Ok(credentials),
            Ok(Poll::Pending) => transport_errors = 0,
            Err(PollError::Unreachable(detail)) => {
                transport_errors += 1;
                if transport_errors >= schedule.tolerated_transport_errors {
                    return Err(PollError::Unreachable(detail));
                }
            }
            Err(error) => return Err(error),
        }
        let interval = if fast_polls > 0 {
            fast_polls -= 1;
            schedule.first_interval
        } else {
            schedule.later_interval
        };
        tokio::time::sleep(interval).await;
    }
    Err(PollError::Timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use stremio_core::types::api::AuthRequest;

    fn immediate() -> PollSchedule {
        PollSchedule {
            first_interval: Duration::from_millis(1),
            later_interval: Duration::from_millis(1),
            fast_polls: 2,
            max_polls: 5,
            tolerated_transport_errors: 3,
        }
    }

    #[test]
    fn parses_facebook_apple_pending_and_refusal_bodies() {
        let Poll::Ready(fb) = parse_account(
            Provider::Facebook,
            r#"{"user":{"email":"someone@stremio.com","fbLoginToken":"token-123"}}"#,
        )
        .unwrap() else {
            panic!("ready");
        };
        assert_eq!(
            fb.into_auth_request(),
            AuthRequest::Login {
                email: "someone@stremio.com".into(),
                password: "token-123".into(),
                facebook: true,
            }
        );

        let Poll::Ready(apple) = parse_account(
            Provider::Apple,
            r#"{"user":{"token":"id-token","sub":"user-1"}}"#,
        )
        .unwrap() else {
            panic!("ready");
        };
        assert_eq!(
            apple.into_auth_request(),
            AuthRequest::Apple {
                token: "id-token".into(),
                sub: "user-1".into(),
                email: String::new(),
                name: String::new(),
            }
        );

        for body in [
            "",
            "Service Unavailable",
            "{}",
            r#"{"user":{}}"#,
            r#"{"user":{"email":"a@b"}}"#,
        ] {
            assert_eq!(parse_account(Provider::Facebook, body), Ok(Poll::Pending));
        }
        assert_eq!(
            parse_account(
                Provider::Facebook,
                r#"{"error":{"message":"Already in use"}}"#
            ),
            Err(PollError::Refused("Already in use".into()))
        );
        assert_eq!(
            parse_account(Provider::Facebook, r#"{"error":"Denied"}"#),
            Err(PollError::Refused("Denied".into()))
        );
    }

    #[tokio::test]
    async fn wait_schedule_handles_ready_timeout_cancel_and_transient_errors() {
        let answers = RefCell::new(vec![
            Ok(Poll::Pending),
            Ok(Poll::Pending),
            Ok(Poll::Ready(Credentials::Password {
                email: "a@b".into(),
                password: "p".into(),
            })),
        ]);
        let mut calls = 0;
        let res = wait_with(
            immediate(),
            || {
                calls += 1;
                std::future::ready(answers.borrow_mut().remove(0))
            },
            || false,
        )
        .await;
        assert!(matches!(res, Ok(Credentials::Password { .. })) && calls == 3);

        let mut timeout_calls = 0;
        let res = wait_with(
            immediate(),
            || {
                timeout_calls += 1;
                std::future::ready(Ok(Poll::Pending))
            },
            || false,
        )
        .await;
        assert_eq!((res, timeout_calls), (Err(PollError::Timeout), 5));

        let cancel_calls = Cell::new(0u32);
        let res = wait_with(
            immediate(),
            || {
                cancel_calls.set(cancel_calls.get() + 1);
                std::future::ready(Ok(Poll::Pending))
            },
            || cancel_calls.get() >= 2,
        )
        .await;
        assert_eq!((res, cancel_calls.get()), (Err(PollError::Cancelled), 2));

        let err = || Err(PollError::Unreachable("timed out".into()));
        let dead = RefCell::new(vec![err(), Ok(Poll::Pending), err(), err(), err()]);
        assert_eq!(
            wait_with(
                immediate(),
                || std::future::ready(dead.borrow_mut().remove(0)),
                || false
            )
            .await,
            Err(PollError::Unreachable("timed out".into()))
        );

        let blip = RefCell::new(vec![
            err(),
            Ok(Poll::Pending),
            Ok(Poll::Pending),
            Ok(Poll::Pending),
            Ok(Poll::Pending),
        ]);
        assert_eq!(
            wait_with(
                immediate(),
                || std::future::ready(blip.borrow_mut().remove(0)),
                || false
            )
            .await,
            Err(PollError::Timeout)
        );
    }
}
