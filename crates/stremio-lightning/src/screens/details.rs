//! The detail screen: the item's meta and the streams the addons answered with, plus the
//! action that opens an item and the one that plays a row.

use stremio_core::constants::{META_RESOURCE_NAME, STREAM_RESOURCE_NAME};
use stremio_core::models::common::Loadable;
use stremio_core::models::meta_details::{MetaDetails, Selected as DetailsSelected};
use stremio_core::runtime::msg::{Action, ActionCtx, ActionLoad, ActionMetaDetails};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::ResourcePath;
use stremio_core::types::rating::Rating;
use stremio_core::types::resource::{MetaItem, Video};

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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Episode {
    pub id: String,
    pub label: String,
    pub title: String,
    /// The release date as the row shows it: `Sep 1, 2023`, empty when the title gives none.
    pub released: String,
    pub thumbnail: Option<String>,
    /// Whether the library's watched bitfield has this Video.
    pub watched: bool,
    /// How far into this Video the viewer is, as a percentage. The core keeps that number for the
    /// single Video the title's library entry is sitting on; every other row carries zero, which
    /// draws no bar.
    pub progress: f64,
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
                "Special".to_owned()
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
    // Web: `Oct 7, 2026` -- the whole year, in the row's own 0.8rem.
    match date {
        Some(date) => format!(
            "{} {}, {}",
            date.format("%b"),
            date.day(),
            date.format("%Y")
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
                watched: false,
                progress: 0.0,
                label: if info.season == 0 {
                    format!("SPECIAL {}", info.episode)
                } else {
                    format!("S{}E{}", info.season, info.episode)
                },
                // Web numbers the row: `15. Scarlett Fever`.
                title: if video.title.trim().is_empty() {
                    format!("{}.", info.episode)
                } else {
                    format!("{}. {}", info.episode, video.title.trim())
                },
                released: released_label(video.released.as_ref()),
                thumbnail: video.thumbnail.clone(),
            })
        })
        .collect()
}

/// Web: `NumberInput` — the Season and Episode steppers above "Show Streams", in the message that
/// stands where an empty Streams list would be. `ask_again` is false while they still name the
/// Video on screen, which is why the button is disabled there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Steps {
    pub season: u32,
    pub episode: u32,
    pub ask_again: bool,
}

/// The Episode numbers one Season lists, in the order the title supplied them.
fn episode_numbers(videos: &[Video], season: u32) -> Vec<u32> {
    videos
        .iter()
        .filter_map(|video| video.series_info.as_ref())
        .filter(|info| info.season == season)
        .map(|info| info.episode)
        .collect()
}

/// The pair the steppers name: the picked Video's own Season and Episode, or wherever the user
/// moved them to — as long as the title still offers that Episode.
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

/// One press of a stepper. Moving the Season keeps the Episode number when the new Season lists
/// it, and pulls it into range when it does not; moving the Episode walks the numbers the Season
/// lists, so a gap in them is a single press rather than a press per missing number.
pub fn step(videos: &[Video], at: (u32, u32), seasons_step: i32, episodes_step: i32) -> (u32, u32) {
    let numbers: Vec<u32> = seasons(videos)
        .into_iter()
        .map(|season| season.number)
        .collect();
    let Some(position) = numbers.iter().position(|number| *number == at.0) else {
        return at;
    };
    let moved = (position as i64 + i64::from(seasons_step)).clamp(0, numbers.len() as i64 - 1);
    let season = numbers[moved as usize];
    let listed = episode_numbers(videos, season);
    let (Some(&first), Some(&last)) = (listed.first(), listed.last()) else {
        return (season, at.1);
    };
    let episode = match listed.iter().position(|number| *number == at.1) {
        Some(position) => {
            let moved =
                (position as i64 + i64::from(episodes_step)).clamp(0, listed.len() as i64 - 1);
            listed[moved as usize]
        }
        None => at.1.clamp(first, last),
    };
    (season, episode)
}

