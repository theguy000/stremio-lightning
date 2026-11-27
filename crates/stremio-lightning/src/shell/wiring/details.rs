//! Details Screen, Episode Picker, Streams List, and Share Modal callbacks.

use slint::{ComponentHandle, ModelRc};

use super::Wiring;
use crate::app::ActiveScreen;
use crate::plumbing::{clipboard, external_player, links};
use crate::screens::{details, player};
use crate::{DetailsUi, MainWindow, PreviewData};

impl Wiring {
    pub(super) fn clear_details_ui(window: &MainWindow, expect_streams: bool) {
        let ui = window.global::<DetailsUi>();
        ui.set_has_preview(false);
        ui.set_preview(PreviewData::default());
        ui.set_play_message("".into());
        ui.set_streams(ModelRc::default());
        ui.set_streams_loading(0);
        ui.set_seasons(ModelRc::default());
        ui.set_episodes(ModelRc::default());
        ui.set_season_watched(false);
        ui.set_show_picker(false);
        ui.set_picked_label("".into());
        ui.set_has_picked_episode(false);
        ui.set_resume_label("".into());
        ui.set_expect_streams(expect_streams);
    }

    pub(super) fn open_item(&self, id: String, kind: String, _title: String) {
        let resume_vid = details::library_video_id(self.app.runtime(), &id);
        let expect_streams = resume_vid.is_some() || kind == "movie";
        self.app.details().clear_snapshot();
        self.app.details().reset_inputs();
        self.app.details().set_picked(resume_vid.clone());
        self.app.viewport().set_active_screen(ActiveScreen::Details);
        if let Some(w) = self.window.upgrade() {
            Self::clear_details_ui(&w, expect_streams);
        }
        details::open(self.app.runtime(), &id, &kind, resume_vid.as_deref());
    }

    pub(super) fn refresh_details(&self) {
        self.app.details().refresh();
        self.app.viewport().request_active_images();
    }

    pub(super) fn select_season(&self, index: i32) {
        let Some(season) = usize::try_from(index)
            .ok()
            .and_then(|i| self.app.details().view().seasons.get(i).map(|s| s.number))
        else {
            return;
        };
        self.app.details().set_season(season as usize);
        self.refresh_details();
    }

    pub(super) fn search_videos(&self, query: String) {
        self.app.details().set_video_query(query);
        self.refresh_details();
    }

    pub(super) fn pick_video(&self, video_id: String) {
        self.app.details().set_picked(Some(video_id.clone()));
        self.app.details().set_moved(None);
        if let Some(w) = self.window.upgrade() {
            let ui = w.global::<DetailsUi>();
            ui.set_show_picker(false);
            ui.set_streams(ModelRc::default());
            ui.set_expect_streams(true);
        }
        details::pick(self.app.runtime(), &video_id);
    }

    pub(super) fn watch_episode(&self, video_id: String) {
        match player::open_video(self.app.runtime(), &video_id) {
            Ok(player::VideoOpen::Opened) => {
                if let Some(window) = self.window.upgrade() {
                    window.global::<DetailsUi>().set_play_message("".into());
                }
            }
            Ok(player::VideoOpen::OpenBrowser(url)) => {
                links::open(&url);
                if let Some(window) = self.window.upgrade() {
                    window.global::<DetailsUi>().set_play_message("".into());
                }
            }
            Ok(player::VideoOpen::ExternalPlayer(plan)) => {
                let msg = match external_player::open(&plan.launch) {
                    Ok(()) => {
                        player::record_external_opened(self.app.runtime(), *plan);
                        self.say("Stream opened in external player");
                        String::new()
                    }
                    Err(error) => error,
                };
                if let Some(window) = self.window.upgrade() {
                    window.global::<DetailsUi>().set_play_message(msg.into());
                }
            }
            Ok(player::VideoOpen::PickFromStreams) => self.pick_video(video_id),
            Err(error) => {
                if let Some(window) = self.window.upgrade() {
                    window
                        .global::<DetailsUi>()
                        .set_play_message(format!("Can't play this episode: {error}").into());
                }
            }
        }
    }

    pub(super) fn toggle_episode_watched(&self, video_id: String) {
        details::mark_video_watched(self.app.runtime(), &video_id);
    }

