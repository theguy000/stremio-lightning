//! The detail screen: the item's meta and the streams the addons answered with, plus the
//! action that opens an item and the one that plays a row.

use stremio_core::constants::{META_RESOURCE_NAME, STREAM_RESOURCE_NAME};
use stremio_core::models::common::{Loadable, ResourceError};
use stremio_core::models::meta_details::{MetaDetails, Selected as DetailsSelected};
use stremio_core::runtime::msg::{Action, ActionLoad};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::ResourcePath;
use stremio_core::types::resource::{MetaItem, Stream, StreamUrls, Video};
use url::Url;

use crate::screens::Preview;
use crate::{AppModel, AppRuntime};

const MAX_STREAMS: usize = 100;

/// One Season of a series, as the episode picker lists it. `label` is what the bar and the
/// dropdown show: `Season 2`, or `SPECIAL` for season 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Season {
    pub number: u32,
    pub label: String,
}

/// One source in the Streams List: what the addon called it, what it says it is, and the addon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub description: String,
    pub addon: String,
}

/// One Episode row in the picker. `label` is `S2E5`, or `SPECIAL 5` for a Video in season 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Episode {
    pub id: String,
    pub label: String,
    pub title: String,
    /// The release date as the row shows it: `Sep 1, 23`, empty when the title gives none.
    pub released: String,
    pub thumbnail: Option<String>,
}

/// The Seasons a series offers, in the order the title supplied them — never re-sorted here,
/// because the core already put them in order with the Specials last.
pub(crate) fn seasons(videos: &[Video]) -> Vec<Season> {
    let mut listed: Vec<Season> = Vec::new();
    for video in videos {
        let Some(info) = &video.series_info else {
            continue;
        };
        if listed.iter().any(|season| season.number == info.season) {
            continue;
        }
        listed.push(Season {
            number: info.season,
            label: if info.season == 0 {
                "SPECIAL".to_owned()
            } else {
                format!("Season {}", info.season)
            },
        });
    }
    listed
}

/// The Season the picker opens on: the one holding the title's default video, else the first
/// regular Season, else the Specials.
pub(crate) fn default_season(videos: &[Video], default_video_id: Option<&str>) -> Option<u32> {
    let listed = seasons(videos);
    default_video_id
        .and_then(|id| videos.iter().find(|video| video.id == id))
        .and_then(|video| video.series_info.as_ref())
        .map(|info| info.season)
        .or_else(|| {
            listed
                .iter()
                .map(|season| season.number)
                .find(|number| *number != 0)
        })
        .or_else(|| listed.first().map(|season| season.number))
}

/// Web: `released.toLocaleString(..., { year: '2-digit', month: 'short', day: 'numeric' })`.
/// Slint has no locale formatting, so this is the same shape spelled out in English.
fn released_label(date: Option<&chrono::DateTime<chrono::Utc>>) -> String {
    use chrono::Datelike;
    match date {
        Some(date) => format!(
            "{} {}, {}",
            date.format("%b"),
            date.day(),
            date.format("%y")
        ),
        None => String::new(),
    }
}

/// The Episodes of one Season, in the order the title supplied them.
pub(crate) fn episodes(videos: &[Video], season: u32) -> Vec<Episode> {
    videos
        .iter()
        .filter_map(|video| {
            let info = video.series_info.as_ref()?;
            if info.season != season {
                return None;
            }
            Some(Episode {
                id: video.id.clone(),
                label: if info.season == 0 {
                    format!("SPECIAL {}", info.episode)
                } else {
                    format!("S{}E{}", info.season, info.episode)
                },
                title: video.title.clone(),
                released: released_label(video.released.as_ref()),
                thumbnail: video.thumbnail.clone(),
            })
        })
        .collect()
}