/// The Video the steppers name, for the request behind "Show Streams".
pub fn stepped_video(videos: &[Video], at: (u32, u32)) -> Option<&str> {
    videos
        .iter()
        .find(|video| {
            video
                .series_info
                .as_ref()
                .map(|info| (info.season, info.episode) == at)
                .unwrap_or(false)
        })
        .map(|video| video.id.as_str())
}

/// The steppers of the no-Streams message, or nothing for a title without Seasons.
pub fn steps(videos: &[Video], picked: &str, moved: Option<(u32, u32)>) -> Option<Steps> {
    let shown = shown_pair(videos, picked, moved)?;
    let own = videos
        .iter()
        .find(|video| video.id == picked)?
        .series_info
        .as_ref()?;
    Some(Steps {
        season: shown.0,
        episode: shown.1,
        ask_again: shown != (own.season, own.episode),
    })
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
    /// The sources on offer, ready to list.
    pub streams: Vec<Source>,
    /// Addons that have not answered yet — the footer counts them while there is something to read.
    pub sources_loading: usize,
    /// Whether the Streams placeholder rows are worth showing while the answers are out.
    pub waits_for_streams: bool,
    /// Web: the like or love the title carries, once the core has asked about it.
    pub rating: Option<Rating>,
    /// Whether the core has a rating to speak of for this title. Web only shows the two rating
    /// buttons when it does, which takes an account: a guest never gets them.
    pub rating_known: bool,
    /// Whether that answer is in yet: the buttons stay dim until it is.
    pub rating_ready: bool,
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
    /// The Season and Episode steppers the no-Streams message shows, for a Series.
    pub steps: Option<Steps>,
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

/// Web: the address its share dialog offers — the short route, the title's name as a slug and
/// the number from its id. `Carrie` with `tt11540908` becomes `.../s/series/carrie-11540908`,
/// exactly as the site writes it, and whatever Episode is on screen: the link is to the title.
pub fn share_link(name: &str, kind: &str, id: &str) -> String {
    let slug = name
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let number = id.trim_start_matches(|character: char| !character.is_numeric());
    format!("https://www.strem.io/s/{kind}/{slug}-{number}")
}

/// Web: pressing the like or love button — the one already set clears, and either one replaces
/// the other, because a title is liked or loved and never both.
pub fn next_rating(current: Option<&Rating>, like: bool) -> Option<Rating> {
    match (current, like) {
        (Some(Rating::Liked), true) | (Some(Rating::Loved), false) => None,
        (_, true) => Some(Rating::Liked),
        (_, false) => Some(Rating::Loved),
    }
}

/// Web: `MetaDetails` / `Rate` — the core keeps the rating for the title it is showing.
pub(crate) fn rate(runtime: &AppRuntime, status: Option<Rating>) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::MetaDetails(ActionMetaDetails::Rate(status)),
    });
}

/// Web: the action group's first button. In the library it dispatches `RemoveFromLibrary` with
/// the id; out of it, `AddToLibrary` with the preview the addons answered with.
pub(crate) fn toggle_library(runtime: &AppRuntime) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(item) = meta_item(&model.meta_details).map(|item| item.preview.clone()) else {
        return;
    };
    let (in_library, _) = crate::screens::library_state(&model, &item.id);
    drop(model);
    let action = if in_library {
        ActionCtx::RemoveFromLibrary(item.id)
    } else {
        ActionCtx::AddToLibrary(item)
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(action),
    });
}

/// Web: the second button of the same group. `MarkAsWatched` is a MetaDetails action, so the
/// core creates the library entry itself when the title is not in the library yet.
pub(crate) fn toggle_watched(runtime: &AppRuntime) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(id) = meta_item(&model.meta_details).map(|item| item.preview.id.clone()) else {
        return;
    };
    let (_, watched) = crate::screens::library_state(&model, &id);
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::MetaDetails(ActionMetaDetails::MarkAsWatched(!watched)),
    });
}

