use std::str::FromStr;

use stremio_core::models::common::{Loadable, ResourceLoadable};
use stremio_core::types::addon::{ResourcePath, ResourceRequest};
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::{MetaItemPreview, Stream, StreamSource};
use url::Url;

use super::content::{self, catalog_key};
use super::content_filter::{
    delete_preset, move_catalog, parse_tags, reset_catalogs, set_active_preset, stream_matches,
    tag_matches, toggle_board_catalog, toggle_stream_addon, upsert_preset,
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
        subtitles: vec![],
        behavior_hints: Default::default(),
    }
}

fn cat_page(kind: &str, id: &str, title: &str) -> Vec<ResourceLoadable<Vec<MetaItemPreview>>> {
    vec![ResourceLoadable {
        request: ResourceRequest {
            base: Url::from_str("https://addon.example/manifest.json").unwrap(),
            path: ResourcePath::without_extra("catalog", kind, id),
        },
        content: Some(Loadable::Ready(vec![MetaItemPreview {
            id: format!("{kind}-{id}"),
            r#type: kind.to_owned(),
            name: title.to_owned(),
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
        }])),
    }]
}

#[test]
fn tag_matching_respects_word_boundaries_for_ascii_tags_and_substrings_for_symbols() {
    assert!(tag_matches("torrentio 4k hdr remux", "4K"));
    assert!(!tag_matches("torrentio 14k bitrate", "4K"));
    assert!(!tag_matches("audio dts-hd ma subtitles", "ts"));
    assert!(tag_matches("camrip ts 720p", "TS"));
    assert!(!tag_matches("advanced audio dvdr", "dv"));
    assert!(tag_matches("2160p dv hdr10", "DV"));
    assert!(tag_matches("movie dolby.vision.hdr", "dolby vision"));
    assert!(tag_matches("[rd+] 5.1 english", "RD+"));
    assert!(tag_matches("[rd+] 5.1 english", "5.1"));
    assert_eq!(
        parse_tags(" 4K, 2160p, 4k , , HDR "),
        vec!["4K", "2160p", "HDR"]
    );
}

#[test]
fn stream_filter_applies_disabled_addons_and_preset_include_exclude_rules() {
    let mut cfg = ContentSettings::default();
    let addon_a = "https://a.example/manifest.json";
    let addon_b = "https://b.example/manifest.json";
    let s_4k = make_stream("Addon 4K", "2160p HDR10");
    let s_cam = make_stream("Addon 4K", "CAM TS rip");
    let s_1080 = make_stream("Addon 1080p", "DTS-HD");

    assert!(upsert_preset(
        &mut cfg,
        "",
        "4K Clean",
        "4K, 2160p",
        "CAM, TS"
    ));
    let pid = cfg.stream_presets[0].id.clone();
    set_active_preset(&mut cfg, &pid);
    toggle_stream_addon(&mut cfg, addon_b, false);

    assert!(stream_matches(addon_a, &s_4k, &cfg));
    assert!(!stream_matches(addon_a, &s_cam, &cfg));
    assert!(!stream_matches(addon_a, &s_1080, &cfg));
    assert!(!stream_matches(addon_b, &s_4k, &cfg));

    delete_preset(&mut cfg, &pid);
    assert!(cfg.active_stream_preset.is_none());
    assert!(stream_matches(addon_a, &s_1080, &cfg));
}

#[test]
fn board_catalog_reordering_and_hiding_persists_and_resets() {
    let (rt, _events) = runtime(Profile::default());
    let mut model = rt.model().unwrap().clone();
    model.board.catalogs = vec![
        cat_page("movie", "top", "Movie 1"),
        cat_page("series", "top", "Series 1"),
        cat_page("movie", "featured", "Movie 2"),
    ];
    let keys: Vec<CatalogKey> = model
        .board
        .catalogs
        .iter()
        .map(|c| catalog_key(&c[0].request))
        .collect();

    let mut extras = LocalExtras::default();
    assert!(move_catalog(&mut extras.content, &keys, 2, -1));
    toggle_board_catalog(&mut extras.content, keys[0].clone());

    let (_, rows, _) = board::data_with_extras(&model, &extras);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].cards[0].title, "Movie 2");
    assert_eq!(rows[1].cards[0].title, "Series 1");

    reset_catalogs(&mut extras.content);
    let (_, rows_reset, _) = board::data_with_extras(&model, &extras);
    assert_eq!(rows_reset.len(), 3);
    assert_eq!(rows_reset[0].cards[0].title, "Movie 1");
}

#[test]
fn details_and_visible_streams_share_exact_filtered_indices_and_flag_filtered_out() {
    let (rt, _events) = runtime(Profile::default());
    let mut model = rt.model().unwrap().clone();
    let base_a = Url::parse("https://addon-a.example/manifest.json").unwrap();
    let base_b = Url::parse("https://addon-b.example/manifest.json").unwrap();
    model.meta_details.streams = vec![
        ResourceLoadable {
            request: ResourceRequest {
                base: base_a.clone(),
                path: ResourcePath::without_extra("stream", "movie", "tt1"),
            },
            content: Some(Loadable::Ready(vec![
                make_stream("Low CAM", "CAM"),
                make_stream("Best 4K", "2160p HDR"),
            ])),
        },
        ResourceLoadable {
            request: ResourceRequest {
                base: base_b.clone(),
                path: ResourcePath::without_extra("stream", "movie", "tt1"),
            },
            content: Some(Loadable::Ready(vec![make_stream("Hidden Addon 4K", "4K")])),
        },
    ];

    let mut extras = LocalExtras::default();
    toggle_stream_addon(&mut extras.content, base_b.as_str(), false);
    assert!(upsert_preset(&mut extras.content, "", "4K", "4K", "CAM"));
    let pid = extras.content.stream_presets[0].id.clone();
    set_active_preset(&mut extras.content, &pid);

    let data = details::data_with_extras(&model, &extras, None, None, None, "");
    assert_eq!(data.streams.len(), 1);
    assert_eq!(data.streams[0].name, "Best 4K");
    assert!(!data.streams_filtered_out);

    let visible: Vec<_> = details::visible_streams(&model, &extras.content).collect();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].1.name.as_deref(), Some("Best 4K"));

    // Disabling addon A as well hides all streams -> streams_filtered_out = true.
    toggle_stream_addon(&mut extras.content, base_a.as_str(), false);
    let empty = details::data_with_extras(&model, &extras, None, None, None, "");
    assert!(empty.streams.is_empty());
    assert!(empty.streams_filtered_out);

    // LocalExtras without `"content"` key still deserializes cleanly.
    let legacy: LocalExtras = serde_json::from_str(r#"{"wrap_streams":true}"#).unwrap();
    assert!(legacy.wrap_streams);
    assert!(legacy.content.stream_presets.is_empty());
    let _ = content::project(&model, &legacy);
}