/// Whether the Details Screen shows the picker instead of the Streams: the title has Seasons to
/// choose from and nothing has been picked yet.
///
/// Deliberately not "no Streams are loaded": the core guesses a stream path of its own for a
/// series carrying a `default_video_id`, so a stream path says nothing about the user having
/// chosen anything. Only a pick hides the picker.
pub(crate) fn shows_picker(picked: Option<&str>, seasons: &[Season]) -> bool {
    picked.is_none() && !seasons.is_empty()
}

/// Web: the Streams list names the chosen Episode as `S1E2 Title` and offers a way back.
pub(crate) fn picked_label(videos: &[Video], picked: &str) -> Option<String> {
    let video = videos.iter().find(|video| video.id == picked)?;
    let info = video.series_info.as_ref();
    let label = match info {
        Some(info) if info.season == 0 => format!("SPECIAL {}", info.episode),
        Some(info) => format!("S{}E{}", info.season, info.episode),
        None => String::new(),
    };
    Some(match (label.is_empty(), video.title.trim().is_empty()) {
        (true, _) => video.title.clone(),
        (false, true) => label,
        (false, false) => format!("{label} {}", video.title),
    })
}

/// `title` is `None` until the addons have answered the meta request, so the window keeps
/// showing the title of the card that was clicked.
#[derive(Clone, Default)]
pub struct Data {
    /// The title's own panel: logo, runtime, links, description. `None` until the addons answer.
    pub preview: Option<Preview>,
    pub status: String,
    /// The sources on offer, ready to list.
    pub streams: Vec<Source>,
    /// Addons that have not answered yet — the footer counts them while there is something to read.
    pub sources_loading: usize,
    /// True while the screen offers the Episode Picker instead of the Streams.
    pub show_picker: bool,
    /// Every Season the title offers, in the title's own order.
    pub seasons: Vec<Season>,
    /// Which of `seasons` the picker is showing.
    pub season: usize,
    /// The Episodes of that Season.
    pub episodes: Vec<Episode>,
    /// The Video id the user picked, while its Streams are on screen.
    pub picked: Option<String>,
    /// How the Streams list names that Video: `S1E2 The Title`.
    pub picked_label: Option<String>,
    /// The Video the core says follows the picked one, if there is one.
    pub next_episode: Option<String>,
}

/// What the browsed-Season slot holds before the picker has been touched.
pub const NO_SEASON: usize = usize::MAX;

/// The Season the picker was left on, if it has been touched.
pub fn chosen_season(stored: usize) -> Option<u32> {
    if stored == NO_SEASON {
        return None;
    }
    u32::try_from(stored).ok()
}

pub fn data(model: &AppModel, chosen: Option<u32>, picked: Option<&str>) -> Data {
    let details = &model.meta_details;
    let item = details
        .meta_items
        .iter()
        .find_map(|page| match &page.content {
            Some(Loadable::Ready(item)) => Some(item),
            _ => None,
        });

    let videos: &[Video] = item.map(|item| item.videos.as_slice()).unwrap_or_default();
    let listed = seasons(videos);
    let default_video_id =
        item.and_then(|item| item.preview.behavior_hints.default_video_id.as_deref());
    // A Season the picker was left on wins, as long as the title still offers it.
    let shown = chosen
        .filter(|chosen| listed.iter().any(|season| season.number == *chosen))
        .or_else(|| default_season(videos, default_video_id));
    let season = shown
        .and_then(|number| listed.iter().position(|season| season.number == number))
        .unwrap_or_default();
    let season_episodes = shown
        .map(|number| episodes(videos, number))
        .unwrap_or_default();

    let (mut ready, mut loading, mut empty, mut failed) = (0, 0, 0, 0);
    for page in &details.streams {
        match &page.content {
            Some(Loadable::Ready(_)) => ready += 1,
            Some(Loadable::Err(ResourceError::EmptyContent)) => empty += 1,
            Some(Loadable::Err(_)) => failed += 1,
            _ => loading += 1,
        }
    }
    let found = streams_of(details).count();
    let streams = source_rows(model, details)
        .into_iter()
        .take(MAX_STREAMS)
        .collect();

    Data {
        preview: item.map(|item| crate::screens::preview_of(&item.preview)),
        status: format!(
            "{found} streams from {ready} sources, {empty} had nothing, {failed} failed, {loading} loading"
        ),
        streams,
        sources_loading: loading,
        show_picker: shows_picker(picked, &listed),
        picked_label: picked.and_then(|picked| picked_label(videos, picked)),
        next_episode: picked.and_then(|picked| {
            item.and_then(|meta| next_episode(meta, picked, chrono::Utc::now()))
        }),
        picked: picked.map(str::to_owned),
        seasons: listed,
        season,
        episodes: season_episodes,
    }
}

