//! Mods Logs and About page callbacks.

use super::Wiring;
use crate::plumbing::{clipboard, diagnostics, logs, update};
use crate::screens::mods;
use crate::shell;
use crate::MainWindow;

impl Wiring {
    pub(super) fn filter_logs(&self, part: &str, value: i32) {
        self.app.mods().set_clearing(false);
        let index = usize::try_from(value).unwrap_or(0);
        self.app.mods().with_log_selection(|sel| match part {
            "source" => mods::logs::pick_source(sel, index),
            "level" => mods::logs::pick_level(sel, index),
            other => debug_assert!(false, "no Logs filter is called {other:?}"),
        });
        self.after_mod_action(None);
    }

    pub(super) fn search_logs(&self, text: String) {
        self.app.mods().set_clearing(false);
        self.app
            .mods()
            .with_log_selection(|sel| sel.set_search(&text));
        self.after_mod_action(None);
    }

    pub(super) fn reset_logs_filter(&self) {
        self.app.mods().set_clearing(false);
        self.app.mods().with_log_selection(|sel| sel.reset_filter());
        self.after_mod_action(None);
    }

    pub(super) fn press_log_record(&self, id: u32, shift: bool, ctrl: bool) {
        self.app.mods().set_clearing(false);
        let entries = self.app.logs().entries();
        self.app
            .mods()
            .with_log_selection(|sel| sel.press_record(&entries, u64::from(id), shift, ctrl));
        self.sync_log_selection_now();
    }

    pub(super) fn drag_log_record(&self, id: u32) {
        let entries = self.app.logs().entries();
        self.app
            .mods()
            .with_log_selection(|sel| sel.drag_record(&entries, u64::from(id)));
        self.sync_log_selection_now();
    }

    pub(super) fn release_log_record(&self, id: u32) {
        let entries = self.app.logs().entries();
        self.app
            .mods()
            .with_log_selection(|sel| sel.release_record(&entries, u64::from(id)));
        self.sync_log_selection_now();
    }

    fn sync_log_selection_now(&self) {
        let data = self.app.mods().publish_logs();
        if let Some(window) = self.window.upgrade() {
            shell::view::apply_logs_data(&window, &data.logs);
        }
    }

    pub(super) fn copy_log_record(&self) {
        let entries = self.app.logs().entries();
        let sel = self.app.mods().log_selection();
        let Some((count, text)) = mods::logs::copy_text(&entries, &sel, sel.filter()) else {
            return;
        };
        let notice = match clipboard::copy(&text) {
            Ok(()) if count == 1 => mods::Notice::said("Copied the record."),
            Ok(()) => mods::Notice::said(format!("Copied {count} records.")),
            Err(error) => mods::Notice::failed(error),
        };
        self.after_mod_action(Some(notice));
    }

    pub(super) fn close_log_record(&self) {
        self.app.mods().set_clearing(false);
        self.app
            .mods()
            .with_log_selection(|sel| sel.clear_selection());
        self.sync_log_selection_now();
    }

    pub(super) fn select_all_logs(&self) {
        self.app.mods().set_clearing(false);
        let entries = self.app.logs().entries();
        self.app
            .mods()
            .with_log_selection(|sel| sel.select_all(&entries));
        self.sync_log_selection_now();
    }

    pub(super) fn step_log_record(&self, delta: i32, shift: bool) {
        self.app.mods().set_clearing(false);
        let entries = self.app.logs().entries();
        self.app
            .mods()
            .with_log_selection(|sel| sel.step_record(&entries, delta as isize, shift));
        self.sync_log_selection_now();
    }

    pub(super) fn clear_logs(&self) {
        let was_armed = self.app.mods().clearing();
        if was_armed {
            self.app.logs().clear();
            self.app
                .mods()
                .with_log_selection(|sel| sel.clear_selection());
        }
        self.app.mods().set_clearing(!was_armed);
        self.after_mod_action(None);
    }

