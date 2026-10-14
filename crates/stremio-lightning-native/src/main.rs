use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel, Weak};
use std::path::{Path, PathBuf};
use std::time::Duration;
use stremio_core::constants::{META_RESOURCE_NAME, STREAM_RESOURCE_NAME};
use stremio_core::models::catalog_with_filters::CatalogWithFilters;
use stremio_core::models::catalogs_with_extra::{CatalogsWithExtra, Selected};
use stremio_core::models::common::{Loadable, ResourceError};
use stremio_core::models::ctx::Ctx;
use stremio_core::models::meta_details::{MetaDetails, Selected as DetailsSelected};
use stremio_core::runtime::msg::{
    Action, ActionCatalogWithFilters, ActionCatalogsWithExtra, ActionLoad,
};
use stremio_core::runtime::{Env, Runtime, RuntimeAction, RuntimeEvent};
use stremio_core::types::addon::{ResourcePath, ResourceRequest};
use stremio_core::types::events::DismissedEventsBucket;
use stremio_core::types::library::LibraryBucket;
use stremio_core::types::notifications::NotificationsBucket;
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::{MetaItemPreview, PosterShape, Stream, StreamUrls};
use stremio_core::types::search_history::SearchHistoryBucket;
use stremio_core::types::server_urls::ServerUrlsBucket;
use stremio_core::types::streams::StreamsBucket;
use stremio_core::Model;
use stremio_lightning_core::streaming_server::{
    CommandSpec, RealProcessSpawner, StreamingServerSupervisor,
};
use stremio_lightning_native_env::AppEnv;
use url::Url;

mod class_styles;
mod player;

slint::include_modules!();

/// stremio-web's `CATALOG_PREVIEW_SIZE`: how many cards a Board row keeps.
const CATALOG_PREVIEW_SIZE: usize = 10;
const MAX_STREAMS: usize = 100;
const STREAM_SERVER_ADDRESS: &str = "127.0.0.1:11470";

#[derive(Model, Clone)]
#[model(AppEnv)]
struct AppModel {
    ctx: Ctx,
    board: CatalogsWithExtra,
    discover: CatalogWithFilters<MetaItemPreview>,
    meta_details: MetaDetails,
}

type AppRuntime = Runtime<AppEnv, AppModel>;
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
    title: String,
    shape: &'static str,
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

/// What the Discover screen shows: the labels of its first three inputs and every card loaded so far.
#[derive(Clone, Default)]
struct DiscoverData {
    shape: &'static str,
    cards: Vec<Card>,
    filters: Vec<String>,
}

#[derive(Clone, Default)]
struct Snapshot {
    status: String,
    rows: Vec<RowData>,
    discover: DiscoverData,
    details: DetailsData,
}

/// "Popular - Movie": the catalog's name (the addon's name when the catalog has none, as
/// stremio-core-web does) and the capitalised type, like stremio-web's `catalogTitle` without
/// its translations.
fn catalog_title(model: &AppModel, request: &ResourceRequest) -> String {
    let name = model
        .ctx
        .profile
        .addons
        .iter()
        .find(|addon| addon.transport_url == request.base)
        .and_then(|addon| {
            addon
                .manifest
                .catalogs
                .iter()
                .find(|catalog| {
                    catalog.id == request.path.id && catalog.r#type == request.path.r#type
                })
                .map(|catalog| {
                    catalog
                        .name
                        .clone()
                        .unwrap_or_else(|| addon.manifest.name.clone())
                })
        });
    let Some(name) = name else {
        return String::new();
    };
    format!("{name} - {}", capitalise(&request.path.r#type))
}

fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn card_of(item: &MetaItemPreview) -> Card {
    Card {
        id: item.id.clone(),
        kind: item.r#type.clone(),
        title: item.name.clone(),
        poster: item.poster.as_ref().map(|url| url.to_string()),
    }
}

fn shape_of<'a>(items: impl IntoIterator<Item = &'a MetaItemPreview>) -> &'static str {
    match items.into_iter().next().map(|item| &item.poster_shape) {
        Some(PosterShape::Square) => "square",
        Some(PosterShape::Landscape) => "landscape",
        _ => "poster",
    }
}

