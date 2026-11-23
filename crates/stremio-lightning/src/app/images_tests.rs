use std::sync::Arc;

use crate::app::{ActiveScreen, BoardData, Snapshot};
use crate::screens::{board, details, discover, player, Card, Preview};

use super::{shown_images, snapshot_images};

fn card(poster: &str) -> Card {
    Card {
        id: poster.to_owned(),
        kind: "movie".to_owned(),
        title: poster.to_owned(),
        poster: Some(poster.to_owned()),
        progress: 0.0,
        watched: false,
        playable: false,
    }
}

fn preview(poster: &str) -> Preview {
    Preview {
        logo: Some(format!("{poster}#logo")),
        background: Some(format!("{poster}#blur")),
        ..Preview::default()
    }
}

fn posters(count: usize) -> Vec<Card> {
    (1..=count)
        .map(|index| card(&format!("https://example.com/card-{index}")))
        .collect()
}

fn snapshot() -> Snapshot {
    Snapshot {
        board: Arc::new(BoardData {
            status: String::new(),
            rows: vec![board::Row {
                title: "Popular - Movie".to_owned(),
                shape: "poster",
                cards: vec![
                    card("https://example.com/board-1"),
                    card("https://example.com/board-2"),
                ],
                menu: false,
            }],
        }),
        discover: Arc::new(discover::Data {
            shape: "poster",
            cards: posters(3),
            inputs: Vec::new(),
            previews: (1..=3)
                .map(|index| preview(&format!("https://example.com/card-{index}")))
                .collect(),
        }),
        ..Snapshot::default()
    }
}

#[test]
fn only_the_active_screen_pins_its_images() {
    let snapshot = snapshot();
    let board_shown = shown_images(
        &snapshot,
        ActiveScreen::Board,
        board::Viewport::default(),
        (0, 2),
        0,
    );
    assert!(board_shown.contains(&"https://example.com/board-1".to_owned()));
    assert!(board_shown.contains(&"https://example.com/board-2".to_owned()));
    assert!(!board_shown.contains(&"https://example.com/card-1".to_owned()));

    let discover_shown = shown_images(
        &snapshot,
        ActiveScreen::Discover,
        board::Viewport::default(),
        (0, 2),
        0,
    );
    assert!(!discover_shown.contains(&"https://example.com/board-1".to_owned()));
    assert!(discover_shown.contains(&"https://example.com/card-1".to_owned()));
}

#[test]
fn the_screenful_on_display_is_shown_and_the_rest_of_the_catalog_is_not() {
    let snapshot = snapshot();
    let shown = shown_images(
        &snapshot,
        ActiveScreen::Discover,
        board::Viewport::default(),
        (0, 2),
        0,
    );
    assert!(shown.contains(&"https://example.com/card-1".to_owned()));
    assert!(shown.contains(&"https://example.com/card-2".to_owned()));
    assert!(!shown.contains(&"https://example.com/card-3".to_owned()));
    assert!(shown.contains(&"https://example.com/card-1#logo".to_owned()));
    assert!(shown.contains(&"https://example.com/card-1#blur".to_owned()));
    assert!(!shown.contains(&"https://example.com/card-2#logo".to_owned()));
}

#[test]
fn what_is_asked_for_ahead_of_being_seen_is_asked_for_but_not_held() {
    let mut snapshot = snapshot();
    Arc::make_mut(&mut snapshot.player).open = true;
    Arc::make_mut(&mut snapshot.player).every_thumbnail =
        vec!["https://example.com/season-4.jpg".to_owned()];
    assert!(snapshot_images(
        &snapshot,
        ActiveScreen::Player,
        board::Viewport::default(),
        (0, 3),
        0,
    )
    .contains(&"https://example.com/season-4.jpg".to_owned()));
    assert!(!shown_images(
        &snapshot,
        ActiveScreen::Player,
        board::Viewport::default(),
        (0, 3),
        0,
    )
    .contains(&"https://example.com/season-4.jpg".to_owned()));
}

#[test]
fn the_details_column_is_held_until_the_player_covers_it() {
    let mut snapshot = snapshot();
    {
        let d = Arc::make_mut(&mut snapshot.details);
        d.preview = Some(Preview {
            logo: Some("https://example.com/art#logo".to_owned()),
            background: Some("https://example.com/art#page".to_owned()),
            ..Preview::default()
        });
        d.episodes = vec![details::Episode {
            thumbnail: Some("https://example.com/episode.jpg".to_owned()),
            ..details::Episode::default()
        }];
    }
    let shown = shown_images(
        &snapshot,
        ActiveScreen::Details,
        board::Viewport::default(),
        (0, 2),
        0,
    );
    assert!(shown.contains(&"https://example.com/art#logo".to_owned()));
    assert!(shown.contains(&"https://example.com/art#page".to_owned()));
    assert!(shown.contains(&"https://example.com/art#backdrop".to_owned()));
    assert!(shown.contains(&"https://example.com/episode.jpg".to_owned()));

    {
        let p = Arc::make_mut(&mut snapshot.player);
        p.open = true;
        p.background = Some("https://example.com/wide#blur".to_owned());
        p.episodes = vec![player::Episode {
            thumbnail: Some("https://example.com/drawer.jpg".to_owned()),
            ..player::Episode::default()
        }];
    }
    let shown = shown_images(
        &snapshot,
        ActiveScreen::Details,
        board::Viewport::default(),
        (0, 2),
        0,
    );
    assert!(!shown.contains(&"https://example.com/episode.jpg".to_owned()));
    assert!(!shown.contains(&"https://example.com/art#page".to_owned()));
    assert!(shown.contains(&"https://example.com/wide#blur".to_owned()));
    assert!(shown.contains(&"https://example.com/drawer.jpg".to_owned()));
}