/// The title the addons answered with, if they have answered: the page holding the meta itself.
pub(crate) fn meta_item(details: &MetaDetails) -> Option<&MetaItem> {
    details
        .meta_items
        .iter()
        .find_map(|page| match &page.content {
            Some(Loadable::Ready(item)) => Some(item),
            _ => None,
        })
}

/// Whether the placeholder rows are worth showing while the Streams load: there is an account, and
/// one of its addons claims the Stream resource for this title or Video.
///
/// Without an account the addons are the stock ones, which answer Streams for almost nothing: the
/// rows would be a promise the app cannot keep, so the message is shown from the first frame. With
/// no addon claiming Streams there is nothing to wait for either way.
pub(crate) fn waits_for_streams(
    model: &AppModel,
    selected: Option<&DetailsSelected>,
    picked: Option<&str>,
) -> bool {
    let Some(selected) = selected else {
        return false;
    };
    if model.ctx.profile.auth.is_none() {
        return false;
    }
    let path = ResourcePath::without_extra(
        STREAM_RESOURCE_NAME,
        &selected.meta_path.r#type,
        picked.unwrap_or(&selected.meta_path.id),
    );
    model
        .ctx
        .profile
        .addons
        .iter()
        .any(|addon| addon.manifest.is_resource_supported(&path))
}

