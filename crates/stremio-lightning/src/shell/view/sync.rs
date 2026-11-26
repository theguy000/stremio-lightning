//! UI-thread Slint texture cache (`SLINT_IMAGES`), incremental `ModelMirror<K, Dst>` list
//! diffing (zero `slint::Image` comparisons), and shared view builders.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use slint::{Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel};

use crate::plumbing::images::{DecodedImage, Images};
use crate::screens::{addons, details, library, videos, Card, Preview, SelectMenu};
use crate::{
    AddonData, LibraryCard, LibrarySort, LinkGroup, PosterData, PreviewData, SelectInputData,
    SelectOption, StreamData, VideoRow,
};

thread_local! {
    /// UI-thread cache from image key to `(DecodedImage::rev, slint::Image)` so unchanged frames
    /// share the same `SharedPixelBuffer` allocation and GPU texture, while entries evicted from
    /// `Images` are pruned immediately on `apply_mask`.
    static SLINT_IMAGES: RefCell<HashMap<String, (u64, Image)>> = RefCell::new(HashMap::new());
    static MIRRORS: RefCell<ViewMirrors> = RefCell::new(ViewMirrors::default());
}

fn to_slint_image(decoded: &DecodedImage) -> Image {
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        decoded.as_raw(),
        decoded.width(),
        decoded.height(),
    ))
}

pub(super) fn image_rev(images: &Images, key: &Option<String>) -> Option<u64> {
    images.get(key).map(|d| d.rev())
}

pub(super) fn slint_image(images: &Images, key: &Option<String>) -> Option<Image> {
    let key = key.as_ref()?;
    let decoded = images.get(&Some(key.clone()))?;
    if !key.contains('#') {
        return Some(to_slint_image(&decoded));
    }
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

pub(super) fn prune_slint_images(images: &Images) {
    SLINT_IMAGES.with(|cell| {
        cell.borrow_mut().retain(|key, _| images.contains(key));
    });
}

/// Incremental Slint `VecModel<Dst>` mirror that diffs plain Rust keys `K: PartialEq` in place.
///
/// Never compares `slint::Image` (avoiding Slint 1.18.1's `Image::default() != Image::default()`
/// bug), skips constructing `Dst` for unchanged rows, preserves Slint repeater item identity, and
/// automatically re-binds when the property was externally reset or belongs to a new `MainWindow`.
pub(crate) struct ModelMirror<K, Dst> {
    keys: Vec<K>,
    model: Rc<VecModel<Dst>>,
}

impl<K, Dst> Default for ModelMirror<K, Dst> {
    fn default() -> Self {
        Self {
            keys: Vec::new(),
            model: Rc::new(VecModel::default()),
        }
    }
}

impl<K: PartialEq, Dst: Clone + 'static> ModelMirror<K, Dst> {
    pub(crate) fn sync<Src>(
        &mut self,
        existing: &ModelRc<Dst>,
        items: &[Src],
        key_of: impl Fn(usize, &Src) -> K,
        build: impl Fn(usize, &Src) -> Dst,
    ) -> Option<ModelRc<Dst>> {
        let attached = existing
            .as_any()
            .downcast_ref::<VecModel<Dst>>()
            .is_some_and(|m| std::ptr::eq(m, Rc::as_ptr(&self.model)));
        if !attached {
            self.keys = items
                .iter()
                .enumerate()
                .map(|(i, s)| key_of(i, s))
                .collect();
            let built: Vec<Dst> = items.iter().enumerate().map(|(i, s)| build(i, s)).collect();
            self.model = Rc::new(VecModel::from(built));
            return Some(ModelRc::from(self.model.clone()));
        }
        let common = self.keys.len().min(items.len());
        for (i, item) in items.iter().enumerate() {
            let key = key_of(i, item);
            if i < common {
                if self.keys[i] != key {
                    self.keys[i] = key;
                    self.model.set_row_data(i, build(i, item));
                }
            } else {
                self.keys.push(key);
                self.model.push(build(i, item));
            }
        }
        if self.keys.len() > items.len() {
            self.keys.truncate(items.len());
            while self.model.row_count() > items.len() {
                self.model.remove(self.model.row_count() - 1);
            }
        }
        None
    }
}

pub(super) type BoardRowMirror = (
    String,
    &'static str,
    bool,
    ModelMirror<(Card, Option<u64>), PosterData>,
);

#[derive(Default)]
pub(super) struct ViewMirrors {
    pub board_rows: Vec<BoardRowMirror>,
    pub discover_cards: ModelMirror<(Card, Option<u64>), PosterData>,
    pub discover_inputs: ModelMirror<SelectMenu, SelectInputData>,
    pub discover_previews: ModelMirror<(Preview, Option<u64>, Option<u64>), PreviewData>,
    pub library_types: ModelMirror<(String, bool), SelectOption>,
    pub library_sorts: ModelMirror<(String, bool), LibrarySort>,
    pub library_cards: ModelMirror<(library::Card, Option<u64>), LibraryCard>,
    pub addons_cards: ModelMirror<(addons::AddonCard, Option<u64>), AddonData>,
    pub addons_inputs: ModelMirror<SelectMenu, SelectInputData>,
    pub details_streams: ModelMirror<details::Source, StreamData>,
    pub details_seasons: ModelMirror<(String, bool), SelectOption>,
    pub details_episodes: ModelMirror<(videos::Episode, bool, Option<u64>), VideoRow>,
}

pub(super) fn with_mirrors<R>(f: impl FnOnce(&mut ViewMirrors) -> R) -> R {
    MIRRORS.with(|cell| f(&mut cell.borrow_mut()))
}

pub(super) fn select_input_data(menu: &SelectMenu) -> SelectInputData {
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

pub(super) fn menu_options(labels: &[String], selected: i32) -> ModelRc<SelectOption> {
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

pub(super) fn poster_data(card: &Card, images: &Images) -> PosterData {
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

pub(super) fn preview_data(preview: &Preview, images: &Images, with_images: bool) -> PreviewData {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn model_mirror_updates_changed_rows_in_place_and_rebinds_after_reset() {
        let mut mirror = ModelMirror::<(u32, Option<u64>), SharedString>::default();
        let builds = Cell::new(0usize);
        let src1 = vec![(1u32, Some(10u64)), (2, None)];
        let bound = mirror
            .sync(
                &ModelRc::default(),
                &src1,
                |_, item| *item,
                |_, (id, rev)| {
                    builds.set(builds.get() + 1);
                    SharedString::from(format!("{id}:{rev:?}"))
                },
            )
            .expect("first sync binds VecModel");
        assert_eq!(builds.get(), 2);
        assert_eq!(bound.row_count(), 2);

        // Second sync where only row 1's image rev changed: builds only 1 row and mutates in place!
        let src2 = vec![(1u32, Some(10u64)), (2, Some(99))];
        assert!(mirror
            .sync(
                &bound,
                &src2,
                |_, item| *item,
                |_, (id, rev)| {
                    builds.set(builds.get() + 1);
                    SharedString::from(format!("{id}:{rev:?}"))
                }
            )
            .is_none());
        assert_eq!(builds.get(), 3);
        assert_eq!(bound.row_data(1).unwrap().as_str(), "2:Some(99)");

        // External reset (`ModelRc::default()`) re-binds cleanly.
        assert!(mirror
            .sync(
                &ModelRc::default(),
                &src2,
                |_, item| *item,
                |_, (id, rev)| { SharedString::from(format!("{id}:{rev:?}")) }
            )
            .is_some());
    }
}
