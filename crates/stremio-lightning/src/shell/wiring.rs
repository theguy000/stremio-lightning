//! The wiring: every window callback in one place, each one naming the action it performs.
//!
//! Nothing here decides anything. A callback works out what was clicked, hands it to a screen's
//! action, and puts the result where the window can see it.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use slint::{ModelRc, Weak};
use stremio_core::models::catalog_with_filters::Selected as CatalogSelected;
use stremio_core::models::installed_addons_with_filters::Selected as InstalledSelected;
use stremio_core::runtime::msg::{Action, ActionCatalogWithFilters, ActionLoad};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::profile::GDPRConsent;

use crate::app::{AppModel, AppRuntime, Snapshot};
use crate::plumbing::clipboard;
use crate::plumbing::images::Images;
use crate::plumbing::links;
use crate::plumbing::player::VideoPlayer;
use crate::plumbing::stream_server::{self, Supervisor};
use crate::screens::{auth, details, discover, settings};
use crate::shell;
use crate::MainWindow;

/// Every handle the callbacks need, in one value, so registering a callback is one line.
#[derive(Clone)]
pub(crate) struct Wiring {
    pub(crate) window: Weak<MainWindow>,
    pub(crate) runtime: Arc<AppRuntime>,
    pub(crate) video: VideoPlayer,
    pub(crate) images: Images,
    pub(crate) latest: Arc<Mutex<Snapshot>>,
    pub(crate) selected: Arc<AtomicUsize>,
    /// The Season the details screen's episode picker is showing, if it has been touched.
    pub(crate) season: Arc<AtomicUsize>,
    /// The Video id whose Streams the details screen is showing, if one was picked.
    pub(crate) picked: Arc<Mutex<Option<String>>>,
    /// What the details screen's search box holds.
    pub(crate) video_query: Arc<Mutex<String>>,
    /// The Season and Episode the no-Streams message's steppers were moved to, while they name a
    /// Video other than the picked one.
    pub(crate) moved: Arc<Mutex<Option<(u32, u32)>>>,
    /// Counts the messages shown at the bottom of the window, so an older one cannot clear a
    /// newer one on its way out.
    pub(crate) said: Arc<AtomicU64>,
    pub(crate) local_extras: Arc<Mutex<settings::LocalExtras>>,
    pub(crate) data_dir: std::path::PathBuf,
    pub(crate) server: Arc<Supervisor>,
    pub(crate) auth: auth::Generation,
    pub(crate) handle: tokio::runtime::Handle,
}

impl Wiring {
    /// Repaints with the snapshot the last event produced.
    fn repaint(&self) {
        let snapshot = self.latest.lock().unwrap().clone();
        shell::show(&self.window, snapshot, &self.images);
    }

    /// Repaints with a snapshot the caller has just changed.
    fn repaint_with(&self, snapshot: Snapshot) {
        *self.latest.lock().unwrap() = snapshot.clone();
        shell::show(&self.window, snapshot, &self.images);
    }

    fn open_item(&self, id: String, kind: String, _title: String) {
        // Drop what the previous item left behind so it can't flash up before the new data.
        self.latest.lock().unwrap().details = details::Data::default();
        // A new title opens on the Season the core suggests, not the one last browsed, and with
        // no Episode picked.
        self.season.store(details::NO_SEASON, Ordering::Relaxed);
        *self.picked.lock().unwrap() = None;
        *self.moved.lock().unwrap() = None;
        *self.video_query.lock().unwrap() = String::new();
        if let Some(window) = self.window.upgrade() {
            window.set_details_has_preview(false);
            window.set_play_message("".into());
            window.set_streams(ModelRc::default());
            window.set_streams_loading(0);
            window.set_details_seasons(ModelRc::default());
            window.set_details_episodes(ModelRc::default());
            window.set_details_show_picker(false);
        }
        details::open(&self.runtime, &id, &kind);
    }

    /// Recomputes the details screen from the model and everything the user has touched there:
    /// the Season being browsed, the Video picked, the steppers' position, the search box.
    fn refresh_details(&self, model: &AppModel) {
        let picked = self.picked.lock().unwrap().clone();
        let moved = *self.moved.lock().unwrap();
        let chosen = details::chosen_season(self.season.load(Ordering::Relaxed));
        let query = self.video_query.lock().unwrap().clone();
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.details = details::data(model, chosen, picked.as_deref(), moved, &query);
        self.repaint_with(snapshot);
    }

