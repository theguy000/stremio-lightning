use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use slint::{Image, ModelRc, Rgba8Pixel, SharedPixelBuffer, VecModel, Weak};
use stremio_core::models::catalogs_with_extra::{CatalogsWithExtra, Selected};
use stremio_core::models::common::Loadable;
use stremio_core::models::ctx::Ctx;
use stremio_core::runtime::msg::{Action, ActionCatalogsWithExtra, ActionLoad};
use stremio_core::runtime::{Env, Runtime, RuntimeAction, RuntimeEvent};
use stremio_core::types::events::DismissedEventsBucket;
use stremio_core::types::library::LibraryBucket;
use stremio_core::types::notifications::NotificationsBucket;
use stremio_core::types::profile::Profile;
use stremio_core::types::search_history::SearchHistoryBucket;
use stremio_core::types::server_urls::ServerUrlsBucket;
use stremio_core::types::streams::StreamsBucket;
use stremio_core::Model;
use stremio_lightning_native_env::NativeEnv;

slint::include_modules!();

const CARDS_PER_ROW: usize = 20;

#[derive(Model, Clone)]
#[model(NativeEnv)]
struct NativeModel {
    ctx: Ctx,
    board: CatalogsWithExtra,
}

/// One poster card: the title and, when the addon gave one, the poster URL.
type CardData = (String, Option<String>);

#[derive(Clone)]
struct RowData {
    name: String,
    cards: Vec<CardData>,
}

type Posters = Arc<Mutex<HashMap<String, SharedPixelBuffer<Rgba8Pixel>>>>;

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
                    .map(|item| {
                        (
                            item.name.clone(),
                            item.poster.as_ref().map(|url| url.to_string()),
                        )
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

fn push_rows(weak: &Weak<MainWindow>, status: String, rows: Vec<RowData>, posters: Posters) {
    let _ = weak.upgrade_in_event_loop(move |window| {
        let cache = posters.lock().unwrap();
        let rows: Vec<CatalogRow> = rows
            .into_iter()
            .map(|row| {
                let cards: Vec<PosterData> = row
                    .cards
                    .into_iter()
                    .map(|(title, url)| {
                        let buffer = url.as_ref().and_then(|url| cache.get(url));
                        PosterData {
                            title: title.into(),
                            has_poster: buffer.is_some(),
                            poster: buffer
                                .map(|buffer| Image::from_rgba8(buffer.clone()))
                                .unwrap_or_default(),
                        }
                    })
                    .collect();
                CatalogRow {
                    name: row.name.into(),
                    cards: ModelRc::new(VecModel::from(cards)),
                }
            })
            .collect();
        window.set_status(status.into());
        window.set_rows(ModelRc::new(VecModel::from(rows)));
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tokio_runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let data_dir = dirs::data_dir()
        .ok_or("no data directory")?
        .join("stremio-lightning-native");
    NativeEnv::init(data_dir, tokio_runtime.handle().clone()).map_err(|e| e.message())?;

    let window = MainWindow::new()?;

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
    };

    let (runtime, events) = Runtime::<NativeEnv, _>::new(model, vec![], 1000);
    let runtime = Arc::new(runtime);

    let weak = window.as_weak();
    let reader = runtime.clone();
    let posters: Posters = Default::default();
    let requested = Arc::new(Mutex::new(HashSet::<String>::new()));
    let latest = Arc::new(Mutex::new((String::new(), Vec::<RowData>::new())));
    let client = reqwest::Client::new();
    tokio_runtime.spawn(events.for_each(move |event| {
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
            drop(model);
            *latest.lock().unwrap() = (status.clone(), rows.clone());

            let urls: Vec<String> = rows
                .iter()
                .flat_map(|row| row.cards.iter())
                .filter_map(|(_, url)| url.clone())
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
                        let (status, rows) = latest.lock().unwrap().clone();
                        push_rows(&weak, status, rows, posters);
                    }
                });
            }
            push_rows(&weak, status, rows, posters);
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
