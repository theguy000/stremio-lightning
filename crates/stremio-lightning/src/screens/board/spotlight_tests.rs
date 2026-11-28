use std::str::FromStr;

use stremio_core::models::common::{Loadable, ResourceLoadable};
use stremio_core::types::addon::{ResourcePath, ResourceRequest};
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::MetaItemPreview;
use url::Url;

use super::spotlight::{self, SPOTLIGHT_ITEMS_COUNT};
use crate::app::build::runtime;
use crate::plumbing::local_settings::{LocalExtras, SpotlightConfig};
use crate::screens::board;

fn preview(id: &str, kind: &str, with_bg: bool) -> MetaItemPreview {
    MetaItemPreview {
        id: id.to_owned(),
        r#type: kind.to_owned(),
        name: format!("Title {id}"),
        poster: None,
        background: with_bg.then(|| Url::parse(&format!("https://img.example/{id}.jpg")).unwrap()),
        logo: with_bg.then(|| Url::parse(&format!("https://img.example/{id}-logo.png")).unwrap()),
        description: Some("Desc".into()),
        release_info: Some("2025".into()),
        runtime: Some("120 min".into()),
        released: None,
        poster_shape: Default::default(),
        links: vec![],
        trailer_streams: vec![],
        behavior_hints: Default::default(),
    }
}

fn catalog_page(
    kind: &str,
    id: &str,
    items: Vec<MetaItemPreview>,
) -> Vec<ResourceLoadable<Vec<MetaItemPreview>>> {
    vec![ResourceLoadable {
        request: ResourceRequest {
            base: Url::from_str("https://addon.example/manifest.json").unwrap(),
            path: ResourcePath::without_extra("catalog", kind, id),
        },
        content: Some(Loadable::Ready(items)),
    }]
}

#[test]
fn spotlight_interleaves_up_to_six_unique_items_and_optionally_hides_rows() {
    let (rt, _events) = runtime(Profile::default());
    let mut model = rt.model().unwrap().clone();
    model.board.catalogs = vec![
        catalog_page(
            "movie",
            "top",
            vec![
                preview("m1", "movie", true),
                preview("m_nobg", "movie", false),
                preview("shared", "movie", true),
                preview("m2", "movie", true),
                preview("m3", "movie", true),
            ],
        ),
        catalog_page(
            "series",
            "top",
            vec![
                preview("s1", "series", true),
                preview("shared", "series", true),
                preview("s2", "series", true),
                preview("s3", "series", true),
            ],
        ),
    ];

    let mut extras = LocalExtras {
        spotlight: SpotlightConfig {
            enabled: true,
            catalogs: vec![],
            show_catalog_rows: false,
        },
        ..Default::default()
    };
    let (_, rows, slides, loading) = board::data_with_extras(&model, &extras);
    assert!(!loading);
    assert_eq!(slides.len(), SPOTLIGHT_ITEMS_COUNT);
    assert_eq!(slides[0].id, "m1");
    assert_eq!(slides[1].id, "s1");
    assert_eq!(slides[2].id, "shared");
    // Duplicate `shared` skipped in series pass -> next is s2
    assert_eq!(slides[3].id, "s2");
    assert!(
        rows.is_empty(),
        "show_catalog_rows=false should hide picked catalog rows"
    );

    let view = spotlight::settings_view(&model, &extras);
    assert!(view.enabled);
    assert!(view.is_default);
    assert_eq!(view.picks_count, 2);
    assert_eq!(view.catalogs.len(), 2);

    extras.spotlight.show_catalog_rows = true;
    let (_, rows_shown, _, _) = board::data_with_extras(&model, &extras);
    assert_eq!(rows_shown.len(), 2);

    let imgs =
        board::visible_images_with_spotlight(&rows_shown, &slides, board::Viewport::default());
    assert!(imgs.iter().any(|u| u.contains("m1.jpg#page")));
    assert!(imgs.iter().any(|u| u.contains("m1-logo.png#logo")));
}

#[test]
fn group_catalogs_filters_and_groups_by_addon() {
    use super::spotlight::SpotlightCatalogOption;
    use super::spotlight_settings::group_catalogs;

    let cats = vec![
        SpotlightCatalogOption {
            transport_url: "https://cinemeta.strem.io/manifest.json".into(),
            kind: "movie".into(),
            id: "top".into(),
            name: "Popular - Movie".into(),
            addon_name: "Cinemeta".into(),
            ..Default::default()
        },
        SpotlightCatalogOption {
            transport_url: "https://cinemeta.strem.io/manifest.json".into(),
            kind: "series".into(),
            id: "top".into(),
            name: "Popular - Series".into(),
            addon_name: "Cinemeta".into(),
            ..Default::default()
        },
        SpotlightCatalogOption {
            transport_url: "https://kitsu.example/manifest.json".into(),
            kind: "anime".into(),
            id: "trending".into(),
            name: "Trending - Anime".into(),
            addon_name: "Kitsu".into(),
            ..Default::default()
        },
    ];
    let all = group_catalogs(&cats, "");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].addon_name, "Cinemeta");
    assert_eq!(all[0].catalogs.len(), 2);
    assert_eq!(all[1].addon_name, "Kitsu");
    assert_eq!(all[1].catalogs.len(), 1);

    let filtered = group_catalogs(&cats, "kitsu");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].catalogs[0].0, 2);

    let empty = group_catalogs(&cats, "nonexistent");
    assert!(empty.is_empty());
}

#[test]
fn spotlight_reports_loading_until_catalogs_arrive_and_resolves_trailer_urls() {
    use stremio_core::types::resource::{Stream, StreamSource};

    let (rt, _events) = runtime(Profile::default());
    let mut model = rt.model().unwrap().clone();
    model.board.catalogs = vec![vec![ResourceLoadable {
        request: ResourceRequest {
            base: Url::from_str("https://addon.example/manifest.json").unwrap(),
            path: ResourcePath::without_extra("catalog", "movie", "top"),
        },
        content: Some(Loadable::Loading),
    }]];
    let extras = LocalExtras {
        spotlight: SpotlightConfig {
            enabled: true,
            catalogs: vec![],
            show_catalog_rows: false,
        },
        ..Default::default()
    };
    let (_, _, slides, loading) = board::data_with_extras(&model, &extras);
    assert!(slides.is_empty());
    assert!(loading);

    let yt = Stream {
        source: StreamSource::YouTube {
            yt_id: "dQw4w9WgXcQ".into(),
        },
        name: None,
        description: None,
        thumbnail: None,
        subtitles: vec![],
        behavior_hints: Default::default(),
    };
    assert_eq!(
        spotlight::trailer_url_of(&[yt]),
        Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ".to_owned())
    );
    let bad_yt = Stream {
        source: StreamSource::YouTube {
            yt_id: "abc&list=evil".into(),
        },
        name: None,
        description: None,
        thumbnail: None,
        subtitles: vec![],
        behavior_hints: Default::default(),
    };
    assert_eq!(spotlight::trailer_url_of(&[bad_yt]), None);
    assert_eq!(spotlight::trailer_url_of(&[]), None);
}