    /// Browsing the episode picker: nothing is loaded, so this only remembers the Season and
    /// repaints the list.
    fn select_season(&self, index: i32) {
        let Some(season) = usize::try_from(index).ok().and_then(|index| {
            self.latest
                .lock()
                .unwrap()
                .details
                .seasons
                .get(index)
                .map(|season| season.number)
        }) else {
            return;
        };
        self.season.store(season as usize, Ordering::Relaxed);
        let Ok(model) = self.runtime.model() else {
            return;
        };
        self.refresh_details(&model);
    }

    /// The search box changed: refilter the rows, nothing to load.
    fn search_videos(&self, query: String) {
        *self.video_query.lock().unwrap() = query;
        let Ok(model) = self.runtime.model() else {
            return;
        };
        self.refresh_details(&model);
    }

    /// Picking an Episode: remember it, then ask the core for that Video's Streams.
    fn pick_video(&self, video_id: String) {
        *self.picked.lock().unwrap() = Some(video_id.clone());
        // The steppers start over from the Video that is now on screen.
        *self.moved.lock().unwrap() = None;
        details::pick(&self.runtime, &video_id);
    }

    /// One press of a stepper in the no-Streams message. It only moves the pair on screen;
    /// "Show Streams" is what asks the addons.
    fn step(&self, seasons: i32, episodes: i32) {
        let Ok(model) = self.runtime.model() else {
            return;
        };
        let Some(item) = details::meta_item(&model.meta_details) else {
            return;
        };
        let Some(picked) = self.picked.lock().unwrap().clone() else {
            return;
        };
        let moved = *self.moved.lock().unwrap();
        let Some(at) = details::shown_pair(&item.videos, &picked, moved) else {
            return;
        };
        *self.moved.lock().unwrap() = Some(details::step(&item.videos, at, seasons, episodes));
        self.refresh_details(&model);
    }

    /// Web: "Show Streams" — the Video the steppers name, loaded the way a row is.
    fn show_streams(&self) {
        let Ok(model) = self.runtime.model() else {
            return;
        };
        let Some(item) = details::meta_item(&model.meta_details) else {
            return;
        };
        let Some(picked) = self.picked.lock().unwrap().clone() else {
            return;
        };
        let moved = *self.moved.lock().unwrap();
        let Some(at) = details::shown_pair(&item.videos, &picked, moved) else {
            return;
        };
        let Some(video) = details::stepped_video(&item.videos, at).map(str::to_owned) else {
            return;
        };
        drop(model);
        self.pick_video(video);
    }

    /// Next Episode: whatever the core says follows the pick, selected the same way a row is.
    fn next_episode(&self) {
        let next = self.latest.lock().unwrap().details.next_episode.clone();
        if let Some(next) = next {
            self.pick_video(next);
        }
    }

    /// A rating button: the like or the love, whichever was pressed, toggled against what the
    /// core last said the title carries.
    fn rate(&self, like: bool) {
        let current = self.latest.lock().unwrap().details.rating.clone();
        details::rate(&self.runtime, details::next_rating(current.as_ref(), like));
    }

    /// The share button: works out the link web would offer and opens the dialog that offers it.
    fn share(&self) {
        let Ok(model) = self.runtime.model() else {
            return;
        };
        let Some(selected) = model.meta_details.selected.as_ref() else {
            return;
        };
        let Some(item) = details::meta_item(&model.meta_details) else {
            return;
        };
        let link = details::share_link(
            &item.preview.name,
            &selected.meta_path.r#type,
            &selected.meta_path.id,
        );
        drop(model);
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_share_link(link.into());
        window.set_share_open(true);
    }

    /// The dialog's way out, by its close button or a click on the cover.
    fn close_share(&self) {
        if let Some(window) = self.window.upgrade() {
            window.set_share_open(false);
        }
    }

    /// The dialog's Copy button. The dialog stays open, as it does on the site, and the window
    /// says what happened at the bottom.
    fn copy_share(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let link = window.get_share_link().to_string();
        let said = match clipboard::copy(&link) {
            Ok(()) => "Link copied".to_owned(),
            Err(error) => error,
        };
        self.say(&said);
    }

