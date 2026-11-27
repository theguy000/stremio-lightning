use super::*;

use super::*;

fn selected_with(source: StreamSource) -> PlayerSelected {
    PlayerSelected {
        stream: Stream {
            source,
            name: None,
            description: None,
            thumbnail: None,
            subtitles: vec![],
            behavior_hints: Default::default(),
        },
        stream_request: None,
        meta_request: None,
        subtitles_path: None,
    }
}

#[test]
fn classify_routes_external_links_external_players_and_internal_playback() {
    let ext = selected_with(StreamSource::External {
        external_url: Some(Url::parse("https://example.com/watch").unwrap()),
        android_tv_url: None,
        tizen_url: None,
        webos_url: None,
    });
    assert_eq!(
        classify_selected(ext, Some("vlc"), None),
        Ok(OpenOutcome::OpenExternal(
            "https://example.com/watch".into()
        ))
    );

    let playable = || {
        selected_with(StreamSource::Url {
            url: Url::parse("https://example.com/movie.mp4").unwrap(),
        })
    };
    assert!(matches!(
        classify_selected(playable(), None, None),
        Ok(OpenOutcome::Load(_))
    ));
    assert_eq!(
        classify_selected(playable(), Some("vlc"), None),
        Ok(OpenOutcome::ExternalPlayer(ExternalLaunch {
            kind: ExternalPlayerKind::Vlc,
            url: "https://example.com/movie.mp4".into(),
        }))
    );
    assert_eq!(
        classify_selected(playable(), Some("mpv"), None),
        Ok(OpenOutcome::ExternalPlayer(ExternalLaunch {
            kind: ExternalPlayerKind::Mpv,
            url: "https://example.com/movie.mp4".into(),
        }))
    );
    let torrent = selected_with(StreamSource::Torrent {
        info_hash: [0xab; 20],
        file_idx: Some(0),
        announce: vec![],
        file_must_include: vec![],
    });
    assert!(classify_selected(torrent.clone(), Some("m3u"), None).is_err());
    let server = Url::parse("http://127.0.0.1:11470/").unwrap();
    let res = classify_selected(torrent, Some("m3u"), Some(&server)).unwrap();
    assert!(matches!(
        res,
        OpenOutcome::ExternalPlayer(ExternalLaunch {
            kind: ExternalPlayerKind::M3u,
            ref url,
        }) if url.starts_with("http://127.0.0.1:11470/")
    ));
}