pub fn data(
    model: &AppModel,
    chosen: Option<u32>,
    picked: Option<&str>,
    moved: Option<(u32, u32)>,
    query: &str,
) -> Data {
    let details = &model.meta_details;
    let item = meta_item(details);

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
    // Web: the search bar above the rows filters them.
    let needle = query.trim().to_lowercase();
    let mut season_episodes = shown
        .map(|number| episodes(videos, number))
        .unwrap_or_default();
    if !needle.is_empty() {
        season_episodes.retain(|episode| episode.title.to_lowercase().contains(&needle));
    }
    // The two marks the row draws: `watched` from the bitfield, `progress` from the one Video the
    // library entry is sitting on -- the same pair the player's drawer reads.
    let watched_field = details.watched.as_ref();
    let library_item = details.library_item.as_ref();
    for episode in &mut season_episodes {
        episode.watched = watched_field.is_some_and(|watched| watched.get_video(&episode.id));
        episode.progress = library_item
            .filter(|item| item.state.video_id.as_deref() == Some(episode.id.as_str()))
            .map(|item| item.progress())
            .unwrap_or_default();
    }

    // The footer counts the addons that have not answered yet.
    let loading = details
        .streams
        .iter()
        .filter(|page| {
            !matches!(
                &page.content,
                Some(Loadable::Ready(_)) | Some(Loadable::Err(_))
            )
        })
        .count();
    let streams = source_rows(model, details)
        .into_iter()
        .take(MAX_STREAMS)
        .collect();

    Data {
        // The details route paints the artwork across the whole screen, so it wants the real
        // image, not the tiny one `preview_of` swaps in for the Discover panel's 10px blur.
        preview: item.map(|item| {
            let mut preview = crate::screens::preview_of(&item.preview);
            preview.background = item
                .preview
                .background
                .as_ref()
                .map(|url| format!("{url}{}", crate::plumbing::images::PAGE_SUFFIX));
            (preview.in_library, preview.watched) =
                crate::screens::library_state(model, &item.preview.id);
            preview
        }),
        steps: picked.and_then(|picked| steps(videos, picked, moved)),
        streams,
        sources_loading: loading,
        waits_for_streams: waits_for_streams(model, details.selected.as_ref(), picked),
        rating: match &details.rating_info {
            Some(Loadable::Ready(info)) => info.status.clone(),
            _ => None,
        },
        rating_known: details.rating_info.is_some(),
        rating_ready: matches!(&details.rating_info, Some(Loadable::Ready(_))),
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

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::types::resource::SeriesInfo;

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
                (0, "Special".to_owned()),
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
        // Web numbers the row: `2. Second`.
        assert_eq!(listed[0].title, "2. Second");
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

    /// Three Episodes over two Seasons, for the steppers.
    fn three() -> Vec<Video> {
        vec![
            video("a", 1, 1, "Pilot"),
            video("b", 1, 2, "Second"),
            video("c", 2, 1, "Next year"),
        ]
    }

    #[test]
    fn each_rating_button_sets_its_own_and_clears_when_pressed_again() {
        assert_eq!(next_rating(None, true), Some(Rating::Liked));
        assert_eq!(next_rating(None, false), Some(Rating::Loved));
        assert_eq!(next_rating(Some(&Rating::Liked), true), None);
        assert_eq!(next_rating(Some(&Rating::Loved), false), None);
        // The two replace each other rather than stacking.
        assert_eq!(
            next_rating(Some(&Rating::Liked), false),
            Some(Rating::Loved)
        );
        assert_eq!(next_rating(Some(&Rating::Loved), true), Some(Rating::Liked));
    }

    #[test]
    fn the_share_link_is_the_short_route_web_offers() {
        // Taken from the site's own share dialog for this title.
        assert_eq!(
            share_link("Carrie", "series", "tt11540908"),
            "https://www.strem.io/s/series/carrie-11540908"
        );
        // Punctuation in a name becomes one hyphen rather than a run of them.
        assert_eq!(
            share_link("Mission: Impossible - Fallout", "movie", "tt4912910"),
            "https://www.strem.io/s/movie/mission-impossible-fallout-4912910"
        );
    }

    #[test]
    fn the_steppers_start_on_the_picked_episode_and_ask_for_nothing() {
        let shown = steps(&three(), "b", None).unwrap();

        assert_eq!((shown.season, shown.episode), (1, 2));
        assert!(!shown.ask_again, "it names the Video already on screen");
    }

    #[test]
    fn a_stepper_moved_to_another_video_asks_for_its_streams() {
        let videos = three();

        let shown = steps(&videos, "b", Some((2, 1))).unwrap();

        assert_eq!((shown.season, shown.episode, shown.ask_again), (2, 1, true));
        assert_eq!(stepped_video(&videos, (2, 1)), Some("c"));
    }

    #[test]
    fn a_move_the_title_does_not_offer_falls_back_to_the_picked_episode() {
        let shown = steps(&three(), "b", Some((9, 9))).unwrap();

        assert_eq!((shown.season, shown.episode), (1, 2));
        assert!(!shown.ask_again);
    }

    #[test]
    fn the_season_stepper_keeps_the_episode_number_where_the_season_lists_it() {
        let videos = [
            video("a", 1, 2, "Second"),
            video("c", 2, 1, "First"),
            video("d", 2, 2, "Second of two"),
        ];

        assert_eq!(step(&videos, (1, 2), 1, 0), (2, 2));
    }

    #[test]
    fn the_season_stepper_pulls_the_episode_number_into_the_season_it_lands_in() {
        let videos = [video("a", 1, 5, "Fifth"), video("c", 2, 1, "Only one")];

        assert_eq!(step(&videos, (1, 5), 1, 0), (2, 1));
        assert_eq!(step(&videos, (2, 1), -1, 0), (1, 5), "all the way back");
    }

    #[test]
    fn the_episode_stepper_stops_at_the_ends_of_the_season() {
        let videos = three();

        assert_eq!(step(&videos, (1, 1), 0, 1), (1, 2));
        assert_eq!(step(&videos, (1, 2), 0, 1), (1, 2), "last Episode");
        assert_eq!(step(&videos, (1, 1), 0, -1), (1, 1), "first Episode");
    }

    #[test]
    fn a_film_has_no_steppers() {
        let videos = [film("m", "A film")];

        assert_eq!(steps(&videos, "m", None), None);
        assert_eq!(shown_pair(&videos, "m", None), None);
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
}