    /// One of the dialog's networks: hand the link to the site that shares it.
    fn open_share_target(&self, network: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let link = window.get_share_link().to_string();
        let target = match network {
            "facebook" => format!("https://www.facebook.com/sharer/sharer.php?u={link}"),
            "x" => format!("https://twitter.com/intent/tweet?text={link}"),
            "reddit" => format!("https://www.reddit.com/submit?url={link}"),
            _ => return,
        };
        crate::plumbing::links::open(&target);
    }

    /// Puts one line at the bottom of the window for a moment. The count makes the last message
    /// win: a copy made while an earlier one is still fading does not get cleared early.
    fn say(&self, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_toast(text.into());
        let generation = self.said.fetch_add(1, Ordering::Relaxed) + 1;
        let said = self.said.clone();
        let window = self.window.clone();
        self.handle.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(2200)).await;
            if said.load(Ordering::Relaxed) == generation {
                let _ = window.upgrade_in_event_loop(|window| window.set_toast("".into()));
            }
        });
    }

    /// Back to the Episode Picker: the pick is gone, and the core drops the Streams with it.
    fn back_to_episodes(&self) {
        *self.picked.lock().unwrap() = None;
        *self.moved.lock().unwrap() = None;
        details::back_to_episodes(&self.runtime);
    }

    /// The Streams list's cut/wrap toggle. The choice is this shell's own, so it goes to
    /// `settings.json` beside the profile rather than through the core, and the screen
    /// only has to redraw: no request is sent for it.
    fn toggle_stream_wrap(&self) {
        let wrap = {
            let mut extras = self.local_extras.lock().unwrap();
            extras.wrap_streams = !extras.wrap_streams;
            extras.save(&self.data_dir);
            extras.wrap_streams
        };
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.settings.wrap_streams = wrap;
        self.repaint_with(snapshot);
    }

    fn play_stream(&self, index: i32) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        let played =
            details::stream_address(&self.runtime, index).and_then(|url| self.video.play(&url));
        match played {
            Ok(()) => {
                window.set_play_message("".into());
                window.set_playing(true);
            }
            Err(error) => {
                window.set_play_message(format!("Can't play this stream: {error}").into());
            }
        }
    }

    fn stop_playback(&self) {
        self.video.stop();
        if let Some(window) = self.window.upgrade() {
            window.set_playing(false);
        }
    }

    fn select_input_option(&self, input: i32, option: i32) {
        let request = {
            let snapshot = self.latest.lock().unwrap();
            usize::try_from(input)
                .ok()
                .and_then(|input| snapshot.discover.inputs.get(input))
                .and_then(|input| {
                    usize::try_from(option)
                        .ok()
                        .and_then(|option| input.requests.get(option))
                })
                .cloned()
        };
        if let Some(request) = request {
            self.selected.store(0, Ordering::Relaxed);
            self.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Load(ActionLoad::CatalogWithFilters(Some(CatalogSelected {
                    request,
                }))),
            });
        }
    }

    fn select_preview(&self, index: i32) {
        let index = usize::try_from(index).unwrap_or(0);
        self.selected.store(index, Ordering::Relaxed);
        let snapshot = self.latest.lock().unwrap().clone();
        // Only the pieces needed to repaint cross into the fetch task; `Wiring` itself holds a
        // non-`Send` player.
        let (window, latest, images) = (
            self.window.clone(),
            self.latest.clone(),
            self.images.clone(),
        );
        let _guard = self.handle.enter();
        for url in discover::preview_images(&snapshot.discover, index) {
            let (window, latest, images) = (window.clone(), latest.clone(), images.clone());
            self.images.request(url, move || {
                shell::show(&window, latest.lock().unwrap().clone(), &images)
            });
        }
    }

    fn select_addon_input_option(&self, input: i32, option: i32) {
        // Input 0 is the catalog menu, which has only "Installed".
        if input != 1 {
            return;
        }
        let request = usize::try_from(option).ok().and_then(|option| {
            self.latest
                .lock()
                .unwrap()
                .addons
                .type_requests
                .get(option)
                .cloned()
        });
        if let Some(request) = request {
            self.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Load(ActionLoad::InstalledAddonsWithFilters(InstalledSelected {
                    request,
                })),
            });
        }
    }

    /// A setting changed: write it, tell the core, and let the player and the screen catch up.
    fn setting_changed(&self, updated: Option<stremio_core::types::profile::Settings>) {
        if let Some(updated) = updated {
            settings::dispatch_settings(&self.runtime, updated);
        }
        let Ok(model) = self.runtime.model() else {
            return;
        };
        let data = settings::data(&model, &self.local_extras.lock().unwrap());
        self.video.apply_settings(&data.player_config);
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.settings = data;
        self.repaint_with(snapshot);
    }

    fn select_setting_option(&self, key: String, option: i32) {
        let Ok(index) = usize::try_from(option) else {
            return;
        };
        let updated = self.with_current_settings(|current, extras| {
            settings::apply_select_option(current, extras, &key, index)
        });
        self.setting_changed(updated);
    }

    fn toggle_setting_option(&self, key: String, enabled: bool) {
        let updated = self.with_current_settings(|current, extras| {
            settings::apply_toggle_option(current, extras, &key, enabled)
        });
        self.setting_changed(updated);
    }

    /// Runs a settings change against the profile's current settings and this shell's extras,
    /// saving the extras whether or not the profile understood the key.
    fn with_current_settings(
        &self,
        change: impl FnOnce(
            &stremio_core::types::profile::Settings,
            &mut settings::LocalExtras,
        ) -> Option<stremio_core::types::profile::Settings>,
    ) -> Option<stremio_core::types::profile::Settings> {
        let current = self.runtime.model().ok()?.ctx.profile.settings.clone();
        let mut extras = self.local_extras.lock().unwrap();
        let updated = change(&current, &mut extras);
        extras.save(&self.data_dir);
        updated
    }

    fn reload_streaming_server(&self) {
        let _ = self.window.upgrade_in_event_loop(|window| {
            window.set_server_status("Stream server: restarting...".into())
        });
        let _ = self.server.stop();
        let started = self.server.start();
        self.report_server_state(started);
        settings::reload_server(&self.runtime);
    }

    /// Waits for the server to answer and shows which of the possible things happened.
    pub(crate) fn report_server_state(&self, started: Result<(), String>) {
        // Only the window handle and the server cross into the task: `VideoPlayer` holds `Rc`s
        // and is not `Send`.
        let (window, server) = (self.window.clone(), self.server.clone());
        self.handle.spawn(async move {
            let state = stream_server::wait_until_answering(&server, started).await;
            let text = settings::server_status(&state);
            let _ =
                window.upgrade_in_event_loop(move |window| window.set_server_status(text.into()));
        });
    }

    fn start_browser_login(&self, provider: auth::Provider) {
        auth::start_browser_login(
            provider,
            &self.window,
            &self.runtime,
            &self.auth,
            &self.handle,
        );
    }

    /// Leaving the sign-in screen: drop the attempt in flight, and drop its spinner with it, or
    /// the overlay would reopen with its buttons still waiting for a sign-in that is gone.
    fn dismiss_sign_in(&self) {
        self.auth.cancel();
        let _ = self.window.upgrade_in_event_loop(|window| {
            auth::clear_progress(&window);
            window.set_auth_open(false);
            window.set_auth_error("".into());
        });
    }

    fn forgot_password(&self, email: String) {
        let email = email.trim();
        if email.is_empty() {
            links::open("reset-password");
        } else {
            let encoded =
                url::form_urlencoded::byte_serialize(email.as_bytes()).collect::<String>();
            links::open(&format!(
                "https://www.strem.io/reset-password/?email={encoded}"
            ));
        }
    }
}

