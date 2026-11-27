//! Pure stream filtering and Content Settings state mutations (zero I/O, zero Slint).

use stremio_core::types::resource::Stream;

use crate::plumbing::local_settings::{CatalogKey, ContentSettings, StreamFilterPreset};

/// Parses comma-separated tag text into trimmed, case-insensitively deduplicated tags.
pub fn parse_tags(input: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in input.split(',') {
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

/// Moves the catalog at `index` up (`delta = -1`) or down (`delta = 1`) in `visible_keys`,
/// persisting the materialized order into `content.catalog_order`.
pub fn move_catalog(
    content: &mut ContentSettings,
    visible_keys: &[CatalogKey],
    index: usize,
    delta: i32,
) -> bool {
    let Some(target) = index.checked_add_signed(delta as isize) else {
        return false;
    };
    if index >= visible_keys.len() || target >= visible_keys.len() || index == target {
        return false;
    }
    let mut order = visible_keys.to_vec();
    order.swap(index, target);
    for key in &content.catalog_order {
        if !order.contains(key) {
            order.push(key.clone());
        }
    }
    content.catalog_order = order;
    true
}

fn toggle_key(list: &mut Vec<CatalogKey>, key: CatalogKey) {
    if let Some(pos) = list.iter().position(|k| k == &key) {
        list.remove(pos);
    } else {
        list.push(key);
    }
}

pub fn toggle_board_catalog(content: &mut ContentSettings, key: CatalogKey) {
    toggle_key(&mut content.hidden_board_catalogs, key);
}

pub fn toggle_discover_catalog(content: &mut ContentSettings, key: CatalogKey) {
    toggle_key(&mut content.hidden_discover_catalogs, key);
}

pub fn reset_catalogs(content: &mut ContentSettings) {
    content.catalog_order.clear();
    content.hidden_board_catalogs.clear();
    content.hidden_discover_catalogs.clear();
}

pub fn toggle_stream_addon(content: &mut ContentSettings, transport_url: &str, enabled: bool) {
    if enabled {
        content
            .disabled_stream_addons
            .retain(|u| u != transport_url);
    } else if !content
        .disabled_stream_addons
        .iter()
        .any(|u| u == transport_url)
    {
        content
            .disabled_stream_addons
            .push(transport_url.to_owned());
    }
}

/// Inserts a new preset (generating a unique id when `id` is empty) or updates an existing one.
/// Returns `false` if `name` is blank after trimming.
pub fn upsert_preset(
    content: &mut ContentSettings,
    id: &str,
    name: &str,
    include_csv: &str,
    exclude_csv: &str,
) -> bool {
    let name = name.trim();
    if name.is_empty() {
        return false;
    }
    let include_tags = parse_tags(include_csv);
    let exclude_tags = parse_tags(exclude_csv);
    if !id.is_empty() {
        if let Some(existing) = content.stream_presets.iter_mut().find(|p| p.id == id) {
            existing.name = name.to_owned();
            existing.include_tags = include_tags;
            existing.exclude_tags = exclude_tags;
            return true;
        }
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

pub fn set_active_preset(content: &mut ContentSettings, id: &str) {
    if id.is_empty() {
        content.active_stream_preset = None;
    } else if content.stream_presets.iter().any(|p| p.id == id) {
        content.active_stream_preset = Some(id.to_owned());
    }
}
