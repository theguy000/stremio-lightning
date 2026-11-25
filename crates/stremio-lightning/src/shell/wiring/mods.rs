//! Mods Screen (Plugins, Themes, Settings, armed Remove) callbacks.

use super::Wiring;
use crate::app::{ArmedMod, ScreenMask};
use crate::screens::mods;
use crate::shell;
use slint::ComponentHandle;

use crate::{MainWindow, ModsUi, PlayerUi};

const DISARM_MS: u64 = 5000;

impl Wiring {
    pub(super) fn select_mods_page(&self, id: String) {
        let page = mods::Page::from_id(&id);
        if page != mods::Page::Settings {
            self.app
                .mods()
                .with_edits_mut(|e| *e = mods::settings::Edits::default());
        }
        if matches!(page, mods::Page::Themes | mods::Page::About) {
            self.app.mods().reload_installed_themes();
        }
        self.disarm_mods();
        self.app.mods().set_page(page.id().to_owned());
        if let Some(window) = self.window.upgrade() {
            window.global::<ModsUi>().set_page(page.id().into());
        }
        let data = self.app.mods().rebuild(None);
        if let Some(window) = self.window.upgrade() {
            shell::view::show_mods(&window, &data);
        }
    }

    fn mods_page_kind(&self) -> Option<mods::catalogue::Kind> {
        mods::Page::from_id(&self.app.mods().page()).kind()
    }

    pub(super) fn toggle_mod(&self, filename: String, enabled: bool) {
        self.disarm_mods();
        let Some(kind) = self.mods_page_kind() else {
            return;
        };
        let notice = {
            let mut extras = self.app.local_extras();
            let notice = match kind {
                mods::catalogue::Kind::Plugin => {
                    mods::builtin::set_enabled(&mut extras, &filename, enabled);
                    None
                }
                mods::catalogue::Kind::Theme => {
                    mods::catalogue::set_theme_enabled(&mut extras, &filename, enabled)
                }
            };
            extras.save(self.app.data_dir());
            notice
        };
        self.recorded(kind, &filename, notice.as_ref());
        if kind == mods::catalogue::Kind::Theme {
            self.apply_themes();
        }
        self.after_mod_action(None);
    }

    pub(crate) fn apply_skip_submit_button(&self) {
        let shown = {
            let extras = self.app.local_extras();
            mods::builtin::fields("builtin:aniskip")
                .iter()
                .find(|field| field.key == "submit_button")
                .is_none_or(|field| {
                    mods::builtin::value(&extras, "builtin:aniskip", field)
                        == serde_json::Value::Bool(true)
                })
        };
        *self.skip_submit.lock().unwrap() = shown;
        if let Some(window) = self.window.upgrade() {
            window.global::<PlayerUi>().set_skip_submit(shown);
        }
    }

    pub(crate) fn init_mods(&self) {
        self.apply_skip_submit_button();
        let data = self.app.mods().rebuild(None);
        self.app.mods().set_plugin(data.plugin.clone());
        if let Some(window) = self.window.upgrade() {
            shell::view::show_mods(&window, &data);
        }
    }

    pub(super) fn pick_plugin(&self, filename: String) {
        let extras = self.app.local_extras().clone();
        self.app.mods().with_edits_mut(|edits| {
            *edits = if filename.is_empty() {
                mods::settings::Edits::default()
            } else {
                mods::settings::edits(&filename, &extras)
            };
        });
        self.app.mods().set_plugin(filename);
        self.select_mods_page(mods::Page::Settings.id().to_owned());
    }

    pub(super) fn edit_plugin_setting(&self, key: String, value: serde_json::Value) {
        self.app
            .mods()
            .with_edits_mut(|edits| mods::settings::set_value(edits, &key, &value));
        self.after_mod_action(None);
    }

    pub(super) fn save_plugin_settings(&self) {
        let outcome = self.app.mods().with_edits_mut(|edits| {
            if edits.filename.is_empty() {
                return None;
            }
            let filename = edits.filename.clone();
            if !edits.dirty() {
                return Some((filename, None));
            }
            let mut extras = self.app.local_extras().clone();
            let saved = mods::settings::save(edits, &mut extras);
            Some((filename, Some((saved, extras))))
        });
        let Some((filename, result)) = outcome else {
            return;
        };
        let Some((saved, extras)) = result else {
            self.after_mod_action(Some(mods::Notice::said("Nothing to save.")));
            return;
        };
        if saved.written {
            *self.app.local_extras() = extras.clone();
            extras.save(self.app.data_dir());
            self.apply_skip_submit_button();
            let _ = saved.values;
            self.after_mod_action(Some(mods::Notice::said(format!("Saved {filename}."))));
        } else {
            self.after_mod_action(Some(mods::Notice::failed(format!(
                "{filename}: {}",
                saved.error
            ))));
        }
    }