/// Registers every callback the window has. Each one is the same shape: work out what was
/// clicked, do it, and let the window catch up.
pub(crate) fn wire(window: &MainWindow, wiring: Wiring) {
    {
        let wiring = wiring.clone();
        window.on_open_item(move |id, kind, title| {
            wiring.open_item(id.to_string(), kind.to_string(), title.to_string())
        });
    }
    {
        let wiring = wiring.clone();
        window.on_play_stream(move |index| wiring.play_stream(index));
    }
    {
        let wiring = wiring.clone();
        window.on_select_season(move |index| wiring.select_season(index));
    }
    {
        let wiring = wiring.clone();
        window.on_pick_video(move |video_id| wiring.pick_video(video_id.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_search_videos(move |query| wiring.search_videos(query.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_back_to_episodes(move || wiring.back_to_episodes());
    }
    {
        let wiring = wiring.clone();
        window.on_toggle_stream_wrap(move || wiring.toggle_stream_wrap());
    }
    {
        let wiring = wiring.clone();
        window.on_step_season(move |step| wiring.step(step, 0));
    }
    {
        let wiring = wiring.clone();
        window.on_step_episode(move |step| wiring.step(0, step));
    }
    {
        let wiring = wiring.clone();
        window.on_show_streams(move || wiring.show_streams());
    }
    {
        let wiring = wiring.clone();
        window.on_share(move || wiring.share());
    }
    {
        let wiring = wiring.clone();
        window.on_rate_like(move || wiring.rate(true));
    }
    {
        let wiring = wiring.clone();
        window.on_rate_love(move || wiring.rate(false));
    }
    {
        let wiring = wiring.clone();
        window.on_share_close(move || wiring.close_share());
    }
    {
        let wiring = wiring.clone();
        window.on_share_copy(move || wiring.copy_share());
    }
    {
        let wiring = wiring.clone();
        window.on_share_to(move |network| wiring.open_share_target(network.as_str()));
    }
    {
        let wiring = wiring.clone();
        window.on_next_episode(move || wiring.next_episode());
    }
    {
        let wiring = wiring.clone();
        window.on_stop_playback(move || wiring.stop_playback());
    }
    {
        let wiring = wiring.clone();
        window.on_load_next_page(move || {
            wiring.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::CatalogWithFilters(ActionCatalogWithFilters::LoadNextPage),
            });
        });
    }
    {
        let wiring = wiring.clone();
        window
            .on_select_input_option(move |input, option| wiring.select_input_option(input, option));
    }
    {
        let wiring = wiring.clone();
        window.on_select_preview(move |index| wiring.select_preview(index));
    }
    {
        let wiring = wiring.clone();
        window.on_select_addon_input_option(move |input, option| {
            wiring.select_addon_input_option(input, option)
        });
    }
    {
        let wiring = wiring.clone();
        window.on_search_addons(move |_| wiring.repaint());
    }
    {
        let wiring = wiring.clone();
        window.on_select_setting_option(move |key, option| {
            wiring.select_setting_option(key.to_string(), option)
        });
    }
    {
        let wiring = wiring.clone();
        window.on_toggle_setting_option(move |key, enabled| {
            wiring.toggle_setting_option(key.to_string(), enabled)
        });
    }
    window.on_open_settings_link(move |kind| links::open(&kind));
    {
        let wiring = wiring.clone();
        window.on_logout_user(move || settings::logout(&wiring.runtime));
    }
    {
        let wiring = wiring.clone();
        window.on_reload_streaming_server(move || wiring.reload_streaming_server());
    }
    {
        let wiring = wiring.clone();
        window.on_submit_auth_login(move |email, password| {
            let _ = wiring.window.upgrade_in_event_loop(|window| {
                window.set_auth_loading(true);
                window.set_auth_error("".into());
            });
            auth::login(
                &wiring.runtime,
                email.trim().to_string(),
                password.to_string(),
            );
        });
    }
    {
        let wiring = wiring.clone();
        window.on_submit_auth_register(move |email, password, tos, privacy, marketing| {
            let _ = wiring.window.upgrade_in_event_loop(|window| {
                window.set_auth_loading(true);
                window.set_auth_error("".into());
            });
            auth::register(
                &wiring.runtime,
                email.trim().to_string(),
                password.to_string(),
                GDPRConsent {
                    tos,
                    privacy,
                    marketing,
                    from: Some("stremio-lightning".to_string()),
                },
            );
        });
    }
    {
        let wiring = wiring.clone();
        window.on_auth_guest_login(move || wiring.dismiss_sign_in());
    }
    {
        let wiring = wiring.clone();
        window.on_auth_facebook_login(move || wiring.start_browser_login(auth::Provider::Facebook));
    }
    {
        let wiring = wiring.clone();
        window.on_auth_apple_login(move || wiring.start_browser_login(auth::Provider::Apple));
    }
    {
        let wiring = wiring.clone();
        window.on_auth_forgot_password(move |email| wiring.forgot_password(email.to_string()));
    }
    window.on_auth_dismissed(move || wiring.dismiss_sign_in());
}
