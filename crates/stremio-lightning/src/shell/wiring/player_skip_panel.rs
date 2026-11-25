//! AniSkip timing panel, submission, and voting callbacks.

use std::sync::Mutex;

use slint::{ComponentHandle, ModelRc, VecModel};

use super::player_skip::sync_skip_overlay;
use super::Wiring;
use crate::plumbing::{logs, skip};
use crate::screens::mods;
use crate::screens::player::skip::{
    error_text, provider_name, validate_submission, SkipPanelData, SkipRowData, SkipSession,
};
use crate::{MainWindow, PlayerUi};

impl Wiring {
    fn current_playhead_for(&self, target_video: Option<&str>) -> f64 {
        if self.skip.lock().unwrap().is_for(target_video) {
            self.playback.lock().unwrap().time
        } else {
            self.video.status().time.unwrap_or_default()
        }
    }

    pub(super) fn open_skip(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let target_video = self.skip_target().map(|(v, ..)| v);
        let playhead = self.current_playhead_for(target_video.as_deref());
        let panel = self
            .skip
            .lock()
            .unwrap()
            .open_panel(target_video.as_deref(), playhead);
        apply_skip_panel(&window, &panel);
        self.ask_skip_rates();
    }

    fn ask_skip_rates(&self) {
        let duration = self.playback.lock().unwrap().duration;
        if duration <= 0.0 {
            return;
        }
        let Some((video, meta_id, season, episode)) = self.skip_target() else {
            return;
        };
        let length = duration.round() as u64;
        let skip_wanted = self.app.local_extras().skip_intros;
        let generation = self.skip.lock().unwrap().generation;
        let (playback, skip, weak, logs_h, client) = (
            self.playback.clone(),
            self.skip.clone(),
            self.window.clone(),
            self.app.logs().clone(),
            self.skip_client.clone(),
        );
        self.handle.spawn(async move {
            let key = skip::EpisodeKey {
                meta_id: &meta_id,
                season,
                episode,
                duration,
            };
            let found = client.rates(&key).await;
            if !skip
                .lock()
                .unwrap()
                .accept_rates(generation, &video, length, false, found.clone())
            {
                return;
            }
            let (playhead, duration_now) = {
                let p = playback.lock().unwrap();
                (p.time, p.duration)
            };
            let msg = format!("The timing panel asked again: {} row(s)", found.len());
            logs_h.record(logs::Source::App, "Skip", logs::Level::Debug, msg);
            let _ = weak.upgrade_in_event_loop(move |window| {
                sync_skip_overlay(
                    &window,
                    &skip,
                    &playback,
                    generation,
                    duration_now,
                    skip_wanted,
                );
                apply_fetched_skip(&window, &skip, &found, playhead);
            });
        });
    }

