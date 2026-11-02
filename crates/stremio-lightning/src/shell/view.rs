//! Turning a snapshot into window properties.
//!
//! This is the only place in the app that knows both a screen's data and the window it lands in.
//! It decides nothing: every value here was decided by the screen that produced it.

use std::sync::Mutex;

use slint::{Model, ModelRc, SharedString, VecModel, Weak};

use crate::app::Snapshot;
use crate::plumbing::images::Images;
use crate::screens::mods::{self, ModCard};
use crate::screens::player::Data as PlayerData;
use crate::screens::settings::Data as SettingsData;
use crate::screens::Preview;
use crate::screens::{Card, Input};
use crate::MarketCardData;
use crate::{
    AddonData, AddonPendingData, CalendarData, CalendarDay, CalendarSlot, CatalogRow, LibraryCard,
    LibrarySort, LinkGroup, MainWindow, ModCardData, ModsNavItem, ModsViewData, PlayerEpisode,
    PlayerOption, PlayerTrack, PosterData, PreviewData, SelectInputData, SelectOption,
    SettingsFieldData, SettingsPluginData, SettingsViewData, StreamData, VideoRow,
};

/// Copies everything a snapshot describes onto the window. Runs on the window's thread.
pub(crate) fn show(weak: &Weak<MainWindow>, snapshot: Snapshot, images: &Images) {
    let images = images.clone();
    let _ = weak.upgrade_in_event_loop(move |window| apply(&window, &snapshot, &images));
}

