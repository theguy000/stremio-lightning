//! Shell adapter for the sign-in overlay: Slint progress indicators and spawning the
//! browser-assisted OAuth poll task in `plumbing::oauth`.

use std::sync::Arc;

use slint::{ComponentHandle, Weak};
use stremio_core::runtime::msg::{Action, ActionCtx};
use stremio_core::runtime::RuntimeAction;

use crate::plumbing::{links, oauth};
use crate::screens::auth::{self, Generation, Provider};
use crate::{AppRuntime, AuthUi, MainWindow};

/// Drops the sign-in screen's live markers (spinner and provider waiting flags).
pub(crate) fn clear_progress(window: &MainWindow) {
    let ui = window.global::<AuthUi>();
    ui.set_loading(false);
    ui.set_facebook_loading(false);
    ui.set_apple_loading(false);
}

/// Opens the browser for Facebook/Apple sign-in and polls `strem.io` until credentials arrive.
pub(crate) fn start_browser_login(
    provider: Provider,
    window: &Weak<MainWindow>,
    runtime: &Arc<AppRuntime>,
    generation: &Generation,
    handle: &tokio::runtime::Handle,
) {
    let attempt = match oauth::Attempt::start(provider) {
        Ok(attempt) => attempt,
        Err(error) => {
            let message = auth::poll_error_message(&error, provider);
            let _ = window.upgrade_in_event_loop(move |w| {
                clear_progress(&w);
                w.global::<AuthUi>().set_error(message.into());
            });
            return;
        }
    };
    let attempt_id = generation.begin();
    let login_url = attempt.login_url();
    let waiting = auth::waiting_message(provider, &login_url);
    let _ = window.upgrade_in_event_loop(move |w| {
        let ui = w.global::<AuthUi>();
        ui.set_loading(true);
        ui.set_error(waiting.into());
        ui.set_facebook_loading(provider == Provider::Facebook);
        ui.set_apple_loading(provider == Provider::Apple);
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
                oauth::wait_for_account(&client, &attempt, || !generation.is_current(attempt_id))
                    .await
            }
            Err(error) => Err(oauth::PollError::Unreachable(error.to_string())),
        };
        if !generation.is_current(attempt_id) {
            return;
        }
        match result {
            Ok(credentials) => runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Ctx(ActionCtx::Authenticate(credentials.into_auth_request())),
            }),
            Err(oauth::PollError::Cancelled) => {}
            Err(error) => {
                let message = auth::poll_error_message(&error, provider);
                let _ = window.upgrade_in_event_loop(move |w| {
                    clear_progress(&w);
                    w.global::<AuthUi>().set_error(message.into());
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaving_the_sign_in_screen_stops_every_spinner_it_started() {
        i_slint_backend_testing::init_no_event_loop();
        let window = MainWindow::new().expect("a window on the testing backend");
        for (loading, facebook, apple) in [
            (true, false, false),
            (true, true, false),
            (true, false, true),
        ] {
            let ui = window.global::<AuthUi>();
            ui.set_loading(loading);
            ui.set_facebook_loading(facebook);
            ui.set_apple_loading(apple);
            clear_progress(&window);
            assert!(!ui.get_loading());
            assert!(!ui.get_facebook_loading());
            assert!(!ui.get_apple_loading());
        }
    }
}
