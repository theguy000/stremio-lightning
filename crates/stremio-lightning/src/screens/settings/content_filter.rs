//! Pure stream filtering and Content Settings state mutations (zero I/O, zero Slint).

use stremio_core::types::resource::Stream;

use super::content::parse_catalog_item_id;
use crate::plumbing::local_settings::{CatalogKey, ContentSettings, StreamFilterPreset};

/// Trims and case-insensitively deduplicates a list of tags.
pub fn normalize_tags<'a>(tags: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out = Vec::new();
    for raw in tags {
        let tag = raw.trim();
        if tag.is_empty() {
            continue;
        }
        if !out
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(tag))
        {
            out.push(tag.to_owned());
        }
    }
    out
}

/// Matches `tag` against lowercased `haystack`:
/// - Pure ASCII alphanumeric/space tags use alphanumeric word boundaries (so `"ts"` never matches
///   `"DTS"` or `"subtitles"`, `"dv"` never matches `"Adv"`, and `"4k"` never matches `"14k"`).
/// - Symbol/emoji tags (e.g. `"5.1"`, `"RD+"`, flags) use case-insensitive substring matching.
pub fn tag_matches(haystack_lower: &str, tag: &str) -> bool {
    let needle = tag.trim().to_lowercase();
    if needle.is_empty() {
        return false;
    }
    let words: Vec<&str> = needle.split_whitespace().collect();
    if words.is_empty() {
        return false;
    }
    let ascii_word_tag = words
        .iter()
        .all(|w| w.bytes().all(|b| b.is_ascii_alphanumeric()));
    if !ascii_word_tag {
        return haystack_lower.contains(&needle);
    }
    let tokens: Vec<&str> = haystack_lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect();
    tokens
        .windows(words.len())
        .any(|win| win == words.as_slice())
}

/// Returns whether a stream from `addon_url` passes disabled-addon and active-preset filters.
pub fn stream_matches(addon_url: &str, stream: &Stream, content: &ContentSettings) -> bool {
    if content
        .disabled_stream_addons
        .iter()
        .any(|disabled| disabled == addon_url)
    {
        return false;
    }
    let Some(active_id) = content.active_stream_preset.as_deref() else {
        return true;
    };
    let Some(preset) = content.stream_presets.iter().find(|p| p.id == active_id) else {
        return true;
    };
    if preset.include_tags.is_empty() && preset.exclude_tags.is_empty() {
        return true;
    }
    let haystack = format!(
        "{} {} {}",
        stream.name.as_deref().unwrap_or(""),
        stream.description.as_deref().unwrap_or(""),
        stream.behavior_hints.filename.as_deref().unwrap_or(""),
    )
    .to_lowercase();
    if !preset.include_tags.is_empty()
        && !preset
            .include_tags
            .iter()
            .any(|tag| tag_matches(&haystack, tag))
    {
        return false;
    }
    !preset
        .exclude_tags
        .iter()
        .any(|tag| tag_matches(&haystack, tag))
}

fn reorder_vec<T: Clone + PartialEq>(
    current_order: &mut Vec<T>,
    visible: &[T],
    from: usize,
    to: usize,
) -> bool {
    if from >= visible.len() || to >= visible.len() || from == to {
        return false;
    }
    let mut order = visible.to_vec();
    let moved = order.remove(from);
    order.insert(to, moved);
    for item in current_order.iter() {
        if !order.contains(item) {
            order.push(item.clone());
        }
    }
    *current_order = order;
    true
}

