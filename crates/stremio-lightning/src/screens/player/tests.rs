use super::*;
use stremio_core::types::resource::{Stream, StreamSource};

#[test]
fn torrent_and_plain_url_resolution_and_season_index() {
    let server = url::Url::parse("http://127.0.0.1:11470/").unwrap();
    let torrent = Stream {
        source: StreamSource::Torrent {
            info_hash: [0xab; 20],
            file_idx: None,
            announce: vec![],
            file_must_include: vec![],
        },
        name: None,
        description: None,
        thumbnail: None,
        subtitles: vec![],
        behavior_hints: Default::default(),
    };
    let urls = StreamUrls::new(torrent.convert(Some(&server)).unwrap(), Some(&server));
    let parsed = url::Url::parse(&address(&urls).unwrap()).unwrap();
    assert_eq!(
        (parsed.host_str(), parsed.port()),
        (Some("127.0.0.1"), Some(11470))
    );
    assert_eq!(parsed.path(), format!("/{}/-1", "ab".repeat(20)));

    let plain = Stream {
        source: StreamSource::Url {
            url: url::Url::parse("https://example.com/movie.mp4").unwrap(),
        },
        name: None,
        description: None,
        thumbnail: None,
        subtitles: vec![],
        behavior_hints: Default::default(),
    };
    let urls = StreamUrls::new(plain.convert(Some(&server)).unwrap(), Some(&server));
    assert_eq!(
        address(&urls).as_deref(),
        Some("https://example.com/movie.mp4")
    );

    let seasons = vec![
        Season {
            number: 0,
            label: "Special".into(),
        },
        Season {
            number: 1,
            label: "Season 1".into(),
        },
    ];
    assert_eq!(season_index(&seasons, None, 1), 1);
    assert_eq!(season_index(&seasons, Some(0), 0), 0);
    assert_eq!(season_index(&seasons, Some(9), 1), 1);
}
