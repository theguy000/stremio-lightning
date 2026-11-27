//! Player Screen playback lifecycle, stream opening, seek/volume/scale, and drawer callbacks.

use slint::ComponentHandle;
use stremio_core::types::player::VideoScale;

use super::Wiring;
use crate::plumbing::{clipboard, links, logs};
use crate::screens::player::{self, should_open};
use crate::PlayerUi;

/// Where the picture is, as mpv last reported it.
#[derive(Clone)]
pub(crate) struct Playback {
    pub(crate) time: f64,
    pub(crate) duration: f64,
    pub(crate) sent_time: f64,
    pub(crate) paused: Option<bool>,
    pub(crate) scale: VideoScale,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            time: 0.0,
            duration: 0.0,
            sent_time: 0.0,
            paused: None,
            scale: VideoScale::Contain,
        }
    }
}

impl Wiring {
    pub(crate) fn playback_changed(&self, status: crate::plumbing::player::Status) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let ui = window.global::<PlayerUi>();
        ui.set_loaded(status.loaded);
        ui.set_buffering(status.buffering);
        ui.set_time(status.time.unwrap_or_default() as f32);
        ui.set_duration(status.duration.unwrap_or_default() as f32);
        ui.set_buffered(status.buffered.unwrap_or_default() as f32);
        ui.set_paused(status.paused.unwrap_or(true));
        ui.set_seekable(
            status.duration.is_some_and(|duration| duration > 0.0)
                && status.time.is_some()
                && status.paused.is_some(),
        );
        ui.set_volume(status.volume.unwrap_or_default() as f32);
        ui.set_muted(status.muted.unwrap_or(false));
        ui.set_speed(status.speed.unwrap_or(1.0) as f32);

        self.sync_tracks_on_tick(&window, &status);

        if status.error.is_empty() {
            if !self.app.player().playback_error().is_empty() {
                self.app.player().set_playback_error(String::new());
                ui.set_error("".into());
            }
        } else if self.app.player().playback_error() != status.error {
            self.app.player().set_playback_error(status.error.clone());
            ui.set_error(status.error.clone().into());
            let logs_h = self.app.logs();
            logs_h.record(
                logs::Source::App,
                "Player",
                logs::Level::Error,
                &status.error,
            );
        }

        let (time, duration, sent, was_paused) = {
            let mut playback = self.playback.lock().unwrap();
            let was_paused = playback.paused;
            playback.time = status.time.unwrap_or_default();
            playback.duration = status.duration.unwrap_or_default();
            playback.paused = status.paused;
            (
                playback.time,
                playback.duration,
                playback.sent_time,
                was_paused,
            )
        };
        if status.loaded && (time - sent).abs() >= 1.0 {
            self.playback.lock().unwrap().sent_time = time;
            player::time_changed(self.app.runtime(), time, duration, "mpv".to_owned());
        }

        self.sync_skip_on_tick(&window, status.loaded, time, duration);
        self.update_discord_playback(&status);

