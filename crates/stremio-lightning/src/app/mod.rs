//! Deep, Slint-free application core (`App`).
//!
//! `App` owns `stremio-core`'s runtime, the structurally shared `Snapshot`, lazy inactive-screen
//! projection (`ScreenMask`), image cache, logs, and local extras. External shells (`shell/`) and
//! tests talk to `App` through typed screen handles and receive push `AppNotice` updates.

pub mod build;
pub(crate) mod events;
pub mod handles;
pub(crate) mod images;
pub mod mask;
pub mod model;
pub mod mods_handle;
pub(crate) mod project;
pub(crate) mod store;

use std::path::{Path, PathBuf};
use std::sync::{Arc, MutexGuard};

use crate::plumbing::images::Images;
use crate::plumbing::local_settings::LocalExtras;
use crate::plumbing::logs::Logs;

pub use handles::{
    AddonsHandle, BoardHandle, CalendarHandle, DetailsHandle, DiscoverHandle, LibraryHandle,
    PlayerHandle, SettingsHandle, ViewportHandle,
};
pub use mask::{ActiveScreen, AppNotice, ScreenMask};
pub use model::{
    AppModel, AppModelField, AppRuntime, ArmedMod, BoardCatalogRange, BoardData, GridReport,
    Snapshot,
};
pub use mods_handle::{AuthHandle, ModsHandle};

use store::AppStore;

/// Single Slint-free entry point for both the desktop shell and tests.
pub struct App {
    store: Arc<AppStore>,
}

impl App {
    /// Initializes storage, logs, local extras, installed themes, image cache, and `stremio-core`
    /// runtime, then spawns the core event pump on `tokio`.
    pub fn start(data_dir: PathBuf, tokio: &tokio::runtime::Handle) -> Result<Arc<Self>, String> {
        let (store, events) = AppStore::new(data_dir, tokio)?;
        tokio.spawn(events::pump(events, store.clone()));
        Ok(Arc::new(Self { store }))
    }

    /// Attaches the push-notification observer (`shell` or a test spy).
    pub fn set_observer(&self, observer: impl Fn(AppNotice) + Send + Sync + 'static) {
        self.store.set_observer(Arc::new(observer));
    }

    /// Dispatches startup catalog/library/addon loads and initial account API sync.
    pub fn load_initial(&self) {
        build::load_everything(&self.store.runtime);
        build::pull_from_api(&self.store.runtime);
    }

    /// Re-syncs profile, library, addons, and notifications (called on focus gain).
    pub fn sync_account(&self) {
        build::pull_from_api(&self.store.runtime);
    }

    /// Atomically drains pending dirty screen bits (`SeqCst`).
    pub fn take_dirty(&self) -> ScreenMask {
        self.store.take_dirty()
    }

    /// Marks specific screen slices dirty and wakes the observer if none were pending.
    pub fn mark_dirty(&self, mask: ScreenMask) {
        self.store.mark_dirty(mask);
    }

    /// Returns an O(1) structurally shared `Snapshot` clone (`Arc` per screen).
    pub fn snapshot(&self) -> Snapshot {
        self.store.snapshot()
    }

    /// Forces immediate materialization of the specified screen slices.
    pub fn materialize(&self, mask: ScreenMask) -> ScreenMask {
        self.store.materialize(mask)
    }

    pub fn data_dir(&self) -> &Path {
        &self.store.data_dir
    }

    pub fn runtime(&self) -> &Arc<AppRuntime> {
        &self.store.runtime
    }

    pub(crate) fn images(&self) -> &Images {
        &self.store.images
    }

    pub fn logs(&self) -> &Arc<Logs> {
        &self.store.logs
    }

    pub fn local_extras(&self) -> MutexGuard<'_, LocalExtras> {
        self.store.local_extras.lock().unwrap()
    }

    pub fn viewport(&self) -> ViewportHandle<'_> {
        ViewportHandle(&self.store)
    }

    pub fn board(&self) -> BoardHandle<'_> {
        BoardHandle(&self.store)
    }

    pub fn discover(&self) -> DiscoverHandle<'_> {
        DiscoverHandle(&self.store)
    }

    pub fn library(&self) -> LibraryHandle<'_> {
        LibraryHandle(&self.store)
    }

    pub fn details(&self) -> DetailsHandle<'_> {
        DetailsHandle(&self.store)
    }

    pub fn player(&self) -> PlayerHandle<'_> {
        PlayerHandle(&self.store)
    }

    pub fn addons(&self) -> AddonsHandle<'_> {
        AddonsHandle(&self.store)
    }

    pub fn calendar(&self) -> CalendarHandle<'_> {
        CalendarHandle(&self.store)
    }

    pub fn settings(&self) -> SettingsHandle<'_> {
        SettingsHandle(&self.store)
    }

    pub fn mods(&self) -> ModsHandle<'_> {
        ModsHandle(&self.store)
    }

    pub fn auth(&self) -> AuthHandle<'_> {
        AuthHandle(&self.store)
    }
}
