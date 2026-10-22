//! Browser-assisted Facebook and Apple sign-in.
//!
//! Neither provider can be signed into from inside this app. Facebook and Apple only hand
//! consent to `strem.io`, whose client ids and callback URLs are registered with them, and
//! `strem.io` parks the resulting account under a state it generated for that browser
//! session. Stremio's own web client does exactly the same thing: it opens
//! `…/login-<provider>/<state>` externally, then reads the account back from
//! `…/login-<provider>-get-acc/<state>` until it appears. This module is the
//! equivalent of that handshake, so the shell never has to hold a provider token itself and
//! the credentials still come from Stremio's API.
//!
//! The state is the only thing tying our poll to the browser's sign-in: it is compared
//! server-side, so it has to be unguessable and fresh for every attempt.

use std::fmt::Write as _;
use std::time::Duration;

use futures::Future;
use stremio_core::types::api::AuthRequest;

/// The site that owns Stremio's Facebook and Apple OAuth applications.
const LOGIN_ORIGIN: &str = "https://www.strem.io";

/// How often the account is asked for, and how long the whole handshake may take.
///
/// A browser that is already signed in finishes in a second or two, so the first polls are
/// a second apart; the rest are paced for a person typing a password into Facebook or
/// Apple. The budget is a little under three minutes, and every attempt ends it early.
struct PollSchedule {
    first_interval: Duration,
    later_interval: Duration,
    fast_polls: u32,
    max_polls: u32,
    /// A single failed request is a blip; this many in a row means the network is gone.
    tolerated_transport_errors: u32,
}

const SCHEDULE: PollSchedule = PollSchedule {
    first_interval: Duration::from_secs(1),
    later_interval: Duration::from_secs(2),
    fast_polls: 10,
    max_polls: 90,
    tolerated_transport_errors: 3,
};

#[cfg(test)]
impl PollSchedule {
    fn immediate() -> Self {
        Self {
            first_interval: Duration::from_millis(1),
            later_interval: Duration::from_millis(1),
            fast_polls: 2,
            max_polls: 5,
            tolerated_transport_errors: 3,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Provider {
    Facebook,
    Apple,
}

impl Provider {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Facebook => "Facebook",
            Self::Apple => "Apple",
        }
    }

    /// The path `strem.io` starts the provider's consent screen on.
    fn login_path(self) -> &'static str {
        match self {
            Self::Facebook => "login-fb",
            Self::Apple => "login-apple",
        }
    }

    /// The path the finished account is read back from.
    fn account_path(self) -> &'static str {
        match self {
            Self::Facebook => "login-fb-get-acc",
            Self::Apple => "login-apple-get-acc",
        }
    }

    /// Shown while the browser is open. The address is part of the message because opening
    /// a browser can silently fail on a machine with no registered handler.
    pub(crate) fn waiting_message(self, login_url: &str) -> String {
        format!(
            "Log in with {} in your browser; this screen finishes on its own. \
             If no browser opened, visit {login_url}",
            self.name()
        )
    }

    pub(crate) fn failure_message(self) -> String {
        format!(
            "{} sign-in didn't finish. Try again, or log in with your email and password.",
            self.name()
        )
    }
}

/// One sign-in in flight: the provider and the state shared with the browser.
pub(crate) struct Attempt {
    provider: Provider,
    state: String,
}

impl Attempt {
    pub(crate) fn start(provider: Provider) -> Result<Self, PollError> {
        Ok(Self {
            provider,
            state: new_state()?,
        })
    }

    pub(crate) fn login_url(&self) -> String {
        format!(
            "{LOGIN_ORIGIN}/{}/{}",
            self.provider.login_path(),
            self.state
        )
    }

    pub(crate) fn account_url(&self) -> String {
        format!(
            "{LOGIN_ORIGIN}/{}/{}",
            self.provider.account_path(),
            self.state
        )
    }
}

/// What the browser's sign-in turned into. Deliberately not `Debug`: it carries the
/// password-equivalent token and must never reach a log line.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Credentials {
    /// Facebook answers with the account's email and a one-shot token used as its password.
    Password { email: String, password: String },
    Apple {
        token: String,
        sub: String,
        email: String,
        name: String,
    },
}