/// Drag-and-drop reorder (`arrayMove` semantics) for `"board"`, `"discover"`, or `"streams"`.
pub fn move_item(
    content: &mut ContentSettings,
    tab: &str,
    visible_ids: &[String],
    from: usize,
    to: usize,
) -> bool {
    match tab {
        "board" => {
            let keys: Vec<CatalogKey> = visible_ids
                .iter()
                .filter_map(|id| parse_catalog_item_id(id).map(|(_, k)| k))
                .collect();
            reorder_vec(&mut content.catalog_order, &keys, from, to)
        }
        "discover" => {
            let keys: Vec<CatalogKey> = visible_ids
                .iter()
                .filter_map(|id| parse_catalog_item_id(id).map(|(_, k)| k))
                .collect();
            reorder_vec(&mut content.discover_order, &keys, from, to)
        }
        "search" => {
            let keys: Vec<CatalogKey> = visible_ids
                .iter()
                .filter_map(|id| parse_catalog_item_id(id).map(|(_, k)| k))
                .collect();
            reorder_vec(&mut content.search_order, &keys, from, to)
        }
        "streams" => {
            let urls: Vec<String> = visible_ids
                .iter()
                .filter_map(|id| id.strip_prefix("streams|").map(ToOwned::to_owned))
                .collect();
            reorder_vec(&mut content.stream_order, &urls, from, to)
        }
        _ => false,
    }
}

fn set_hidden_key(list: &mut Vec<CatalogKey>, key: CatalogKey, active: bool) {
    if active {
        list.retain(|k| k != &key);
    } else if !list.contains(&key) {
        list.push(key);
    }
}

/// Sets visibility/enabled state for a Board catalog, Discover catalog, or Stream addon item.
pub fn set_item_active(content: &mut ContentSettings, id: &str, active: bool) -> bool {
    if let Some(url) = id.strip_prefix("streams|") {
        if active {
            content.disabled_stream_addons.retain(|u| u != url);
        } else if !content.disabled_stream_addons.iter().any(|u| u == url) {
            content.disabled_stream_addons.push(url.to_owned());
        }
        return true;
    }
    let Some((tab, key)) = parse_catalog_item_id(id) else {
        return false;
    };
    match tab {
        "board" => {
            set_hidden_key(&mut content.hidden_board_catalogs, key, active);
            true
        }
        "discover" => {
            set_hidden_key(&mut content.hidden_discover_catalogs, key, active);
            true
        }
        "search" => {
            set_hidden_key(&mut content.hidden_search_catalogs, key, active);
            true
        }
        _ => false,
    }
}

/// Renames an item or resets its custom label when `title` is `None` / blank.
pub fn set_item_title(content: &mut ContentSettings, id: &str, title: Option<&str>) {
    match title.map(str::trim).filter(|s| !s.is_empty()) {
        Some(custom) => {
            content
                .custom_titles
                .insert(id.to_owned(), custom.to_owned());
        }
        None => {
            content.custom_titles.remove(id);
        }
    }
}

/// Creates a new Stream Filter Preset (requires non-blank name and >= 1 include or exclude tag).
pub fn create_preset(
    content: &mut ContentSettings,
    name: &str,
    include: &[String],
    exclude: &[String],
) -> bool {
    let name = name.trim();
    let include_tags = normalize_tags(include.iter().map(String::as_str));
    let exclude_tags = normalize_tags(exclude.iter().map(String::as_str));
    if name.is_empty() || (include_tags.is_empty() && exclude_tags.is_empty()) {
        return false;
    }
    let mut seq = content.stream_presets.len() + 1;
    let new_id = loop {
        let candidate = format!("preset-{seq}");
        if !content.stream_presets.iter().any(|p| p.id == candidate) {
            break candidate;
        }
        seq += 1;
    };
    content.stream_presets.push(StreamFilterPreset {
        id: new_id,
        name: name.to_owned(),
        include_tags,
        exclude_tags,
    });
    true
}

pub fn delete_preset(content: &mut ContentSettings, id: &str) {
    content.stream_presets.retain(|p| p.id != id);
    if content.active_stream_preset.as_deref() == Some(id) {
        content.active_stream_preset = None;
    }
}

/// Toggleable radio selection matching Stremio's `<RadioButton toggleable />`:
/// clicking the active preset clears it; clicking another activates it.
pub fn toggle_active_preset(content: &mut ContentSettings, id: &str) {
    if id.is_empty() || content.active_stream_preset.as_deref() == Some(id) {
        content.active_stream_preset = None;
    } else if content.stream_presets.iter().any(|p| p.id == id) {
        content.active_stream_preset = Some(id.to_owned());
    }
}