fn board_rows(model: &AppModel) -> (String, Vec<RowData>) {
    let mut rows = Vec::new();
    let (mut loading, mut failed) = (0, 0);
    for catalog in &model.board.catalogs {
        let Some(page) = catalog.first() else {
            continue;
        };
        match &page.content {
            Some(Loadable::Ready(items)) => rows.push(RowData {
                title: catalog_title(model, &page.request),
                shape: shape_of(items),
                cards: items
                    .iter()
                    .take(CATALOG_PREVIEW_SIZE)
                    .map(card_of)
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

fn discover_data(model: &AppModel) -> DiscoverData {
    let discover = &model.discover;
    let mut items: Vec<&MetaItemPreview> = Vec::new();
    for page in &discover.catalog {
        if let Some(Loadable::Ready(page_items)) = &page.content {
            items.extend(page_items);
        }
    }
    let selectable = &discover.selectable;
    let kind = selectable
        .types
        .iter()
        .find(|item| item.selected)
        .map(|item| capitalise(&item.r#type));
    let catalog = selectable
        .catalogs
        .iter()
        .find(|item| item.selected)
        .map(|item| item.catalog.clone());
    let extras = selectable.extra.iter().map(|extra| {
        extra
            .options
            .iter()
            .find(|option| option.selected)
            .and_then(|option| option.value.clone())
            .unwrap_or_else(|| capitalise(&extra.name))
    });
    DiscoverData {
        shape: shape_of(items.iter().copied()),
        cards: items.iter().copied().map(card_of).collect(),
        filters: kind
            .into_iter()
            .chain(catalog)
            .chain(extras)
            .take(3)
            .collect(),
    }
}

fn details_data(model: &AppModel) -> DetailsData {
    let details = &model.meta_details;
    let item = details
        .meta_items
        .iter()
        .find_map(|page| match &page.content {
            Some(Loadable::Ready(item)) => Some(item),
            _ => None,
        });

    let (mut ready, mut loading, mut empty, mut failed) = (0, 0, 0, 0);
    for page in &details.streams {
        match &page.content {
            Some(Loadable::Ready(_)) => ready += 1,
            Some(Loadable::Err(ResourceError::EmptyContent)) => empty += 1,
            Some(Loadable::Err(_)) => failed += 1,
            _ => loading += 1,
        }
    }
    let found = streams_of(details).count();
    let streams = streams_of(details)
        .take(MAX_STREAMS)
        .map(|stream| {
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
        })
        .collect();

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

/// Every stream the addons have answered with, in the order the Details screen lists them, so the
/// index of a clicked row finds its stream again.
fn streams_of(details: &MetaDetails) -> impl Iterator<Item = &Stream> {
    details
        .streams
        .iter()
        .filter_map(|page| match &page.content {
            Some(Loadable::Ready(list)) => Some(list.iter()),
            _ => None,
        })
        .flatten()
}

/// The address the player opens: torrents and archives go through the stream server, plain
/// URLs are played as they are.
fn stream_url(stream: &Stream, server: &Url) -> Result<String, String> {
    let converted = stream
        .convert(Some(server))
        .map_err(|error| error.message())?;
    StreamUrls::new(converted, Some(server))
        .streaming_url
        .map(String::from)
        .ok_or_else(|| "it has no address the player can open".to_owned())
}

fn play_selected(
    runtime: &AppRuntime,
    video: &player::VideoPlayer,
    index: usize,
) -> Result<(), String> {
    let url = {
        let model = runtime
            .model()
            .map_err(|_| "the app state is not available".to_owned())?;
        let stream = streams_of(&model.meta_details)
            .nth(index)
            .ok_or("the list changed, pick it again")?;
        stream_url(stream, &model.ctx.profile.settings.streaming_server_url)?
    };
    video.play(&url)
}

fn push(weak: &Weak<MainWindow>, snapshot: Snapshot, posters: Posters) {
    let _ = weak.upgrade_in_event_loop(move |window| {
        let cache = posters.lock().unwrap();
        let image_for = |url: &Option<String>| {
            url.as_ref()
                .and_then(|url| cache.get(url))
                .map(|buffer| Image::from_rgba8(buffer.clone()))
        };

        let poster_card = |card: Card| {
            let image = image_for(&card.poster);
            PosterData {
                item_id: card.id.into(),
                kind: card.kind.into(),
                title: card.title.into(),
                has_poster: image.is_some(),
                poster: image.unwrap_or_default(),
            }
        };

        let rows: Vec<CatalogRow> = snapshot
            .rows
            .into_iter()
            .map(|row| {
                let cards: Vec<PosterData> = row.cards.into_iter().map(&poster_card).collect();
                CatalogRow {
                    title: row.title.into(),
                    shape: row.shape.into(),
                    cards: ModelRc::new(VecModel::from(cards)),
                }
            })
            .collect();
        window.set_status(snapshot.status.into());
        window.set_rows(ModelRc::new(VecModel::from(rows)));
        let discover_cards: Vec<PosterData> = snapshot
            .discover
            .cards
            .into_iter()
            .map(&poster_card)
            .collect();
        window.set_discover_cards(ModelRc::new(VecModel::from(discover_cards)));
        window.set_discover_shape(snapshot.discover.shape.into());
        let filters: Vec<SharedString> = snapshot
            .discover
            .filters
            .into_iter()
            .map(Into::into)
            .collect();
        window.set_discover_filters(ModelRc::new(VecModel::from(filters)));

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
fn open_details(runtime: &AppRuntime, id: &str, kind: &str) {
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
    AppEnv::init(data_dir.clone(), tokio_runtime.handle().clone()).map_err(|e| e.message())?;

    let window = MainWindow::new()?;
    class_styles::apply(&window);
    let video = player::VideoPlayer::install(&window);

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
        .block_on(AppEnv::get_storage(
            stremio_core::constants::PROFILE_STORAGE_KEY,
        ))
        .map_err(|e| e.message())?
        .unwrap_or_default();
    let uid = profile.uid();
    // `default()` has nothing to pick from, so a `Load` with no selection would select nothing.
    let (discover, discover_effects) = CatalogWithFilters::<MetaItemPreview>::new(&profile);
    let model = AppModel {
        ctx: Ctx::new(
            profile,
            LibraryBucket::new(uid.clone(), vec![]),
            StreamsBucket::new(uid.clone()),
            ServerUrlsBucket::new::<AppEnv>(uid.clone()),
            NotificationsBucket::new::<AppEnv>(uid.clone(), vec![]),
            SearchHistoryBucket::new(uid.clone()),
            DismissedEventsBucket::new(uid),
        ),
        board: Default::default(),
        discover,
        meta_details: Default::default(),
    };

    let (runtime, events) = AppRuntime::new(model, discover_effects.into_iter().collect(), 1000);
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
                window.set_play_message("".into());
                window.set_streams(ModelRc::default());
            }
            open_details(&runtime, &id, &kind);
        });
    }

    {
        let weak = window.as_weak();
        let runtime = runtime.clone();
        let video = video.clone();
        window.on_play_stream(move |index| {
            let Some(window) = weak.upgrade() else { return };
            let index = usize::try_from(index).unwrap_or(usize::MAX);
            match play_selected(&runtime, &video, index) {
                Ok(()) => {
                    window.set_play_message("".into());
                    window.set_playing(true);
                }
                Err(error) => {
                    window.set_play_message(format!("Can't play this stream: {error}").into());
                }
            }
        });
    }
    {
        let weak = window.as_weak();
        window.on_stop_playback(move || {
            video.stop();
            if let Some(window) = weak.upgrade() {
                window.set_playing(false);
            }
        });
    }

    {
        let runtime = runtime.clone();
        window.on_load_next_page(move || {
            runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::CatalogWithFilters(ActionCatalogWithFilters::LoadNextPage),
            });
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
                let discover = discover_data(&model);
                let details = details_data(&model);
                drop(model);
                let snapshot = Snapshot {
                    status,
                    rows,
                    discover,
                    details,
                };
                *latest.lock().unwrap() = snapshot.clone();

                let urls: Vec<String> = snapshot
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
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::CatalogWithFilters(None)),
    });

    window.run()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::types::resource::StreamSource;

    fn stream(source: StreamSource) -> Stream {
        Stream {
            source,
            name: None,
            description: None,
            thumbnail: None,
            subtitles: vec![],
            behavior_hints: Default::default(),
        }
    }

    #[test]
    fn torrents_play_through_the_stream_server_and_plain_urls_as_they_are() {
        let server = Url::parse("http://127.0.0.1:11470/").unwrap();

        let torrent = stream(StreamSource::Torrent {
            info_hash: [0xab; 20],
            file_idx: None,
            announce: vec![],
            file_must_include: vec![],
        });
        // The core leaves an empty query on the end of torrent addresses, so compare the parts.
        let url = Url::parse(&stream_url(&torrent, &server).unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.port(), Some(11470));
        assert_eq!(url.path(), format!("/{}/-1", "ab".repeat(20)));

        let plain = stream(StreamSource::Url {
            url: Url::parse("https://example.com/movie.mp4").unwrap(),
        });
        assert_eq!(
            stream_url(&plain, &server).unwrap(),
            "https://example.com/movie.mp4"
        );
    }

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
