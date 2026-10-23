//! The app's state: the stremio-core models the UI reads, the runtime that owns them, and the
//! snapshot of "what the window currently shows".

use stremio_core::models::catalog_with_filters::CatalogWithFilters;
use stremio_core::models::catalogs_with_extra::CatalogsWithExtra;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;
use stremio_core::models::ctx::Ctx;
use stremio_core::models::installed_addons_with_filters::InstalledAddonsWithFilters;
use stremio_core::models::meta_details::MetaDetails;
use stremio_core::models::player::Player;
use stremio_core::runtime::Runtime;
use stremio_core::types::resource::MetaItemPreview;
use stremio_core::Model;
use stremio_lightning_env::AppEnv;

use crate::screens::{addons, board, details, discover, player, settings};

/// Everything stremio-core holds for this app. One value of this type is the whole app state.
#[derive(Model, Clone)]
#[model(AppEnv)]
pub struct AppModel {
    pub ctx: Ctx,
    pub board: CatalogsWithExtra,
    /// The titles the account left unfinished, kept by the core from the library. The Board
    /// draws them as its first row, above the addon catalogs.
    pub continue_watching_preview: ContinueWatchingPreview,
    pub discover: CatalogWithFilters<MetaItemPreview>,
    pub installed_addons: InstalledAddonsWithFilters,
    pub meta_details: MetaDetails,
    pub player: Player,
}

pub type AppRuntime = Runtime<AppEnv, AppModel>;

/// What the window currently shows, built from the model whenever it changes. Every field is
/// plain data, so a test can build one and ask what a screen would look like.
#[derive(Clone, Default)]
pub(crate) struct Snapshot {
    pub(crate) status: String,
    pub(crate) rows: Vec<board::Row>,
    pub discover: discover::Data,
    pub(crate) details: details::Data,
    pub(crate) player: player::Data,
    pub(crate) addons: addons::Data,
    pub(crate) settings: settings::Data,
}
