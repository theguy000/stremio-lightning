use std::str::FromStr;

use stremio_core::models::common::{Loadable, ResourceLoadable};
use stremio_core::types::addon::{ResourcePath, ResourceRequest};
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::{MetaItemPreview, Stream, StreamSource};
use url::Url;

use super::content::{catalog_item_id, catalog_key};
use super::content_filter::{
    create_preset, delete_preset, move_item, normalize_tags, set_item_active, set_item_title,
    stream_matches, tag_matches, toggle_active_preset,
};
use crate::app::build::runtime;
use crate::plumbing::local_settings::{CatalogKey, ContentSettings, LocalExtras};
use crate::screens::{board, details};

fn make_stream(name: &str, desc: &str) -> Stream {
    Stream {
        source: StreamSource::Url {
            url: Url::parse("https://video.example/movie.mp4").unwrap(),
        },
        name: Some(name.to_owned()),
        description: Some(desc.to_owned()),
        thumbnail: None,
        subtitles: Vec::new(),
        behavior_hints: Default::default(),
    }
}

#[test]
fn normalize_tags_and_word_boundary_matching_behave_cleanly() {
    assert_eq!(
        normalize_tags([" 4K ", "4k", "HDR", "", "5.1"]),
        vec!["4K", "HDR", "5.1"]
    );
    assert!(tag_matches("torrentio 2160p 4k hdr", "4K"));
    assert!(!tag_matches("torrentio 14k bitrate", "4K"));
    assert!(!tag_matches("movie.1080p.dts.5.1.subtitles", "TS"));
    assert!(tag_matches("movie.2026.ts.rip", "TS"));
    assert!(tag_matches("movie.dts.5.1.rd+", "5.1"));
    assert!(tag_matches("movie.dts.5.1.rd+", "RD+"));
}

#[test]
fn stream_filter_respects_disabled_addons_order_and_toggleable_presets() {
    let mut cfg = ContentSettings::default();
    let s4k = make_stream("Torrentio 4K", "2160p HDR10");
    let scam = make_stream("Torrentio CAM", "2160p CAM");
    let s1080 = make_stream("Comet 1080p", "1080p BluRay");

    assert!(!create_preset(&mut cfg, "   ", &["4k".into()], &[]));
    assert!(!create_preset(&mut cfg, "Empty", &[], &[]));
    assert!(create_preset(
        &mut cfg,
        "4K Clean",
        &["4k".into(), "2160p".into()],
        &["cam".into()]
    ));
    let id = cfg.stream_presets[0].id.clone();
    toggle_active_preset(&mut cfg, &id);
    assert_eq!(cfg.active_stream_preset.as_deref(), Some(id.as_str()));

    assert!(stream_matches("https://a.test/manifest.json", &s4k, &cfg));
    assert!(!stream_matches("https://a.test/manifest.json", &scam, &cfg));
    assert!(!stream_matches(
        "https://b.test/manifest.json",
        &s1080,
        &cfg
    ));

    set_item_active(&mut cfg, "streams|https://a.test/manifest.json", false);
    assert!(!stream_matches("https://a.test/manifest.json", &s4k, &cfg));

    toggle_active_preset(&mut cfg, &id);
    assert_eq!(cfg.active_stream_preset, None);
    toggle_active_preset(&mut cfg, &id);
    delete_preset(&mut cfg, &id);
    assert_eq!(cfg.active_stream_preset, None);
}

