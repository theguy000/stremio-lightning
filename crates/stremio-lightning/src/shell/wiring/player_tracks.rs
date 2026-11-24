//! Player audio and subtitle track synchronization, language browsing, and style steppers.

use slint::{ModelRc, VecModel};

use super::Wiring;
use crate::screens::player::{
    self, auto_subtitle, browsed_language, Subtitle, SubtitleChoice, SubtitleTrackFacts,
};
use crate::shell;
use crate::MainWindow;

fn track_info_of(track: &crate::plumbing::player::Track) -> player::TrackInfo {
    player::TrackInfo {
        id: track.id.clone(),
        title: track.title.clone(),
        language: track.language.clone(),
        selected: track.selected,
        external: track.external,
    }
}

/// The track lists the window was last given, so the same list is not handed over again and again.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct ShownTracks {
    audio: Vec<crate::plumbing::player::Track>,
    subtitles: Vec<crate::plumbing::player::Track>,
    candidates: Vec<Subtitle>,
    languages: Vec<(String, String)>,
    active_language: String,
    browsed_language: String,
}

impl Wiring {
    pub(super) fn sync_tracks_on_tick(
        &self,
        window: &MainWindow,
        status: &crate::plumbing::player::Status,
    ) {
        let candidates = self.app.player().view().subtitles.clone();
        let active_language = status
            .subtitles
            .iter()
            .find(|track| track.selected)
            .map(|track| track.language.clone())
            .unwrap_or_default();
        let track_infos: Vec<player::TrackInfo> =
            status.subtitles.iter().map(track_info_of).collect();
        let shown = ShownTracks {
            audio: status.audio.clone(),
            subtitles: status.subtitles.clone(),
            candidates: candidates.clone(),
            languages: player::subtitle_languages(
                &track_infos,
                &candidates,
                &[
                    self.subtitles_language().as_str(),
                    self.interface_language().as_str(),
                ],
            ),
            active_language: active_language.clone(),
            browsed_language: browsed_language(
                self.browsed_language.lock().unwrap().as_str(),
                &active_language,
            ),
        };
        let changed = {
            let mut last = self.shown_tracks.lock().unwrap();
            if *last == shown {
                None
            } else {
                *last = shown.clone();
                Some(shown)
            }
        };
        if let Some(shown) = changed {
            self.emit_tracks(&shown);
        }
        window.set_player_subtitles_enabled(status.subtitles.iter().any(|track| track.selected));
        window.set_player_subtitles_size((status.subtitles_scale * 100.0) as f32);
        window.set_player_subtitles_offset((status.subtitles_delay * 1000.0) as f32);
        window.set_player_subtitles_position(status.subtitles_position as f32);

        if status.loaded && !status.subtitles.is_empty() {
            let applied = {
                let mut applied = self.subtitle_auto.lock().unwrap();
                if *applied {
                    true
                } else {
                    *applied = true;
                    false
                }
            };
            if !applied {
                let facts: Vec<SubtitleTrackFacts> = status
                    .subtitles
                    .iter()
                    .map(|track| SubtitleTrackFacts {
                        id: track.id.clone(),
                        language: track.language.clone(),
                        external: track.external,
                    })
                    .collect();
                let choice = self.subtitle_choice.lock().unwrap().clone();
                let preferred = self.subtitles_language();
                let wanted = auto_subtitle(&facts, &choice, &preferred);
                let _ = self.video.select_subtitles(wanted.as_deref());
            }
        }
    }

    pub(super) fn player_select_audio(&self, id: String) {
        let _ = self.video.select_audio(&id);
    }

