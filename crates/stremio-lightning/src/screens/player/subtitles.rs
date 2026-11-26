//! Pure player subtitle selection, language priority ordering, and variant rows.
//! Zero Slint types and zero `plumbing::player` imports.

use stremio_core::models::common::Loadable;
use stremio_core::types::resource::Subtitles;

use crate::app::AppModel;
use crate::screens::{addon_name, language_names};

/// One subtitle track offered by the stream itself or an addon.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subtitle {
    pub id: String,
    pub title: String,
    pub language: String,
    pub origin: String,
    pub url: String,
}

/// Plain snapshot of one subtitle track currently loaded in the video backend.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackInfo {
    pub id: String,
    pub title: String,
    pub language: String,
    pub selected: bool,
    pub external: bool,
}

/// One row for the Subtitles Variants column (plain data; `shell/` projects to `PlayerTrack`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubtitleVariant {
    pub id: String,
    pub title: String,
    pub language: String,
    pub selected: bool,
    pub external: bool,
    pub origin: String,
}

/// Session-scoped subtitle choice (`SubtitlePreference`: enabled, external/embedded, language).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubtitleChoice {
    pub chosen: bool,
    pub enabled: bool,
    pub external: bool,
    pub language: Option<String>,
}

impl SubtitleChoice {
    pub fn off(self) -> Self {
        Self {
            chosen: true,
            enabled: false,
            ..self
        }
    }

    pub fn picked(language: &str, external: bool) -> Self {
        Self {
            chosen: true,
            enabled: true,
            external,
            language: Some(language.to_owned()).filter(|l| !l.is_empty()),
        }
    }
}

/// Stream subtitles first, then every addon's ready subtitles, labelled by addon manifest name.
pub(crate) fn subtitles_of(model: &AppModel) -> Vec<Subtitle> {
    let player = &model.player;
    let mut rows: Vec<Subtitle> = Vec::new();
    let mut push = |source: &str, list: &[Subtitles]| {
        for sub in list {
            rows.push(Subtitle {
                id: format!("{source}:{}", sub.id),
                title: sub
                    .label
                    .as_deref()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with("http"))
                    .map(str::to_owned)
                    .unwrap_or_default(),
                language: sub.lang.clone(),
                origin: source.to_owned(),
                url: sub.url.to_string(),
            });
        }
    };
    if let Some(selected) = player.selected.as_ref() {
        push("Stream", &selected.stream.subtitles);
    }
    for page in &player.subtitles {
        if let Some(Loadable::Ready(list)) = page.content.as_ref() {
            let name = addon_name(model, &page.request.base);
            push(if name.is_empty() { "Add-on" } else { &name }, list);
        }
    }
    rows
}

/// Minimal subtitle track facts needed for auto-selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtitleTrackFacts {
    pub id: String,
    pub language: String,
    pub external: bool,
}

/// Picks the initial subtitle track following stremio-web's `useSubtitles` priority order:
/// session pick -> profile preferred language -> `"eng"` (only when the viewer picked),
/// preferring the last-used side (embedded vs external) within each language.
pub fn auto_subtitle(
    tracks: &[SubtitleTrackFacts],
    choice: &SubtitleChoice,
    preferred_language: &str,
) -> Option<String> {
    if choice.chosen && !choice.enabled {
        return None;
    }
    let mut languages: Vec<String> = Vec::new();
    let mut push = |language: Option<&str>| {
        if let Some(lang) = language.map(str::trim).filter(|l| !l.is_empty()) {
            if !languages.iter().any(|seen| same_language(seen, lang)) {
                languages.push(lang.to_owned());
            }
        }
    };
    if choice.chosen {
        push(choice.language.as_deref());
    }
    push(Some(preferred_language));
    if choice.chosen {
        push(Some("eng"));
    }
    let sources = if choice.chosen && choice.external {
        [true, false]
    } else {
        [false, true]
    };
    for language in &languages {
        for external in sources {
            if let Some(track) = tracks
                .iter()
                .find(|t| t.external == external && same_language(&t.language, language))
            {
                return Some(track.id.clone());
            }
        }
    }
    None
}

/// Display label for an addon subtitle variant (`hasValidLabel ? label : languages.label(lang)`).
pub(crate) fn variant_label(candidate: &Subtitle) -> String {
    let label = candidate.title.trim();
    if label.is_empty() {
        language_names::label_or_code(&candidate.language)
    } else {
        label.to_owned()
    }
}

/// Variants column rows for one language: loaded file tracks first, then unloaded addon offers.
pub(crate) fn subtitle_rows(
    tracks: &[TrackInfo],
    candidates: &[Subtitle],
    language: &str,
) -> Vec<SubtitleVariant> {
    let language_label = language_names::label_or_code(language);
    let mut rows: Vec<SubtitleVariant> = tracks
        .iter()
        .filter(|t| same_language(&t.language, language))
        .map(|t| SubtitleVariant {
            id: t.id.clone(),
            title: if t.title.trim().is_empty() {
                language_label.clone()
            } else {
                t.title.clone()
            },
            language: t.language.clone(),
            selected: t.selected,
            external: t.external,
            origin: String::new(),
        })
        .collect();
    rows.extend(
        candidates
            .iter()
            .filter(|c| same_language(&c.language, language))
            .filter(|c| {
                let label = variant_label(c);
                !tracks.iter().any(|t| t.external && t.title == label)
            })
            .map(|c| SubtitleVariant {
                id: c.id.clone(),
                title: variant_label(c),
                language: c.language.clone(),
                selected: false,
                external: true,
                origin: c.origin.clone(),
            }),
    );
    rows
}

/// Union of languages in `tracks` and `candidates`, ordered with `priorities` first then sorted.
pub(crate) fn subtitle_languages(
    tracks: &[TrackInfo],
    candidates: &[Subtitle],
    priorities: &[&str],
) -> Vec<(String, String)> {
    let mut codes: Vec<String> = Vec::new();
    let mut collect = |language: &str| {
        let language = language.trim();
        if !language.is_empty() && !codes.iter().any(|c| same_language(c, language)) {
            codes.push(language.to_owned());
        }
    };
    for track in tracks {
        collect(&track.language);
    }
    for candidate in candidates {
        collect(&candidate.language);
    }
    codes.sort();
    let mut ordered: Vec<String> = Vec::new();
    for priority in priorities {
        if !priority.is_empty() {
            if let Some(pos) = codes.iter().position(|c| same_language(c, priority)) {
                ordered.push(codes.remove(pos));
            }
        }
    }
    ordered.extend(codes);
    ordered
        .into_iter()
        .map(|code| {
            let label = language_names::label_or_code(&code);
            (code, label)
        })
        .collect()
}

pub(crate) fn same_language(left: &str, right: &str) -> bool {
    let left = left.trim().to_ascii_lowercase();
    let right = right.trim().to_ascii_lowercase();
    if left.is_empty() || right.is_empty() {
        return false;
    }
    left == right
        || (left.len() == 2 && right.starts_with(&left))
        || (right.len() == 2 && left.starts_with(&right))
}

/// The language the variants column shows: the one the user pressed, or the language currently
/// playing when nothing has been pressed yet.
pub(crate) fn browsed_language(pressed: &str, active: &str) -> String {
    if pressed.is_empty() {
        active.to_owned()
    } else {
        pressed.to_owned()
    }
}

#[cfg(test)]
#[path = "subtitles_tests.rs"]
mod tests;
