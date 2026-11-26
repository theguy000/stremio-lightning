//! Typed `ModsHandle` and `AuthHandle` views into `AppStore`.

use std::sync::Arc;

use super::store::AppStore;
use super::{ArmedMod, ScreenMask};
use crate::plumbing::theme::store as theme_store;
use crate::screens::{auth, mods};

pub struct ModsHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> ModsHandle<'a> {
    pub fn view(&self) -> Arc<mods::Data> {
        self.0.snapshot.lock().unwrap().mods.clone()
    }

    pub fn page(&self) -> String {
        self.0.mods_page.lock().unwrap().clone()
    }

    pub fn set_page(&self, page: String) {
        *self.0.mods_page.lock().unwrap() = page;
    }

    pub fn plugin(&self) -> String {
        self.0.mods_plugin.lock().unwrap().clone()
    }

    pub fn set_plugin(&self, plugin: String) {
        *self.0.mods_plugin.lock().unwrap() = plugin;
    }

    pub fn edits(&self) -> mods::settings::Edits {
        self.0.mods_edits.lock().unwrap().clone()
    }

    pub fn with_edits_mut<R>(&self, f: impl FnOnce(&mut mods::settings::Edits) -> R) -> R {
        f(&mut self.0.mods_edits.lock().unwrap())
    }

    pub fn armed(&self) -> Option<ArmedMod> {
        self.0.mods_armed.lock().unwrap().clone()
    }

    pub fn set_armed(&self, armed: Option<ArmedMod>) -> bool {
        let id = armed.as_ref().map(|a| a.id.clone());
        let had = std::mem::replace(&mut *self.0.mods_armed.lock().unwrap(), armed).is_some();
        let mut slot = self.0.snapshot.lock().unwrap();
        let mut next = (*slot.mods).clone();
        next.armed = id;
        slot.mods = Arc::new(next);
        had
    }

    pub fn clearing(&self) -> bool {
        *self.0.mods_clearing.lock().unwrap()
    }

    pub fn set_clearing(&self, armed: bool) -> bool {
        std::mem::replace(&mut *self.0.mods_clearing.lock().unwrap(), armed)
    }

    pub fn set_export(&self, path: Option<String>) {
        *self.0.mods_export.lock().unwrap() = path;
    }

    pub fn with_log_selection<R>(
        &self,
        f: impl FnOnce(&mut mods::log_selection::LogSelection) -> R,
    ) -> R {
        f(&mut self.0.log_selection.lock().unwrap())
    }

    pub fn log_selection(&self) -> mods::log_selection::LogSelection {
        self.0.log_selection.lock().unwrap().clone()
    }

    pub fn reload_installed_themes(&self) {
        *self.0.installed_themes.lock().unwrap() = theme_store::list(&self.0.data_dir);
    }

    pub fn rebuild(&self, notice: Option<mods::Notice>) -> Arc<mods::Data> {
        let mut data = self.0.build_mods_data();
        let mut slot = self.0.snapshot.lock().unwrap();
        data.notice = notice.unwrap_or_else(|| slot.mods.notice.clone());
        let arc = Arc::new(data);
        slot.mods = arc.clone();
        drop(slot);
        self.0.mark_dirty(ScreenMask::MODS);
        arc
    }

    pub fn publish_logs(&self) -> Arc<mods::Data> {
        self.0.refresh_mods_logs();
        self.0.snapshot.lock().unwrap().mods.clone()
    }
}

pub struct AuthHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> AuthHandle<'a> {
    pub fn generation(&self) -> auth::Generation {
        self.0.auth.clone()
    }
}
