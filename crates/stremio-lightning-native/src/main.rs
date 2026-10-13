use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel, Weak};
use std::path::{Path, PathBuf};
use std::time::Duration;
use stremio_core::constants::{META_RESOURCE_NAME, STREAM_RESOURCE_NAME};
use stremio_core::models::catalogs_with_extra::{CatalogsWithExtra, Selected};
use stremio_core::models::common::{Loadable, ResourceError};
use stremio_core::models::ctx::Ctx;
use stremio_core::models::meta_details::{MetaDetails, Selected as DetailsSelected};
use stremio_core::runtime::msg::{Action, ActionCatalogsWithExtra, ActionLoad};
use stremio_core::runtime::{Env, Runtime, RuntimeAction, RuntimeEvent};
use stremio_core::types::addon::ResourcePath;
use stremio_core::types::events::DismissedEventsBucket;
use stremio_core::types::library::LibraryBucket;
use stremio_core::types::notifications::NotificationsBucket;
use stremio_core::types::profile::Profile;
use stremio_core::types::search_history::SearchHistoryBucket;
use stremio_core::types::server_urls::ServerUrlsBucket;
use stremio_core::types::streams::StreamsBucket;
use stremio_core::Model;
use stremio_lightning_core::streaming_server::{
    CommandSpec, RealProcessSpawner, StreamingServerSupervisor,
};
use stremio_lightning_native_env::NativeEnv;

slint::include_modules!();

const CARDS_PER_ROW: usize = 20;
const MAX_STREAMS: usize = 100;
const STREAM_SERVER_ADDRESS: &str = "127.0.0.1:11470";

#[derive(Model, Clone)]
#[model(NativeEnv)]
struct NativeModel {
    ctx: Ctx,
    board: CatalogsWithExtra,
    meta_details: MetaDetails,
}

type NativeRuntime = Runtime<NativeEnv, NativeModel>;
type Posters = Arc<Mutex<HashMap<String, SharedPixelBuffer<Rgba8Pixel>>>>;

#[derive(Clone)]
struct Card {
    id: String,
    kind: String,
    title: String,
    poster: Option<String>,
}

#[derive(Clone)]
struct RowData {
    name: String,
    cards: Vec<Card>,
}

/// `title` is `None` until the addons have answered the meta request, so the window keeps
/// showing the title of the card that was clicked.
#[derive(Clone, Default)]
struct DetailsData {
    title: Option<String>,
    description: String,
    poster: Option<String>,
    status: String,
    streams: Vec<(String, String)>,
}

#[derive(Clone, Default)]
struct Snapshot {
    status: String,
    rows: Vec<RowData>,
    details: DetailsData,
}

fn board_rows(model: &NativeModel) -> (String, Vec<RowData>) {
    let mut rows = Vec::new();
    let (mut loading, mut failed) = (0, 0);
    for catalog in &model.board.catalogs {
        let Some(page) = catalog.first() else {
            continue;
        };
        let name = format!("{} / {}", page.request.path.r#type, page.request.path.id);
        match &page.content {
            Some(Loadable::Ready(items)) => rows.push(RowData {
                name,
                cards: items
                    .iter()
                    .take(CARDS_PER_ROW)
                    .map(|item| Card {
                        id: item.id.clone(),
                        kind: item.r#type.clone(),
                        title: item.name.clone(),
                        poster: item.poster.as_ref().map(|url| url.to_string()),
                    })
                    .collect(),
            }),
            Some(Loadable::Err(_)) => failed += 1,
            _ => loading += 1,
        }
    }
    let status = format!(
        "{} addons, {} catalogs ready, {} loading, {} failed",
        model.ctx.profile.addons.len(),
        rows.len(),
        loading,
        failed
    );
    (status, rows)
}

fn details_data(model: &NativeModel) -> DetailsData {
    let details = &model.meta_details;
    let item = details
        .meta_items
        .iter()
        .find_map(|page| match &page.content {
            Some(Loadable::Ready(item)) => Some(item),
            _ => None,
        });

    let (mut ready, mut loading, mut empty, mut failed) = (0, 0, 0, 0);
    let mut streams = Vec::new();
    for page in &details.streams {
        match &page.content {
            Some(Loadable::Ready(list)) => {
                ready += 1;
                streams.extend(list.iter().map(|stream| {
                    (
                        stream
                            .name
                            .clone()
                            .unwrap_or_else(|| "Stream".to_owned())
                            .replace('\n', " "),
                        stream
                            .description
                            .clone()
                            .unwrap_or_default()
                            .replace('\n', " | "),
                    )
                }));
            }
            Some(Loadable::Err(ResourceError::EmptyContent)) => empty += 1,
            Some(Loadable::Err(_)) => failed += 1,
            _ => loading += 1,
        }
    }
    let found = streams.len();
    streams.truncate(MAX_STREAMS);

    DetailsData {
        title: item.map(|item| item.preview.name.clone()),
        description: item
            .and_then(|item| item.preview.description.clone())
            .unwrap_or_default(),
        poster: item.and_then(|item| item.preview.poster.as_ref().map(|url| url.to_string())),
        status: format!(
            "{found} streams from {ready} sources, {empty} had nothing, {failed} failed, {loading} loading"
        ),
        streams,
    }
}

fn push(weak: &Weak<MainWindow>, snapshot: Snapshot, posters: Posters) {
    let _ = weak.upgrade_in_event_loop(move |window| {
        let cache = posters.lock().unwrap();
        let image_for = |url: &Option<String>| {
            url.as_ref()
                .and_then(|url| cache.get(url))
                .map(|buffer| Image::from_rgba8(buffer.clone()))
        };

        let rows: Vec<CatalogRow> = snapshot
            .rows
            .into_iter()
            .map(|row| {
                let cards: Vec<PosterData> = row
                    .cards
                    .into_iter()
                    .map(|card| {
                        let image = image_for(&card.poster);
                        PosterData {
                            item_id: card.id.into(),
                            kind: card.kind.into(),
                            title: card.title.into(),
                            has_poster: image.is_some(),
                            poster: image.unwrap_or_default(),
                        }
                    })
                    .collect();
                CatalogRow {
                    name: row.name.into(),
                    cards: ModelRc::new(VecModel::from(cards)),
                }
            })
            .collect();
        window.set_status(snapshot.status.into());
        window.set_rows(ModelRc::new(VecModel::from(rows)));

        let details = snapshot.details;
        if let Some(title) = details.title {
            let image = image_for(&details.poster);
            window.set_details_title(title.into());
            window.set_details_description(details.description.into());
            window.set_details_has_poster(image.is_some());
            window.set_details_poster(image.unwrap_or_default());
        }
        window.set_details_status(details.status.into());
        let streams: Vec<StreamData> = details
            .streams
            .into_iter()
            .map(|(name, description)| StreamData {
                name: name.into(),
                description: description.into(),
            })
            .collect();
        window.set_streams(ModelRc::new(VecModel::from(streams)));
    });
}

async fn fetch_poster(
    client: &reqwest::Client,
    url: &str,
) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let bytes = client
        .get(url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .bytes()
        .await
        .ok()?;
    tokio::task::spawn_blocking(move || {
        let image = image::load_from_memory(&bytes)
            .ok()?
            .thumbnail(300, 450)
            .to_rgba8();
        Some(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            image.as_raw(),
            image.width(),
            image.height(),
        ))
    })
    .await
    .ok()?
}

