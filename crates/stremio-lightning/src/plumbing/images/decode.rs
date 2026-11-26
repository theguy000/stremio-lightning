//! Pure image key parsing, decoding, portrait corner pre-rounding, and in-memory variant
//! derivation (no Slint, no network).

use super::cache::{Cached, DecodedImage};

/// The size each kind of image is stored at, so a 4K poster is never held in memory.
pub(crate) const LOGO_SUFFIX: &str = "#logo";
/// Web: `MetaPreview .background-image { filter: blur(10px); opacity: 0.3 }`.
pub(crate) const BLUR_SUFFIX: &str = "#blur";
/// Artwork that fills the window, so it must not be squeezed into a poster.
pub(crate) const PAGE_SUFFIX: &str = "#page";
/// Web: the details column's `backdrop-filter: blur(15px)` over the artwork behind it.
pub(crate) const BACKDROP_SUFFIX: &str = "#backdrop";

const BLUR_SIGMA: f32 = 3.0;
const BACKDROP_SIGMA: f32 = 3.5;

/// Standard `MetaItem` portrait poster aspect ratio (`ui/board.slint`: `1.464`).
const POSTER_ASPECT: f32 = 1.464;
/// Anti-aliased corner radius as a fraction of poster width (~`0.75rem` on a ~200px card),
/// pre-baked into portrait poster alpha so `MetaItem` needs no offscreen rounded-clip FBO.
const POSTER_CORNER_RATIO: f32 = 0.055;

/// The URL, the target bounding box, and the optional Gaussian blur sigma for a key.
pub(crate) fn source(key: &str) -> (&str, (u32, u32), Option<f32>) {
    if let Some(url) = key.strip_suffix(LOGO_SUFFIX) {
        (url, (672, 168), None)
    } else if let Some(url) = key.strip_suffix(BLUR_SUFFIX) {
        (url, (150, 225), Some(BLUR_SIGMA))
    } else if let Some(url) = key.strip_suffix(BACKDROP_SUFFIX) {
        (url, (240, 135), Some(BACKDROP_SIGMA))
    } else if let Some(url) = key.strip_suffix(PAGE_SUFFIX) {
        (url, (1280, 720), None)
    } else {
        (key, (240, 360), None)
    }
}

pub(crate) fn is_plain_poster(key: &str) -> bool {
    !key.ends_with(LOGO_SUFFIX)
        && !key.ends_with(BLUR_SUFFIX)
        && !key.ends_with(BACKDROP_SUFFIX)
        && !key.ends_with(PAGE_SUFFIX)
}

pub(crate) fn is_blurred(key: &str) -> bool {
    key.ends_with(BLUR_SUFFIX) || key.ends_with(BACKDROP_SUFFIX)
}

/// A key whose picture a click is waiting on takes the urgent lane.
pub(crate) fn is_urgent(key: &str) -> bool {
    is_blurred(key) || key.ends_with(LOGO_SUFFIX) || key.ends_with(PAGE_SUFFIX)
}

pub(crate) fn logo_key(url: impl std::fmt::Display) -> String {
    format!("{url}{LOGO_SUFFIX}")
}

pub(crate) fn blur_key(url: impl std::fmt::Display) -> String {
    format!("{url}{BLUR_SUFFIX}")
}

pub(crate) fn page_key(url: impl std::fmt::Display) -> String {
    format!("{url}{PAGE_SUFFIX}")
}

/// The blurred copy of a page-size artwork that the details column paints under itself.
pub(crate) fn backdrop_key(key: &str) -> String {
    key.strip_suffix(PAGE_SUFFIX)
        .map_or_else(|| key.to_owned(), |url| format!("{url}{BACKDROP_SUFFIX}"))
}

/// Decodes raw image bytes into the target variant for `key`.
pub(crate) fn decode_bytes(bytes: &[u8], key: &str) -> Option<DecodedImage> {
    let (_, size, blur) = source(key);
    let image = image::load_from_memory(bytes)
        .map_err(|error| eprintln!("[image] decode failed: {error}"))
        .ok()?
        .thumbnail(size.0, size.1)
        .to_rgba8();
    let image = if is_plain_poster(key) {
        prepare_portrait_poster(image)
    } else {
        image
    };
    let image = match blur {
        Some(sigma) => image::imageops::blur(&image, sigma),
        None => image,
    };
    Some(DecodedImage::new(image))
}

/// Picks the best cached unblurred source for `key` so derivation runs off the cache lock.
pub(crate) fn find_derivation_source(cache: &Cached, key: &str) -> Option<DecodedImage> {
    let (url, size, blur) = source(key);
    cache
        .iter()
        .filter(|(other, _)| !is_blurred(other) && source(other).0 == url)
        .map(|(_, image)| image)
        .filter(|image| blur.is_some() || (image.width() >= size.0 && image.height() >= size.1))
        .max_by_key(|image| image.width() * image.height())
        .cloned()
}

/// Resizes and optionally blurs a previously decoded image without re-downloading.
pub(crate) fn prepare_variant(source_img: &DecodedImage, key: &str) -> DecodedImage {
    let (_, (width, height), blur) = source(key);
    if blur.is_none() && (source_img.width(), source_img.height()) == (width, height) {
        return source_img.clone();
    }
    let mut image = source_img.rgba().clone();
    if blur.is_some() {
        // Restore opaque corner alpha if deriving a blurred backdrop from a pre-rounded poster.
        for pixel in image.pixels_mut() {
            pixel[3] = 255;
        }
    }
    let image = if (image.width(), image.height()) == (width, height) {
        image
    } else {
        image::imageops::thumbnail(&image, width, height)
    };
    let image = match blur {
        Some(sigma) => image::imageops::blur(&image, sigma),
        None => image,
    };
    DecodedImage::new(image)
}

