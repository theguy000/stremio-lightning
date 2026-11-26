//! Authoritative Slint-free application state (`AppStore`).
//!
//! Owns the single copy of `stremio-core`'s runtime, `Snapshot` (structurally shared `Arc<Data>`
//! per screen), dirty/stale `ScreenMask`s for incremental & lazy projection, image cache, logs,
//! local extras, and per-screen view state. Emits `AppNotice` push notifications to its observer.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::Receiver;
use stremio_core::runtime::msg::{Action, ActionCatalogsWithExtra};
use stremio_core::runtime::{Env, RuntimeAction, RuntimeEvent};
use stremio_core::types::profile::Profile;

use super::images::{shown_images, snapshot_images};
use super::{
    ActiveScreen, AppModel, AppModelField, AppNotice, AppRuntime, ArmedMod, BoardCatalogRange,
    GridReport, ScreenMask, Snapshot,
};
use crate::plumbing::env::AppEnv;
use crate::plumbing::images::Images;
use crate::plumbing::local_settings::{self, LocalExtras};
use crate::plumbing::logs::Logs;
use crate::plumbing::theme::store::{self as theme_store, InstalledTheme};
use crate::screens::{auth, board, calendar, details, discover, mods, player, settings};

pub(crate) type Observer = Arc<dyn Fn(AppNotice) + Send + Sync>;

pub(crate) struct AppStore {
    pub(crate) data_dir: PathBuf,
    pub(crate) runtime: Arc<AppRuntime>,
    pub(crate) images: Images,
    pub(crate) logs: Arc<Logs>,
    pub(crate) local_extras: Mutex<LocalExtras>,
    pub(crate) installed_themes: Mutex<Result<Vec<InstalledTheme>, String>>,
    pub(crate) snapshot: Mutex<Snapshot>,
    dirty: AtomicU16,
    pub(crate) stale: Mutex<ScreenMask>,
    observer: Mutex<Option<Observer>>,

    pub(crate) active_screen: Mutex<ActiveScreen>,
    pub(crate) board_visible: Mutex<board::Viewport>,
    pub(crate) discover_visible: Mutex<(usize, usize)>,
    pub(crate) discover_grid: Mutex<Option<GridReport>>,
    pub(crate) discover_selected: AtomicUsize,

    pub(crate) details_season: AtomicUsize,
    pub(crate) details_picked: Mutex<Option<String>>,
    pub(crate) details_video_query: Mutex<String>,
    pub(crate) details_moved: Mutex<Option<(u32, u32)>>,

    pub(crate) player_season: AtomicUsize,
    pub(crate) playback_error: Mutex<String>,

    pub(crate) mods_page: Mutex<String>,
    pub(crate) mods_plugin: Mutex<String>,
    pub(crate) mods_edits: Mutex<mods::settings::Edits>,
    pub(crate) log_selection: Mutex<mods::log_selection::LogSelection>,
    pub(crate) mods_clearing: Mutex<bool>,
    pub(crate) mods_export: Mutex<Option<String>>,
    pub(crate) mods_armed: Mutex<Option<ArmedMod>>,

    pub(crate) board_catalogs: Mutex<Option<BoardCatalogRange>>,
    pub(crate) calendar_library: Mutex<Option<calendar::LibrarySignature>>,
    pub(crate) auth: auth::Generation,
}

pub(crate) type CoreEventReceiver = Receiver<RuntimeEvent<AppEnv, AppModel>>;

impl AppStore {
    pub(crate) fn new(
        data_dir: PathBuf,
        tokio: &tokio::runtime::Handle,
    ) -> Result<(Arc<Self>, CoreEventReceiver), String> {
        AppEnv::init(data_dir.clone(), tokio.clone()).map_err(|e| e.message())?;
        let logs = Arc::new(Logs::recording_to(data_dir.join("logs").join("app.log")));
        let profile: Profile = tokio
            .block_on(AppEnv::get_storage(
                stremio_core::constants::PROFILE_STORAGE_KEY,
            ))
            .map_err(|e| e.message())?
            .unwrap_or_default();
        let (runtime, events) = super::build::runtime(profile);
        let runtime = Arc::new(runtime);
        let images = tokio.block_on(async { Images::default() });
        let mut extras = LocalExtras::load(&data_dir);
        if let Some(id) = local_settings::new_uuid() {
            if mods::builtin::ensure_submitter(&mut extras, &id) {
                extras.save(&data_dir);
            }
        }
        let installed_themes = theme_store::list(&data_dir);

        let mut initial = Snapshot::default();
        if let Ok(model) = runtime.model() {
            initial.settings = Arc::new(settings::data(&model, &extras));
        }
        initial.mods = Arc::new(mods::data(
            &theme_store::themes_dir(&data_dir),
            &extras,
            installed_themes.as_deref().map_err(String::as_str),
        ));

        let store = Arc::new(Self {
            data_dir,
            runtime,
            images,
            logs: logs.clone(),
            local_extras: Mutex::new(extras),
            installed_themes: Mutex::new(installed_themes),
            snapshot: Mutex::new(initial),
            dirty: AtomicU16::new(0),
            stale: Mutex::new(ScreenMask::EMPTY),
            observer: Mutex::new(None),
            active_screen: Mutex::new(ActiveScreen::Board),
            board_visible: Mutex::new(board::Viewport::default()),
            discover_visible: Mutex::new((0, 24)),
            discover_grid: Mutex::new(None),
            discover_selected: AtomicUsize::new(0),
            details_season: AtomicUsize::new(details::NO_SEASON),
            details_picked: Mutex::new(None),
            details_video_query: Mutex::new(String::new()),
            details_moved: Mutex::new(None),
            player_season: AtomicUsize::new(player::FOLLOW_PLAYING_SEASON),
            playback_error: Mutex::new(String::new()),
            mods_page: Mutex::new(mods::Page::default().id().to_owned()),
            mods_plugin: Mutex::new(String::new()),
            mods_edits: Mutex::new(mods::settings::Edits::default()),
            log_selection: Mutex::new(mods::log_selection::LogSelection::default()),
            mods_clearing: Mutex::new(false),
            mods_export: Mutex::new(None),
            mods_armed: Mutex::new(None),
            board_catalogs: Mutex::new(None),
            calendar_library: Mutex::new(None),
            auth: auth::Generation::default(),
        });

        let weak = Arc::downgrade(&store);
        logs.watch_records(move || {
            let Some(store) = weak.upgrade() else { return };
            if store.mods_page.lock().unwrap().as_str() != mods::Page::Logs.id() {
                return;
            }
            store.refresh_mods_logs();
        });

        Ok((store, events))
    }