    pub(super) fn remove_mod(&self, id: String) {
        let Some(kind) = self.mods_page_kind() else {
            return;
        };
        let pressed = ArmedMod {
            id: id.clone(),
            page: self.app.mods().page(),
        };
        if self.app.mods().armed() != Some(pressed.clone()) {
            self.app.mods().set_armed(Some(pressed));
            self.restart_disarm();
            self.show_mods_now();
            return;
        }
        self.disarm_mods();
        let notice = {
            let mut extras = self.app.local_extras();
            let notice = match kind {
                mods::catalogue::Kind::Plugin => mods::catalogue::non_removable_plugin_notice(&id),
                mods::catalogue::Kind::Theme => match mods::builtin::get_theme(&id) {
                    Some(b) => mods::catalogue::builtin_not_removable(b.name),
                    None => {
                        let deleted =
                            crate::plumbing::theme::store::delete(self.app.data_dir(), &id);
                        mods::catalogue::finish_theme_remove(&mut extras, &id, deleted)
                    }
                },
            };
            extras.save(self.app.data_dir());
            notice
        };
        if kind == mods::catalogue::Kind::Theme {
            self.app.mods().reload_installed_themes();
            self.apply_themes();
        }
        self.recorded(kind, &id, Some(&notice));
        self.after_mod_action(Some(notice));
    }

    pub(super) fn dismiss_mods_remove(&self) {
        if self.disarm_mods() {
            self.show_mods_now();
        }
    }

    fn show_mods_now(&self) {
        if let Some(window) = self.window.upgrade() {
            let data = self.app.mods().view();
            shell::view::show_mods(&window, &data);
        }
    }

    fn disarm_mods(&self) -> bool {
        let had = self.app.mods().set_armed(None);
        if had {
            self.mods_disarm.stop();
        }
        had
    }

    fn restart_disarm(&self) {
        let app = self.app.clone();
        self.mods_disarm.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(DISARM_MS),
            move || {
                if app.mods().set_armed(None) {
                    app.mark_dirty(ScreenMask::MODS);
                }
            },
        );
    }

    fn recorded(&self, kind: mods::catalogue::Kind, filename: &str, notice: Option<&mods::Notice>) {
        let Some(notice) = notice else { return };
        let source = match kind {
            mods::catalogue::Kind::Plugin => crate::plumbing::logs::Source::Plugin,
            mods::catalogue::Kind::Theme => crate::plumbing::logs::Source::Theme,
        };
        let level = if notice.failed {
            crate::plumbing::logs::Level::Error
        } else {
            crate::plumbing::logs::Level::Info
        };
        self.app
            .logs()
            .record(source, filename, level, notice.text.clone());
    }

    pub(super) fn after_mod_action(&self, notice: Option<mods::Notice>) {
        let data = self.app.mods().rebuild(notice);
        if let Some(window) = self.window.upgrade() {
            shell::view::show_mods(&window, &data);
        }
    }

    fn apply_themes(&self) {
        let data_dir = self.app.data_dir().to_path_buf();
        let logs = self.app.logs().clone();
        let _ = self.window.upgrade_in_event_loop(move |window| {
            crate::shell::theme_apply::apply_and_record(&window, &data_dir, &logs);
        });
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let ui = window.global::<ModsUi>();
    let w = wiring.clone();
    ui.on_select_page(move |page| w.select_mods_page(page.to_string()));
    let w = wiring.clone();
    ui.on_toggle_mod(move |filename, enabled| w.toggle_mod(filename.to_string(), enabled));
    let w = wiring.clone();
    ui.on_remove_mod(move |filename| w.remove_mod(filename.to_string()));
    let w = wiring.clone();
    ui.on_dismiss_remove(move || w.dismiss_mods_remove());
    let w = wiring.clone();
    ui.on_pick_settings_plugin(move |filename| w.pick_plugin(filename.to_string()));
    let w = wiring.clone();
    ui.on_edit_settings_toggle(move |key, on| {
        w.edit_plugin_setting(key.to_string(), serde_json::Value::Bool(on))
    });
    let w = wiring.clone();
    ui.on_edit_settings_text(move |key, text| {
        w.edit_plugin_setting(key.to_string(), serde_json::Value::String(text.to_string()))
    });
    let w = wiring.clone();
    ui.on_edit_settings_number(move |key, number| {
        if let Some(n) = serde_json::Number::from_f64(f64::from(number)) {
            w.edit_plugin_setting(key.to_string(), serde_json::Value::Number(n));
        }
    });
    let w = wiring.clone();
    ui.on_save_settings(move || w.save_plugin_settings());
}
