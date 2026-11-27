//! Settings Screen actions: streaming server URLs/options, Trakt, Calendar, Account, and Search.

use slint::{ComponentHandle, ModelRc, VecModel};

use super::Wiring;
use crate::plumbing::{clipboard, file_dialog, links};
use crate::screens::settings::{self, cache_root, streaming};
use crate::{MainWindow, SettingsSearchHit, SettingsUi};

impl Wiring {
    pub(crate) fn sync_live_settings(&self) {
        let view = self.app.settings().view();
        self.video.apply_settings(&view.player_config);
        self.sync_discord_enabled();
        self.gamepad.set_enabled(view.gamepad_support);
    }

    pub(super) fn try_select_server_option(&self, key: &str, index: usize) -> bool {
        if !matches!(
            key,
            "caching" | "torrentProfile" | "transcodingProfile" | "remoteHttps"
        ) {
            return false;
        }
        let Ok(model) = self.app.runtime().model() else {
            return false;
        };
        let next = streaming::update_server_option(&model.streaming_server, key, index);
        drop(model);
        let Some(next) = next else {
            return false;
        };
        streaming::dispatch_server_settings(self.app.runtime(), next);
        true
    }

    pub(super) fn set_interface_scale(&self, scale: i32) {
        let index = match scale {
            ..=75 => 0,
            76..=100 => 1,
            101..=125 => 2,
            126..=150 => 3,
            _ => 4,
        };
        self.select_setting_option("interfaceScale".to_owned(), index);
    }

    pub(super) fn add_server_url(&self, raw: String) {
        match streaming::add_server_url(self.app.runtime(), &raw) {
            Ok(()) => self.say("New URL added"),
            Err(err) => self.say(&err),
        }
    }

    pub(super) fn select_server_url(&self, raw: String) {
        if let Err(err) = streaming::select_server_url(self.app.runtime(), &raw) {
            self.say(&err);
        }
    }

    pub(super) fn delete_server_url(&self, raw: String) {
        streaming::delete_server_url(self.app.runtime(), &raw);
    }

    pub(super) fn copy_remote_url(&self) {
        let remote = self.app.settings().view().streaming.remote_url.clone();
        if remote.is_empty() {
            return;
        }
        let msg = clipboard::copy(&remote).map_or_else(
            |e| e,
            |_| "Remote URL has been copied to your clipboard".to_owned(),
        );
        self.say(&msg);
    }

    pub(super) fn toggle_trakt(&self) {
        let view = self.app.settings().view();
        if !view.logged_in {
            return;
        }
        if view.trakt_authenticated {
            settings::logout_trakt(self.app.runtime());
        } else if !view.user_id.is_empty() {
            links::open(&format!("https://www.strem.io/trakt/auth/{}", view.user_id));
        }
    }

    pub(super) fn subscribe_calendar(&self) {
        let view = self.app.settings().view();
        if !view.logged_in || view.user_id.is_empty() {
            return;
        }
        links::open(&format!(
            "https://www.strem.io/calendar/{}.ics",
            view.user_id
        ));
        self.say("Calendar file opened. Import it to your calendar app.");
    }

    pub(super) fn change_password(&self) {
        let view = self.app.settings().view();
        if !view.logged_in || view.user_email.is_empty() {
            return;
        }
        links::open(&format!(
            "https://www.strem.io/reset-password/{}",
            view.user_email
        ));
    }

    pub(super) fn export_user_data(&self) {
        if self.app.settings().view().logged_in {
            settings::start_data_export(self.app.runtime());
        }
    }

    pub(super) fn copy_version_details(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let ui = window.global::<SettingsUi>();
        let mut lines = vec![
            format!("App Version: {}", ui.get_app_version()),
            format!("Build Version: {}", ui.get_build_version()),
        ];
        let server_ver = ui.get_data().server_version;
        if !server_ver.is_empty() {
            lines.push(format!("Server Version: {server_ver}"));
        }
        let text = lines.join(
            "
",
        );
        let msg = clipboard::copy(&text).map_or_else(
            |e| e,
            |_| "Version details were copied to your clipboard".to_owned(),
        );
        self.say(&msg);
    }

