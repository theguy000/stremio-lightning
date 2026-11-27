//! The app's state: `stremio-core` models, runtime alias, and structurally shared `Snapshot`.

use std::sync::Arc;
use std::time::Instant;

use stremio_core::models::addon_details::AddonDetails;
use stremio_core::models::calendar::Calendar;
use stremio_core::models::catalog_with_filters::CatalogWithFilters;
use stremio_core::models::catalogs_with_extra::CatalogsWithExtra;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;
use stremio_core::models::ctx::Ctx;
use stremio_core::models::data_export::DataExport;
use stremio_core::models::installed_addons_with_filters::InstalledAddonsWithFilters;
use stremio_core::models::library_with_filters::{LibraryWithFilters, NotRemovedFilter};
use stremio_core::models::meta_details::MetaDetails;
use stremio_core::models::player::Player;
use stremio_core::models::streaming_server::StreamingServer;
use stremio_core::runtime::Runtime;
use stremio_core::types::addon::Descriptor;
use stremio_core::types::resource::MetaItemPreview;
use stremio_core::Model;

use crate::plumbing::env::AppEnv;
use crate::screens::{addons, board, calendar, details, discover, library, mods, player, settings};

/// Everything `stremio-core` holds for this app.
#[derive(Model, Clone)]
#[model(AppEnv)]
pub struct AppModel {
    pub ctx: Ctx,
    pub board: CatalogsWithExtra,
    pub continue_watching_preview: ContinueWatchingPreview,
    pub discover: CatalogWithFilters<MetaItemPreview>,
    pub library: LibraryWithFilters<NotRemovedFilter>,
    pub installed_addons: InstalledAddonsWithFilters,
    pub remote_addons: CatalogWithFilters<Descriptor>,
    pub addon_details: AddonDetails,
    pub calendar: Calendar,
    pub meta_details: MetaDetails,
    pub player: Player,
    pub streaming_server: StreamingServer,
    pub data_export: DataExport,
}

pub type AppRuntime = Runtime<AppEnv, AppModel>;

/// The Board screen's plain-Rust state slice.
#[derive(Clone, Default)]
pub struct BoardData {
    pub status: String,
    pub rows: Vec<board::Row>,
    pub spotlight: Vec<board::SpotlightItem>,
    pub spotlight_loading: bool,
}

/// Complete, structurally shared plain-Rust snapshot of every screen.
///
/// Each screen lives behind an `Arc`, so reading or cloning `Snapshot` is O(1) and unaffected
/// screens are never copied when one screen updates.
#[derive(Clone, Default)]
pub struct Snapshot {
    pub board: Arc<BoardData>,
    pub discover: Arc<discover::Data>,
    pub library: Arc<library::Data>,
    pub details: Arc<details::Data>,
    pub player: Arc<player::Data>,
    pub addons: Arc<addons::Data>,
    pub calendar: Arc<calendar::Data>,
    pub settings: Arc<settings::Data>,
    pub mods: Arc<mods::Data>,
}

/// Last reported Discover grid scroll geometry, used to detect fast flicks and compute settled
/// preload ranges without process-global static state.
#[derive(Clone, Copy, Debug)]
pub struct GridReport {
    pub scroll: f32,
    pub height: f32,
    pub pitch: f32,
    pub columns: i32,
    pub at: Instant,
}

/// A Mods card waiting for a second Remove press on the same page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmedMod {
    pub id: String,
    pub page: String,
}

/// Addon ids + catalog count for which Board's `LoadRange` was last dispatched.
pub type BoardCatalogRange = (Vec<String>, usize);
