//! The runtime's events, turned into three things: repaint the window, ask for any new images,
//! and keep the sign-in screen's state honest.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use slint::Weak;
use stremio_core::runtime::msg::Event;
use stremio_core::runtime::RuntimeEvent;
use stremio_lightning_env::AppEnv;

use super::{AppModel, AppRuntime, Snapshot};
use crate::plumbing::images::Images;
use crate::screens::{auth, discover, settings};
use crate::shell;
use crate::MainWindow;

/// Everything an event needs in order to repaint. Cloned once per event, because the stream
/// outlives any one of them.
#[derive(Clone)]
pub(crate) struct Context {
    pub(crate) weak: Weak<MainWindow>,
    pub(crate) runtime: Arc<AppRuntime>,
    pub(crate) images: Images,
    pub(crate) latest: Arc<Mutex<Snapshot>>,
    /// The Discover card whose preview panel is open.
    pub(crate) selected: Arc<AtomicUsize>,
    /// The Season the details screen's episode picker is showing, if it has been touched.
    pub(crate) season: Arc<AtomicUsize>,
    /// The Video id whose Streams the details screen is showing, if one was picked.
    pub(crate) picked: Arc<Mutex<Option<String>>>,
    /// What the details screen's search box holds.
    pub(crate) video_query: Arc<Mutex<String>>,
    /// The Season and Episode the no-Streams message's steppers were moved to.
    pub(crate) moved: Arc<Mutex<Option<(u32, u32)>>>,
    pub(crate) local_extras: Arc<Mutex<settings::LocalExtras>>,
    pub(crate) auth: auth::Generation,
    /// Repaint now, with whatever the cache holds. Used when an image arrives.
    pub(crate) repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Context {
    fn model(&self) -> Option<std::sync::RwLockReadGuard<'_, AppModel>> {
        self.runtime.model().ok()
    }
    fn extras(&self) -> settings::LocalExtras {
        self.local_extras.lock().unwrap().clone()
    }
}

/// Runs until the runtime stops. One event at a time, in order.
pub(crate) async fn pump(
    events: futures::channel::mpsc::Receiver<RuntimeEvent<AppEnv, AppModel>>,
    context: Context,
) {
    events
        .for_each(move |event| {
            let context = context.clone();
            async move { handle(event, &context) }
        })
        .await;
}

fn handle(event: RuntimeEvent<AppEnv, AppModel>, context: &Context) {
    match event {
        RuntimeEvent::CoreEvent(Event::Error { error, source }) => {
            // Any error means the attempt has finished, so always stop the spinner; only
            // authentication errors belong in the sign-in banner.
            let message = matches!(*source, Event::UserAuthenticated { .. })
                .then(|| auth::error_message(&error));
            let weak = context.weak.clone();
            let _ = weak.upgrade_in_event_loop(move |window| {
                auth::clear_progress(&window);
                if let Some(message) = message {
                    window.set_auth_error(message.into());
                }
            });
            return;
        }
        RuntimeEvent::CoreEvent(Event::UserAuthenticated { .. }) => {
            close_sign_in(context);
            return;
        }
        RuntimeEvent::NewState(..) => {}
        _ => return,
    }

    let Some(model) = context.model() else { return };
    // Whatever signed in, no other browser handshake is still wanted.
    if model.ctx.profile.auth.is_some() {
        close_sign_in(context);
    }
    let snapshot = snapshot(&model, context);
    drop(model);

    *context.latest.lock().unwrap() = snapshot.clone();
    request_images(&snapshot, context);
    shell::show(&context.weak, snapshot, &context.images);
}

/// Rebuilds everything the window shows from the model.
fn snapshot(model: &AppModel, context: &Context) -> Snapshot {
    let (status, rows) = crate::screens::board::data(model);
    Snapshot {
        status,
        rows,
        discover: discover::data(model),
        addons: crate::screens::addons::data(model),
        details: crate::screens::details::data(
            model,
            crate::screens::details::chosen_season(context.season.load(Ordering::Relaxed)),
            context.picked.lock().unwrap().as_deref(),
            *context.moved.lock().unwrap(),
            &context.video_query.lock().unwrap(),
        ),
        settings: settings::data(model, &context.extras()),
    }
}

/// Asks for every image the new snapshot needs. The cache ignores keys it has already asked for.
fn request_images(snapshot: &Snapshot, context: &Context) {
    let urls = snapshot
        .rows
        .iter()
        .flat_map(|row| row.cards.iter())
        .filter_map(|card| card.poster.clone())
        .chain(
            snapshot
                .discover
                .cards
                .iter()
                .filter_map(|card| card.poster.clone()),
        )
        .chain(
            snapshot
                .details
                .preview
                .as_ref()
                .and_then(|preview| preview.logo.clone()),
        )
        .chain(
            snapshot
                .details
                .preview
                .as_ref()
                .and_then(|preview| preview.background.clone()),
        )
        .chain(
            snapshot
                .details
                .episodes
                .iter()
                .filter_map(|episode| episode.thumbnail.clone()),
        )
        .chain(
            snapshot
                .addons
                .cards
                .iter()
                .filter_map(|card| card.logo.clone()),
        )
        .chain(discover::preview_images(
            &snapshot.discover,
            context.selected.load(Ordering::Relaxed),
        ));
    for url in urls {
        let repaint = context.repaint.clone();
        context.images.request(url, move || repaint());
    }
}

/// The sign-in screen is done, whatever finished it.
fn close_sign_in(context: &Context) {
    context.auth.cancel();
    let _ = context.weak.upgrade_in_event_loop(|window| {
        auth::clear_progress(&window);
        window.set_auth_error("".into());
        window.set_auth_open(false);
    });
}