/// Series have no stream until an episode is picked, so the core guesses one. Everything else
/// asks the addons for streams of the item itself.
fn open_details(runtime: &NativeRuntime, id: &str, kind: &str) {
    let (stream_path, guess_stream) = if kind == "series" {
        (None, true)
    } else {
        (
            Some(ResourcePath::without_extra(STREAM_RESOURCE_NAME, kind, id)),
            false,
        )
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::MetaDetails(DetailsSelected {
            meta_path: ResourcePath::without_extra(META_RESOURCE_NAME, kind, id),
            stream_path,
            guess_stream,
        })),
    });
}

/// The folder holding `resources/`: the executable's folder unless `STREMIO_LIGHTNING_BUNDLE_DIR`
/// says otherwise.
fn bundle_dir() -> PathBuf {
    std::env::var_os("STREMIO_LIGHTNING_BUNDLE_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf))
        })
        .unwrap_or_default()
}

/// The stream server and its ffmpeg live in `<bundle>/resources`.
fn stream_server_spec(bundle: &Path, data_dir: &Path) -> CommandSpec {
    let resources = bundle.join("resources");
    let program = resources.join(if cfg!(windows) {
        "stream-server.exe"
    } else {
        "stream-server"
    });

    // The server finds ffmpeg and ffprobe through PATH, so the bundled ones go first.
    let mut search = vec![resources];
    search.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(search)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default();

    let logs = data_dir.join("logs");
    CommandSpec {
        program,
        // The Windows build of the server opens a tray icon unless told not to.
        args: if cfg!(windows) {
            vec![PathBuf::from("--no-tray")]
        } else {
            Vec::new()
        },
        env: [("PATH".to_owned(), path)].into(),
        stdout_log: logs.join("stremio-server.stdout.log"),
        stderr_log: logs.join("stremio-server.stderr.log"),
    }
}

type Supervisor = StreamingServerSupervisor<RealProcessSpawner>;