    pub(super) fn refresh_about(&self) {
        self.app.mods().reload_installed_themes();
        self.after_mod_action(Some(mods::Notice::said(
            "Read the plugin and theme folders again.",
        )));
        let logs_handle = self.app.logs().clone();
        self.handle.spawn(async move {
            let (level, text) = match update::check_app_update(mods::about::VERSION).await {
                Ok(info) => match &info.new_version {
                    Some(newer) => (
                        logs::Level::Info,
                        format!("A newer version is available: {newer}."),
                    ),
                    None => (
                        logs::Level::Info,
                        format!(
                            "Version {} is the newest released version.",
                            info.current_version
                        ),
                    ),
                },
                Err(error) => (logs::Level::Warn, error),
            };
            logs_handle.record(logs::Source::App, "About", level, &text);
        });
    }

    pub(super) fn export_diagnostics(&self) {
        self.app.mods().reload_installed_themes();
        let about = self.app.mods().rebuild(None).about.clone();
        let records: Vec<_> = self
            .app
            .logs()
            .entries()
            .into_iter()
            .rev()
            .take(mods::about::RECORDS)
            .collect();
        let text = mods::about::report(&about, &records);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default();
        let result = diagnostics::write_report(self.app.data_dir(), now, &text);
        let notice = match result {
            Ok(path) => {
                self.app.mods().set_export(Some(path.display().to_string()));
                self.app.logs().record(
                    logs::Source::App,
                    "About",
                    logs::Level::Info,
                    format!("The diagnostics file was written to {}.", path.display()),
                );
                mods::Notice::said(format!("Wrote the diagnostics file to {}.", path.display()))
            }
            Err(error) => {
                self.app
                    .logs()
                    .record(logs::Source::App, "About", logs::Level::Error, &error);
                mods::Notice::failed(error)
            }
        };
        self.after_mod_action(Some(notice));
    }

    pub(super) fn copy_version(&self) {
        let text = format!(
            "Stremio Lightning {} ({}) on {} {}",
            mods::about::VERSION,
            mods::about::BUILD,
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        let notice = match clipboard::copy(&text) {
            Ok(()) => mods::Notice::said(format!("Copied: {text}")),
            Err(error) => mods::Notice::failed(format!("Nothing was copied: {error}")),
        };
        self.after_mod_action(Some(notice));
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let w = wiring.clone();
    window.on_filter_mods_logs(move |part, value| w.filter_logs(&part, value));
    let w = wiring.clone();
    window.on_search_mods_logs(move |text| w.search_logs(text.to_string()));
    let w = wiring.clone();
    window.on_reset_mods_logs_filter(move || w.reset_logs_filter());
    let w = wiring.clone();
    window.on_press_mods_log(move |id, shift, ctrl| {
        w.press_log_record(id.max(0) as u32, shift, ctrl)
    });
    let w = wiring.clone();
    window.on_drag_mods_log(move |id| w.drag_log_record(id.max(0) as u32));
    let w = wiring.clone();
    window.on_release_mods_log(move |id| w.release_log_record(id.max(0) as u32));
    let w = wiring.clone();
    window.on_select_all_mods_logs(move || w.select_all_logs());
    let w = wiring.clone();
    window.on_step_mods_log(move |delta, shift| w.step_log_record(delta, shift));
    let w = wiring.clone();
    window.on_copy_mods_log(move || w.copy_log_record());
    let w = wiring.clone();
    window.on_close_mods_log(move || w.close_log_record());
    let w = wiring.clone();
    window.on_clear_mods_logs(move || w.clear_logs());
    let w = wiring.clone();
    window.on_refresh_mods_about(move || w.refresh_about());
    let w = wiring.clone();
    window.on_export_mods_diagnostics(move || w.export_diagnostics());
    let w = wiring.clone();
    window.on_copy_mods_version(move || w.copy_version());
}
