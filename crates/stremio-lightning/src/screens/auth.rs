//! Pure sign-in screen logic: attempt generation tracking, human-readable error copy,
//! provider messages, and login/register core actions.
//! Zero Slint types, zero `MainWindow`, zero async/HTTP, zero browser I/O.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use stremio_core::models::ctx::CtxError;
use stremio_core::runtime::msg::{Action, ActionCtx};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::api::AuthRequest;
use stremio_core::types::profile::GDPRConsent;

pub(crate) use crate::plumbing::oauth::{PollError, Provider};
use crate::AppRuntime;

/// Monotonic generation token for browser sign-in attempts; bumping it invalidates in-flight polls.
#[derive(Clone, Default)]
pub struct Generation(Arc<AtomicU64>);

impl Generation {
    pub fn begin(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn cancel(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_current(&self, id: u64) -> bool {
        self.0.load(Ordering::SeqCst) == id
    }
}

pub(crate) fn waiting_message(provider: Provider, login_url: &str) -> String {
    format!(
        "Log in with {} in your browser; this screen finishes on its own. \
         If no browser opened, visit {login_url}",
        provider.name()
    )
}

pub(crate) fn failure_message(provider: Provider) -> String {
    format!(
        "{} sign-in didn't finish. Try again, or log in with your email and password.",
        provider.name()
    )
}

pub(crate) fn poll_error_message(error: &PollError, provider: Provider) -> String {
    match error {
        PollError::Cancelled => String::new(),
        PollError::Timeout => failure_message(provider),
        PollError::Unreachable(detail) => format!(
            "Couldn't reach Stremio to finish the {} sign-in ({detail}). \
             Check your connection and try again.",
            provider.name()
        ),
        PollError::Refused(detail) => format!("{} sign-in failed: {detail}", provider.name()),
    }
}

/// Rewrites Stremio's three terse login API errors into actionable sentences; passes others through.
pub fn error_message(error: &CtxError) -> String {
    match error {
        CtxError::API(api_err) => match api_err.message.as_str() {
            msg if msg.starts_with("Wrong passphrase") && msg.contains("Facebook") => {
                "Wrong password. This account was created with Facebook, so log in with the Facebook button instead.".to_owned()
            }
            msg if msg.starts_with("Wrong passphrase") => {
                "Wrong password. Please try again.".to_owned()
            }
            "User not found" => {
                "No account uses that email address. Check the spelling, or sign up.".to_owned()
            }
            msg => msg.to_owned(),
        },
        CtxError::Env(env_err) => env_err.message(),
        CtxError::Other(other_err) => other_err.message(),
    }
}

pub(crate) fn login(runtime: &AppRuntime, email: String, password: String) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::Authenticate(AuthRequest::Login {
            email,
            password,
            facebook: false,
        })),
    });
}

pub(crate) fn register(runtime: &AppRuntime, email: String, password: String, gdpr: GDPRConsent) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::Authenticate(AuthRequest::Register {
            email,
            password,
            gdpr_consent: gdpr,
        })),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::models::ctx::OtherError;
    use stremio_core::runtime::EnvError;
    use stremio_core::types::api::APIError;

    #[test]
    fn auth_and_provider_messages_stay_actionable() {
        let api_err = |message: &str| {
            error_message(&CtxError::API(APIError {
                message: message.to_owned(),
                code: 1,
            }))
        };
        assert_eq!(
            api_err("Wrong passphrase"),
            "Wrong password. Please try again."
        );
        assert_eq!(
            api_err("Wrong passphrase (hint: account created with Facebook login)"),
            "Wrong password. This account was created with Facebook, so log in with the Facebook button instead."
        );
        assert_eq!(
            api_err("User not found"),
            "No account uses that email address. Check the spelling, or sign up."
        );
        assert_eq!(
            api_err("User with this email already exists"),
            "User with this email already exists"
        );
        assert_eq!(
            error_message(&CtxError::Env(EnvError::StorageUnavailable)),
            "Storage is not available"
        );
        assert_eq!(
            error_message(&CtxError::Other(OtherError::UserNotLoggedIn)),
            "User is not logged in"
        );

        assert!(
            waiting_message(Provider::Facebook, "https://www.strem.io/login-fb/abc")
                .contains("login-fb/abc")
        );
        assert!(failure_message(Provider::Apple).contains("email and password"));
        assert!(
            poll_error_message(&PollError::Refused("Nope".into()), Provider::Facebook)
                .contains("Nope")
        );
    }
}