    fn emit_tracks(&self, shown: &ShownTracks) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_player_audio_tracks(shell::view::tracks(&shown.audio));
        window.set_player_subtitle_tracks(shell::view::tracks(&shown.subtitles));
        window.set_player_subtitle_languages(ModelRc::new(VecModel::from(
            shown
                .languages
                .iter()
                .map(|(code, label)| crate::LanguageOption {
                    code: code.as_str().into(),
                    label: label.as_str().into(),
                    selected: player::same_language(code, &shown.active_language),
                })
                .collect::<Vec<_>>(),
        )));
        window.set_player_subtitle_language(shown.active_language.as_str().into());
        window.set_player_subtitle_variants(ModelRc::new(VecModel::from(self.variants(shown))));
    }

    fn browse_language(&self, language: String) {
        *self.browsed_language.lock().unwrap() = language;
        let last = self.shown_tracks.lock().unwrap().clone();
        let shown = ShownTracks {
            browsed_language: browsed_language(
                self.browsed_language.lock().unwrap().as_str(),
                &last.active_language,
            ),
            ..last
        };
        *self.shown_tracks.lock().unwrap() = shown.clone();
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_player_subtitle_variants(ModelRc::new(VecModel::from(self.variants(&shown))));
    }

    fn variants(&self, shown: &ShownTracks) -> Vec<crate::PlayerTrack> {
        let track_infos: Vec<player::TrackInfo> =
            shown.subtitles.iter().map(track_info_of).collect();
        player::subtitle_rows(&track_infos, &shown.candidates, &shown.browsed_language)
            .into_iter()
            .map(|row| crate::PlayerTrack {
                id: row.id.into(),
                title: row.title.into(),
                language: row.language.into(),
                selected: row.selected,
                external: row.external,
                origin: row.origin.into(),
            })
            .collect()
    }

    fn subtitles_language(&self) -> String {
        let Ok(model) = self.app.runtime().model() else {
            return String::new();
        };
        model
            .ctx
            .profile
            .settings
            .subtitles_language
            .clone()
            .unwrap_or_default()
    }

    fn interface_language(&self) -> String {
        let Ok(model) = self.app.runtime().model() else {
            return String::new();
        };
        model.ctx.profile.settings.interface_language.clone()
    }

    pub(super) fn player_choose_language(&self, language: String) {
        if language.is_empty() {
            let choice = self.subtitle_choice.lock().unwrap().clone();
            *self.subtitle_choice.lock().unwrap() = choice.off();
            let _ = self.video.select_subtitles(None);
        }
        self.browse_language(language);
    }

    pub(super) fn player_choose_subtitles(&self, id: String) {
        let candidate = self
            .app
            .player()
            .view()
            .subtitles
            .iter()
            .find(|subtitle| subtitle.id == id)
            .cloned();
        if let Some(candidate) = &candidate {
            *self.subtitle_choice.lock().unwrap() =
                SubtitleChoice::picked(&candidate.language, true);
            let _ = self.video.add_subtitles(
                &candidate.url,
                &player::variant_label(candidate),
                &candidate.language,
            );
            return;
        }
        let track = self
            .shown_tracks
            .lock()
            .unwrap()
            .subtitles
            .iter()
            .find(|track| track.id == id)
            .cloned();
        if let Some(track) = &track {
            *self.subtitle_choice.lock().unwrap() =
                SubtitleChoice::picked(&track.language, track.external);
        }
        let _ = self.video.select_subtitles(Some(&id));
    }

    pub(super) fn player_toggle_subtitles(&self) {
        let showing = self
            .video
            .status()
            .subtitles
            .iter()
            .any(|track| track.selected);
        if showing {
            let choice = self.subtitle_choice.lock().unwrap().clone();
            *self.subtitle_choice.lock().unwrap() = choice.off();
            let _ = self.video.select_subtitles(None);
        } else {
            let choice = self.subtitle_choice.lock().unwrap().clone();
            let tracks = self.shown_tracks.lock().unwrap().subtitles.clone();
            let facts: Vec<SubtitleTrackFacts> = tracks
                .iter()
                .map(|track| SubtitleTrackFacts {
                    id: track.id.clone(),
                    language: track.language.clone(),
                    external: track.external,
                })
                .collect();
            let preferred = self.subtitles_language();
            let wanted = auto_subtitle(
                &facts,
                &SubtitleChoice {
                    chosen: true,
                    enabled: true,
                    ..choice
                },
                &preferred,
            );
            let _ = self.video.select_subtitles(wanted.as_deref());
        }
    }

    pub(super) fn player_step_subtitles_size(&self, step: i32) {
        let current = self.video.status().subtitles_scale;
        let next = (current + f64::from(step) * 0.1).clamp(0.2, 3.0);
        let _ = self.video.set_subtitles_scale(next);
    }

    pub(super) fn player_step_subtitles_position(&self, step: i32) {
        let current = self.video.status().subtitles_position;
        let next = (current + f64::from(step)).clamp(0.0, 100.0);
        let _ = self.video.set_subtitles_position(next);
    }

    pub(super) fn player_step_subtitles_offset(&self, step: i32) {
        let current = self.video.status().subtitles_delay;
        let next = (current + f64::from(step) * 0.1).clamp(-60.0, 60.0);
        let _ = self.video.set_subtitles_delay(next);
    }
}
