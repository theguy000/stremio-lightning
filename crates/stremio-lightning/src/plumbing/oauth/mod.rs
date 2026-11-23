//! Browser-assisted Facebook and Apple OAuth handshake via `https://www.strem.io`.
//! Pure plumbing (Category 4 external HTTP + OS entropy); zero Slint and zero `screens::*`.

mod parse;

use std::fmt::Write as _;

use stremio_core::types::api::AuthRequest;

use parse::{parse_account, wait_with, Poll, SCHEDULE};

const LOGIN_ORIGIN: &str = "https://www.strem.io";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Provider {
    Facebook,
    Apple,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::Facebook => "Facebook",
            Self::Apple => "Apple",
        }
    }

    fn login_path(self) -> &'static str {
        match self {
            Self::Facebook => "login-fb",
            Self::Apple => "login-apple",
        }
    }

    fn account_path(self) -> &'static str {
        match self {
            Self::Facebook => "login-fb-get-acc",
            Self::Apple => "login-apple-get-acc",
        }
    }
}

/// One browser OAuth attempt in flight: the provider and its 128-bit random state token.
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

    fn account_url(&self) -> String {
        format!(
            "{LOGIN_ORIGIN}/{}/{}",
            self.provider.account_path(),
            self.state
        )
    }
}

/// Credentials returned by `strem.io` after browser consent (tokens masked in `Debug`).
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Credentials {
    Password {
        email: String,
        password: String,
    },
    Apple {
        token: String,
        sub: String,
        email: String,
        name: String,
    },
}

impl std::fmt::Debug for Credentials {
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
pub(crate) enum PollError {
    Cancelled,
    Timeout,
    Unreachable(String),
    Refused(String),
}

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

async fn fetch_account(client: &reqwest::Client, attempt: &Attempt) -> Result<Poll, PollError> {
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

/// Polls `strem.io` until the browser's sign-in finishes, is cancelled, or times out.
pub(crate) async fn wait_for_account(
    client: &reqwest::Client,
    attempt: &Attempt,
    cancelled: impl Fn() -> bool,
) -> Result<Credentials, PollError> {
    wait_with(SCHEDULE, || fetch_account(client, attempt), cancelled).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempts_use_unique_128bit_hex_state_and_provider_paths() {
        let fb = Attempt::start(Provider::Facebook).unwrap();
        let state = fb
            .login_url()
            .strip_prefix("https://www.strem.io/login-fb/")
            .unwrap()
            .to_owned();
        assert_eq!(state.len(), 32);
        assert!(state
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(
            fb.account_url(),
            format!("https://www.strem.io/login-fb-get-acc/{state}")
        );

        let apple = Attempt::start(Provider::Apple).unwrap();
        assert!(apple
            .login_url()
            .starts_with("https://www.strem.io/login-apple/"));
        assert!(apple
            .account_url()
            .starts_with("https://www.strem.io/login-apple-get-acc/"));

        let mut urls: Vec<_> = (0..64)
            .map(|_| Attempt::start(Provider::Facebook).unwrap().login_url())
            .collect();
        urls.sort_unstable();
        urls.dedup();
        assert_eq!(urls.len(), 64);
    }

    /// Live contract check against strem.io. Run with
    /// `cargo test -p stremio-lightning --lib -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "reaches strem.io"]
    async fn the_live_handshake_starts_and_reports_pending() {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("client");
        for provider in [Provider::Facebook, Provider::Apple] {
            let attempt = Attempt::start(provider).unwrap();
            let start = client.get(attempt.login_url()).send().await.unwrap();
            assert!((300..400).contains(&start.status().as_u16()));
            assert_eq!(fetch_account(&client, &attempt).await, Ok(Poll::Pending));
        }
    }
}
