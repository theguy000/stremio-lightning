//! Pure AniSkip formatting, clock parsing, seek-bar mark normalization, and submission
//! validation helpers. Zero Slint or network I/O.

use crate::plumbing::skip;
use crate::screens::{mods, settings};

/// One display row in the AniSkip timing panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkipRowData {
    pub key: &'static str,
    pub label: &'static str,
    pub text: String,
    pub id: String,
}

/// Complete view update for the AniSkip timing panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SkipPanelData {
    pub kind: String,
    pub rows: Vec<SkipRowData>,
    pub seed: Option<(String, String)>,
    pub clear_notice: bool,
}

/// Whether a playback tick needs a new AniSkip fetch for the current video.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FetchPlan {
    AlreadyCurrent,
    Fetch { generation: u64, reset_window: bool },
}

/// Overlay deltas produced by evaluating segments against the current playhead.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OverlayDiff {
    pub total_changed: Option<usize>,
    pub video: String,
    pub active_changed: bool,
    pub active: Option<skip::Segment>,
    pub marks_changed: Option<Vec<(f32, f32)>>,
}

/// Validated submission values ready for `SkipClient::submit`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ValidatedSubmission {
    pub kind: skip::Kind,
    pub start: f64,
    pub end: f64,
    pub length: f64,
}

pub(crate) fn compute_marks(
    segments: &[skip::Segment],
    duration: f64,
    skip_wanted: bool,
) -> Vec<(f32, f32)> {
    if !skip_wanted || duration <= 0.0 {
        return Vec::new();
    }
    segments
        .iter()
        .filter_map(|s| {
            let start = (s.start / duration).clamp(0.0, 1.0) as f32;
            let end = (s.end / duration).clamp(0.0, 1.0) as f32;
            (end > start).then_some((start, end))
        })
        .collect()
}

pub(crate) fn validate_submission(
    kind: &str,
    start_text: &str,
    end_text: &str,
    duration: f64,
) -> Result<ValidatedSubmission, String> {
    let kind =
        skip::Kind::from_api(kind).ok_or_else(|| "Pick Opening, Ending or Recap.".to_owned())?;
    let (Some(start), Some(end)) = (parse_clock(start_text), parse_clock(end_text)) else {
        return Err("Times read as 12:34 or 92.".to_owned());
    };
    if duration <= 0.0 {
        return Err("The picture has not loaded yet.".to_owned());
    }
    if !start.is_finite() || !end.is_finite() || end <= start || end > duration + 1.5 {
        return Err(format!(
            "The end has to be after the start and inside the file ({}).",
            clock(duration)
        ));
    }
    Ok(ValidatedSubmission {
        kind,
        start,
        end,
        length: duration.max(end),
    })
}

pub(crate) fn should_reseed(
    rates: &[skip::Rate],
    start: &str,
    end: &str,
    auto_times: Option<&(String, String)>,
) -> bool {
    if rates.is_empty() {
        return false;
    }
    (start.is_empty() && end.is_empty())
        || auto_times.is_some_and(|(auto_s, auto_e)| auto_s == start && auto_e == end)
}

pub(crate) fn kind_with_data(rates: &[skip::Rate], wanted: &str) -> String {
    if rates.is_empty() || rates.iter().any(|r| r.segment.kind.api() == wanted) {
        return wanted.to_owned();
    }
    rates[0].segment.kind.api().to_owned()
}

pub(crate) fn current_kind(stored: &str) -> String {
    if stored.is_empty() {
        "op".to_owned()
    } else {
        stored.to_owned()
    }
}

pub(crate) fn rows(rates: &[skip::Rate]) -> Vec<SkipRowData> {
    rates
        .iter()
        .map(|rate| SkipRowData {
            key: rate.segment.kind.api(),
            label: kind_short(rate.segment.kind),
            text: format!(
                "{} – {}",
                clock(rate.segment.start),
                clock(rate.segment.end)
            ),
            id: rate.id.clone(),
        })
        .collect()
}

pub(crate) fn format_rates_listing(rates: &[skip::Rate]) -> String {
    rates
        .iter()
        .map(|r| {
            format!(
                "{} {:.1}-{:.1}",
                kind_label(r.segment.kind),
                r.segment.start,
                r.segment.end
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn clock(seconds: f64) -> String {
    let whole = seconds.max(0.0).round() as u64;
    let (hours, minutes, secs) = (whole / 3600, (whole % 3600) / 60, whole % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

pub(crate) fn parse_clock(text: &str) -> Option<f64> {
    let mut seconds = 0.0;
    let mut parts = 0;
    for part in text.split(':') {
        seconds = seconds * 60.0 + part.trim().parse::<f64>().ok()?;
        parts += 1;
    }
    (1..=3).contains(&parts).then_some(seconds)
}

pub(crate) fn kind_short(kind: skip::Kind) -> &'static str {
    match kind {
        skip::Kind::Opening => "Opening",
        skip::Kind::Ending => "Ending",
        skip::Kind::Recap => "Recap",
    }
}

pub(crate) fn kind_label(kind: skip::Kind) -> &'static str {
    match kind {
        skip::Kind::Opening => "Skip Opening",
        skip::Kind::Ending => "Skip Ending",
        skip::Kind::Recap => "Skip Recap",
    }
}

pub(crate) fn error_text(error: skip::SkipError) -> String {
    match error {
        skip::SkipError::UnknownShow => "AniSkip does not know this show.".to_owned(),
        skip::SkipError::MissingId => "AniSkip took the timing but named no id for it.".to_owned(),
        skip::SkipError::RateLimited { voting: true } => {
            "AniSkip takes four votes an hour from one address.".to_owned()
        }
        skip::SkipError::RateLimited { voting: false } => {
            "AniSkip takes ten submissions a day for one episode.".to_owned()
        }
        skip::SkipError::Rejected(msg) | skip::SkipError::Transport(msg) => msg,
    }
}

pub(crate) fn provider_name(extras: &settings::LocalExtras) -> String {
    let configured = mods::builtin::fields("builtin:aniskip")
        .iter()
        .find(|f| f.key == "provider_name")
        .map(|f| mods::builtin::value(extras, "builtin:aniskip", f))
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    if configured.is_empty() {
        "Stremio Lightning".to_owned()
    } else {
        configured
    }
}
