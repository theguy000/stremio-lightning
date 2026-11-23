//! Turning a snapshot into window properties.
//!
//! This is the only place in the app that knows both a screen's data and the window it lands in.
//! It decides nothing: every value here was decided by the screen that produced it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel, Weak};

use crate::app::Snapshot;
use crate::plumbing::images::{DecodedImage, Images};

thread_local! {
    /// UI-thread cache from image key to `(DecodedImage::rev, slint::Image)` so unchanged frames
    /// share the same `SharedPixelBuffer` allocation and GPU texture, while entries evicted from
    /// `Images` are pruned immediately on `apply`.
    static SLINT_IMAGES: RefCell<HashMap<String, (u64, Image)>> = RefCell::new(HashMap::new());
}

fn to_slint_image(decoded: &DecodedImage) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        decoded.as_raw(),
        decoded.width(),
        decoded.height(),
    ))
}

fn slint_image(images: &Images, key: &Option<String>) -> Option<Image> {
    let key = key.as_ref()?;
    let decoded = images.get(&Some(key.clone()))?;
    let rev = decoded.rev();
    SLINT_IMAGES.with(|cell| {
        let mut cache = cell.borrow_mut();
        if let Some((cached_rev, img)) = cache.get(key) {
            if *cached_rev == rev {
                return Some(img.clone());
            }
        }
        let img = to_slint_image(&decoded);
        cache.insert(key.clone(), (rev, img.clone()));
        Some(img)
    })
}

fn prune_slint_images(images: &Images) {
    SLINT_IMAGES.with(|cell| {
        cell.borrow_mut().retain(|key, _| images.contains(key));
    });
}
use crate::screens::mods::{self, ModCard};
use crate::screens::player::Data as PlayerData;
use crate::screens::settings::Data as SettingsData;
use crate::screens::Preview;
use crate::screens::{Card, SelectMenu};
use crate::{
    AddonData, AddonPendingData, CalendarData, CalendarDay, CalendarSlot, CatalogRow, LibraryCard,
    LibrarySort, LinkGroup, MainWindow, ModCardData, ModsAboutLine, ModsLogRow, ModsNavItem,
    ModsViewData, PlayerEpisode, PlayerOption, PlayerTrack, PosterData, PreviewData,
    SelectInputData, SelectOption, SettingsFieldData, SettingsPluginData, SettingsViewData,
    StreamData, VideoRow,
};

/// Copies the latest snapshot onto the window on the UI thread, coalescing bursts of repaints
/// (such as posters finishing in parallel) into one pass and reading `latest` on the UI thread so
/// an already-queued repaint cannot roll back a newer synchronous UI update.
pub(crate) fn show(
    weak: &Weak<MainWindow>,
    latest: &Arc<Mutex<Snapshot>>,
    pending: &Arc<AtomicBool>,
    images: &Images,
) {
    if pending.swap(true, Ordering::SeqCst) {
        return;
    }
    let latest = latest.clone();
    let pending_run = pending.clone();
    let images = images.clone();
    if weak
        .upgrade_in_event_loop(move |window| {
            pending_run.store(false, Ordering::SeqCst);
            let snapshot = latest.lock().unwrap().clone();
            apply(&window, &snapshot, &images);
        })
        .is_err()
    {
        pending.store(false, Ordering::SeqCst);
    }
}

/// Slint's `Image::default()` (`ImageInner::None`) does not compare equal to itself, so structs
/// carrying an optional `Image` + `has_*` flag must compare loaded images only.
fn opt_image_eq(has_a: bool, a: &slint::Image, has_b: bool, b: &slint::Image) -> bool {
    has_a == has_b && (!has_a || a == b)
}

fn poster_same(a: &PosterData, b: &PosterData) -> bool {
    let PosterData {
        item_id,
        kind,
        title,
        has_poster,
        poster,
        progress,
        watched,
        playable,
    } = a;
    item_id == &b.item_id
        && kind == &b.kind
        && title == &b.title
        && opt_image_eq(*has_poster, poster, b.has_poster, &b.poster)
        && progress == &b.progress
        && watched == &b.watched
        && playable == &b.playable
}