#[test]
fn board_catalogs_respect_drag_order_hide_and_custom_titles() {
    let (rt, _events) = runtime(Profile::default());
    let mut model = rt.model().unwrap().clone();
    let req_a = ResourceRequest {
        base: Url::from_str("https://addon.a/manifest.json").unwrap(),
        path: ResourcePath::without_extra("catalog", "movie", "top"),
    };
    let req_b = ResourceRequest {
        base: Url::from_str("https://addon.b/manifest.json").unwrap(),
        path: ResourcePath::without_extra("catalog", "series", "featured"),
    };
    let item = |id: &str| MetaItemPreview {
        id: id.to_owned(),
        r#type: "movie".to_owned(),
        name: id.to_owned(),
        poster: None,
        background: None,
        logo: None,
        description: None,
        release_info: None,
        runtime: None,
        released: None,
        poster_shape: Default::default(),
        links: vec![],
        trailer_streams: vec![],
        behavior_hints: Default::default(),
    };
    model.board.catalogs = vec![
        vec![ResourceLoadable {
            request: req_a.clone(),
            content: Some(Loadable::Ready(vec![item("tt1")])),
        }],
        vec![ResourceLoadable {
            request: req_b.clone(),
            content: Some(Loadable::Ready(vec![item("tt2")])),
        }],
    ];

    let id_a = catalog_item_id("board", &catalog_key(&req_a));
    let id_b = catalog_item_id("board", &catalog_key(&req_b));
    let mut extras = LocalExtras::default();
    assert!(move_item(
        &mut extras.content,
        "board",
        &[id_a.clone(), id_b.clone()],
        0,
        1
    ));
    set_item_title(&mut extras.content, &id_b, Some("My Picks"));

    let (_, rows, _) = board::data_with_extras(&model, &extras);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].title, "My Picks");
    assert_eq!(rows[0].cards[0].id, "tt2");
    assert_eq!(rows[1].cards[0].id, "tt1");

    set_item_title(&mut extras.content, &id_b, None);
    set_item_active(&mut extras.content, &id_b, false);
    let (_, hidden_rows, _) = board::data_with_extras(&model, &extras);
    assert_eq!(hidden_rows.len(), 1);
    assert_eq!(hidden_rows[0].cards[0].id, "tt1");
}

#[test]
fn details_streams_respect_stream_order_and_filtered_out_flag() {
    let (rt, _events) = runtime(Profile::default());
    let mut model = rt.model().unwrap().clone();
    let req_a = ResourceRequest {
        base: Url::from_str("https://a.test/manifest.json").unwrap(),
        path: ResourcePath::without_extra("stream", "movie", "tt1"),
    };
    let req_b = ResourceRequest {
        base: Url::from_str("https://b.test/manifest.json").unwrap(),
        path: ResourcePath::without_extra("stream", "movie", "tt1"),
    };
    model.meta_details.streams = vec![
        ResourceLoadable {
            request: req_a.clone(),
            content: Some(Loadable::Ready(vec![make_stream("A 1080p", "1080p")])),
        },
        ResourceLoadable {
            request: req_b.clone(),
            content: Some(Loadable::Ready(vec![make_stream("B 4K", "2160p")])),
        },
    ];

    let mut extras = LocalExtras::default();
    let ids = vec![
        format!("streams|{}", req_a.base),
        format!("streams|{}", req_b.base),
    ];
    assert!(move_item(&mut extras.content, "streams", &ids, 1, 0));
    let ordered = details::data_with_extras(&model, &extras, None, None, None, "");
    assert_eq!(ordered.streams[0].name, "B 4K");
    assert_eq!(ordered.streams[1].name, "A 1080p");

    assert!(create_preset(
        &mut extras.content,
        "Only 8K",
        &["8k".into()],
        &[]
    ));
    let pid = extras.content.stream_presets[0].id.clone();
    toggle_active_preset(&mut extras.content, &pid);
    let empty = details::data_with_extras(&model, &extras, None, None, None, "");
    assert!(empty.streams.is_empty());
    assert!(empty.streams_filtered_out);
}

#[test]
fn legacy_settings_json_without_content_key_loads_default() {
    let extras: LocalExtras = serde_json::from_str(
        r#"{"overrides":{},"spotlight":{"enabled":true,"catalogs":[{"transportUrl":"https://a","type":"movie","id":"top"}]}}"#,
    )
    .unwrap();
    assert_eq!(extras.content, ContentSettings::default());
    assert_eq!(
        extras.spotlight.catalogs[0],
        CatalogKey {
            transport_url: "https://a".into(),
            r#type: "movie".into(),
            id: "top".into(),
        }
    );
}