    pub(super) fn toggle_season_watched(&self) {
        let details = self.app.details().view();
        if let Some(season) = details.seasons.get(details.season).map(|s| s.number) {
            details::mark_season_watched(self.app.runtime(), season);
        }
    }

    pub(super) fn step(&self, seasons: i32, episodes: i32) {
        let next = {
            let Ok(model) = self.app.runtime().model() else {
                return;
            };
            let Some(item) = details::meta_item(&model.meta_details) else {
                return;
            };
            let Some(picked) = self.app.details().picked() else {
                return;
            };
            let moved = self.app.details().moved();
            let Some(at) = details::shown_pair(&item.videos, &picked, moved) else {
                return;
            };
            details::step(&item.videos, at, seasons, episodes)
        };
        self.app.details().set_moved(Some(next));
        self.refresh_details();
    }

    pub(super) fn show_streams(&self) {
        let video = {
            let Ok(model) = self.app.runtime().model() else {
                return;
            };
            let Some(item) = details::meta_item(&model.meta_details) else {
                return;
            };
            let Some(picked) = self.app.details().picked() else {
                return;
            };
            let moved = self.app.details().moved();
            let Some(at) = details::shown_pair(&item.videos, &picked, moved) else {
                return;
            };
            details::stepped_video(&item.videos, at).map(str::to_owned)
        };
        if let Some(video) = video {
            self.pick_video(video);
        }
    }

    pub(super) fn back_to_episodes(&self) {
        self.app.details().set_picked(None);
        self.app.details().set_moved(None);
        details::back_to_episodes(self.app.runtime());
    }

    pub(super) fn toggle_stream_wrap(&self) {
        {
            let mut extras = self.app.local_extras();
            extras.wrap_streams = !extras.wrap_streams;
            extras.save(self.app.data_dir());
        }
        self.app.settings().refresh();
    }

    pub(super) fn play_stream(&self, index: i32) {
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        let message = match player::open(self.app.runtime(), index) {
            Ok(player::OpenResult::OpenBrowser(url)) => {
                links::open(&url);
                String::new()
            }
            Ok(player::OpenResult::ExternalPlayer(plan)) => {
                match external_player::open(&plan.launch) {
                    Ok(()) => {
                        player::record_external_opened(self.app.runtime(), *plan);
                        self.say("Stream opened in external player");
                        String::new()
                    }
                    Err(error) => error,
                }
            }
            Ok(player::OpenResult::Loaded) => String::new(),
            Err(error) => format!("Can't play this stream: {error}"),
        };
        if let Some(window) = self.window.upgrade() {
            window
                .global::<DetailsUi>()
                .set_play_message(message.into());
        }
    }

    pub(super) fn copy_stream_link(&self, link: &str) {
        if !link.is_empty() {
            let msg = clipboard::copy(link).map_or_else(|e| e, |_| "Link copied".to_owned());
            self.say(&msg);
        }
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let w = wiring.clone();
    window.on_open_item(move |id, kind, title| {
        w.open_item(id.to_string(), kind.to_string(), title.to_string())
    });
    let ui = window.global::<DetailsUi>();
    let w = wiring.clone();
    ui.on_play_stream(move |index| w.play_stream(index));
    let w = wiring.clone();
    ui.on_copy_stream_link(move |link| w.copy_stream_link(link.as_str()));
    let w = wiring.clone();
    ui.on_select_season(move |index| w.select_season(index));
    let w = wiring.clone();
    ui.on_pick_video(move |video_id| w.pick_video(video_id.to_string()));
    let w = wiring.clone();
    ui.on_search_videos(move |query| w.search_videos(query.to_string()));
    let w = wiring.clone();
    ui.on_watch_episode(move |video_id| w.watch_episode(video_id.to_string()));
    let w = wiring.clone();
    ui.on_toggle_episode_watched(move |id| w.toggle_episode_watched(id.to_string()));
    let w = wiring.clone();
    ui.on_toggle_season_watched(move || w.toggle_season_watched());
    let w = wiring.clone();
    ui.on_back_to_episodes(move || w.back_to_episodes());
    let w = wiring.clone();
    ui.on_toggle_stream_wrap(move || w.toggle_stream_wrap());
    let w = wiring.clone();
    ui.on_step_season(move |step| w.step(step, 0));
    let w = wiring.clone();
    ui.on_step_episode(move |step| w.step(0, step));
    let w = wiring.clone();
    ui.on_show_streams(move || w.show_streams());
}