impl std::fmt::Debug for Credentials {
    /// Debug output is masked: this type carries a password-equivalent token, and it is
    /// reachable from `Poll`, which callers may print.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Password { email, .. } => f
                .debug_struct("Password")
                .field("email", email)
                .field("password", &"<SENSITIVE>")
                .finish(),
            Self::Apple { email, .. } => f
                .debug_struct("Apple")
                .field("email", email)
                .field("token", &"<SENSITIVE>")
                .finish(),
        }
    }
}

impl Credentials {
    pub(crate) fn into_auth_request(self) -> AuthRequest {
        match self {
            Self::Password { email, password } => AuthRequest::Login {
                email,
                password,
                // The account was created through Facebook, so the API has to accept a
                // token in place of a password.
                facebook: true,
            },
            Self::Apple {
                token,
                sub,
                email,
                name,
            } => AuthRequest::Apple {
                token,
                sub,
                email,
                name,
            },
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Poll {
    /// The browser has not finished the provider's consent screen yet.
    Pending,
    Ready(Credentials),
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum PollError {
    /// Superseded by another attempt or dismissed by the user.
    Cancelled,
    Timeout,
    /// `strem.io` could not be reached at all.
    Unreachable(String),
    /// `strem.io` answered but refused the sign-in.
    Refused(String),
}

impl PollError {
    pub(crate) fn message(&self, provider: Provider) -> String {
        match self {
            Self::Cancelled => String::new(),
            Self::Timeout => provider.failure_message(),
            Self::Unreachable(detail) => format!(
                "Couldn't reach Stremio to finish the {} sign-in ({detail}). \
                 Check your connection and try again.",
                provider.name()
            ),
            Self::Refused(detail) => format!("{} sign-in failed: {detail}", provider.name()),
        }
    }
}

/// 128 random bits as lowercase hex: 32 characters that no other client can guess.
fn new_state() -> Result<String, PollError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        PollError::Unreachable(format!("the system has no entropy source: {error}"))
    })?;
    let mut state = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(state, "{byte:02x}");
    }
    Ok(state)
}

/// Reads one answer from the account endpoint.
///
/// Anything that is not a complete account means "the browser is still busy": the endpoint
/// answers a bare-text 503 until the consent screen is done, so a non-JSON body is not an
/// error.
pub(crate) fn parse_account(provider: Provider, body: &str) -> Result<Poll, PollError> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return Ok(Poll::Pending);
    };
    if let Some(error) = value.get("error").filter(|error| !error.is_null()) {
        return Err(PollError::Refused(
            refusal_message(error).unwrap_or_else(|| "Stremio refused the sign-in.".to_string()),
        ));
    }
    let Some(user) = value.get("user") else {
        return Ok(Poll::Pending);
    };
    let field = |name: &str| {
        user.get(name)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
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
            .map(str::to_string),
        _ => None,
    }
    .filter(|message| !message.is_empty())
}

/// Asks the account endpoint once.
pub(crate) async fn fetch_account(
    client: &reqwest::Client,
    attempt: &Attempt,
) -> Result<Poll, PollError> {
    let response = client
        .get(attempt.account_url())
        .send()
        .await
        .map_err(|error| PollError::Unreachable(error.to_string()))?;
    if !response.status().is_success() {
        return Ok(Poll::Pending);
    }
    let body = response
        .text()
        .await
        .map_err(|error| PollError::Unreachable(error.to_string()))?;
    parse_account(attempt.provider, &body)
}

/// Polls `strem.io` until the browser's sign-in shows up, the user cancels, or the schedule
/// runs out.
pub(crate) async fn wait_for_account(
    client: &reqwest::Client,
    attempt: &Attempt,
    cancelled: impl Fn() -> bool,
) -> Result<Credentials, PollError> {
    wait_with(SCHEDULE, || fetch_account(client, attempt), cancelled).await
}