pub(crate) fn apply(window: &MainWindow, snapshot: &Snapshot, images: &Images) {
    window.set_status(snapshot.status.clone().into());
    window.set_rows(ModelRc::new(VecModel::from(
        snapshot
            .rows
            .iter()
            .map(|row| CatalogRow {
                title: row.title.clone().into(),
                shape: row.shape.into(),
                menu: row.menu,
                cards: ModelRc::new(VecModel::from(
                    row.cards
                        .iter()
                        .map(|card| poster_data(card, images))
                        .collect::<Vec<_>>(),
                )),
            })
            .collect::<Vec<_>>(),
    )));

    window.set_discover_shape(snapshot.discover.shape.into());
    window.set_discover_cards(ModelRc::new(VecModel::from(
        snapshot
            .discover
            .cards
            .iter()
            .map(|card| poster_data(card, images))
            .collect::<Vec<_>>(),
    )));
    window.set_discover_inputs(inputs(snapshot.discover.inputs.clone()));
    window.set_discover_previews(ModelRc::new(VecModel::from(
        snapshot
            .discover
            .previews
            .iter()
            .map(|preview| preview_data(preview, images))
            .collect::<Vec<_>>(),
    )));

    let library = &snapshot.library;
    window.set_library_type_title(library.type_title.clone().into());
    window.set_library_type_index(
        i32::try_from(
            library
                .type_options
                .iter()
                .position(|(_, selected)| *selected)
                .unwrap_or(0),
        )
        .unwrap_or_default(),
    );
    window.set_library_types(ModelRc::new(VecModel::from(
        library
            .type_options
            .iter()
            .map(|(label, selected)| SelectOption {
                label: label.clone().into(),
                selected: *selected,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_library_sorts(ModelRc::new(VecModel::from(
        library
            .sorts
            .iter()
            .map(|sort| LibrarySort {
                label: sort.label.clone().into(),
                selected: sort.selected,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_library_cards(ModelRc::new(VecModel::from(
        library
            .cards
            .iter()
            .map(|card| {
                let poster = images.get(&card.poster);
                LibraryCard {
                    item_id: card.id.clone().into(),
                    kind: card.kind.clone().into(),
                    title: card.title.clone().into(),
                    has_poster: poster.is_some(),
                    poster: poster.unwrap_or_default(),
                    shape: card.shape.into(),
                    progress: card.progress as f32,
                    watched: card.watched,
                    playable: card.playable,
                    new_videos: i32::try_from(card.new_videos).unwrap_or(i32::MAX),
                }
            })
            .collect::<Vec<_>>(),
    )));
    window.set_library_message(library.message.clone().into());
    window.set_library_has_more(library.has_more);
    window.set_library_grid_key(library.grid_key.clone().into());

    window.set_addons(ModelRc::new(VecModel::from(
        crate::screens::addons::visible(&snapshot.addons.cards, &window.get_addons_query())
            .into_iter()
            .map(|card| {
                let logo = images.get(&card.logo);
                AddonData {
                    transport_url: card.transport_url.clone().into(),
                    name: card.name.clone().into(),
                    version: card.version.clone().into(),
                    types: card.types.clone().into(),
                    description: card.description.clone().into(),
                    has_logo: logo.is_some(),
                    logo: logo.unwrap_or_default(),
                    installed: card.installed,
                    configuration_required: card.configuration_required,
                    configurable: card.configurable,
                }
            })
            .collect::<Vec<_>>(),
    )));
    window.set_calendar(calendar_view(&snapshot.calendar, images));
    window.set_addon_inputs(inputs(snapshot.addons.inputs.clone()));
    window.set_addon_pending(addon_pending(&snapshot.addons.pending, images));
    window.set_addons_message(snapshot.addons.message.clone().into());

    window.set_settings(settings_view(&snapshot.settings));
    show_mods(window, &snapshot.mods);
    player_view(window, &snapshot.player, images);

    let details = &snapshot.details;
    window.set_details_has_preview(details.preview.is_some());
    if let Some(preview) = &details.preview {
        let mut data = preview_data(preview, images);
        // Web: the column on the right has `backdrop-filter: blur(15px)`, so it paints the artwork
        // behind it blurred under its own 0.4 wash. Slint has no backdrop filter; the column draws
        // this copy instead, lined up with the page's layer.
        let backdrop = preview
            .background
            .as_ref()
            .map(|key| crate::plumbing::images::backdrop_key(key));
        let backdrop = images.get(&backdrop);
        data.has_panel_backdrop = backdrop.is_some();
        data.panel_backdrop = backdrop.unwrap_or_default();
        window.set_details_preview(data);
    }
    window.set_streams(ModelRc::new(VecModel::from(
        details
            .streams
            .iter()
            .map(|source| StreamData {
                name: source.name.clone().into(),
                description: source.description.clone().into(),
                addon: source.addon.clone().into(),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_streams_loading(i32::try_from(details.sources_loading).unwrap_or_default());
    window.set_details_waits_for_streams(details.waits_for_streams);

    window.set_details_rate_visible(details.rating_known);
    window.set_details_rate_enabled(details.rating_ready);
    window.set_details_rated_liked(
        details.rating == Some(stremio_core::types::rating::Rating::Liked),
    );
    window.set_details_rated_loved(
        details.rating == Some(stremio_core::types::rating::Rating::Loved),
    );
    window.set_details_show_picker(details.show_picker);
    window.set_details_has_steps(details.steps.is_some());
    window.set_details_season_number(
        details
            .steps
            .map(|steps| i32::try_from(steps.season).unwrap_or_default())
            .unwrap_or_default(),
    );
    window.set_details_episode_number(
        details
            .steps
            .map(|steps| i32::try_from(steps.episode).unwrap_or_default())
            .unwrap_or_default(),
    );
    window
        .set_details_can_show_streams(details.steps.map(|steps| steps.ask_again).unwrap_or(false));
    window.set_details_season_index(i32::try_from(details.season).unwrap_or_default());
    window.set_details_picked_label(details.picked_label.clone().unwrap_or_default().into());
    window.set_details_wrap_streams(snapshot.settings.wrap_streams);
    window.set_details_has_next_episode(details.next_episode.is_some());
    window.set_details_season_title(
        details
            .seasons
            .get(details.season)
            .map(|season| season.label.clone())
            .unwrap_or_default()
            .into(),
    );
    window.set_details_seasons(ModelRc::new(VecModel::from(
        details
            .seasons
            .iter()
            .enumerate()
            .map(|(index, season)| SelectOption {
                label: season.label.clone().into(),
                selected: index == details.season,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_details_episodes(ModelRc::new(VecModel::from(
        details
            .episodes
            .iter()
            .map(|episode| {
                let thumbnail = images.get(&episode.thumbnail);
                VideoRow {
                    id: episode.id.clone().into(),
                    title: episode.title.clone().into(),
                    released: episode.released.clone().into(),
                    has_thumbnail: thumbnail.is_some(),
                    thumbnail: thumbnail.unwrap_or_default(),
                    selected: details.picked.as_deref() == Some(episode.id.as_str()),
                    watched: episode.watched,
                    progress: episode.progress as f32,
                }
            })
            .collect::<Vec<_>>(),
    )));
}

/// The preview panel's content — the same panel Discover opens on hover and the details screen
/// shows beside its list.
fn preview_data(preview: &Preview, images: &Images) -> PreviewData {
    let logo = images.get(&preview.logo);
    let background = images.get(&preview.background);
    PreviewData {
        title: preview.title.clone().into(),
        runtime: preview.runtime.clone().into(),
        release_info: preview.release.clone().into(),
        rating: preview.rating.clone().into(),
        description: preview.description.clone().into(),
        groups: ModelRc::new(VecModel::from(
            preview
                .groups
                .iter()
                .map(|(label, links)| LinkGroup {
                    label: label.clone().into(),
                    links: ModelRc::new(VecModel::from(
                        links
                            .iter()
                            .map(|link| SharedString::from(link.clone()))
                            .collect::<Vec<_>>(),
                    )),
                })
                .collect::<Vec<_>>(),
        )),
        has_trailer: preview.has_trailer,
        has_logo: logo.is_some(),
        logo: logo.unwrap_or_default(),
        has_background: background.is_some(),
        background: background.unwrap_or_default(),
        has_panel_backdrop: false,
        panel_backdrop: Default::default(),
        in_library: preview.in_library,
        watched: preview.watched,
    }
}

/// The add-addon dialog's manifest: what the pasted URL answered with, and whether it can be
/// installed. `open` is whether there is a dialog at all.
fn addon_pending(
    pending: &Option<crate::screens::addons::Pending>,
    images: &Images,
) -> AddonPendingData {
    let Some(pending) = pending else {
        return AddonPendingData::default();
    };
    let logo = images.get(&pending.logo);
    AddonPendingData {
        open: true,
        transport_url: pending.transport_url.clone().into(),
        name: pending.name.clone().into(),
        version: pending.version.clone().into(),
        types: pending.types.clone().into(),
        description: pending.description.clone().into(),
        has_logo: logo.is_some(),
        logo: logo.unwrap_or_default(),
        ready: pending.ready,
        configuration_required: pending.configuration_required,
        installed: pending.installed,
        error: pending.error.clone().into(),
    }
}

/// The Calendar's month, its grid and its list, with every picture already in the cache.
fn calendar_view(data: &crate::screens::calendar::Data, images: &Images) -> CalendarData {
    let day = |day: &crate::screens::calendar::Day| CalendarDay {
        day: i32::try_from(day.day).unwrap_or(i32::MAX),
        today: day.today,
        heading: day.heading.clone().into(),
        slots: ModelRc::new(VecModel::from(
            day.slots
                .iter()
                .map(|slot| {
                    let poster = images.get(&slot.poster);
                    let thumbnail = images.get(&slot.thumbnail);
                    CalendarSlot {
                        video_id: slot.video_id.clone().into(),
                        meta_id: slot.meta_id.clone().into(),
                        kind: slot.kind.clone().into(),
                        title: slot.title.clone().into(),
                        name: slot.name.clone().into(),
                        label: slot.label.clone().into(),
                        has_poster: poster.is_some(),
                        poster: poster.unwrap_or_default(),
                        has_thumbnail: thumbnail.is_some(),
                        thumbnail: thumbnail.unwrap_or_default(),
                    }
                })
                .collect::<Vec<_>>(),
        )),
    };
    CalendarData {
        signed_in: data.signed_in,
        loading: data.loading,
        year: data.year,
        month: data.month.clone().into(),
        prev: data.prev.clone().into(),
        next: data.next.clone().into(),
        first_weekday: i32::try_from(data.first_weekday).unwrap_or_default(),
        days: ModelRc::new(VecModel::from(
            data.days.iter().map(day).collect::<Vec<_>>(),
        )),
        list: ModelRc::new(VecModel::from(
            data.list.iter().map(day).collect::<Vec<_>>(),
        )),
    }
}

/// The Mods section: its page menu, the line about the last action, and the plugins and themes
/// on their pages. What is armed for removal is the window's own state, not this snapshot's, so
/// it is left alone here and the cards carry whatever arm the wiring last set.
pub(crate) fn mods_view(data: &crate::screens::mods::Data) -> ModsViewData {
    let armed = armed_card();
    let card = |kind: mods::catalogue::Kind, card: &ModCard| {
        let armed_here = armed.as_deref() == Some(card.filename.as_str());
        let state = match kind {
            mods::catalogue::Kind::Plugin => "enabled",
            mods::catalogue::Kind::Theme => "on",
        };
        ModCardData {
            id: card.id.clone().into(),
            filename: card.filename.clone().into(),
            name: mods::catalogue::title(card).into(),
            byline: mods::catalogue::byline(card).into(),
            description: card.description.clone().into(),
            unreadable: mods::catalogue::unreadable(card).into(),
            warnings: ModelRc::new(VecModel::from(
                if kind == mods::catalogue::Kind::Theme {
                    card.warnings.clone()
                } else {
                    // A plugin's state line: whether it runs, why it stopped, or its last warning.
                    // A card the runtime has not been asked about says nothing.
                    match card.running.said(card.enabled) {
                        line if line.is_empty() => Vec::new(),
                        line => vec![line],
                    }
                }
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
            )),
            enabled: card.enabled,
            armed: armed_here,
            state: state.into(),
            update: card.update.clone().into(),
        }
    };
    ModsViewData {
        page: data.page.clone().into(),
        menu: ModelRc::new(VecModel::from(
            data.menu
                .iter()
                .map(|item| ModsNavItem {
                    id: item.id.clone().into(),
                    label: item.label.clone().into(),
                    selected: item.selected,
                })
                .collect::<Vec<_>>(),
        )),
        notice: data.notice.text.clone().into(),
        notice_failed: data.notice.failed,
        plugins: ModelRc::new(VecModel::from(
            data.plugins
                .iter()
                .map(|card_data| card(mods::catalogue::Kind::Plugin, card_data))
                .collect::<Vec<_>>(),
        )),
        themes: ModelRc::new(VecModel::from(
            data.themes
                .iter()
                .map(|card_data| card(mods::catalogue::Kind::Theme, card_data))
                .collect::<Vec<_>>(),
        )),
        plugins_note: data.plugins_note.clone().into(),
        themes_note: data.themes_note.clone().into(),
        settings_plugins: ModelRc::new(VecModel::from(
            data.settings_plugins
                .iter()
                .map(settings_plugin)
                .collect::<Vec<_>>(),
        )),
        settings_plugin: data.plugin.clone().into(),
        settings_fields: ModelRc::new(VecModel::from(
            data.edits
                .controls
                .iter()
                .map(settings_field)
                .collect::<Vec<_>>(),
        )),
        settings_trouble: ModelRc::new(VecModel::from(
            trouble(data)
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )),
        settings_empty: empty_settings(data).into(),
        settings_plugin_running: data.picked_state.running,
        settings_state: state_said(data).into(),
        settings_state_failed: data.picked_enabled && !data.picked_state.running,
        settings_dirty: data.edits.dirty(),
        ..mods_market(data)
    }
}

/// The Marketplace's own properties, kept apart from `mods_view` so a later stage adds its fields
/// to that function without touching this one.
fn mods_market(data: &crate::screens::mods::Data) -> ModsViewData {
    // Everything the other pages draw is left at its default: `mods_view` fills it, and the
    // struct update that joins the two keeps what it filled.
    ModsViewData {
        market_note: data.market.note.clone().into(),
        market_summary: ModelRc::new(VecModel::from(
            data.market
                .summary
                .iter()
                .map(|notice| SharedString::from(notice.text.clone()))
                .collect::<Vec<_>>(),
        )),
        market_trouble: data.market.trouble.clone().into(),
        market_plugins: ModelRc::new(VecModel::from(
            data.market
                .plugins
                .iter()
                .map(market_card)
                .collect::<Vec<_>>(),
        )),
        market_themes: ModelRc::new(VecModel::from(
            data.market
                .themes
                .iter()
                .map(market_card)
                .collect::<Vec<_>>(),
        )),
        market_empty: data.market.empty.clone().into(),
        market_search: data.market_search.clone().into(),
        market_checking: checking(),
        updates_ready: data.updates_ready(),
        ..ModsViewData::default()
    }
}

/// One Marketplace card, with the two pieces of the window's own state it draws: whether its
/// Uninstall is armed, and whether its file is being fetched this instant.
fn market_card(card: &mods::marketplace::MarketCard) -> MarketCardData {
    let armed = armed_market();
    MarketCardData {
        id: card.id.clone().into(),
        kind: card.kind.clone().into(),
        name: card.name.clone().into(),
        author: card.author.clone().into(),
        version: card.version.clone().into(),
        description: card.description.clone().into(),
        filename: card.filename.clone().into(),
        action: card.action.clone().into(),
        armed: armed.as_deref() == Some(card.id.as_str()),
        installing: installing().as_deref() == Some(card.id.as_str()),
    }
}

/// One row of the Plugin settings page's left list.
fn settings_plugin(plugin: &mods::settings::Plugin) -> SettingsPluginData {
    SettingsPluginData {
        filename: plugin.filename.clone().into(),
        name: plugin.name.clone().into(),
        controls: match plugin.fields {
            0 => "no controls".to_owned(),
            1 => "1 control".to_owned(),
            count => format!("{count} controls"),
        }
        .into(),
        enabled: plugin.enabled,
    }
}

/// What a control on the Plugin settings page holds: the value the user is editing, made into
/// what its kind of control draws. The value the file holds is the screen's business.
pub(crate) fn settings_field(control: &mods::settings::Control) -> SettingsFieldData {
    // A kind this app cannot draw shows its label and says so, and offers no control.
    let mut field = SettingsFieldData {
        key: control.field.key().into(),
        label: control.field.label().into(),
        kind: "unsupported".into(),
        hint: "This app cannot draw this kind of setting.".into(),
        on: false,
        text: "".into(),
        options: ModelRc::new(VecModel::from(Vec::<SharedString>::new())),
        number: 0.0,
        min: 0.0,
        max: 1.0,
        shown: "".into(),
    };
    match &control.field {
        mods::plugin_schema::Field::Toggle { .. } => {
            field.kind = "toggle".into();
            field.hint = mods::plugin_schema::hint(&control.field).into();
            field.on = control.edited.as_bool().unwrap_or(false);
        }
        mods::plugin_schema::Field::Text { .. } => {
            field.kind = "text".into();
            field.hint = mods::plugin_schema::hint(&control.field).into();
            field.text = control.edited.as_str().unwrap_or_default().into();
        }
        mods::plugin_schema::Field::Select { options, .. } => {
            field.kind = "select".into();
            // The field's own hint names every option; under a select the list is already on
            // screen, so the hint says what picking one does.
            field.hint = "One of the options above.".into();
            field.text = control.edited.as_str().unwrap_or_default().into();
            field.options = ModelRc::new(VecModel::from(
                options
                    .iter()
                    .map(|option| SharedString::from(option.clone()))
                    .collect::<Vec<_>>(),
            ));
        }
        mods::plugin_schema::Field::Range { min, max, .. } => {
            field.kind = "range".into();
            field.hint = mods::plugin_schema::hint(&control.field).into();
            let number = control.edited.as_f64().unwrap_or(*min).clamp(*min, *max);
            field.number = number as f32;
            field.min = *min as f32;
            field.max = *max as f32;
            field.shown = match serde_json::Number::from_f64(number) {
                Some(number) => number.to_string(),
                None => String::new(),
            }
            .into();
        }
    }
    field
}

/// What the Plugin settings page's name line says about the plugin picked: the runtime's own
/// sentence for a plugin that is on, and nothing for one that is off.
fn state_said(data: &crate::screens::mods::Data) -> String {
    if data.plugin.is_empty() {
        return String::new();
    }
    data.picked_state.said(data.picked_enabled)
}

/// What the Plugin settings page says is wrong with itself: a schema that cannot be used, a file
/// that cannot be read, and values typed and not saved.
fn trouble(data: &crate::screens::mods::Data) -> Vec<String> {
    let edits = &data.edits;
    let mut said = Vec::new();
    if !edits.filename.is_empty() {
        if !edits.trouble.is_empty() {
            said.push(edits.trouble.clone());
        }
        if !edits.error.is_empty() {
            said.push(edits.error.clone());
        }
        if edits.dirty() {
            said.push(
                "Typed and not saved. Press Save to write it, or Revert to drop it.".to_owned(),
            );
        }
    }
    said
}

/// What the Plugin settings page says when it holds no control: nothing picked, or a plugin that
/// registered none.
fn empty_settings(data: &crate::screens::mods::Data) -> String {
    if data.plugin.is_empty() {
        mods::settings::NOTHING_PICKED.to_owned()
    } else if data.edits.controls.is_empty() {
        mods::settings::NO_SCHEMA.to_owned()
    } else {
        String::new()
    }
}

/// The card whose Remove button has been pressed once and waits for the second press.
///
/// One card at a time: pressing Remove on another card moves the arm to it, and `None` takes it
/// back. `shell::wiring` takes it back when the page changes, another control is pressed, or five
/// seconds pass.
static ARMED: Mutex<Option<String>> = Mutex::new(None);

/// Arms `filename`'s Remove button, or takes the arm back with `None`.
pub(crate) fn arm(filename: Option<&str>) {
    if let Ok(mut armed) = ARMED.lock() {
        *armed = filename.map(str::to_owned);
    }
}

/// The card whose Remove button is armed, if any.
fn armed_card() -> Option<String> {
    ARMED.lock().ok().and_then(|armed| armed.clone())
}

/// The Marketplace card whose Uninstall button has been pressed once and waits for the second
/// press, by the entry's download. One at a time, and taken back by any other press on the page.
static ARMED_MARKET: Mutex<Option<String>> = Mutex::new(None);

/// Arms this Marketplace card's Uninstall button, or takes the arm back with `None`.
pub(crate) fn arm_market(id: Option<&str>) {
    if let Ok(mut armed) = ARMED_MARKET.lock() {
        *armed = id.map(str::to_owned);
    }
}

pub(crate) fn armed_market() -> Option<String> {
    ARMED_MARKET.lock().ok().and_then(|armed| armed.clone())
}

/// The Marketplace entry being fetched or written right now, by its download. The window draws
/// that card as waiting and refuses every other press on the page until the fetch lands.
static INSTALLING: Mutex<Option<String>> = Mutex::new(None);

/// Says which entry is being fetched.
pub(crate) fn set_installing(id: Option<&str>) {
    if let Ok(mut busy) = INSTALLING.lock() {
        *busy = id.map(str::to_owned);
    }
}

fn installing() -> Option<String> {
    INSTALLING.lock().ok().and_then(|busy| busy.clone())
}

/// Whether the update check is running: Check for updates says so and refuses a second press.
static CHECKING: Mutex<bool> = Mutex::new(false);

pub(crate) fn set_checking(checking: bool) {
    if let Ok(mut busy) = CHECKING.lock() {
        *busy = checking;
    }
}

fn checking() -> bool {
    CHECKING.lock().map(|busy| *busy).unwrap_or(false)
}

/// Sets every property the Mods section draws from: the page on screen, the menu with it, and the
/// content itself. The menu is written only when its rows differ from the ones the window holds,
/// because a new model tears down every row -- including the one a press may be standing on. Which
/// row is marked is not in the model: the menu marks the page `data.page` names.
pub(crate) fn show_mods(window: &MainWindow, data: &crate::screens::mods::Data) {
    let view = mods_view(data);
    let rows: Vec<(String, String)> = data
        .menu
        .iter()
        .map(|item| (item.id.clone(), item.label.clone()))
        .collect();
    if rows != window_nav_rows(window) {
        window.set_mods_nav(nav_rows(data));
    }
    window.set_mods_page(data.page.clone().into());
    window.set_mods(view);
}

/// The menu's rows the window holds now, as pairs of id and label.
fn window_nav_rows(window: &MainWindow) -> Vec<(String, String)> {
    let nav = window.get_mods_nav();
    (0..nav.row_count())
        .filter_map(|row| nav.row_data(row))
        .map(|item| (item.id.to_string(), item.label.to_string()))
        .collect()
}

/// The menu's rows as the window holds them, compared by id and label: whether a row is marked
/// follows the page, so it is not part of what makes the model a new one.
type NavRows = slint::ModelRc<ModsNavItem>;

fn nav_rows(data: &crate::screens::mods::Data) -> NavRows {
    let rows: Vec<ModsNavItem> = data
        .menu
        .iter()
        .map(|item| ModsNavItem {
            id: item.id.clone().into(),
            label: item.label.clone().into(),
            selected: false,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn poster_data(card: &Card, images: &Images) -> PosterData {
    let poster = images.get(&card.poster);
    PosterData {
        item_id: card.id.clone().into(),
        kind: card.kind.clone().into(),
        title: card.title.clone().into(),
        has_poster: poster.is_some(),
        poster: poster.unwrap_or_default(),
        progress: card.progress as f32,
        watched: card.watched,
        playable: card.playable,
    }
}

fn inputs(inputs: Vec<Input>) -> ModelRc<SelectInputData> {
    ModelRc::new(VecModel::from(
        inputs
            .into_iter()
            .map(crate::screens::select_input_data)
            .collect::<Vec<_>>(),
    ))
}

pub(crate) fn settings_view(data: &SettingsData) -> SettingsViewData {
    let select = crate::screens::select_input_data;
    SettingsViewData {
        user_email: data.user_email.clone().into(),
        logged_in: data.logged_in,
        trakt_authenticated: data.trakt_authenticated,
        streaming_server_url: data.streaming_server_url.clone().into(),
        ui_language: select(data.ui_language.clone()),
        quit_on_close: data.quit_on_close,
        esc_exit_fullscreen: data.esc_exit_fullscreen,
        hide_spoilers: data.hide_spoilers,
        subtitles_language: select(data.subtitles_language.clone()),
        subtitles_size: select(data.subtitles_size.clone()),
        subtitles_text_color: select(data.subtitles_text_color.clone()),
        subtitles_bg_color: select(data.subtitles_bg_color.clone()),
        subtitles_outline_color: select(data.subtitles_outline_color.clone()),
        audio_language: select(data.audio_language.clone()),
        surround_sound: data.surround_sound,
        seek_time: select(data.seek_time.clone()),
        seek_short_time: select(data.seek_short_time.clone()),
        play_in_background: data.play_in_background,
        binge_watching: data.binge_watching,
        next_video_duration: select(data.next_video_duration.clone()),
        external_player: select(data.external_player.clone()),
        hardware_decoding: data.hardware_decoding,
        pause_on_minimize: data.pause_on_minimize,
        caching: select(data.caching.clone()),
        torrent_profile: select(data.torrent_profile.clone()),
        transcoding_profile: select(data.transcoding_profile.clone()),
    }
}

/// Everything the player screen draws that the core decided. The values mpv decides -- where the
/// play head is, the tracks, the volume -- are set by the wiring as they change, so they are left
/// alone here and do not flicker back to their defaults on every core event.
fn player_view(window: &MainWindow, data: &PlayerData, images: &Images) {
    window.set_playing(data.open);
    window.set_player_title(data.title.clone().into());
    window.set_player_error(data.error.clone().into());
    let logo = images.get(&data.logo);
    window.set_player_has_logo(logo.is_some());
    window.set_player_logo(logo.unwrap_or_default());
    let backdrop = images.get(&data.background);
    window.set_player_has_backdrop(backdrop.is_some());
    window.set_player_backdrop(backdrop.unwrap_or_default());
    window.set_player_options(ModelRc::new(VecModel::from(
        data.options
            .iter()
            .map(|option| PlayerOption {
                label: option.label.clone().into(),
                action: option.action.into(),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_player_episodes(ModelRc::new(VecModel::from(
        data.episodes
            .iter()
            .map(|episode| PlayerEpisode {
                id: episode.id.clone().into(),
                label: episode.label.clone().into(),
                title: episode.title.clone().into(),
                selected: episode.picked,
                watched: episode.watched,
                progress: episode.progress as f32,
                released: episode.released.clone().into(),
                thumbnail: images.get(&episode.thumbnail).unwrap_or_default(),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_player_seasons(ModelRc::new(VecModel::from(
        data.seasons
            .iter()
            .enumerate()
            .map(|(index, (label, _))| SelectOption {
                label: label.clone().into(),
                selected: index == data.season,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_player_season_index(i32::try_from(data.season).unwrap_or_default());
    window.set_player_season_title(
        data.seasons
            .get(data.season)
            .map(|(label, _)| label.clone())
            .unwrap_or_default()
            .into(),
    );
    window.set_player_has_episodes(data.seasons.len() > 1 || !data.episodes.is_empty());
    window.set_player_has_preview(data.preview.is_some());
    if let Some(preview) = &data.preview {
        window.set_player_preview(preview_data(preview, images));
    }
    window.set_player_next_label(data.next_label.clone().into());
    window.set_player_has_next(data.has_next);
    window.set_player_next_notice(data.next_notice as f32);
    window.set_player_scale(data.scale.into());
}

pub(crate) fn tracks(tracks: &[crate::plumbing::player::Track]) -> ModelRc<PlayerTrack> {
    ModelRc::new(VecModel::from(
        tracks
            .iter()
            .map(|track| PlayerTrack {
                id: track.id.clone().into(),
                title: track.title.clone().into(),
                // The Audio menu's first line is the language's own name, as in the web
                // (`languages.label(lang)`); the code stays in the model for the rest of the shell.
                language: crate::screens::language_names::label_or_code(&track.language).into(),
                selected: track.selected,
                external: track.external,
                origin: Default::default(),
            })
            .collect::<Vec<_>>(),
    ))
}
