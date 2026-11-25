//! Pure AniSkip player session state machine. Zero Slint or network I/O.

#[path = "skip_format.rs"]
mod format;

pub(crate) use format::{
    clock, compute_marks, error_text, format_rates_listing, kind_label, provider_name, rows,
    validate_submission, FetchPlan, OverlayDiff, SkipPanelData, SkipRowData,
};
#[cfg(test)]
pub(crate) use format::{parse_clock, should_reseed};

use format::{current_kind, kind_with_data};

use crate::plumbing::skip;

/// Pure AniSkip session state for the currently playing Video.
pub(crate) type OverlaySync = (bool, Option<Vec<(f32, f32)>>);

#[derive(Default)]
pub(crate) struct SkipSession {
    pub generation: u64,
    pub video: String,
    pub length: Option<u64>,
    pub segments: Vec<skip::Segment>,
    pub rates: Vec<skip::Rate>,
    pub active: Option<skip::Segment>,
    pub total: usize,
    pub marks: Vec<(f32, f32)>,
    pub kind: String,
    pub seeded: String,
    pub auto_times: Option<(String, String)>,
}

impl SkipSession {
    pub(crate) fn has_state(&self) -> (bool, bool) {
        let had_segments = !self.segments.is_empty();
        let had_any = had_segments || !self.rates.is_empty() || !self.video.is_empty();
        (had_any, had_segments)
    }

    pub(crate) fn is_for(&self, video: Option<&str>) -> bool {
        video.is_some_and(|v| v == self.video)
    }

    pub(crate) fn clear(&mut self) -> u64 {
        let generation = self.generation.wrapping_add(1);
        *self = Self {
            generation,
            ..Self::default()
        };
        generation
    }

    pub(crate) fn prepare_fetch(&mut self, video: &str, length: u64) -> FetchPlan {
        let same_for = self.video == video;
        let length_changed = same_for && self.length.is_some_and(|prev| length.abs_diff(prev) > 2);
        if same_for && !length_changed {
            if self.length.is_none() {
                self.length = Some(length);
            }
            return FetchPlan::AlreadyCurrent;
        }
        if length_changed {
            self.generation = self.generation.wrapping_add(1);
            self.seeded.clear();
            self.auto_times = None;
        }
        self.segments.clear();
        self.rates.clear();
        self.active = None;
        self.video = video.to_owned();
        self.length = Some(length);
        FetchPlan::Fetch {
            generation: self.generation,
            reset_window: length_changed,
        }
    }

    pub(crate) fn accept_rates(
        &mut self,
        generation: u64,
        video: &str,
        length: u64,
        exact_length: bool,
        rates: Vec<skip::Rate>,
    ) -> bool {
        let length_ok = if exact_length {
            self.length == Some(length)
        } else {
            !self.length.is_some_and(|prev| prev.abs_diff(length) > 2)
        };
        if self.generation != generation || self.video != video || !length_ok {
            return false;
        }
        self.segments = rates.iter().map(skip::Rate::segment).collect();
        self.rates = rates;
        true
    }

    pub(crate) fn insert_submitted(
        &mut self,
        generation: u64,
        video: &str,
        length_key: u64,
        rate: skip::Rate,
    ) -> Option<(u64, Vec<SkipRowData>)> {
        if self.generation != generation
            || self.video != video
            || self.length.is_some_and(|p| p.abs_diff(length_key) > 2)
        {
            return None;
        }
        self.segments.push(rate.segment);
        self.segments.sort_by(|a, b| a.start.total_cmp(&b.start));
        self.rates.push(rate);
        self.rates
            .sort_by(|a, b| a.segment.start.total_cmp(&b.segment.start));
        self.generation = self.generation.wrapping_add(1);
        Some((self.generation, rows(&self.rates)))
    }

    fn refresh_overlay(
        &mut self,
        time: f64,
        duration: f64,
        same_video: bool,
        skip_wanted: bool,
    ) -> Option<Vec<(f32, f32)>> {
        self.active = if skip_wanted && same_video {
            self.segments.iter().copied().find(|s| s.holds(time))
        } else {
            None
        };
        let marks = if same_video {
            compute_marks(&self.segments, duration, skip_wanted)
        } else {
            Vec::new()
        };
        (marks != self.marks).then(|| {
            self.marks = marks.clone();
            marks
        })
    }