    pub(crate) fn set_observer(&self, observer: Observer) {
        *self.observer.lock().unwrap() = Some(observer);
    }

    pub(crate) fn notify(&self, notice: AppNotice) {
        let cb = self.observer.lock().unwrap().clone();
        if let Some(cb) = cb {
            cb(notice);
        }
    }

    pub(crate) fn mark_dirty(&self, mask: ScreenMask) {
        if mask.is_empty() {
            return;
        }
        let prev = self.dirty.fetch_or(mask.bits(), Ordering::SeqCst);
        if prev == 0 {
            self.notify(AppNotice::Dirty);
        }
    }

    pub(crate) fn take_dirty(&self) -> ScreenMask {
        ScreenMask::from_bits(self.dirty.swap(0, Ordering::SeqCst))
    }

    pub(crate) fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }

    pub(crate) fn drawn_range(&self) -> (usize, usize) {
        match *self.discover_grid.lock().unwrap() {
            Some(grid) => {
                discover::settled_range(grid.scroll, grid.height, grid.pitch, grid.columns)
            }
            None => *self.discover_visible.lock().unwrap(),
        }
    }

    pub(crate) fn on_new_state(self: &Arc<Self>, fields: &[AppModelField]) {
        self.sync_board_catalogs();
        if self
            .runtime
            .model()
            .is_ok_and(|m| m.ctx.profile.auth.is_some())
        {
            self.close_sign_in();
        }
        let affected = ScreenMask::from_fields(fields);
        let active = *self.active_screen.lock().unwrap();
        // Eager screens: active tab + Player (detects stream open/close) + Settings (logged_in &
        // player config) + Board (home) + Details when MetaDetails itself changed.
        let mut eager = active
            .mask()
            .union(ScreenMask::PLAYER)
            .union(ScreenMask::SETTINGS)
            .union(ScreenMask::BOARD);
        if affected.contains(ScreenMask::DETAILS) {
            eager = eager.union(ScreenMask::DETAILS);
        }
        let now = affected.intersection(eager);
        let later = affected.difference(eager);
        if !later.is_empty() {
            let mut stale = self.stale.lock().unwrap();
            *stale = stale.union(later);
        }
        let rebuilt = self.materialize(now);
        self.mark_dirty(rebuilt);
        if rebuilt.contains(ScreenMask::PLAYER) {
            self.notify(AppNotice::PlayerStreamChanged);
        }
        self.request_active_images();
    }

    pub(crate) fn close_sign_in(&self) {
        self.auth.cancel();
        self.notify(AppNotice::AuthSucceeded);
    }

    pub(crate) fn sync_images(&self) -> bool {
        let snap = self.snapshot();
        let screen = *self.active_screen.lock().unwrap();
        let board_view = *self.board_visible.lock().unwrap();
        let range = self.drawn_range();
        let selected = self.discover_selected.load(Ordering::Relaxed);
        let evicted = self
            .images
            .set_wanted(shown_images(&snap, screen, board_view, range, selected));
        if evicted {
            self.mark_dirty(ScreenMask::IMAGE_SCREENS);
        }
        evicted
    }

    pub(crate) fn request_active_images(self: &Arc<Self>) {
        self.sync_images();
        let snap = self.snapshot();
        let screen = *self.active_screen.lock().unwrap();
        let board_view = *self.board_visible.lock().unwrap();
        let range = self.drawn_range();
        let selected = self.discover_selected.load(Ordering::Relaxed);
        for url in snapshot_images(&snap, screen, board_view, range, selected) {
            self.request_image(url);
        }
    }

    pub(crate) fn request_image(self: &Arc<Self>, url: String) {
        let weak = Arc::downgrade(self);
        self.images.request(url, move |evicted| {
            let Some(store) = weak.upgrade() else { return };
            let mut dirty = if evicted {
                ScreenMask::IMAGE_SCREENS
            } else {
                store.active_screen.lock().unwrap().mask()
            };
            if store.snapshot.lock().unwrap().player.open {
                dirty = dirty.union(ScreenMask::PLAYER);
            }
            store.mark_dirty(dirty);
        });
    }

    fn sync_board_catalogs(&self) {
        let count = {
            let Ok(model) = self.runtime.model() else {
                return;
            };
            let ids: Vec<String> = model
                .ctx
                .profile
                .addons
                .iter()
                .map(|a| a.manifest.id.clone())
                .collect();
            let count = model.board.catalogs.len();
            let sig = (ids, count);
            let mut known = self.board_catalogs.lock().unwrap();
            if known.as_ref() == Some(&sig) {
                return;
            }
            *known = Some(sig);
            count
        };
        if count > 0 {
            self.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::CatalogsWithExtra(ActionCatalogsWithExtra::LoadRange(0..count)),
            });
        }
    }
}