fn library_card_same(a: &LibraryCard, b: &LibraryCard) -> bool {
    let LibraryCard {
        item_id,
        kind,
        title,
        has_poster,
        poster,
        shape,
        progress,
        watched,
        playable,
        new_videos,
    } = a;
    item_id == &b.item_id
        && kind == &b.kind
        && title == &b.title
        && opt_image_eq(*has_poster, poster, b.has_poster, &b.poster)
        && shape == &b.shape
        && progress == &b.progress
        && watched == &b.watched
        && playable == &b.playable
        && new_videos == &b.new_videos
}

fn addon_data_same(a: &AddonData, b: &AddonData) -> bool {
    let AddonData {
        transport_url,
        name,
        version,
        types,
        description,
        has_logo,
        logo,
        installed,
        configuration_required,
        configurable,
    } = a;
    transport_url == &b.transport_url
        && name == &b.name
        && version == &b.version
        && types == &b.types
        && description == &b.description
        && opt_image_eq(*has_logo, logo, b.has_logo, &b.logo)
        && installed == &b.installed
        && configuration_required == &b.configuration_required
        && configurable == &b.configurable
}

fn video_row_same(a: &VideoRow, b: &VideoRow) -> bool {
    let VideoRow {
        id,
        title,
        released,
        has_thumbnail,
        thumbnail,
        selected,
        watched,
        progress,
    } = a;
    id == &b.id
        && title == &b.title
        && released == &b.released
        && opt_image_eq(*has_thumbnail, thumbnail, b.has_thumbnail, &b.thumbnail)
        && selected == &b.selected
        && watched == &b.watched
        && progress == &b.progress
}

/// Updates an existing `VecModel` in place using `same` so unchanged rows keep their Slint items
/// instead of tearing down and rebuilding every repeater on each poster load.
fn sync_vec_model_by<T: Clone + 'static>(
    existing: &ModelRc<T>,
    next: Vec<T>,
    same: impl Fn(&T, &T) -> bool,
) -> Option<ModelRc<T>> {
    if let Some(model) = existing.as_any().downcast_ref::<VecModel<T>>() {
        let old_len = model.row_count();
        let new_len = next.len();
        let common = old_len.min(new_len);
        for (index, item) in next.into_iter().enumerate() {
            if index < common {
                if !model.row_data(index).is_some_and(|old| same(&old, &item)) {
                    model.set_row_data(index, item);
                }
            } else {
                model.push(item);
            }
        }
        while model.row_count() > new_len {
            model.remove(model.row_count() - 1);
        }
        return None;
    }
    Some(ModelRc::new(VecModel::from(next)))
}

fn sync_vec_model<T: Clone + PartialEq + 'static>(
    existing: &ModelRc<T>,
    next: Vec<T>,
) -> Option<ModelRc<T>> {
    sync_vec_model_by(existing, next, |a, b| a == b)
}