/// Cover-crops a portrait poster to `POSTER_ASPECT` and modulates only the alpha channel in the
/// four corners (leaving RGB untouched so `#blur` derivation restores opaque corners cleanly).
fn prepare_portrait_poster(image: image::RgbaImage) -> image::RgbaImage {
    let (w, h) = image.dimensions();
    if w == 0 || h == 0 || (h as f32) <= (w as f32) * 1.2 {
        return image;
    }
    let target_h = ((w as f32) * POSTER_ASPECT).round() as u32;
    let mut cropped = if target_h > 0 && target_h < h {
        let top = (h - target_h) / 2;
        image::imageops::crop_imm(&image, 0, top, w, target_h).to_image()
    } else if target_h > h {
        let target_w = ((h as f32) / POSTER_ASPECT).round() as u32;
        if target_w > 0 && target_w < w {
            let left = (w - target_w) / 2;
            image::imageops::crop_imm(&image, left, 0, target_w, h).to_image()
        } else {
            image
        }
    } else {
        image
    };
    let radius = (cropped.width() as f32) * POSTER_CORNER_RATIO;
    round_corners_alpha(&mut cropped, radius);
    cropped
}

fn round_corners_alpha(image: &mut image::RgbaImage, radius: f32) {
    let (w, h) = image.dimensions();
    let r = radius.min(w as f32 * 0.5).min(h as f32 * 0.5);
    if r <= 0.5 {
        return;
    }
    let limit = r.ceil() as u32;
    for y in 0..h {
        let dy = if y < limit {
            r - (y as f32 + 0.5)
        } else if y >= h - limit {
            (y as f32 + 0.5) - (h as f32 - r)
        } else {
            continue;
        };
        if dy <= 0.0 {
            continue;
        }
        for x in 0..limit {
            let dx = r - (x as f32 + 0.5);
            if dx <= 0.0 {
                continue;
            }
            let dist = (dx * dx + dy * dy).sqrt();
            let cov = (r + 0.5 - dist).clamp(0.0, 1.0);
            if cov < 1.0 {
                for px in [x, w - 1 - x] {
                    let pixel = image.get_pixel_mut(px, y);
                    pixel[3] = ((pixel[3] as f32) * cov).round() as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn key_sizes_and_backdrop_key() {
        assert_eq!(
            source("https://example.com/a.jpg#logo"),
            ("https://example.com/a.jpg", (672, 168), None)
        );
        assert_eq!(
            source("https://example.com/a.jpg#blur"),
            ("https://example.com/a.jpg", (150, 225), Some(BLUR_SIGMA))
        );
        assert_eq!(
            source("https://example.com/a.jpg#backdrop"),
            (
                "https://example.com/a.jpg",
                (240, 135),
                Some(BACKDROP_SIGMA)
            )
        );
        assert_eq!(
            source("https://example.com/a.jpg"),
            ("https://example.com/a.jpg", (240, 360), None)
        );
        assert_eq!(
            backdrop_key("https://example.com/a.jpg#page"),
            "https://example.com/a.jpg#backdrop"
        );
        assert_eq!(
            backdrop_key("https://example.com/a.jpg#blur"),
            "https://example.com/a.jpg#blur"
        );
    }

    #[test]
    fn portrait_posters_are_pre_rounded_while_landscape_thumbs_stay_square_cornered() {
        let portrait = image::RgbaImage::from_pixel(240, 360, image::Rgba([20, 40, 60, 255]));
        let rounded = prepare_portrait_poster(portrait);
        assert_eq!((rounded.width(), rounded.height()), (240, 351));
        assert_eq!(rounded.get_pixel(0, 0)[3], 0);
        assert_eq!(rounded.get_pixel(239, 350)[3], 0);
        assert_eq!(rounded.get_pixel(120, 175)[3], 255);
        assert_eq!(&rounded.get_pixel(0, 0).0[..3], &[20, 40, 60]);

        let thumb = image::RgbaImage::from_pixel(240, 135, image::Rgba([20, 40, 60, 255]));
        let untouched = prepare_portrait_poster(thumb);
        assert_eq!(untouched.dimensions(), (240, 135));
        assert_eq!(untouched.get_pixel(0, 0)[3], 255);
    }

    #[test]
    fn blur_derives_to_target_box_and_never_upscales_blurred_to_page() {
        let mut cache = Cached::default();
        cache.insert(
            "https://example.com/a.jpg".to_owned(),
            DecodedImage::new(image::RgbaImage::new(300, 450)),
            &HashSet::new(),
        );
        let src = find_derivation_source(&cache, "https://example.com/a.jpg#blur").unwrap();
        let blurred = prepare_variant(&src, "https://example.com/a.jpg#blur");
        assert_eq!((blurred.width(), blurred.height()), (150, 225));

        let mut blur_only = Cached::default();
        blur_only.insert(
            "https://example.com/a.jpg#blur".to_owned(),
            DecodedImage::new(image::RgbaImage::new(150, 225)),
            &HashSet::new(),
        );
        assert!(find_derivation_source(&blur_only, "https://example.com/a.jpg#page").is_none());
        assert!(find_derivation_source(&blur_only, "https://example.com/b.jpg#page").is_none());
    }
}
