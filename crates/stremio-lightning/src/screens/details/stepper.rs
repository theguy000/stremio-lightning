//! Empty-Streams Season/Episode stepper state machine (`NumberInput` above "Show Streams").

use stremio_core::types::resource::Video;

use crate::screens::videos::seasons;

/// Season and Episode steppers shown in the empty-Streams message.
/// `ask_again` is false while they still name the Video already on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Steps {
    pub season: u32,
    pub episode: u32,
    pub ask_again: bool,
}

fn episode_numbers(videos: &[Video], season: u32) -> Vec<u32> {
    videos
        .iter()
        .filter_map(|video| video.series_info.as_ref())
        .filter(|info| info.season == season)
        .map(|info| info.episode)
        .collect()
}

/// The pair the steppers name: the picked Video's own Season and Episode, or wherever the user
/// moved them — as long as the title still offers that Episode.
pub fn shown_pair(videos: &[Video], picked: &str, moved: Option<(u32, u32)>) -> Option<(u32, u32)> {
    let info = videos
        .iter()
        .find(|video| video.id == picked)?
        .series_info
        .as_ref()?;
    Some(
        moved
            .filter(|(season, episode)| episode_numbers(videos, *season).contains(episode))
            .unwrap_or((info.season, info.episode)),
    )
}

/// One press of a stepper: moving Season keeps Episode when present (else clamps into range);
/// moving Episode steps across listed numbers so gaps take a single press.
pub fn step(videos: &[Video], at: (u32, u32), seasons_step: i32, episodes_step: i32) -> (u32, u32) {
    let numbers: Vec<u32> = seasons(videos).into_iter().map(|s| s.number).collect();
    let Some(position) = numbers.iter().position(|n| *n == at.0) else {
        return at;
    };
    let moved = (position as i64 + i64::from(seasons_step)).clamp(0, numbers.len() as i64 - 1);
    let season = numbers[moved as usize];
    let listed = episode_numbers(videos, season);
    let (Some(&first), Some(&last)) = (listed.first(), listed.last()) else {
        return (season, at.1);
    };
    let episode = match listed.iter().position(|n| *n == at.1) {
        Some(pos) => {
            let moved = (pos as i64 + i64::from(episodes_step)).clamp(0, listed.len() as i64 - 1);
            listed[moved as usize]
        }
        None => at.1.clamp(first, last),
    };
    (season, episode)
}

/// The Video id named by the steppers, for "Show Streams".
pub fn stepped_video(videos: &[Video], at: (u32, u32)) -> Option<&str> {
    videos
        .iter()
        .find(|v| {
            v.series_info
                .as_ref()
                .is_some_and(|info| (info.season, info.episode) == at)
        })
        .map(|v| v.id.as_str())
}

/// The steppers for the empty-Streams message, or `None` for a title without Seasons.
pub fn steps(videos: &[Video], picked: &str, moved: Option<(u32, u32)>) -> Option<Steps> {
    let shown = shown_pair(videos, picked, moved)?;
    let own = videos
        .iter()
        .find(|v| v.id == picked)?
        .series_info
        .as_ref()?;
    Some(Steps {
        season: shown.0,
        episode: shown.1,
        ask_again: shown != (own.season, own.episode),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::types::resource::SeriesInfo;

    fn video(id: &str, season: u32, episode: u32, title: &str) -> Video {
        Video {
            id: id.to_owned(),
            title: title.to_owned(),
            released: None,
            overview: None,
            thumbnail: None,
            streams: vec![],
            series_info: Some(SeriesInfo { season, episode }),
            epg_info: None,
            trailer_streams: vec![],
        }
    }

    fn three() -> Vec<Video> {
        vec![
            video("a", 1, 1, "Pilot"),
            video("b", 1, 2, "Second"),
            video("c", 2, 1, "Next year"),
        ]
    }

    #[test]
    fn steppers_start_on_picked_and_move_to_valid_episodes() {
        let videos = three();
        let shown = steps(&videos, "b", None).unwrap();
        assert_eq!(
            (shown.season, shown.episode, shown.ask_again),
            (1, 2, false)
        );

        let moved = steps(&videos, "b", Some((2, 1))).unwrap();
        assert_eq!((moved.season, moved.episode, moved.ask_again), (2, 1, true));
        assert_eq!(stepped_video(&videos, (2, 1)), Some("c"));

        let fallback = steps(&videos, "b", Some((9, 9))).unwrap();
        assert_eq!(
            (fallback.season, fallback.episode, fallback.ask_again),
            (1, 2, false)
        );
    }

    #[test]
    fn season_and_episode_steppers_preserve_or_clamp_ranges() {
        let two_seasons = [
            video("a", 1, 2, "Second"),
            video("c", 2, 1, "First"),
            video("d", 2, 2, "Second of two"),
        ];
        assert_eq!(step(&two_seasons, (1, 2), 1, 0), (2, 2));

        let unequal = [video("a", 1, 5, "Fifth"), video("c", 2, 1, "Only one")];
        assert_eq!(step(&unequal, (1, 5), 1, 0), (2, 1));
        assert_eq!(step(&unequal, (2, 1), -1, 0), (1, 5));

        let videos = three();
        assert_eq!(step(&videos, (1, 1), 0, 1), (1, 2));
        assert_eq!(step(&videos, (1, 2), 0, 1), (1, 2));
        assert_eq!(step(&videos, (1, 1), 0, -1), (1, 1));

        let film = [Video {
            series_info: None,
            ..video("m", 0, 0, "A film")
        }];
        assert_eq!(steps(&film, "m", None), None);
        assert_eq!(shown_pair(&film, "m", None), None);
    }
}