/// The sources the addons have answered with, each with the addon's own name — web shows that in
/// the row's left column instead of a thumbnail.
fn source_rows(model: &AppModel, details: &MetaDetails) -> Vec<Source> {
    details
        .streams
        .iter()
        .filter_map(|page| match &page.content {
            Some(Loadable::Ready(list)) => Some((&page.request.base, list)),
            _ => None,
        })
        .flat_map(|(base, list)| {
            let addon = model
                .ctx
                .profile
                .addons
                .iter()
                .find(|addon| &addon.transport_url == base)
                .map(|addon| addon.manifest.name.clone())
                .unwrap_or_else(|| base.to_string());
            list.iter().map(move |stream| Source {
                name: stream
                    .name
                    .clone()
                    .unwrap_or_else(|| "Stream".to_owned())
                    .replace('\n', " "),
                description: stream
                    .description
                    .clone()
                    .unwrap_or_default()
                    .replace('\n', " | "),
                addon: addon.clone(),
            })
        })
        .collect()
}

/// Every stream the addons have answered with, in the order the details screen lists them, so
/// the index of a clicked row finds its stream again.
fn streams_of(details: &MetaDetails) -> impl Iterator<Item = &Stream> {
    details
        .streams
        .iter()
        .filter_map(|page| match &page.content {
            Some(Loadable::Ready(list)) => Some(list.iter()),
            _ => None,
        })
        .flatten()
}

/// The address the player opens: torrents and archives go through the stream server, plain
/// URLs are played as they are.
pub(crate) fn stream_url(stream: &Stream, server: &Url) -> Result<String, String> {
    let converted = stream
        .convert(Some(server))
        .map_err(|error| error.message())?;
    StreamUrls::new(converted, Some(server))
        .streaming_url
        .map(String::from)
        .ok_or_else(|| "it has no address the player can open".to_owned())
}

/// Series have no stream until an episode is picked, so the core guesses one. Everything else
/// asks the addons for streams of the item itself.
pub(crate) fn open(runtime: &AppRuntime, id: &str, kind: &str) {
    let (stream_path, guess_stream) = if kind == "series" {
        (None, true)
    } else {
        (
            Some(ResourcePath::without_extra(STREAM_RESOURCE_NAME, kind, id)),
            false,
        )
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::MetaDetails(DetailsSelected {
            meta_path: ResourcePath::without_extra(META_RESOURCE_NAME, kind, id),
            stream_path,
            guess_stream,
        })),
    });
}

/// What follows the picked Video, as the core decides it: a season's last Episode leads into the
/// next season, the last of the final season has none, an unreleased Episode is skipped, and a
/// regular Episode never leads into the Specials.
pub(crate) fn next_episode(
    meta: &MetaItem,
    picked: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    meta.next_video(picked, &now).map(|video| video.id.clone())
}

/// Web: picking an Episode asks for its Streams. The Video id is the core's own, so a list that
/// changed between render and click still resolves to the Episode the user pointed at.
pub(crate) fn pick(runtime: &AppRuntime, video_id: &str) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(selected) = model.meta_details.selected.clone() else {
        return;
    };
    let kind = selected.meta_path.r#type.clone();
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::MetaDetails(DetailsSelected {
            meta_path: selected.meta_path,
            stream_path: Some(ResourcePath::without_extra(
                STREAM_RESOURCE_NAME,
                &kind,
                video_id,
            )),
            guess_stream: true,
        })),
    });
}