    pub(crate) fn evaluate_overlay(
        &mut self,
        time: f64,
        duration: f64,
        same_video: bool,
        skip_wanted: bool,
    ) -> OverlayDiff {
        let had = self.segments.len();
        let total_changed = (had != self.total).then_some(had);
        if total_changed.is_some() {
            self.total = had;
        }
        let before = self.active.map(|s| s.kind);
        let marks_changed = self.refresh_overlay(time, duration, same_video, skip_wanted);
        OverlayDiff {
            total_changed,
            video: self.video.clone(),
            active_changed: before != self.active.map(|s| s.kind),
            active: self.active,
            marks_changed,
        }
    }

    pub(crate) fn sync_overlay_for_generation(
        &mut self,
        generation: u64,
        time: f64,
        duration: f64,
        skip_wanted: bool,
    ) -> Option<OverlaySync> {
        if self.generation != generation {
            return None;
        }
        let marks_changed = self.refresh_overlay(time, duration, true, skip_wanted);
        Some((self.active.is_some(), marks_changed))
    }

    pub(crate) fn skip_seek_target(&self, time: f64) -> Option<f32> {
        self.active
            .map(|s| s.end)
            .filter(|end| *end > time)
            .map(|end| (end + 0.5) as f32)
    }

    fn seed_context(&self, target_video: Option<&str>, kind: &str) -> String {
        match target_video {
            Some(video) => format!("{video}:{kind}"),
            None => format!("gen:{}:{kind}", self.generation),
        }
    }

    pub(crate) fn open_panel(
        &mut self,
        target_video: Option<&str>,
        playhead: f64,
    ) -> SkipPanelData {
        let kind = current_kind(&self.kind);
        self.kind = kind.clone();
        let context = self.seed_context(target_video, &kind);
        let seed = self.seeded != context;
        if seed {
            self.seeded = context;
        }
        self.build_panel(&kind, true, seed, true, target_video, playhead)
    }

    pub(crate) fn pick_kind(
        &mut self,
        kind: &str,
        target_video: Option<&str>,
        playhead: f64,
    ) -> SkipPanelData {
        self.kind = kind.to_owned();
        self.seeded = self.seed_context(target_video, kind);
        self.build_panel(kind, false, true, true, target_video, playhead)
    }

    pub(crate) fn apply_fetched_panel(
        &mut self,
        rates: &[skip::Rate],
        playhead: f64,
        current_start: &str,
        current_end: &str,
    ) -> SkipPanelData {
        let seed =
            format::should_reseed(rates, current_start, current_end, self.auto_times.as_ref());
        let kind = if self.kind.is_empty() {
            kind_with_data(rates, "op")
        } else {
            self.kind.clone()
        };
        self.kind = kind.clone();
        self.panel_for_rates(rates, &kind, playhead, seed, false)
    }

    pub(crate) fn panel_for_rates(
        &mut self,
        rates: &[skip::Rate],
        kind: &str,
        playhead: f64,
        seed: bool,
        clear_notice: bool,
    ) -> SkipPanelData {
        let times = match rates.iter().find(|r| r.segment.kind.api() == kind) {
            Some(rate) => (clock(rate.segment.start), clock(rate.segment.end)),
            None => (clock(playhead), clock(playhead)),
        };
        let seed_out = if seed {
            self.auto_times = Some(times.clone());
            Some(times)
        } else {
            None
        };
        SkipPanelData {
            kind: kind.to_owned(),
            rows: rows(rates),
            seed: seed_out,
            clear_notice,
        }
    }

    fn build_panel(
        &mut self,
        kind: &str,
        auto: bool,
        seed: bool,
        clear_notice: bool,
        target_video: Option<&str>,
        playhead: f64,
    ) -> SkipPanelData {
        let rates = if self.is_for(target_video) {
            self.rates.clone()
        } else {
            Vec::new()
        };
        let resolved = if auto {
            kind_with_data(&rates, kind)
        } else {
            kind.to_owned()
        };
        self.kind = resolved.clone();
        self.panel_for_rates(&rates, &resolved, playhead, seed, clear_notice)
    }
}

#[cfg(test)]
#[path = "skip_tests.rs"]
mod tests;
