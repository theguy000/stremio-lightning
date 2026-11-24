//! Thin Slint/async shell adapter over `screens::player::skip::SkipSession` (tick, fetch, overlay).

use std::sync::{Arc, Mutex};

use slint::{ModelRc, VecModel};
use stremio_core::models::common::Loadable;

use super::player_skip_panel::apply_fetched_skip;
use super::{Playback, Wiring};
use crate::plumbing::{logs, skip};
use crate::screens::player::skip::{format_rates_listing, kind_label, FetchPlan, SkipSession};
use crate::MainWindow;

impl Wiring {
    pub(super) fn clear_skip(&self, window: Option<&MainWindow>, close_menu: bool) {
        self.skip.lock().unwrap().clear();
        if let Some(window) = window {
            reset_skip_window(window, close_menu);
        }
    }

    pub(super) fn clear_skip_if_needed(&self, window: Option<&MainWindow>) {
        let has_skip = window.is_some_and(|w| w.get_player_menu() == "skip") || {
            let s = self.skip.lock().unwrap();
            s.has_state().0 || !s.seeded.is_empty() || !s.kind.is_empty() || s.auto_times.is_some()
        };
        if has_skip {
            self.clear_skip(window, true);
        }
    }

    pub(super) fn sync_skip_on_tick(
        &self,
        window: &MainWindow,
        loaded: bool,
        time: f64,
        duration: f64,
    ) {
        let skip_wanted = self.app.local_extras().skip_intros;
        let rates_wanted = skip_wanted || *self.skip_submit.lock().unwrap();
        let target = self.skip_target();
        if !rates_wanted {
            let (had_any, had_segments) = self.skip.lock().unwrap().has_state();
            if had_segments {
                self.app.logs().record(
                    logs::Source::App,
                    "Skip",
                    logs::Level::Info,
                    "Skip intros is switched off; the segments are dropped.",
                );
            }
            if had_any {
                self.clear_skip(Some(window), true);
            }
        } else if loaded && duration > 0.0 {
            if let Some((video, meta_id, season, episode)) = target.as_ref() {
                let length = duration.round() as u64;
                let plan = self.skip.lock().unwrap().prepare_fetch(video, length);
                if let FetchPlan::Fetch {
                    generation,
                    reset_window,
                } = plan
                {
                    if reset_window {
                        reset_skip_window(window, false);
                    }
                    self.spawn_skip_fetch(
                        generation,
                        video.clone(),
                        meta_id.clone(),
                        *season,
                        *episode,
                        duration,
                        length,
                        skip_wanted,
                    );
                }
            } else if self.skip.lock().unwrap().has_state().0 {
                self.clear_skip(Some(window), false);
            }
        }

        let same_video = loaded
            && target
                .as_ref()
                .is_some_and(|(v, ..)| self.skip.lock().unwrap().is_for(Some(v.as_str())));
        let diff =
            self.skip
                .lock()
                .unwrap()
                .evaluate_overlay(time, duration, same_video, skip_wanted);
        if let Some(had) = diff.total_changed {
            self.app.logs().record(
                logs::Source::App,
                "Skip",
                logs::Level::Debug,
                format!("The player now holds {had} segment(s) for {:?}", diff.video),
            );
        }
        if diff.active_changed {
            match diff.active {
                Some(s) => self.app.logs().record(
                    logs::Source::App,
                    "Skip",
                    logs::Level::Info,
                    format!("Offering {} at {time:.0}s", kind_label(s.kind)),
                ),
                None => self.app.logs().record(
                    logs::Source::App,
                    "Skip",
                    logs::Level::Debug,
                    format!("The skip button is away at {time:.0}s"),
                ),
            };
        }
        window.set_player_skip_visible(diff.active.is_some());
        if let Some(marks) = diff.marks_changed {
            set_window_skip_marks(window, &marks);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_skip_fetch(
        &self,
        generation: u64,
        for_video: String,
        meta_id: String,
        season: u32,
        episode: u32,
        duration: f64,
        length: u64,
        skip_wanted: bool,
    ) {
        self.app.logs().record(
            logs::Source::App,
            "Skip",
            logs::Level::Info,
            format!(
                "Asking AniSkip for {meta_id} season {season} episode {episode} ({duration:.0}s)"
            ),
        );
        let (playback, skip, logs_h, skip_client, weak) = (
            Arc::clone(&self.playback),
            Arc::clone(&self.skip),
            Arc::clone(self.app.logs()),
            Arc::clone(&self.skip_client),
            self.window.clone(),
        );
        self.handle.spawn(async move {
            let found = skip_client
                .rates(&skip::EpisodeKey {
                    meta_id: &meta_id,
                    season,
                    episode,
                    duration,
                })
                .await;
            let listing = format_rates_listing(&found);
            logs_h.record(
                logs::Source::App,
                "Skip",
                logs::Level::Info,
                match found.len() {
                    0 => format!(
                        "AniSkip has no skip times for {meta_id} season {season} episode {episode}"
                    ),
                    n => format!(
                        "AniSkip answered {n} segment(s) for {meta_id} season {season} episode {episode}: {listing}"
                    ),
                },
            );
            let accepted =
                skip.lock()
                    .unwrap()
                    .accept_rates(generation, &for_video, length, true, found.clone());
            if accepted {
                let playhead = playback.lock().map(|p| p.time).unwrap_or_default();
                let _ = weak.upgrade_in_event_loop(move |window| {
                    sync_skip_overlay(&window, &skip, &playback, generation, duration, skip_wanted);
                    apply_fetched_skip(&window, &skip, &found, playhead);
                });
            }
        });
    }

    pub(super) fn skip_target(&self) -> Option<(String, String, u32, u32)> {
        let loaded = self.player_loaded.lock().unwrap().clone();
        let latest_player = self.app.player().view();
        if loaded.is_empty() || loaded != latest_player.key {
            return None;
        }
        let model = self.app.runtime().model().ok()?;
        let video = model
            .player
            .selected
            .as_ref()?
            .stream_request
            .as_ref()?
            .path
            .id
            .clone();
        if video != latest_player.video_id {
            return None;
        }
        let series = model.player.series_info.as_ref()?;
        let Loadable::Ready(meta) = model.player.meta_item.as_ref()?.content.as_ref()? else {
            return None;
        };
        Some((
            video,
            meta.preview.id.to_string(),
            series.season,
            series.episode,
        ))
    }

    pub(super) fn player_skip(&self) {
        let time = self.playback.lock().unwrap().time;
        let target = self.skip.lock().unwrap().skip_seek_target(time);
        if let Some(target) = target {
            self.player_seek(target);
        }
    }
}

pub(super) fn set_window_skip_marks(window: &MainWindow, marks: &[(f32, f32)]) {
    window.set_player_skip_marks(ModelRc::new(VecModel::from(
        marks
            .iter()
            .map(|(start, end)| crate::SkipMark {
                start: *start,
                end: *end,
            })
            .collect::<Vec<_>>(),
    )));
}

pub(super) fn reset_skip_window(window: &MainWindow, close_menu: bool) {
    if close_menu && window.get_player_menu() == "skip" {
        window.set_player_menu("".into());
    }
    window.set_player_skip_visible(false);
    window.set_player_skip_marks(ModelRc::default());
    window.set_player_skip_rows(ModelRc::default());
    window.set_player_skip_start("".into());
    window.set_player_skip_end("".into());
    window.set_player_skip_notice("".into());
}

pub(super) fn sync_skip_overlay(
    window: &MainWindow,
    skip: &Mutex<SkipSession>,
    playback: &Mutex<Playback>,
    generation: u64,
    duration: f64,
    skip_wanted: bool,
) {
    let time = playback.lock().map(|p| p.time).unwrap_or_default();
    let Some((visible, marks_changed)) =
        skip.lock()
            .unwrap()
            .sync_overlay_for_generation(generation, time, duration, skip_wanted)
    else {
        return;
    };
    window.set_player_skip_visible(visible);
    if let Some(marks) = marks_changed {
        set_window_skip_marks(window, &marks);
    }
}