    pub(super) fn pick_skip_kind(&self, kind: String) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let target_video = self.skip_target().map(|(v, ..)| v);
        let playhead = self.current_playhead_for(target_video.as_deref());
        let panel = self
            .skip
            .lock()
            .unwrap()
            .pick_kind(&kind, target_video.as_deref(), playhead);
        apply_skip_panel(&window, &panel);
    }

    pub(super) fn submit_skip(&self, kind: String, start: String, end: String) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let duration = self.playback.lock().unwrap().duration;
        let ui = window.global::<PlayerUi>();
        let validated = match validate_submission(&kind, &start, &end, duration) {
            Ok(v) => v,
            Err(notice) => {
                ui.set_skip_notice(notice.into());
                return;
            }
        };
        let length_key = duration.round() as u64;
        let Some((video, meta_id, season, episode)) = self.skip_target() else {
            ui.set_skip_notice("AniSkip has no show for this video.".into());
            return;
        };
        let (provider, submitter, skip_wanted) = {
            let extras = self.app.local_extras();
            (
                provider_name(&extras),
                mods::builtin::submitter(&extras),
                extras.skip_intros,
            )
        };
        let Some(submitter) = submitter else {
            ui.set_skip_notice("Set a submitter ID in AniSkip's settings first.".into());
            return;
        };
        ui.set_skip_notice("Sending…".into());
        let (logs_h, playback, skip, weak, client) = (
            self.app.logs().clone(),
            self.playback.clone(),
            self.skip.clone(),
            self.window.clone(),
            self.skip_client.clone(),
        );
        let (generation, cache_dur) = {
            let s = skip.lock().unwrap();
            (s.generation, s.length.map(|l| l as f64))
        };
        self.handle.spawn(async move {
            let ep = skip::EpisodeKey {
                meta_id: &meta_id,
                season,
                episode,
                duration: validated.length,
            };
            let sub = skip::Submission {
                episode: ep,
                kind: validated.kind,
                start: validated.start,
                end: validated.end,
                provider: &provider,
                submitter: &submitter,
            };
            match client.submit(&sub, cache_dur).await {
                Ok(rate) => {
                    let msg = format!(
                        "AniSkip keeps {:.1}-{:.1} as {}",
                        validated.start, validated.end, rate.id
                    );
                    logs_h.record(logs::Source::App, "Skip", logs::Level::Info, msg);
                    let Some((new_gen, rows)) = skip
                        .lock()
                        .unwrap()
                        .insert_submitted(generation, &video, length_key, rate)
                    else {
                        return;
                    };
                    let dur_now = playback.lock().map(|p| p.duration).unwrap_or_default();
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        sync_skip_overlay(&window, &skip, &playback, new_gen, dur_now, skip_wanted);
                        let ui = window.global::<PlayerUi>();
                        ui.set_skip_rows(to_slint_rows(&rows));
                        ui.set_skip_notice("Sent. Thank you.".into());
                    });
                }
                Err(err) => report_skip_error(
                    &logs_h,
                    &weak,
                    &skip,
                    generation,
                    "AniSkip refused a timing",
                    err,
                ),
            }
        });
    }

    pub(super) fn vote_skip(&self, id: String, up: bool) {
        if id.is_empty() {
            return;
        }
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.global::<PlayerUi>().set_skip_notice("Sending…".into());
        let (logs_h, weak, skip, skip_client) = (
            self.app.logs().clone(),
            self.window.clone(),
            self.skip.clone(),
            self.skip_client.clone(),
        );
        let generation = skip.lock().unwrap().generation;
        self.handle.spawn(async move {
            match skip_client.vote(&id, up).await {
                Ok(()) => {
                    logs_h.record(
                        logs::Source::App,
                        "Skip",
                        logs::Level::Info,
                        format!("{} {id}", if up { "Upvoted" } else { "Downvoted" }),
                    );
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        if skip.lock().unwrap().generation == generation {
                            window
                                .global::<PlayerUi>()
                                .set_skip_notice("Thanks for the vote.".into());
                        }
                    });
                }
                Err(err) => {
                    report_skip_error(&logs_h, &weak, &skip, generation, "A vote was refused", err);
                }
            }
        });
    }
}

fn report_skip_error(
    logs_h: &logs::Logs,
    weak: &slint::Weak<MainWindow>,
    skip: &std::sync::Arc<Mutex<SkipSession>>,
    generation: u64,
    prefix: &str,
    err: skip::SkipError,
) {
    let why = error_text(err);
    logs_h.record(
        logs::Source::App,
        "Skip",
        logs::Level::Error,
        format!("{prefix}: {why}"),
    );
    let skip = std::sync::Arc::clone(skip);
    let _ = weak.upgrade_in_event_loop(move |window| {
        if skip.lock().unwrap().generation == generation {
            window.global::<PlayerUi>().set_skip_notice(why.into());
        }
    });
}

fn to_slint_rows(rows: &[SkipRowData]) -> ModelRc<crate::SkipRow> {
    ModelRc::new(VecModel::from(
        rows.iter()
            .map(|r| crate::SkipRow {
                label: r.label.into(),
                text: r.text.clone().into(),
                id: r.id.clone().into(),
            })
            .collect::<Vec<_>>(),
    ))
}

fn apply_skip_panel(window: &MainWindow, panel: &SkipPanelData) {
    let ui = window.global::<PlayerUi>();
    ui.set_skip_kind(panel.kind.clone().into());
    ui.set_skip_rows(to_slint_rows(&panel.rows));
    if let Some((start, end)) = &panel.seed {
        ui.set_skip_start(start.clone().into());
        ui.set_skip_end(end.clone().into());
    }
    if panel.clear_notice {
        ui.set_skip_notice("".into());
    }
}

pub(super) fn apply_fetched_skip(
    window: &MainWindow,
    skip: &Mutex<SkipSession>,
    rates: &[skip::Rate],
    playhead: f64,
) {
    let ui = window.global::<PlayerUi>();
    let start = ui.get_skip_start().to_string();
    let end = ui.get_skip_end().to_string();
    let panel = skip
        .lock()
        .unwrap()
        .apply_fetched_panel(rates, playhead, &start, &end);
    apply_skip_panel(window, &panel);
}