/// Waits for something to answer on the server port and says in the window what that is. The port
/// is fixed, so an already running server (the official app, say) answers instead of ours. Ours
/// then exits at once, because it cannot bind the port, and the app uses the one that is there.
async fn report_server_state(
    weak: Weak<MainWindow>,
    server: Arc<Supervisor>,
    started: Result<(), String>,
) {
    let text = match started {
        Err(error) => format!("Stream server: not started ({error})"),
        Ok(()) => {
            let mut answering = false;
            for _ in 0..60 {
                if tokio::net::TcpStream::connect(STREAM_SERVER_ADDRESS)
                    .await
                    .is_ok()
                {
                    answering = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            // Give a child that cannot bind the port time to exit before judging.
            tokio::time::sleep(Duration::from_secs(1)).await;
            match (answering, server.is_running()) {
                (true, true) => "Stream server: ready".to_owned(),
                (true, false) => format!(
                    "Stream server: using the one already running on {STREAM_SERVER_ADDRESS}"
                ),
                (false, _) => "Stream server: started but not answering".to_owned(),
            }
        }
    };
    let _ = weak.upgrade_in_event_loop(move |window| window.set_server_status(text.into()));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let data_dir = dirs::data_dir()
        .ok_or("no data directory")?
        .join("stremio-lightning-native");
    NativeEnv::init(data_dir.clone(), tokio_runtime.handle().clone()).map_err(|e| e.message())?;

    let window = MainWindow::new()?;

    let server = Arc::new(Supervisor::new(
        RealProcessSpawner,
        stream_server_spec(&bundle_dir(), &data_dir),
    ));
    tokio_runtime.spawn(report_server_state(
        window.as_weak(),
        server.clone(),
        server.start(),
    ));

    let profile: Profile = tokio_runtime
        .block_on(NativeEnv::get_storage(
            stremio_core::constants::PROFILE_STORAGE_KEY,
        ))
        .map_err(|e| e.message())?
        .unwrap_or_default();
    let uid = profile.uid();
    let model = NativeModel {
        ctx: Ctx::new(
            profile,
            LibraryBucket::new(uid.clone(), vec![]),
            StreamsBucket::new(uid.clone()),
            ServerUrlsBucket::new::<NativeEnv>(uid.clone()),
            NotificationsBucket::new::<NativeEnv>(uid.clone(), vec![]),
            SearchHistoryBucket::new(uid.clone()),
            DismissedEventsBucket::new(uid),
        ),
        board: Default::default(),
        meta_details: Default::default(),
    };

    let (runtime, events) = NativeRuntime::new(model, vec![], 1000);
    let runtime = Arc::new(runtime);

    let posters: Posters = Default::default();
    let requested = Arc::new(Mutex::new(HashSet::<String>::new()));
    let latest = Arc::new(Mutex::new(Snapshot::default()));
    let client = reqwest::Client::new();

    {
        let weak = window.as_weak();
        let runtime = runtime.clone();
        let latest = latest.clone();
        window.on_open_item(move |id, kind, title| {
            // Drop what the previous item left behind so it can't flash up before the new data.
            latest.lock().unwrap().details = DetailsData::default();
            if let Some(window) = weak.upgrade() {
                window.set_details_title(title);
                window.set_details_description("".into());
                window.set_details_has_poster(false);
                window.set_details_status("Loading...".into());
                window.set_streams(ModelRc::default());
            }
            open_details(&runtime, &id, &kind);
        });
    }

    let weak = window.as_weak();
    let reader = runtime.clone();
    tokio_runtime.spawn(events.for_each({
        let posters = posters.clone();
        let latest = latest.clone();
        move |event| {
            let weak = weak.clone();
            let reader = reader.clone();
            let posters = posters.clone();
            let requested = requested.clone();
            let latest = latest.clone();
            let client = client.clone();
            async move {
                if !matches!(event, RuntimeEvent::NewState(..)) {
                    return;
                }
                let Ok(model) = reader.model() else { return };
                let (status, rows) = board_rows(&model);
                let details = details_data(&model);
                drop(model);
                let snapshot = Snapshot {
                    status,
                    rows,
                    details,
                };
                *latest.lock().unwrap() = snapshot.clone();

                let urls: Vec<String> = snapshot
                    .rows
                    .iter()
                    .flat_map(|row| row.cards.iter())
                    .filter_map(|card| card.poster.clone())
                    .chain(snapshot.details.poster.clone())
                    .collect();
                for url in urls {
                    if !requested.lock().unwrap().insert(url.clone()) {
                        continue;
                    }
                    let (client, posters, latest, weak) = (
                        client.clone(),
                        posters.clone(),
                        latest.clone(),
                        weak.clone(),
                    );
                    tokio::spawn(async move {
                        if let Some(buffer) = fetch_poster(&client, &url).await {
                            posters.lock().unwrap().insert(url, buffer);
                            let snapshot = latest.lock().unwrap().clone();
                            push(&weak, snapshot, posters);
                        }
                    });
                }
                push(&weak, snapshot, posters);
            }
        }
    }));

    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::CatalogsWithExtra(Selected {
            r#type: None,
            extra: vec![],
        })),
    });
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::CatalogsWithExtra(ActionCatalogsWithExtra::LoadRange(0..8)),
    });

    window.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_server_runs_from_the_bundle_with_its_ffmpeg_first_on_path() {
        let bundle = Path::new("bundle");
        let spec = stream_server_spec(bundle, Path::new("data"));

        let resources = bundle.join("resources");
        assert_eq!(spec.program.parent(), Some(resources.as_path()));
        let path = spec.env.get("PATH").expect("PATH is set");
        assert_eq!(std::env::split_paths(path).next(), Some(resources));
        assert_eq!(spec.stdout_log.parent(), Some(Path::new("data/logs")));
    }
}
