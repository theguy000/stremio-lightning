//! The Calendar: one month of the Episodes the account's titles are releasing, laid out as the
//! web's `/calendar` route lays it out -- a selector with the month either side, a seven-column
//! month grid whose cells hold the posters released that day, and a list of the days that have
//! any, each with its Episodes. Based on stremio-web src/routes/Calendar (Calendar.tsx, Selector,
//! Table, List, Episode).
//!
//! The core does the work: `Calendar` loads the library's titles through the addons and hands back
//! one `Item` per day of the selected month, plus the month's shape (`days`, `first_weekday`,
//! which day is `today`) and the months either side.

use stremio_core::models::calendar::Selected as CalendarSelected;
use stremio_core::models::common::Loadable;
use stremio_core::runtime::msg::{Action, ActionLoad};
use stremio_core::runtime::RuntimeAction;

use crate::app::{AppModel, AppRuntime};

/// Web: `toMonth(..., 'long')` and `'short'` off a `Date`. The app speaks English, so the names
/// are the English ones.
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The week headers, Monday first: the core's `first_weekday` counts from Monday.
const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// One Episode (or film) released on a day: what the cell's poster and the list's row draw.
#[derive(Clone, Default)]
pub struct Slot {
    pub video_id: String,
    pub meta_id: String,
    pub kind: String,
    /// The title's own name, which the web calls `name`.
    pub title: String,
    /// The Episode's name, which the web calls `title`; empty for a film.
    pub name: String,
    /// "S1E5", and empty for a film.
    pub label: String,
    pub poster: Option<String>,
    pub thumbnail: Option<String>,
}

#[derive(Clone, Default)]
pub struct Day {
    pub day: u32,
    pub today: bool,
    /// "Friday, October 3": what the list's heading shows.
    pub heading: String,
    pub slots: Vec<Slot>,
}

#[derive(Clone, Default)]
pub struct Data {
    pub signed_in: bool,
    /// Whether any of the month's requests is still out.
    pub loading: bool,
    pub year: i32,
    /// "October", and the two months the selector's buttons lead to.
    pub month: String,
    pub prev: String,
    pub next: String,
    /// Where the month's first day sits in a Monday-first week, 0-6.
    pub first_weekday: u32,
    /// Every day of the month, for the grid.
    pub days: Vec<Day>,
    /// Only the days that have something on them, for the list.
    pub list: Vec<Day>,
}

pub fn data(model: &AppModel) -> Data {
    let calendar = &model.calendar;
    let signed_in = model.ctx.profile.auth.is_some();
    let Some(selected) = calendar.selected.as_ref() else {
        return Data {
            signed_in,
            ..Data::default()
        };
    };
    let month = month_name(selected.month);
    let first_weekday = calendar.month_info.first_weekday;
    let days = calendar
        .items
        .iter()
        .map(|item| {
            let mut slots: Vec<Slot> = Vec::new();
            for content in &item.items {
                let slot = slot_of(content);
                // Web: `.unique_by(|item| item.id)` -- one poster per Video, even when two addons
                // answered for the same title.
                if !slots.iter().any(|known| known.video_id == slot.video_id) {
                    slots.push(slot);
                }
            }
            let weekday = WEEKDAYS[((first_weekday + item.date.day - 1) % 7) as usize];
            Day {
                day: item.date.day,
                today: calendar.month_info.today == Some(item.date.day),
                heading: format!("{weekday}, {month} {}", item.date.day),
                slots,
            }
        })
        .collect::<Vec<_>>();
    let list = days
        .iter()
        .filter(|day| !day.slots.is_empty())
        .cloned()
        .collect();
    Data {
        signed_in,
        loading: calendar
            .meta_items
            .iter()
            .any(|items| !matches!(items.content.as_ref(), Some(Loadable::Ready(_)))),
        year: selected.year,
        month: month.to_owned(),
        prev: month_name(calendar.selectable.prev.month).to_owned(),
        next: month_name(calendar.selectable.next.month).to_owned(),
        first_weekday,
        days,
        list,
    }
}

fn month_name(month: u32) -> &'static str {
    let index = month.saturating_sub(1) as usize;
    MONTHS.get(index).copied().unwrap_or_default()
}

fn slot_of(content: &stremio_core::models::calendar::ContentItem) -> Slot {
    let meta = &content.meta_item.preview;
    let video = &content.video;
    Slot {
        video_id: video.id.clone(),
        meta_id: meta.id.clone(),
        kind: meta.r#type.clone(),
        title: meta.name.clone(),
        name: video.title.clone(),
        label: video
            .series_info
            .as_ref()
            .map(|info| format!("S{}E{}", info.season, info.episode))
            .unwrap_or_default(),
        poster: meta.poster.as_ref().map(|url| url.to_string()),
        thumbnail: video.thumbnail.clone(),
    }
}

/// What a loaded month was built from: how many titles the library held, and the newest of them.
/// Two visits with the same signature are looking at the same library.
pub type LibrarySignature = (usize, Option<chrono::DateTime<chrono::Utc>>);

/// The signature of the library the app holds now.
pub fn library_signature(model: &AppModel) -> LibrarySignature {
    (
        model.ctx.library.items.len(),
        model
            .ctx
            .library
            .items
            .values()
            .map(|item| item.mtime)
            .max(),
    )
}

/// Whether the month on screen has to be asked for again: nothing has been loaded yet, or the
/// library is not the one the month was built from -- which is what hid a title added after the
/// Calendar had been visited once.
pub fn needs_load(
    loaded: Option<LibrarySignature>,
    now: LibrarySignature,
    calendar_empty: bool,
) -> bool {
    calendar_empty || loaded != Some(now)
}

/// Web: `useCalendar` dispatches `Load` with the route's year and month, and the core fills in
/// the current month when it is given none.
pub(crate) fn load(runtime: &AppRuntime, month: Option<(i32, u32)>) {
    let selected = month.map(|(year, month)| CalendarSelected { month, year });
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::Calendar(selected)),
    });
}

/// One press of the selector's arrows: the month either side of the one on screen, as the core
/// itself computes it (`selectable.prev` / `selectable.next`).
pub(crate) fn step(runtime: &AppRuntime, forward: bool) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let selectable = &model.calendar.selectable;
    let date = if forward {
        &selectable.next
    } else {
        &selectable.prev
    };
    let target = (date.year, date.month);
    drop(model);
    load(runtime, Some(target));
}
