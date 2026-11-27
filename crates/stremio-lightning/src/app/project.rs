//! Lazy & incremental screen projection for `AppStore` (`materialize`, `build_mods_data`,
//! `refresh_mods_logs`). Pure plain Rust; zero Slint dependencies.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::store::AppStore;
use super::{BoardData, ScreenMask};
use crate::plumbing::theme::store as theme_store;
use crate::screens::{addons, board, calendar, details, discover, library, mods, player, settings};

impl AppStore {
    /// Rebuilds the requested `mask` of screens from `stremio-core`'s model, clearing their
    /// `stale` bits and updating `snapshot` without touching unaffected screens.
    pub(crate) fn materialize(&self, mask: ScreenMask) -> ScreenMask {
        if mask.is_empty() {
            return ScreenMask::EMPTY;
        }
        let Ok(model) = self.runtime.model() else {
            return ScreenMask::EMPTY;
        };
        let library_blank = model.ctx.library.items.is_empty() && model.ctx.profile.auth.is_some();

        let board = mask.contains(ScreenMask::BOARD).then(|| {
            let extras = self.local_extras.lock().unwrap().clone();
            let (status, rows, spotlight) = board::data_with_extras(&model, &extras);
            Arc::new(BoardData {
                status,
                rows,
                spotlight,
            })
        });
        let discover = mask.contains(ScreenMask::DISCOVER).then(|| {
            let extras = self.local_extras.lock().unwrap().clone();
            Arc::new(discover::data_with_extras(&model, &extras))
        });
        let library = mask
            .contains(ScreenMask::LIBRARY)
            .then(|| Arc::new(library::data(&model)));
        let addons = mask
            .contains(ScreenMask::ADDONS)
            .then(|| Arc::new(addons::data(&model)));
        let calendar = mask
            .contains(ScreenMask::CALENDAR)
            .then(|| Arc::new(calendar::data(&model)));
        let details_data = mask.contains(ScreenMask::DETAILS).then(|| {
            let extras = self.local_extras.lock().unwrap().clone();
            Arc::new(details::data_with_extras(
                &model,
                &extras,
                details::chosen_season(self.details_season.load(Ordering::Relaxed)),
                self.details_picked.lock().unwrap().as_deref(),
                *self.details_moved.lock().unwrap(),
                &self.details_video_query.lock().unwrap(),
            ))
        });
        let settings_data = mask.contains(ScreenMask::SETTINGS).then(|| {
            let extras = self.local_extras.lock().unwrap().clone();
            Arc::new(settings::data(&model, &extras))
        });
        let player_data = mask.contains(ScreenMask::PLAYER).then(|| {
            let mut p = player::data(
                &model,
                player::chosen_season_index(self.player_season.load(Ordering::Relaxed)),
            );
            if p.error.is_empty() {
                p.error = self.playback_error.lock().unwrap().clone();
            }
            p
        });
        drop(model);

        let mods_data = mask
            .contains(ScreenMask::MODS)
            .then(|| self.build_mods_data());

        {
            let mut stale = self.stale.lock().unwrap();
            *stale = stale.difference(mask);
        }

        let mut slot = self.snapshot.lock().unwrap();
        if let Some(b) = board {
            slot.board = b;
        }
        if let Some(d) = discover {
            slot.discover = d;
        }
        if let Some(l) = library {
            slot.library = l;
        }
        if let Some(a) = addons {
            slot.addons = a;
        }
        if let Some(c) = calendar {
            slot.calendar = c;
        }
        if let Some(det) = details_data {
            slot.details = det;
        }
        if let Some(set) = settings_data {
            slot.settings = set;
        }
        if let Some(mut p) = player_data {
            if library_blank && slot.player.episodes.iter().any(|e| e.watched) {
                p.episodes = slot.player.episodes.clone();
            }
            slot.player = Arc::new(p);
        }
        if let Some(m) = mods_data {
            let notice = slot.mods.notice.clone();
            slot.mods = Arc::new(m.with_notice(notice));
        }
        mask
    }

    pub(crate) fn materialize_if_stale(&self, mask: ScreenMask) -> bool {
        let needed = self.stale.lock().unwrap().intersection(mask);
        if needed.is_empty() {
            return false;
        }
        let built = self.materialize(needed);
        self.mark_dirty(built);
        !built.is_empty()
    }

    pub(crate) fn build_mods_data(&self) -> mods::Data {
        let extras = self.local_extras.lock().unwrap().clone();
        let page = mods::Page::from_id(&self.mods_page.lock().unwrap());
        let pick = self.mods_plugin.lock().unwrap().clone();
        let edits = self.mods_edits.lock().unwrap().clone();
        let armed = self
            .mods_armed
            .lock()
            .unwrap()
            .as_ref()
            .map(|a| a.id.clone());
        let entries = self.logs.entries();
        let sel = self.log_selection.lock().unwrap().clone();
        let clearing = *self.mods_clearing.lock().unwrap();
        let export = self.mods_export.lock().unwrap().clone();
        let cached = self.installed_themes.lock().unwrap();
        let screen = mods::data(
            &theme_store::themes_dir(&self.data_dir),
            &extras,
            cached.as_deref().map_err(String::as_str),
        )
        .with_page(page);
        drop(cached);
        let pick = if pick.is_empty() {
            screen.plugin.clone()
        } else {
            pick
        };
        screen
            .with_edits(pick, edits)
            .with_armed(armed)
            .with_logs(&entries, &sel, clearing)
            .with_export(export.as_ref())
    }

    pub(crate) fn refresh_mods_logs(&self) {
        let entries = self.logs.entries();
        let sel = self.log_selection.lock().unwrap().clone();
        let clearing = *self.mods_clearing.lock().unwrap();
        {
            let mut slot = self.snapshot.lock().unwrap();
            let next = (*slot.mods).clone().with_logs(&entries, &sel, clearing);
            slot.mods = Arc::new(next);
        }
        self.mark_dirty(ScreenMask::MODS_LOGS);
    }
}
