//! Discover catalog filtering/fallback and Details/Player stream-filtering parity tests.

use stremio_core::models::catalog_with_filters::{
    SelectableCatalog, SelectableType, Selected as DiscoverSelected,
};
use stremio_core::models::common::{Loadable, ResourceLoadable};
use stremio_core::types::addon::{
    Descriptor, Manifest, ManifestCatalog, ManifestResource, ResourcePath, ResourceRequest,
};
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::{Stream, StreamSource};
use url::Url;

use super::content::{self, catalog_key};
use super::content_filter::{set_active_preset, toggle_stream_addon, upsert_preset};
use crate::app::build::runtime;
use crate::plumbing::local_settings::LocalExtras;
use crate::screens::{details, discover, player};

fn make_stream(name: &str, description: &str) -> Stream {
    Stream {
        source: StreamSource::Url {
            url: Url::parse("https://example.com/v.mp4").unwrap(),
        },
        name: Some(name.into()),
        description: Some(description.into()),
        thumbnail: None,
        subtitles: vec![],
        behavior_hints: Default::default(),
    }
}

#[test]
fn discover_hiding_fallback_and_type_rewrite_work() {
    let (rt, _events) = runtime(Profile::default());
    let mut model = rt.model().unwrap().clone();
    let base = Url::parse("https://addon.example/manifest.json").unwrap();
    let req_m1 = ResourceRequest {
        base: base.clone(),
        path: ResourcePath::without_extra("catalog", "movie", "top"),
    };
    let req_m2 = ResourceRequest {
        base: base.clone(),
        path: ResourcePath::without_extra("catalog", "movie", "featured"),
    };
    let req_s1 = ResourceRequest {
        base: base.clone(),
        path: ResourcePath::without_extra("catalog", "series", "top"),
    };
    let cat = |id: &str, ty: &str, name: &str| ManifestCatalog {
        id: id.into(),
        r#type: ty.into(),
        name: Some(name.into()),
        extra: Default::default(),
    };
    model.ctx.profile.addons = vec![Descriptor {
        transport_url: base,
        manifest: Manifest {
            id: "addon".into(),
            version: serde_json::from_str("\"1.0.0\"").unwrap(),
            name: "Demo".into(),
            contact_email: None,
            description: None,
            logo: None,
            background: None,
            types: vec!["movie".into(), "series".into()],
            resources: vec![ManifestResource::Short("stream".into())],
            id_prefixes: None,
            catalogs: vec![
                cat("top", "movie", "Top"),
                cat("featured", "movie", "Featured"),
                cat("top", "series", "Series Top"),
            ],
            addon_catalogs: vec![],
            behavior_hints: Default::default(),
        },
        flags: Default::default(),
    }];
    model.discover.selected = Some(DiscoverSelected {
        request: req_m1.clone(),
    });
    model.discover.selectable.types = vec![
        SelectableType {
            r#type: "movie".into(),
            selected: true,
            request: req_m1.clone(),
        },
        SelectableType {
            r#type: "series".into(),
            selected: false,
            request: req_s1.clone(),
        },
    ];
    model.discover.selectable.catalogs = vec![
        SelectableCatalog {
            catalog: "Top".into(),
            selected: true,
            is_epg_guide: false,
            request: req_m1.clone(),
        },
        SelectableCatalog {
            catalog: "Featured".into(),
            selected: false,
            is_epg_guide: false,
            request: req_m2.clone(),
        },
    ];

    let mut extras = LocalExtras::default();
    extras
        .content
        .hidden_discover_catalogs
        .push(catalog_key(&req_m1));
    assert_eq!(
        discover::fallback_request_if_hidden(&model, &extras.content),
        Some(req_m2.clone())
    );
    let data = discover::data_with_extras(&model, &extras);
    assert_eq!(data.inputs[0].values[0], req_m2);
    assert_eq!(data.inputs[1].menu.options.len(), 1);
    assert_eq!(data.inputs[1].menu.options[0].0, "Featured");

    extras
        .content
        .hidden_discover_catalogs
        .push(catalog_key(&req_m2));
    assert_eq!(
        discover::fallback_request_if_hidden(&model, &extras.content),
        Some(req_s1)
    );
    let all_hidden = discover::data_with_extras(&model, &extras);
    assert_eq!(all_hidden.inputs.len(), 1);
    assert!(all_hidden.cards.is_empty());

    let view = content::project(&model, &extras);
    assert_eq!(view.catalogs.len(), 3);
    assert_eq!(view.stream_addons.len(), 1);
    assert!(!super::search::search("presets", false, false).is_empty());
}

#[test]
fn disabled_stream_addons_excluded_from_loading_and_player_index_matches() {
    let (rt, _events) = runtime(Profile::default());
    let mut model = rt.model().unwrap().clone();
    let base_a = Url::parse("https://a.example/manifest.json").unwrap();
    let base_b = Url::parse("https://b.example/manifest.json").unwrap();
    model.meta_details.streams = vec![
        ResourceLoadable {
            request: ResourceRequest {
                base: base_a.clone(),
                path: ResourcePath::without_extra("stream", "movie", "tt1"),
            },
            content: Some(Loadable::Loading),
        },
        ResourceLoadable {
            request: ResourceRequest {
                base: base_b,
                path: ResourcePath::without_extra("stream", "movie", "tt1"),
            },
            content: Some(Loadable::Ready(vec![
                make_stream("CAM Rip", "CAM"),
                make_stream("4K Clean", "2160p HDR"),
            ])),
        },
    ];

    let mut extras = LocalExtras::default();
    toggle_stream_addon(&mut extras.content, base_a.as_str(), false);
    assert!(upsert_preset(&mut extras.content, "", "Clean", "", "CAM"));
    let pid = extras.content.stream_presets[0].id.clone();
    set_active_preset(&mut extras.content, &pid);

    let det = details::data_with_extras(&model, &extras, None, None, None, "");
    assert_eq!(det.sources_loading, 0);
    assert_eq!(det.streams.len(), 1);
    assert_eq!(det.streams[0].name, "4K Clean");
    assert!(!upsert_preset(&mut extras.content, &pid, "   ", "", ""));
    assert!(upsert_preset(
        &mut extras.content,
        &pid,
        "Ultra",
        "4K",
        "CAM"
    ));
    assert_eq!(extras.content.stream_presets[0].name, "Ultra");

    let vis = details::visible_streams(&model, &extras.content)
        .next()
        .unwrap();
    assert_eq!(vis.1.name.as_deref(), Some("4K Clean"));
    assert!(player::open(&rt, &extras.content, 0).is_err());
}
