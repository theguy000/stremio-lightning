//! Settings Screen, Stream Server supervisor, and Auth Modal callbacks.

use stremio_core::types::profile::GDPRConsent;

use super::Wiring;
use crate::plumbing::{links, logs, stream_server};
use crate::screens::{auth, settings};
use crate::MainWindow;

impl Wiring {
    fn setting_changed(&self, updated: Option<stremio_core::types::profile::Settings>) {
        if let Some(updated) = updated {
            settings::dispatch_settings(self.app.runtime(), updated);
        }
        let data = self.app.settings().refresh();
        self.video.apply_settings(&data.player_config);
    }

    pub(super) fn select_setting_option(&self, key: String, option: i32) {
        let Ok(index) = usize::try_from(option) else {
            return;
        };
        let updated = self.with_current_settings(|current, extras| {
            settings::apply_select_option(current, extras, &key, index)
        });
        self.setting_changed(updated);
    }

    pub(super) fn toggle_setting_option(&self, key: String, enabled: bool) {
        let updated = self.with_current_settings(|current, extras| {
            settings::apply_toggle_option(current, extras, &key, enabled)
        });
        self.setting_changed(updated);
    }

    fn with_current_settings(
        &self,
        change: impl FnOnce(
            &stremio_core::types::profile::Settings,
            &mut settings::LocalExtras,
        ) -> Option<stremio_core::types::profile::Settings>,
    ) -> Option<stremio_core::types::profile::Settings> {
        let current = self
            .app
            .runtime()
            .model()
            .ok()?
            .ctx
            .profile
            .settings
            .clone();
        let mut extras = self.app.local_extras();
        let updated = change(&current, &mut extras);
        extras.save(self.app.data_dir());
        updated
    }

    pub(super) fn reload_streaming_server(&self) {
        let _ = self.window.upgrade_in_event_loop(|window| {
            window.set_server_status("Stream server: restarting...".into())
        });
        let _ = self.server.stop();
        let started = self.server.start();
        self.report_server_state(started);
        settings::reload_server(self.app.runtime());
    }

    pub(crate) fn report_server_state(&self, started: Result<(), String>) {
        let (window, server) = (self.window.clone(), self.server.clone());
        let logs_handle = self.app.logs().clone();
        self.handle.spawn(async move {
            let state = stream_server::wait_until_answering(&server, started).await;
            let text = settings::server_status(&state);
            let level = match state {
                stream_server::State::Ready => logs::Level::Info,
                stream_server::State::PortTaken => logs::Level::Debug,
                _ => logs::Level::Warn,
            };
            logs_handle.record(logs::Source::Server, stream_server::ADDRESS, level, &text);
            let _ =
                window.upgrade_in_event_loop(move |window| window.set_server_status(text.into()));
        });
    }

    pub(super) fn start_browser_login(&self, provider: auth::Provider) {
        crate::shell::auth::start_browser_login(
            provider,
            &self.window,
            self.app.runtime(),
            &self.app.auth().generation(),
            &self.handle,
        );
    }

    pub(super) fn dismiss_sign_in(&self) {
        self.app.auth().generation().cancel();
        let _ = self.window.upgrade_in_event_loop(|window| {
            crate::shell::auth::clear_progress(&window);
            window.set_auth_open(false);
            window.set_auth_error("".into());
        });
    }

    pub(super) fn forgot_password(&self, email: String) {
        let email = email.trim();
        if email.is_empty() {
            links::open("reset-password");
        } else {
            let encoded =
                url::form_urlencoded::byte_serialize(email.as_bytes()).collect::<String>();
            links::open(&format!(
                "https://www.strem.io/reset-password/?email={encoded}"
            ));
        }
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let w = wiring.clone();
    window.on_select_setting_option(move |key, opt| w.select_setting_option(key.to_string(), opt));
    let w = wiring.clone();
    window.on_toggle_setting_option(move |key, on| w.toggle_setting_option(key.to_string(), on));
    window.on_open_settings_link(move |kind| links::open(&kind));
    let w = wiring.clone();
    window.on_logout_user(move || settings::logout(w.app.runtime()));
    let w = wiring.clone();
    window.on_reload_streaming_server(move || w.reload_streaming_server());
    let w = wiring.clone();
    window.on_submit_auth_login(move |email, password| {
        let _ = w.window.upgrade_in_event_loop(|window| {
            window.set_auth_loading(true);
            window.set_auth_error("".into());
        });
        auth::login(
            w.app.runtime(),
            email.trim().to_string(),
            password.to_string(),
        );
    });
    let w = wiring.clone();
    window.on_submit_auth_register(move |email, password, tos, privacy, marketing| {
        let _ = w.window.upgrade_in_event_loop(|window| {
            window.set_auth_loading(true);
            window.set_auth_error("".into());
        });
        auth::register(
            w.app.runtime(),
            email.trim().to_string(),
            password.to_string(),
            GDPRConsent {
                tos,
                privacy,
                marketing,
                from: Some("stremio-lightning".to_string()),
            },
        );
    });
    let w = wiring.clone();
    window.on_auth_guest_login(move || w.dismiss_sign_in());
    let w = wiring.clone();
    window.on_auth_facebook_login(move || w.start_browser_login(auth::Provider::Facebook));
    let w = wiring.clone();
    window.on_auth_apple_login(move || w.start_browser_login(auth::Provider::Apple));
    let w = wiring.clone();
    window.on_auth_forgot_password(move |email| w.forgot_password(email.to_string()));
    let w = wiring.clone();
    window.on_auth_dismissed(move || w.dismiss_sign_in());
}
