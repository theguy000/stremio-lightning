//! The Addons screen's two menus: "Installed", then every addon catalog the installed addons
//! declare -- and the type menu under whichever of the two is on screen.

use std::sync::Once;

use stremio_core::models::catalog_with_filters::CatalogWithFilters;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;
use stremio_core::models::ctx::Ctx;
use stremio_core::models::installed_addons_with_filters::InstalledAddonsWithFilters;
use stremio_core::models::installed_addons_with_filters::{
    InstalledAddonsRequest, Selected as InstalledSelected,
};
use stremio_core::models::library_with_filters::{LibraryWithFilters, NotRemovedFilter};
use stremio_core::runtime::msg::{Action, ActionLoad};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::Descriptor;
use stremio_core::types::events::DismissedEventsBucket;
use stremio_core::types::library::LibraryBucket;
use stremio_core::types::notifications::NotificationsBucket;
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::MetaItemPreview;
use stremio_core::types::search_history::SearchHistoryBucket;
use stremio_core::types::server_urls::ServerUrlsBucket;
use stremio_core::types::streams::StreamsBucket;
use stremio_lightning::{app, screens};

fn init_env() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("stremio-lightning-market-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp folder");
        let tokio = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a tokio runtime");
        stremio_lightning_env::AppEnv::init(dir, tokio.handle().clone()).expect("an environment");
    });
}

/// Cinemeta, the way its own manifest declares it: the marketplace is a resource it offers.
fn cinemeta() -> Descriptor {
    serde_json::from_value(serde_json::json!({
        "transportUrl": "https://v3-cinemeta.strem.io/manifest.json",
        "manifest": {
            "id": "com.linvo.cinemeta",
            "version": "3.0.14",
            "name": "Cinemeta",
            "description": "The official addon for movie and series catalogs",
            "resources": ["catalog", "meta", "addon_catalog"],
            "types": ["movie", "series"],
            "idPrefixes": ["tt"],
            "catalogs": [],
            "addonCatalogs": [
                { "type": "all", "id": "official", "name": "Official" },
                { "type": "movie", "id": "official", "name": "Official" },
                { "type": "all", "id": "community", "name": "Community" },
                { "type": "movie", "id": "community", "name": "Community" }
            ]
        },
        "flags": { "official": true, "protected": false }
    }))
    .expect("a descriptor")
}

fn runtime_with(
    addons: Vec<Descriptor>,
) -> (
    std::sync::Arc<app::AppRuntime>,
    futures::channel::mpsc::Receiver<
        stremio_core::runtime::RuntimeEvent<stremio_lightning_env::AppEnv, app::AppModel>,
    >,
) {
    let model = model_with(addons);
    let (runtime, events) = app::AppRuntime::new(model, vec![], 1000);
    (std::sync::Arc::new(runtime), events)
}

fn model_with(addons: Vec<Descriptor>) -> app::AppModel {
    init_env();
    let profile = Profile {
        addons,
        ..Profile::default()
    };
    let uid = profile.uid();
    let library = LibraryBucket::new(uid.clone(), vec![]);
    let notifications =
        NotificationsBucket::new::<stremio_lightning_env::AppEnv>(uid.clone(), vec![]);
    let (continue_watching_preview, _) = ContinueWatchingPreview::new(&library, &notifications);
    let (library_with_filters, _) =
        LibraryWithFilters::<NotRemovedFilter>::new(&library, &notifications);
    let (discover, _) = CatalogWithFilters::<MetaItemPreview>::new(&profile);
    let (installed_addons, _) = InstalledAddonsWithFilters::new(&profile);
    let (remote_addons, _) = CatalogWithFilters::<Descriptor>::new(&profile);
    app::AppModel {
        ctx: Ctx::new(
            profile,
            library,
            StreamsBucket::new(uid.clone()),
            ServerUrlsBucket::new::<stremio_lightning_env::AppEnv>(uid.clone()),
            notifications,
            SearchHistoryBucket::new(uid.clone()),
            DismissedEventsBucket::new(uid),
        ),
        continue_watching_preview,
        board: Default::default(),
        calendar: Default::default(),
        discover,
        library: library_with_filters,
        installed_addons,
        remote_addons,
        addon_details: Default::default(),
        meta_details: Default::default(),
        player: Default::default(),
    }
}

#[test]
fn the_catalog_menu_is_installed_and_then_every_catalog_the_addons_declare() {
    let model = model_with(vec![cinemeta()]);
    let data = screens::addons::data(&model);

    let catalog = &data.inputs[0];
    let labels: Vec<&str> = catalog
        .options
        .iter()
        .map(|(label, _)| label.as_str())
        .collect();
    assert_eq!(
        labels,
        ["Installed", "Official", "Community"],
        "one entry per catalog, not one per type it is offered for"
    );
    assert_eq!(catalog.selected, 0);
    assert_eq!(
        data.inputs[0].title, "Installed",
        "the closed menu names what is on screen"
    );

    // The type menu under "Installed" is the types the account's addons cover.
    let kinds = &data.inputs[1];
    assert_eq!(kinds.options[0].0, "All");
    assert!(
        kinds.options.iter().any(|(label, _)| label == "Movie"),
        "Cinemeta covers movies"
    );

    // Choosing "Community" is a marketplace load, not an installed-addons one.
    let community = data.choices[0]
        .iter()
        .find(|choice| {
            matches!(
                choice,
                screens::addons::Choice::Remote(request)
                    if request.path.resource == "addon_catalog" && request.path.id == "community"
            )
        })
        .expect("Community loads its catalog");
    assert!(
        matches!(community, screens::addons::Choice::Remote(_)),
        "the marketplace list comes from the remote model"
    );
}

#[test]
fn an_addon_the_account_lacks_offers_install_and_one_it_has_offers_uninstall() {
    let (runtime, _events) = runtime_with(vec![cinemeta()]);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::InstalledAddonsWithFilters(InstalledSelected {
            request: InstalledAddonsRequest { r#type: None },
        })),
    });

    let model = runtime.model().expect("the model is readable");
    let data = screens::addons::data(&model);

    assert!(!data.remote, "the screen opens on the installed list");
    assert_eq!(data.cards.len(), 1);
    assert_eq!(data.cards[0].name, "Cinemeta");
    assert!(
        data.cards[0].installed,
        "an installed addon's card offers Uninstall"
    );
}
