//! The sign-in screen: the browser-assisted Facebook and Apple handshake, and the wording of
//! every failure it can produce.
//!
//! `browser` does the handshake (`Attempt`, `wait_for_account`). This file does the things a
//! screen needs: remembering whether the attempt in flight is still wanted, and saying what
//! went wrong in words a person can act on.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use slint::Weak;
use stremio_core::models::ctx::CtxError;
use stremio_core::runtime::msg::{Action, ActionCtx};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::api::AuthRequest;
use stremio_core::types::profile::GDPRConsent;

use crate::plumbing::links;
use crate::{AppRuntime, MainWindow};

mod browser;

pub(crate) use browser::Provider;

/// Every browser sign-in owns a generation; bumping it stops that attempt's poll task without
/// letting it touch the window or the runtime again.
#[derive(Clone, Default)]
pub struct Generation(Arc<AtomicU64>);

impl Generation {
    /// Takes over: the returned id is the one this attempt must still hold to be heard.
    pub fn begin(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Drops whatever attempt is in flight.
    pub fn cancel(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_current(&self, id: u64) -> bool {
        self.0.load(Ordering::SeqCst) == id
    }
}

/// Drops the sign-in screen's live markers: the spinner and either provider's
/// "waiting for the browser" state. The caller sets the message to leave behind.
pub(crate) fn clear_progress(window: &MainWindow) {
    window.set_auth_loading(false);
    window.set_auth_facebook_loading(false);
    window.set_auth_apple_loading(false);
}

/// Stremio's login API has three failure answers that do not read like a login form: a wrong
/// password is "Wrong passphrase", an account created with Facebook gets that plus "(hint:
/// account created with Facebook login, you can try logging in with Facebook)", and an address
/// with no account at all is "User not found". Each becomes a sentence that says what to do
/// next; every other message already reads like one, so it passes through untouched.
pub fn error_message(error: &CtxError) -> String {
    match error {
        CtxError::API(api_err) => match api_err.message.as_str() {
            message if message.starts_with("Wrong passphrase") && message.contains("Facebook") => {
                "Wrong password. This account was created with Facebook, so log in with the Facebook button instead.".to_string()
            }
            message if message.starts_with("Wrong passphrase") => {
                "Wrong password. Please try again.".to_string()
            }
            "User not found" => {
                "No account uses that email address. Check the spelling, or sign up.".to_string()
            }
            message => message.to_string(),
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

/// Hands a Facebook or Apple sign-in to the system browser and picks the account back up once
/// `strem.io` has it.
///
/// The browser cannot report back to us, and the providers only consent to `strem.io`, so the
/// shell opens the same URL Stremio's web client opens and then asks `strem.io` for the account
/// behind that state until it answers.
pub(crate) fn start_browser_login(
    provider: Provider,
    window: &Weak<MainWindow>,
    runtime: &Arc<AppRuntime>,
    generation: &Generation,
    handle: &tokio::runtime::Handle,
) {
    let attempt = match browser::Attempt::start(provider) {
        Ok(attempt) => attempt,
        Err(error) => {
            let message = error.message(provider);
            let _ = window.upgrade_in_event_loop(move |w| {
                clear_progress(&w);
                w.set_auth_error(message.into());
            });
            return;
        }
    };
    let attempt_id = generation.begin();
    let login_url = attempt.login_url();
    let waiting = provider.waiting_message(&login_url);
    let _ = window.upgrade_in_event_loop(move |w| {
        w.set_auth_loading(true);
        w.set_auth_error(waiting.into());
        w.set_auth_facebook_loading(provider == Provider::Facebook);
        w.set_auth_apple_loading(provider == Provider::Apple);
    });
    links::open(&login_url);

    let (window, runtime, generation) = (window.clone(), runtime.clone(), generation.clone());
    handle.spawn(async move {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(20))
            .build();
        let result = match client {
            Ok(client) => {
                browser::wait_for_account(&client, &attempt, || !generation.is_current(attempt_id))
                    .await
            }
            Err(error) => Err(browser::PollError::Unreachable(error.to_string())),
        };
        // A dismissed overlay or a newer attempt owns the sign-in screen now.
        if !generation.is_current(attempt_id) {
            return;
        }
        match result {
            Ok(credentials) => runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Ctx(ActionCtx::Authenticate(credentials.into_auth_request())),
            }),
            Err(browser::PollError::Cancelled) => {}
            Err(error) => {
                let message = error.message(provider);
                let _ = window.upgrade_in_event_loop(move |w| {
                    clear_progress(&w);
                    w.set_auth_error(message.into());
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::models::ctx::OtherError;
    use stremio_core::runtime::EnvError;
    use stremio_core::types::api::APIError;

    #[test]
    fn auth_failures_read_as_instructions() {
        let api_err = |message: &str| {
            error_message(&CtxError::API(APIError {
                message: message.to_string(),
                code: 1,
            }))
        };
        // A wrong password on an email account.
        assert_eq!(
            api_err("Wrong passphrase"),
            "Wrong password. Please try again."
        );
        // A wrong password on an account created with Facebook: the API appends its hint in the
        // message. The wording below must stay in step with the branch order above.
        let hinted = "Wrong passphrase (hint: account created with Facebook login, you can try logging in with Facebook)";
        assert_eq!(
            api_err(hinted),
            "Wrong password. This account was created with Facebook, so log in with the Facebook button instead."
        );
        // An address with no account at all.
        assert_eq!(
            api_err("User not found"),
            "No account uses that email address. Check the spelling, or sign up."
        );
        // Other API messages are already written for people.
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
    }
}