    pub(super) fn search_settings(&self, query: String) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let view = self.app.settings().view();
        let hits =
            settings::search_settings(&query, view.logged_in, view.streaming.status == "online");
        let rows: Vec<SettingsSearchHit> = hits
            .into_iter()
            .map(|h| SettingsSearchHit {
                label: h.label.into(),
                crumb: h.crumb.into(),
                page: h.page.into(),
                row: h.row.into(),
            })
            .collect();
        window
            .global::<SettingsUi>()
            .set_search_results(ModelRc::new(VecModel::from(rows)));
    }

    pub(super) fn select_cache_root(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let cr = self.app.settings().view().streaming.cache_root.clone();
        if !cr.supported || cr.updating {
            return;
        }
        if let Some(chosen) = cr.values.get(index) {
            if chosen != &cr.current {
                cache_root::update_cache_root(self.app.runtime(), chosen.clone());
            }
            return;
        }
        if index == cr.values.len() {
            let app = self.app.clone();
            let start = (!cr.current.is_empty()).then(|| std::path::PathBuf::from(cr.current));
            self.handle.spawn(async move {
                if let Some(picked) =
                    file_dialog::pick_folder("Choose cache folder", start.as_deref()).await
                {
                    cache_root::update_cache_root(app.runtime(), picked);
                }
            });
        }
    }

    pub(super) fn select_mpv_config(&self, index: i32) {
        let prev = self.app.local_extras().mpv_config_path.clone();
        match index {
            0 => {
                let start = file_dialog::default_mpv_dir(prev.as_deref());
                let app = self.app.clone();
                let weak = self.window.clone();
                self.handle.spawn(async move {
                    let Some(picked) = file_dialog::pick_mpv_config(start.as_deref()).await else {
                        return;
                    };
                    if prev.as_deref() == Some(picked.as_str()) {
                        return;
                    }
                    {
                        let mut extras = app.local_extras();
                        extras.mpv_config_path = Some(picked);
                        extras.save(app.data_dir());
                    }
                    app.settings().refresh();
                    let _ = weak.upgrade_in_event_loop(|window| {
                        window.set_toast(
                            "Stremio needs to restart for this setting to take effect.".into(),
                        );
                    });
                });
            }
            1 => {
                if prev.is_none() {
                    return;
                }
                {
                    let mut extras = self.app.local_extras();
                    extras.mpv_config_path = None;
                    extras.save(self.app.data_dir());
                }
                self.app.settings().refresh();
                self.video
                    .apply_settings(&self.app.settings().view().player_config);
                self.say("Stremio needs to restart for this setting to take effect.");
            }
            _ => {}
        }
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let set = window.global::<SettingsUi>();
    let w = wiring.clone();
    set.on_set_interface_scale(move |s| w.set_interface_scale(s));
    let w = wiring.clone();
    set.on_add_server_url(move |u| w.add_server_url(u.to_string()));
    let w = wiring.clone();
    set.on_select_server_url(move |u| w.select_server_url(u.to_string()));
    let w = wiring.clone();
    set.on_delete_server_url(move |u| w.delete_server_url(u.to_string()));
    let w = wiring.clone();
    set.on_copy_remote_url(move || w.copy_remote_url());
    let w = wiring.clone();
    set.on_toggle_trakt(move || w.toggle_trakt());
    let w = wiring.clone();
    set.on_subscribe_calendar(move || w.subscribe_calendar());
    let w = wiring.clone();
    set.on_change_password(move || w.change_password());
    let w = wiring.clone();
    set.on_export_user_data(move || w.export_user_data());
    let w = wiring.clone();
    set.on_copy_version_details(move || w.copy_version_details());
    let w = wiring.clone();
    set.on_search_changed(move |q| w.search_settings(q.to_string()));
    let w = wiring.clone();
    set.on_select_cache_root(move |i| w.select_cache_root(i));
    let w = wiring.clone();
    set.on_select_mpv_config(move |i| w.select_mpv_config(i));
}
