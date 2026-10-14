use std::sync::Arc;

use futures::StreamExt;
use slint::{ModelRc, SharedString, VecModel};
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

#[derive(Model, Clone)]
#[model(NativeEnv)]
struct NativeModel {
    ctx: Ctx,
    board: CatalogsWithExtra,
}

fn board_lines(model: &NativeModel) -> (String, Vec<String>) {
    let mut lines = Vec::new();
    let mut loading = 0;
    for catalog in &model.board.catalogs {
        let Some(page) = catalog.first() else {
            continue;
        };
        let heading = format!("{} / {}", page.request.path.r#type, page.request.path.id);
        match &page.content {
            Some(Loadable::Ready(items)) => {
                let names: Vec<&str> = items
                    .iter()
                    .take(8)
                    .map(|item| item.name.as_str())
                    .collect();
                lines.push(format!("{heading}: {}", names.join(", ")));
            }
            Some(Loadable::Err(error)) => lines.push(format!("{heading}: failed ({error:?})")),
            _ => loading += 1,
        }
    }
    let status = format!(
        "{} addons, {} catalogs ready, {} loading",
        model.ctx.profile.addons.len(),
        lines.len(),
        loading
    );
    (status, lines)
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
    tokio_runtime.spawn(events.for_each(move |event| {
        let weak = weak.clone();
        let reader = reader.clone();
        async move {
            if !matches!(event, RuntimeEvent::NewState(..)) {
                return;
            }
            let Ok(model) = reader.model() else { return };
            let (status, lines) = board_lines(&model);
            drop(model);
            let _ = weak.upgrade_in_event_loop(move |window| {
                window.set_status(SharedString::from(status));
                let rows: Vec<SharedString> = lines.into_iter().map(SharedString::from).collect();
                window.set_lines(ModelRc::new(VecModel::from(rows)));
            });
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