fn sync_board_rows(window: &MainWindow, rows: &[crate::screens::board::Row], images: &Images) {
    let existing = window.get_rows();
    if let Some(model) = existing.as_any().downcast_ref::<VecModel<CatalogRow>>() {
        if model.row_count() == rows.len()
            && rows.iter().enumerate().all(|(index, row)| {
                model.row_data(index).is_some_and(|old| {
                    old.title.as_str() == row.title
                        && old.shape.as_str() == row.shape
                        && old.menu == row.menu
                })
            })
        {
            for (index, row) in rows.iter().enumerate() {
                if let Some(mut existing_row) = model.row_data(index) {
                    let cards: Vec<PosterData> = row
                        .cards
                        .iter()
                        .map(|card| poster_data(card, images))
                        .collect();
                    if let Some(replaced) =
                        sync_vec_model_by(&existing_row.cards, cards, poster_same)
                    {
                        existing_row.cards = replaced;
                        model.set_row_data(index, existing_row);
                    }
                }
            }
            return;
        }
    }
    window.set_rows(ModelRc::new(VecModel::from(
        rows.iter()
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
}

fn preview_same(old: &PreviewData, preview: &Preview, images: &Images, with_images: bool) -> bool {
    let logo = if with_images {
        slint_image(images, &preview.logo)
    } else {
        None
    };
    let background = if with_images {
        slint_image(images, &preview.background)
    } else {
        None
    };
    old.title.as_str() == preview.title
        && old.runtime.as_str() == preview.runtime
        && old.release_info.as_str() == preview.release
        && old.rating.as_str() == preview.rating
        && old.description.as_str() == preview.description
        && old.has_trailer == preview.has_trailer
        && opt_image_eq(
            old.has_logo,
            &old.logo,
            logo.is_some(),
            &logo.unwrap_or_default(),
        )
        && opt_image_eq(
            old.has_background,
            &old.background,
            background.is_some(),
            &background.unwrap_or_default(),
        )
        && !old.has_panel_backdrop
        && old.in_library == preview.in_library
        && old.watched == preview.watched
        && old.groups.row_count() == preview.groups.len()
        && preview
            .groups
            .iter()
            .enumerate()
            .all(|(i, (label, links))| {
                old.groups.row_data(i).is_some_and(|group| {
                    group.label.as_str() == label
                        && group.links.row_count() == links.len()
                        && links.iter().enumerate().all(|(j, link)| {
                            group.links.row_data(j).is_some_and(|l| l.as_str() == link)
                        })
                })
            })
}

fn sync_discover_previews(window: &MainWindow, previews: &[Preview], images: &Images) {
    let selected = usize::try_from(window.get_discover_selected_index()).unwrap_or(0);
    let existing = window.get_discover_previews();
    if let Some(model) = existing.as_any().downcast_ref::<VecModel<PreviewData>>() {
        let old_len = model.row_count();
        let new_len = previews.len();
        let common = old_len.min(new_len);
        for (index, preview) in previews.iter().enumerate() {
            let with_images = index == selected;
            if index < common {
                if model
                    .row_data(index)
                    .is_some_and(|old| preview_same(&old, preview, images, with_images))
                {
                    continue;
                }
                model.set_row_data(index, preview_data(preview, images, with_images));
            } else {
                model.push(preview_data(preview, images, with_images));
            }
        }
        while model.row_count() > new_len {
            model.remove(model.row_count() - 1);
        }
        return;
    }
    window.set_discover_previews(ModelRc::new(VecModel::from(
        previews
            .iter()
            .enumerate()
            .map(|(index, preview)| preview_data(preview, images, index == selected))
            .collect::<Vec<_>>(),
    )));
}

fn select_input_data(menu: &SelectMenu) -> SelectInputData {
    SelectInputData {
        title: menu.title.clone().into(),
        selected_index: i32::try_from(menu.selected).unwrap_or(0),
        options: ModelRc::new(VecModel::from(
            menu.options
                .iter()
                .map(|(label, selected)| SelectOption {
                    label: label.clone().into(),
                    selected: *selected,
                })
                .collect::<Vec<_>>(),
        )),
    }
}

fn select_input_same(old: &SelectInputData, menu: &SelectMenu) -> bool {
    let selected_index = i32::try_from(menu.selected).unwrap_or_default();
    old.title.as_str() == menu.title
        && old.selected_index == selected_index
        && old.options.row_count() == menu.options.len()
        && menu
            .options
            .iter()
            .enumerate()
            .all(|(i, (label, selected))| {
                old.options
                    .row_data(i)
                    .is_some_and(|opt| opt.label.as_str() == label && opt.selected == *selected)
            })
}

fn sync_inputs<'a>(
    existing: &ModelRc<SelectInputData>,
    next: impl ExactSizeIterator<Item = &'a SelectMenu>,
) -> Option<ModelRc<SelectInputData>> {
    let next: Vec<&SelectMenu> = next.collect();
    if let Some(model) = existing
        .as_any()
        .downcast_ref::<VecModel<SelectInputData>>()
    {
        let old_len = model.row_count();
        let new_len = next.len();
        let common = old_len.min(new_len);
        for (index, menu) in next.iter().enumerate() {
            if index < common {
                if model
                    .row_data(index)
                    .is_some_and(|old| select_input_same(&old, menu))
                {
                    continue;
                }
                model.set_row_data(index, select_input_data(menu));
            } else {
                model.push(select_input_data(menu));
            }
        }
        while model.row_count() > new_len {
            model.remove(model.row_count() - 1);
        }
        return None;
    }
    Some(inputs(&next))
}

pub(crate) fn apply(window: &MainWindow, snapshot: &Snapshot, images: &Images) {
    prune_slint_images(images);
    window.set_status(snapshot.status.clone().into());
    sync_board_rows(window, &snapshot.rows, images);

    window.set_discover_shape(snapshot.discover.shape.into());
    let discover_cards: Vec<PosterData> = snapshot
        .discover
        .cards
        .iter()
        .map(|card| poster_data(card, images))
        .collect();
    if let Some(replaced) =
        sync_vec_model_by(&window.get_discover_cards(), discover_cards, poster_same)
    {
        window.set_discover_cards(replaced);
    }
    if let Some(replaced) = sync_inputs(
        &window.get_discover_inputs(),
        snapshot.discover.inputs.iter().map(|i| &i.menu),
    ) {
        window.set_discover_inputs(replaced);
    }
    sync_discover_previews(window, &snapshot.discover.previews, images);

    let library = &snapshot.library;
    window.set_library_type_title(library.types.menu.title.clone().into());
    window.set_library_type_index(i32::try_from(library.types.menu.selected).unwrap_or_default());
    let library_types: Vec<SelectOption> = library
        .types
        .menu
        .options
        .iter()
        .map(|(label, selected)| SelectOption {
            label: label.clone().into(),
            selected: *selected,
        })
        .collect();
    if let Some(replaced) = sync_vec_model(&window.get_library_types(), library_types) {
        window.set_library_types(replaced);
    }
    let library_sorts: Vec<LibrarySort> = library
        .sorts
        .iter()
        .map(|sort| LibrarySort {
            label: sort.label.clone().into(),
            selected: sort.selected,
        })
        .collect();
    if let Some(replaced) = sync_vec_model(&window.get_library_sorts(), library_sorts) {
        window.set_library_sorts(replaced);
    }
    let library_cards: Vec<LibraryCard> = library
        .cards
        .iter()
        .map(|card| {
            let poster = slint_image(images, &card.poster);
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
        .collect();
    if let Some(replaced) = sync_vec_model_by(
        &window.get_library_cards(),
        library_cards,
        library_card_same,
    ) {
        window.set_library_cards(replaced);
    }
    window.set_library_message(library.message.clone().into());
    window.set_library_has_more(library.has_more);
    window.set_library_grid_key(library.grid_key.clone().into());

    let addons: Vec<AddonData> =
        crate::screens::addons::visible(&snapshot.addons.cards, &window.get_addons_query())
            .into_iter()
            .map(|card| {
                let logo = slint_image(images, &card.logo);
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
            .collect();
    if let Some(replaced) = sync_vec_model_by(&window.get_addons(), addons, addon_data_same) {
        window.set_addons(replaced);
    }
    window.set_calendar(calendar_view(&snapshot.calendar, images));
    if let Some(replaced) = sync_inputs(
        &window.get_addon_inputs(),
        snapshot.addons.inputs.iter().map(|i| &i.menu),
    ) {
        window.set_addon_inputs(replaced);
    }
    window.set_addon_pending(addon_pending(&snapshot.addons.pending, images));
    window.set_addons_message(snapshot.addons.message.clone().into());

    window.set_settings(settings_view(&snapshot.settings));
    show_mods(window, &snapshot.mods);
    player_view(window, &snapshot.player, images);

    let details = &snapshot.details;
    window.set_details_has_preview(details.preview.is_some());
    if let Some(preview) = &details.preview {
        let mut data = preview_data(preview, images, true);
        // Web: the column on the right has `backdrop-filter: blur(15px)`, so it paints the artwork
        // behind it blurred under its own 0.4 wash. Slint has no backdrop filter; the column draws
        // this copy instead, lined up with the page's layer.
        let backdrop = preview
            .background
            .as_ref()
            .map(|key| crate::plumbing::images::backdrop_key(key));
        let backdrop = slint_image(images, &backdrop);
        data.has_panel_backdrop = backdrop.is_some();
        data.panel_backdrop = backdrop.unwrap_or_default();
        window.set_details_preview(data);
    } else {
        window.set_details_preview(PreviewData::default());
    }
    let streams: Vec<StreamData> = details
        .streams
        .iter()
        .map(|source| StreamData {
            name: source.name.clone().into(),
            description: source.description.clone().into(),
            addon: source.addon.clone().into(),
        })
        .collect();
    if let Some(replaced) = sync_vec_model(&window.get_streams(), streams) {
        window.set_streams(replaced);
    }
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
    window.set_details_season_watched(details.season_watched);
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
    let details_seasons: Vec<SelectOption> = details
        .seasons
        .iter()
        .enumerate()
        .map(|(index, season)| SelectOption {
            label: season.label.clone().into(),
            selected: index == details.season,
        })
        .collect();
    if let Some(replaced) = sync_vec_model(&window.get_details_seasons(), details_seasons) {
        window.set_details_seasons(replaced);
    }
    let details_episodes: Vec<VideoRow> = details
        .episodes
        .iter()
        .map(|episode| {
            let thumbnail = slint_image(images, &episode.thumbnail);
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
        .collect();
    if let Some(replaced) = sync_vec_model_by(
        &window.get_details_episodes(),
        details_episodes,
        video_row_same,
    ) {
        window.set_details_episodes(replaced);
    }
}

/// The preview panel's content — the same panel Discover opens on hover and the details screen
/// shows beside its list. Non-selected Discover previews drop their `Image` handles so previously
/// hovered cards do not retain decoded `#logo` and `#blur` buffers inside the `VecModel`.
fn preview_data(preview: &Preview, images: &Images, with_images: bool) -> PreviewData {
    let logo = if with_images {
        slint_image(images, &preview.logo)
    } else {
        None
    };
    let background = if with_images {
        slint_image(images, &preview.background)
    } else {
        None
    };
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
    let logo = slint_image(images, &pending.logo);
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
                    let poster = slint_image(images, &slot.poster);
                    let thumbnail = slint_image(images, &slot.thumbnail);
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
    let card = |kind: mods::catalogue::Kind, card: &ModCard| {
        let armed_here = data.armed.as_deref() == Some(card.filename.as_str());
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
                if card.builtin {
                    Vec::new()
                } else {
                    card.warnings.clone()
                }
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
            )),
            enabled: card.enabled,
            armed: armed_here,
            state: state.into(),
            removable: card.removable,
            has_settings: card.has_settings,
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
        themes_note: data.themes_note.clone().into(),
        settings_plugins: ModelRc::new(VecModel::from(
            data.settings_plugins
                .iter()
                .map(settings_plugin)
                .collect::<Vec<_>>(),
        )),
        settings_plugin: data.plugin.clone().into(),
        settings_title: data
            .settings_plugins
            .iter()
            .find(|entry| entry.filename == data.plugin)
            .map(|entry| entry.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| data.plugin.clone())
            .into(),
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
        settings_dirty: data.edits.dirty(),
        log_source_options: menu_options(&data.logs.source_options, data.logs.source_index),
        log_level_options: menu_options(&data.logs.level_options, data.logs.level_index),
        log_source_index: data.logs.source_index,
        log_level_index: data.logs.level_index,
        log_search: data.logs.filter.search.clone().into(),
        log_rows: ModelRc::new(VecModel::from(
            data.logs.rows.iter().map(log_row).collect::<Vec<_>>(),
        )),
        log_empty: data.logs.empty.clone().into(),
        log_detail_heading: data.logs.detail_heading.clone().into(),
        log_open_id: i32::try_from(data.logs.open_id).unwrap_or(0),
        log_active_index: data.logs.active_index,
        log_selected_count: i32::try_from(data.logs.selected_count).unwrap_or(0),
        log_count: data.logs.count.clone().into(),
        log_filtered: data.logs.filtered,
        log_prepended: 0,
        log_prepend_seq: 0,
        log_detail: ModelRc::new(VecModel::from(
            data.logs
                .detail
                .iter()
                .map(|line| SharedString::from(line.clone()))
                .collect::<Vec<_>>(),
        )),
        log_clear_armed: data.logs.clear_armed,
        about_build: about_lines(&data.about.build),
        about_places: about_lines(&data.about.places),
        about_counts: about_lines(&data.about.counts),
        about_enabled: about_lines(&data.about.enabled),
        about_license: data.about.license.clone().into(),
        about_export: data.about.export.clone().into(),
    }
}

/// Draws the Logs page's list again after a record landed, on the window's thread. Only the list's
/// own properties change: the menus, the search box and the scroll stay exactly as they were, so a
/// line arriving mid-sentence never moves what the reader is looking at.
pub(crate) fn refresh_logs(
    weak: &Weak<MainWindow>,
    latest: &Arc<Mutex<Snapshot>>,
    logs: Arc<crate::plumbing::logs::Logs>,
    selection: Arc<Mutex<mods::log_selection::LogSelection>>,
    armed: Arc<Mutex<bool>>,
) {
    let latest = latest.clone();
    let _ = weak.upgrade_in_event_loop(move |window| {
        let armed = *armed.lock().unwrap();
        let entries = logs.entries();
        let sel = selection.lock().unwrap().clone();
        {
            let mut snapshot = latest.lock().unwrap();
            let notice = snapshot.mods.notice.clone();
            snapshot.mods = snapshot
                .mods
                .clone()
                .with_logs(&entries, &sel, armed)
                .with_notice(notice);
        }
        apply_logs(&window, &entries, &sel, armed);
    });
}

/// Applies the current log list, selection, and detail panel onto `window`, updating row selection
/// flags in place when the filtered row ids are unchanged and reporting live prepends so a
/// scrolled reader stays anchored on the same rows.
pub(crate) fn apply_logs(
    window: &MainWindow,
    entries: &[crate::plumbing::logs::Entry],
    sel: &mods::log_selection::LogSelection,
    armed: bool,
) {
    let logs = mods::logs::data(entries, sel, armed);
    let old = window.get_mods();
    let mut data = old.clone();
    data.log_source_index = logs.source_index;
    data.log_level_index = logs.level_index;
    data.log_search = logs.filter.search.into();
    data.log_empty = logs.empty.into();
    data.log_count = logs.count.into();
    data.log_filtered = logs.filtered;
    data.log_detail_heading = logs.detail_heading.into();
    data.log_detail = ModelRc::new(VecModel::from(
        logs.detail
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ));
    data.log_open_id = i32::try_from(logs.open_id).unwrap_or(0);
    data.log_active_index = logs.active_index;
    data.log_selected_count = i32::try_from(logs.selected_count).unwrap_or(0);
    data.log_clear_armed = logs.clear_armed;
    reconcile_log_rows(&old, &logs.rows, &mut data);
    window.set_mods(data);
}

/// Keeps the existing `log_rows` model in place when the visible row ids are unchanged, and
/// detects live records prepended at the top under an unchanged filter so the UI can preserve
/// the reader's scroll offset.
fn reconcile_log_rows(old: &ModsViewData, rows: &[mods::logs::Row], next: &mut ModsViewData) {
    next.log_prepend_seq = old.log_prepend_seq;
    next.log_prepended = 0;
    let same_filter = old.log_source_index == next.log_source_index
        && old.log_level_index == next.log_level_index
        && old.log_search == next.log_search;
    let same_rows = old.log_rows.row_count() == rows.len()
        && rows.iter().enumerate().all(|(at, row)| {
            old.log_rows
                .row_data(at)
                .is_some_and(|existing| existing.id == row.id as i32)
        });
    if same_rows {
        if let Some(existing) = old.log_rows.as_any().downcast_ref::<VecModel<ModsLogRow>>() {
            for (at, row) in rows.iter().enumerate() {
                if let Some(mut cur) = existing.row_data(at) {
                    if cur.selected != row.selected {
                        cur.selected = row.selected;
                        existing.set_row_data(at, cur);
                    }
                }
            }
            next.log_rows = old.log_rows.clone();
            return;
        }
    } else if same_filter {
        if let Some(old_top) = old.log_rows.row_data(0).map(|row| row.id) {
            if let Some(prepended) = rows.iter().position(|row| row.id as i32 == old_top) {
                if prepended > 0 {
                    next.log_prepended = i32::try_from(prepended).unwrap_or(0);
                    next.log_prepend_seq = old.log_prepend_seq.wrapping_add(1);
                }
            }
        }
    }
    next.log_rows = ModelRc::new(VecModel::from(rows.iter().map(log_row).collect::<Vec<_>>()));
}

/// One filter menu of the Logs page: its choices, with the one the menu opens on marked.
fn menu_options(labels: &[String], selected: i32) -> ModelRc<SelectOption> {
    ModelRc::new(VecModel::from(
        labels
            .iter()
            .enumerate()
            .map(|(at, label)| SelectOption {
                label: label.clone().into(),
                selected: i32::try_from(at).unwrap_or(-1) == selected,
            })
            .collect::<Vec<_>>(),
    ))
}

/// One record of the Logs page. A record id past what the window's integer holds keeps its low
/// bits, which still tell one row from its neighbours.
fn log_row(row: &mods::logs::Row) -> ModsLogRow {
    ModsLogRow {
        id: row.id as i32,
        source: row.source.clone().into(),
        level: row.level.clone().into(),
        level_id: row.level_id.clone().into(),
        time: row.time.clone().into(),
        text: row.text.clone().into(),
        selected: row.selected,
    }
}

/// One section of the About page, as the rows it draws.
fn about_lines(lines: &[mods::about::Line]) -> ModelRc<ModsAboutLine> {
    ModelRc::new(VecModel::from(
        lines
            .iter()
            .map(|line| ModsAboutLine {
                label: line.label.clone().into(),
                value: line.value.clone().into(),
            })
            .collect::<Vec<_>>(),
    ))
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
    let mut field = SettingsFieldData {
        key: control.field.key.into(),
        label: control.field.label.into(),
        kind: "text".into(),
        hint: control.hint().into(),
        on: false,
        text: "".into(),
        options: ModelRc::new(VecModel::from(Vec::<SharedString>::new())),
        number: 0.0,
        min: 0.0,
        max: 1.0,
        shown: "".into(),
    };
    match control.field.kind {
        mods::builtin::FieldKind::Toggle { .. } => {
            field.kind = "toggle".into();
            field.on = control.edited.as_bool().unwrap_or(false);
        }
        mods::builtin::FieldKind::Text { .. } => {
            field.kind = "text".into();
            field.text = control.edited.as_str().unwrap_or_default().into();
        }
    }
    field
}

fn trouble(data: &crate::screens::mods::Data) -> Vec<String> {
    let edits = &data.edits;
    if !edits.filename.is_empty() && !edits.error.is_empty() {
        vec![edits.error.clone()]
    } else {
        Vec::new()
    }
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

/// Sets every property the Mods section draws from: the page on screen, the menu with it, and the
/// content itself. The menu is written only when its rows differ from the ones the window holds,
/// because a new model tears down every row -- including the one a press may be standing on. Which
/// row is marked is not in the model: the menu marks the page `data.page` names.
pub(crate) fn show_mods(window: &MainWindow, data: &crate::screens::mods::Data) {
    let old = window.get_mods();
    let mut view = mods_view(data);
    keep_settings_fields(window, &mut view);
    reconcile_log_rows(&old, &data.logs.rows, &mut view);
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

/// Keeps the settings field rows the window already holds when the same plugin's page is drawn
/// again. A fresh model tears every row down, and the text box a person is typing into loses its
/// focus with them; the data of each row is updated in place instead, so the box survives the
/// repaint and the letters after the first are not typed into an editor that no longer exists.
fn keep_settings_fields(window: &MainWindow, view: &mut ModsViewData) {
    let old = window.get_mods();
    if old.settings_plugin != view.settings_plugin {
        return;
    }
    let model = old.settings_fields.clone();
    let Some(fields) = model.as_any().downcast_ref::<VecModel<SettingsFieldData>>() else {
        return;
    };
    let keys = |fields: &ModelRc<SettingsFieldData>| -> Vec<String> {
        (0..fields.row_count())
            .filter_map(|at| fields.row_data(at))
            .map(|field| field.key.to_string())
            .collect()
    };
    if keys(&view.settings_fields) != keys(&old.settings_fields) {
        return;
    }
    for at in 0..view.settings_fields.row_count() {
        if let Some(field) = view.settings_fields.row_data(at) {
            fields.set_row_data(at, field);
        }
    }
    view.settings_fields = model;
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
    let poster = slint_image(images, &card.poster);
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

fn inputs(menus: &[&SelectMenu]) -> ModelRc<SelectInputData> {
    ModelRc::new(VecModel::from(
        menus
            .iter()
            .map(|menu| select_input_data(menu))
            .collect::<Vec<_>>(),
    ))
}

pub(crate) fn settings_view(data: &SettingsData) -> SettingsViewData {
    SettingsViewData {
        user_email: data.user_email.clone().into(),
        logged_in: data.logged_in,
        trakt_authenticated: data.trakt_authenticated,
        streaming_server_url: data.streaming_server_url.clone().into(),
        ui_language: select_input_data(&data.ui_language),
        quit_on_close: data.quit_on_close,
        esc_exit_fullscreen: data.esc_exit_fullscreen,
        hide_spoilers: data.hide_spoilers,
        subtitles_language: select_input_data(&data.subtitles_language),
        subtitles_size: select_input_data(&data.subtitles_size),
        subtitles_text_color: select_input_data(&data.subtitles_text_color),
        subtitles_bg_color: select_input_data(&data.subtitles_bg_color),
        subtitles_outline_color: select_input_data(&data.subtitles_outline_color),
        audio_language: select_input_data(&data.audio_language),
        surround_sound: data.surround_sound,
        seek_time: select_input_data(&data.seek_time),
        seek_short_time: select_input_data(&data.seek_short_time),
        play_in_background: data.play_in_background,
        binge_watching: data.binge_watching,
        next_video_duration: select_input_data(&data.next_video_duration),
        external_player: select_input_data(&data.external_player),
        hardware_decoding: data.hardware_decoding,
        pause_on_minimize: data.pause_on_minimize,
        caching: select_input_data(&data.caching),
        torrent_profile: select_input_data(&data.torrent_profile),
        transcoding_profile: select_input_data(&data.transcoding_profile),
    }
}

/// Everything the player screen draws that the core decided. The values mpv decides -- where the
/// play head is, the tracks, the volume -- are set by the wiring as they change, so they are left
/// alone here and do not flicker back to their defaults on every core event.
fn player_view(window: &MainWindow, data: &PlayerData, images: &Images) {
    window.set_playing(data.open);
    window.set_player_title(data.title.clone().into());
    window.set_player_error(data.error.clone().into());
    let logo = slint_image(images, &data.logo);
    window.set_player_has_logo(logo.is_some());
    window.set_player_logo(logo.unwrap_or_default());
    let backdrop = slint_image(images, &data.background);
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
                thumbnail: slint_image(images, &episode.thumbnail).unwrap_or_default(),
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
        window.set_player_preview(preview_data(preview, images, true));
    } else {
        window.set_player_preview(PreviewData::default());
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