async fn wait_with<F, Fut>(
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
            Err(PollError::Cancelled) => return Err(PollError::Cancelled),
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
    use std::cell::RefCell;

    fn attempt(provider: Provider) -> Attempt {
        Attempt::start(provider).expect("a state was generated")
    }

    #[test]
    fn login_and_account_urls_share_one_state() {
        let attempt = attempt(Provider::Facebook);
        let login = attempt.login_url();
        let state = login
            .strip_prefix("https://www.strem.io/login-fb/")
            .expect("the browser is sent to strem.io's Facebook login");
        assert_eq!(state.len(), 32, "the state is 128 bits as hex");
        assert!(state
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(
            attempt.account_url(),
            format!("https://www.strem.io/login-fb-get-acc/{state}")
        );
    }

    #[test]
    fn apple_uses_its_own_paths() {
        let attempt = attempt(Provider::Apple);
        assert!(attempt
            .login_url()
            .starts_with("https://www.strem.io/login-apple/"));
        assert!(attempt
            .account_url()
            .starts_with("https://www.strem.io/login-apple-get-acc/"));
    }

    #[test]
    fn every_attempt_gets_its_own_state() {
        let mut states: Vec<String> = (0..64)
            .map(|_| attempt(Provider::Facebook).login_url())
            .collect();
        states.sort_unstable();
        states.dedup();
        assert_eq!(
            states.len(),
            64,
            "a reused state could answer for another client"
        );
    }

    #[test]
    fn facebook_account_becomes_a_facebook_password_login() {
        let poll = parse_account(
            Provider::Facebook,
            r#"{"user":{"email":"someone@stremio.com","fbLoginToken":"token-123"}}"#,
        )
        .expect("the account parsed");
        let Poll::Ready(credentials) = poll else {
            panic!("a complete account is ready");
        };
        assert_eq!(
            credentials.into_auth_request(),
            AuthRequest::Login {
                email: "someone@stremio.com".to_string(),
                password: "token-123".to_string(),
                facebook: true,
            }
        );
    }

    #[test]
    fn apple_account_becomes_an_apple_login() {
        let poll = parse_account(
            Provider::Apple,
            r#"{"user":{"token":"id-token","sub":"user-1","email":"a@b.c","name":"Ada"}}"#,
        )
        .expect("the account parsed");
        let Poll::Ready(credentials) = poll else {
            panic!("a complete account is ready");
        };
        assert_eq!(
            credentials.into_auth_request(),
            AuthRequest::Apple {
                token: "id-token".to_string(),
                sub: "user-1".to_string(),
                email: "a@b.c".to_string(),
                name: "Ada".to_string(),
            }
        );
    }

    #[test]
    fn apple_account_without_a_name_still_signs_in() {
        let poll = parse_account(
            Provider::Apple,
            r#"{"user":{"token":"id-token","sub":"user-1"}}"#,
        )
        .expect("the account parsed");
        let Poll::Ready(Credentials::Apple { email, name, .. }) = poll else {
            panic!("a complete account is ready");
        };
        assert!(email.is_empty());
        assert!(name.is_empty());
    }

    #[test]
    fn unfinished_or_noisy_bodies_keep_waiting() {
        for body in [
            "",
            "Service Unavailable",
            "<html><body>503</body></html>",
            "{}",
            r#"{"user":{}}"#,
            r#"{"user":{"email":"someone@stremio.com"}}"#,
            r#"{"user":{"token":"id-token"}}"#,
        ] {
            assert_eq!(
                parse_account(Provider::Facebook, body),
                Ok(Poll::Pending),
                "body {body:?} is not an account yet"
            );
        }
    }

    #[test]
    fn a_refusal_stops_the_wait() {
        assert_eq!(
            parse_account(
                Provider::Facebook,
                r#"{"error":{"message":"This account is already in use"}}"#
            ),
            Err(PollError::Refused(
                "This account is already in use".to_string()
            ))
        );
        assert_eq!(
            parse_account(Provider::Facebook, r#"{"error":"Denied"}"#),
            Err(PollError::Refused("Denied".to_string()))
        );
    }

    #[tokio::test]
    async fn the_wait_returns_the_account_once_it_appears() {
        let answers = RefCell::new(vec![
            Ok(Poll::Pending),
            Ok(Poll::Pending),
            Ok(Poll::Ready(Credentials::Password {
                email: "someone@stremio.com".to_string(),
                password: "token-123".to_string(),
            })),
        ]);
        let mut calls = 0;
        let result = wait_with(
            PollSchedule::immediate(),
            || {
                calls += 1;
                std::future::ready(answers.borrow_mut().remove(0))
            },
            || false,
        )
        .await;
        assert!(matches!(result, Ok(Credentials::Password { .. })));
        assert_eq!(calls, 3, "it stops as soon as the account is ready");
    }

    #[tokio::test]
    async fn the_wait_gives_up_after_the_schedule() {
        let mut calls = 0;
        let result = wait_with(
            PollSchedule::immediate(),
            || {
                calls += 1;
                std::future::ready(Ok(Poll::Pending))
            },
            || false,
        )
        .await;
        assert_eq!(result, Err(PollError::Timeout));
        assert_eq!(calls, 5, "every slot in the schedule was used");
    }

    #[tokio::test]
    async fn dismissing_the_overlay_stops_the_wait() {
        let calls = std::cell::Cell::new(0u32);
        let result = wait_with(
            PollSchedule::immediate(),
            || {
                calls.set(calls.get() + 1);
                std::future::ready(Ok(Poll::Pending))
            },
            || calls.get() >= 2,
        )
        .await;
        assert_eq!(result, Err(PollError::Cancelled));
        assert_eq!(
            calls.get(),
            2,
            "no further request goes out after the cancel"
        );
    }

    #[tokio::test]
    async fn a_blip_is_tolerated_but_a_dead_network_is_not() {
        let unreachable = || Err(PollError::Unreachable("timed out".to_string()));
        let answers = RefCell::new(vec![
            unreachable(),
            Ok(Poll::Pending),
            unreachable(),
            unreachable(),
            unreachable(),
        ]);
        let result = wait_with(
            PollSchedule::immediate(),
            || std::future::ready(answers.borrow_mut().remove(0)),
            || false,
        )
        .await;
        assert_eq!(result, Err(PollError::Unreachable("timed out".to_string())));

        // One failed request in the middle of the schedule is only a blip.
        let answers = RefCell::new(vec![
            unreachable(),
            Ok(Poll::Pending),
            Ok(Poll::Pending),
            Ok(Poll::Pending),
            Ok(Poll::Pending),
        ]);
        let result = wait_with(
            PollSchedule::immediate(),
            || std::future::ready(answers.borrow_mut().remove(0)),
            || false,
        )
        .await;
        assert_eq!(result, Err(PollError::Timeout));
    }

    /// Live contract check against strem.io. Run with
    /// `cargo test -p stremio-lightning --bins -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "reaches strem.io"]
    async fn the_live_handshake_starts_and_reports_pending() {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client");
        for provider in [Provider::Facebook, Provider::Apple] {
            let attempt = Attempt::start(provider).expect("a state was generated");
            println!("{}: {}", provider.name(), attempt.login_url());
            let start = client
                .get(attempt.login_url())
                .send()
                .await
                .expect("strem.io answers");
            assert!(
                (300..400).contains(&start.status().as_u16()),
                "{} should redirect to the provider's consent screen, got {}",
                provider.name(),
                start.status()
            );
            assert_eq!(
                fetch_account(&client, &attempt).await,
                Ok(Poll::Pending),
                "{}: no browser finished this state, so strem.io has no account yet",
                provider.name()
            );
        }
    }

    #[test]
    fn messages_name_the_provider_and_stay_actionable() {
        assert!(Provider::Facebook
            .waiting_message("https://www.strem.io/login-fb/abc")
            .contains("https://www.strem.io/login-fb/abc"));
        assert!(Provider::Apple
            .failure_message()
            .contains("email and password"));
        assert!(PollError::Refused("Nope".to_string())
            .message(Provider::Facebook)
            .contains("Nope"));
    }
}