/// Web: the chevron beside the Episode's name, back to the Episode Picker. Sending no stream path
/// is what clears the Streams the core holds.
pub(crate) fn back_to_episodes(runtime: &AppRuntime) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(selected) = model.meta_details.selected.clone() else {
        return;
    };
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::MetaDetails(DetailsSelected {
            meta_path: selected.meta_path,
            stream_path: None,
            guess_stream: false,
        })),
    });
}

/// The address of the stream at `index`, as of right now.
pub(crate) fn stream_address(runtime: &AppRuntime, index: usize) -> Result<String, String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let stream = streams_of(&model.meta_details)
        .nth(index)
        .ok_or("the list changed, pick it again")?;
    stream_url(stream, &model.ctx.profile.settings.streaming_server_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::types::resource::{SeriesInfo, StreamSource};

    fn stream(source: StreamSource) -> Stream {
        Stream {
            source,
            name: None,
            description: None,
            thumbnail: None,
            subtitles: vec![],
            behavior_hints: Default::default(),
        }
    }

    /// One Video of a series, as the core hands it over: already ordered, Specials last.
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

    fn film(id: &str, title: &str) -> Video {
        Video {
            series_info: None,
            ..video(id, 0, 0, title)
        }
    }

    #[test]
    fn seasons_keep_the_order_the_title_supplied() {
        let videos = [
            video("a", 1, 1, "Pilot"),
            video("b", 1, 2, "Second"),
            video("c", 2, 1, "Next year"),
            video("s", 0, 1, "Bonus"),
        ];

        let listed: Vec<_> = seasons(&videos)
            .into_iter()
            .map(|season| (season.number, season.label))
            .collect();

        assert_eq!(
            listed,
            [
                (1, "Season 1".to_owned()),
                (2, "Season 2".to_owned()),
                (0, "SPECIAL".to_owned()),
            ]
        );
    }

    #[test]
    fn the_picker_opens_on_the_season_holding_the_default_video() {
        let videos = [video("a", 1, 1, "Pilot"), video("b", 2, 1, "Next year")];
        assert_eq!(default_season(&videos, Some("b")), Some(2));
    }

    #[test]
    fn without_a_usable_default_the_picker_opens_on_the_first_regular_season() {
        let videos = [video("s", 0, 1, "Bonus"), video("a", 1, 1, "Pilot")];
        assert_eq!(default_season(&videos, None), Some(1));
        assert_eq!(default_season(&videos, Some("not-in-this-title")), Some(1));
    }

    #[test]
    fn a_title_of_specials_only_opens_on_specials() {
        let videos = [video("s", 0, 1, "Bonus")];
        assert_eq!(default_season(&videos, None), Some(0));
    }

    #[test]
    fn an_episode_row_is_labelled_with_its_season_and_number() {
        let videos = [video("a", 1, 2, "Second")];

        let listed = episodes(&videos, 1);

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "a");
        assert_eq!(listed[0].label, "S1E2");
        assert_eq!(listed[0].title, "Second");
    }

    #[test]
    fn a_special_is_labelled_special_rather_than_season_zero() {
        let videos = [video("s", 0, 3, "Bonus")];
        assert_eq!(episodes(&videos, 0)[0].label, "SPECIAL 3");
    }

    #[test]
    fn a_film_offers_no_seasons_and_so_no_picker() {
        let videos = [film("m", "A film")];

        let seasons = seasons(&videos);

        assert!(seasons.is_empty());
        assert!(!shows_picker(None, &seasons));
    }

    #[test]
    fn a_series_offers_the_picker_until_something_is_picked() {
        let videos = [video("a", 1, 1, "Pilot"), video("b", 2, 1, "Next year")];
        let seasons = seasons(&videos);

        assert!(shows_picker(None, &seasons), "nothing picked yet");
        assert!(
            !shows_picker(Some("b"), &seasons),
            "b is picked, so its Streams show"
        );
    }

    #[test]
    fn the_streams_list_names_the_picked_episode_and_never_a_stand_in() {
        let videos = [
            video("a", 1, 2, "Second"),
            video("s", 0, 3, "Bonus"),
            film("m", "A film"),
        ];

        assert_eq!(picked_label(&videos, "a"), Some("S1E2 Second".to_owned()));
        assert_eq!(
            picked_label(&videos, "s"),
            Some("SPECIAL 3 Bonus".to_owned())
        );
        assert_eq!(picked_label(&videos, "m"), Some("A film".to_owned()));
        assert_eq!(picked_label(&videos, "gone"), None);
    }

    /// A title carrying `videos`, so the core's own rules can be asked rather than re-implemented.
    fn meta_of(videos: Vec<Video>) -> MetaItem {
        use stremio_core::types::resource::{MetaItemPreview, PosterShape};
        MetaItem {
            preview: MetaItemPreview {
                id: "tt1".to_owned(),
                r#type: "series".to_owned(),
                name: "A series".to_owned(),
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

    /// A Video that came out long enough ago to count as released.
    fn aired(id: &str, season: u32, episode: u32) -> Video {
        Video {
            released: Some(chrono::Utc::now() - chrono::Duration::days(400)),
            ..video(id, season, episode, "An episode")
        }
    }

    #[test]
    fn the_last_episode_of_a_season_leads_into_the_next_season() {
        let meta = meta_of(vec![
            aired("s1e1", 1, 1),
            aired("s1e2", 1, 2),
            aired("s2e1", 2, 1),
        ]);
        assert_eq!(
            next_episode(&meta, "s1e2", chrono::Utc::now()),
            Some("s2e1".to_owned())
        );
    }

    #[test]
    fn the_last_episode_of_the_final_season_has_nothing_after_it() {
        let meta = meta_of(vec![aired("s1e1", 1, 1), aired("s2e1", 2, 1)]);
        assert_eq!(next_episode(&meta, "s2e1", chrono::Utc::now()), None);
    }

    #[test]
    fn a_regular_episode_never_leads_into_the_specials() {
        // The core sorts the Specials last, so the last regular Episode's neighbour is one.
        let meta = meta_of(vec![aired("s1e1", 1, 1), aired("special", 0, 1)]);
        assert_eq!(next_episode(&meta, "s1e1", chrono::Utc::now()), None);
    }

    #[test]
    fn an_episode_that_has_not_aired_yet_is_not_next() {
        let mut later = aired("s1e2", 1, 2);
        later.released = Some(chrono::Utc::now() + chrono::Duration::days(30));
        let meta = meta_of(vec![aired("s1e1", 1, 1), later]);
        assert_eq!(next_episode(&meta, "s1e1", chrono::Utc::now()), None);
    }

    #[test]
    fn torrents_play_through_the_stream_server_and_plain_urls_as_they_are() {
        let server = Url::parse("http://127.0.0.1:11470/").unwrap();

        let torrent = stream(StreamSource::Torrent {
            info_hash: [0xab; 20],
            file_idx: None,
            announce: vec![],
            file_must_include: vec![],
        });
        // The core leaves an empty query on the end of torrent addresses, so compare the parts.
        let url = Url::parse(&stream_url(&torrent, &server).unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.port(), Some(11470));
        assert_eq!(url.path(), format!("/{}/-1", "ab".repeat(20)));

        let plain = stream(StreamSource::Url {
            url: Url::parse("https://example.com/movie.mp4").unwrap(),
        });
        assert_eq!(
            stream_url(&plain, &server).unwrap(),
            "https://example.com/movie.mp4"
        );
    }
}