        if status.paused.is_some() && status.paused != was_paused {
            player::paused_changed(self.app.runtime(), status.paused.unwrap_or(false));
        }
        if status.ended {
            self.ended();
        }
    }

    fn ended(&self) {
        player::ended(self.app.runtime());
        let binge = self
            .app
            .runtime()
            .model()
            .map(|model| {
                model.ctx.profile.settings.binge_watching && model.player.next_video.is_some()
            })
            .unwrap_or(false);
        if binge && player::next(self.app.runtime()).is_ok() {
            return;
        }
        self.player_back();
    }

    pub(super) fn player_back(&self) {
        *self.browsed_language.lock().unwrap() = String::new();
        self.video.stop();
        self.video.show(false);
        *self.player_loaded.lock().unwrap() = String::new();
        *self.player_attempted.lock().unwrap() = None;
        self.app.player().set_playback_error(String::new());
        let upgraded = self.window.upgrade();
        self.clear_skip(upgraded.as_ref(), true);
        self.clear_discord();
        if let Some(window) = upgraded {
            window.global::<PlayerUi>().set_error("".into());
            window.set_playing(false);
        }
        self.app.player().set_season(player::FOLLOW_PLAYING_SEASON);
        player::unload(self.app.runtime());
    }

    pub(super) fn player_play_pause(&self) {
        let paused = self.video.status().paused.unwrap_or(true);
        let _ = self.video.set_paused(!paused);
    }

    pub(super) fn player_play_next(&self) {
        let _ = player::next(self.app.runtime());
    }

    pub(super) fn player_seek(&self, time: f32) {
        let time = f64::from(time);
        let _ = self.video.seek(time);
        let duration = self.playback.lock().unwrap().duration;
        self.playback.lock().unwrap().sent_time = time;
        player::seek(self.app.runtime(), time, duration, "mpv".to_owned());
    }

    pub(super) fn player_seek_by(&self, offset: f32) {
        let status = self.video.status();
        let Some(time) = status.time else { return };
        let duration = status.duration.unwrap_or_default();
        let target = (time + f64::from(offset)).clamp(0.0, duration.max(0.0));
        self.player_seek(target as f32);
    }

    pub(super) fn player_set_volume(&self, volume: f32) {
        let _ = self.video.set_volume(f64::from(volume).clamp(0.0, 200.0));
    }

    pub(super) fn player_toggle_muted(&self) {
        let muted = self.video.status().muted.unwrap_or(false);
        let _ = self.video.set_muted(!muted);
    }

    pub(super) fn player_set_speed(&self, speed: f32) {
        let _ = self.video.set_speed(f64::from(speed).clamp(0.1, 4.0));
    }

    pub(super) fn player_cycle_scale(&self) {
        let next = match self.playback.lock().unwrap().scale {
            VideoScale::Contain => VideoScale::Cover,
            VideoScale::Cover => VideoScale::Fill,
            VideoScale::Fill => VideoScale::Contain,
        };
        self.playback.lock().unwrap().scale = next;
        let _ = self.video.set_scale(next);
        player::scale_changed(self.app.runtime(), next);
        let label = match next {
            VideoScale::Contain => "contain",
            VideoScale::Cover => "cover",
            VideoScale::Fill => "fill",
        };
        if let Some(window) = self.window.upgrade() {
            window.global::<PlayerUi>().set_scale(label.into());
        }
    }

    pub(super) fn player_choose_option(&self, action: String) {
        let Ok(model) = self.app.runtime().model() else {
            return;
        };
        let target = match action.as_str() {
            "copy-stream" => player::stream_link(&model.player),
            "copy-magnet" => player::magnet_link(&model.player),
            "download" => player::download_link(&model.player),
            "download-subtitles" => self
                .app
                .player()
                .view()
                .subtitles
                .first()
                .map(|subtitle| subtitle.url.clone()),
            _ => None,
        };
        drop(model);
        match action.as_str() {
            "download" | "download-subtitles" => match target {
                Some(url) => links::open(&url),
                None => self.say("Nothing to open"),
            },
            _ => match target {
                Some(url) => {
                    let said = match clipboard::copy(&url) {
                        Ok(()) => "Copied".to_owned(),
                        Err(error) => error,
                    };
                    self.say(&said);
                }
                None => self.say("Nothing to copy"),
            },
        }
    }

    /// Event-driven replacement for the old 200ms `player_sync` polling timer.
    pub(crate) fn sync_player_stream(&self) {
        let data = self.app.player().view();
        if !data.open {
            self.video.show(false);
            *self.player_attempted.lock().unwrap() = None;
            return;
        }
        if !should_open(
            &data.key,
            &self.player_loaded.lock().unwrap(),
            self.player_attempted.lock().unwrap().as_deref(),
        ) {
            return;
        }
        let upgraded = self.window.upgrade();
        self.clear_skip_if_needed(upgraded.as_ref());
        let Some(url) = data.url.clone() else { return };
        match self.video.play(&url, data.resume_at) {
            Ok(()) => {
                *self.player_loaded.lock().unwrap() = data.key.clone();
                *self.subtitle_auto.lock().unwrap() = false;
                *self.browsed_language.lock().unwrap() = String::new();
                self.app.player().set_playback_error(String::new());
                if let Ok(mut playing) = self.playback.lock() {
                    playing.time = data.resume_at;
                    playing.duration = 0.0;
                    playing.sent_time = data.resume_at;
                    playing.paused = None;
                    playing.scale = match data.scale {
                        "cover" => VideoScale::Cover,
                        "fill" => VideoScale::Fill,
                        _ => VideoScale::Contain,
                    };
                    let _ = self.video.set_scale(playing.scale);
                }
                if let Ok(model) = self.app.runtime().model() {
                    let hints = model
                        .player
                        .selected
                        .as_ref()
                        .map(|selected| selected.stream.behavior_hints.clone())
                        .unwrap_or_default();
                    drop(model);
                    player::video_params_changed(
                        self.app.runtime(),
                        hints.video_size,
                        hints.filename.clone(),
                    );
                }
                self.video.show(true);
                let _ = self.window.upgrade_in_event_loop(|window| {
                    let ui = window.global::<PlayerUi>();
                    ui.set_error("".into());
                    ui.set_loaded(false);
                });
            }
            Err(message) => {
                *self.player_attempted.lock().unwrap() = Some(data.key.clone());
                let _ = self.window.upgrade_in_event_loop(move |window| {
                    window
                        .global::<PlayerUi>()
                        .set_error(format!("Can't play this stream: {message}").into())
                });
            }
        }
    }
}
