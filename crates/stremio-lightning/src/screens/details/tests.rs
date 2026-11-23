use super::*;
use stremio_core::types::resource::{MetaItemPreview, PosterShape, SeriesInfo};

fn aired(id: &str, season: u32, episode: u32) -> Video {
    Video {
        id: id.to_owned(),
        title: "An episode".to_owned(),
        released: Some(chrono::Utc::now() - chrono::Duration::days(400)),
        overview: None,
        thumbnail: None,
        streams: vec![],
        series_info: Some(SeriesInfo { season, episode }),
        epg_info: None,
        trailer_streams: vec![],
    }
}

fn meta_of(videos: Vec<Video>) -> MetaItem {
    MetaItem {
        preview: MetaItemPreview {
            id: "tt1".into(),
            r#type: "series".into(),
            name: "A series".into(),
            poster: None,
            background: None,
            logo: None,
            description: None,
            release_info: None,
            runtime: None,
            released: None,
            poster_shape: PosterShape::Poster,
            links: vec![],
            trailer_streams: vec![],
            behavior_hints: Default::default(),
        },
        videos,
    }
}

#[test]
fn ratings_share_links_picker_and_next_episode_rules() {
    assert_eq!(next_rating(None, true), Some(Rating::Liked));
    assert_eq!(next_rating(Some(&Rating::Liked), true), None);
    assert_eq!(
        next_rating(Some(&Rating::Liked), false),
        Some(Rating::Loved)
    );
    assert_eq!(
        share_link("Mission: Impossible - Fallout", "movie", "tt4912910"),
        "https://www.strem.io/s/movie/mission-impossible-fallout-4912910"
    );
    let s = seasons(&[aired("a", 1, 1)]);
    assert!(shows_picker(None, &s) && !shows_picker(Some("a"), &s));
    assert!(!shows_picker(None, &[]));

    let now = chrono::Utc::now();
    let meta = meta_of(vec![
        aired("s1e1", 1, 1),
        aired("s1e2", 1, 2),
        aired("s2e1", 2, 1),
    ]);
    assert_eq!(next_episode(&meta, "s1e2", now), Some("s2e1".into()));
    assert_eq!(next_episode(&meta, "s2e1", now), None);

    let with_special = meta_of(vec![aired("s1e1", 1, 1), aired("sp", 0, 1)]);
    assert_eq!(next_episode(&with_special, "s1e1", now), None);

    let mut future = aired("s1e2", 1, 2);
    future.released = Some(now + chrono::Duration::days(30));
    assert_eq!(
        next_episode(&meta_of(vec![aired("s1e1", 1, 1), future]), "s1e1", now),
        None
    );
}
