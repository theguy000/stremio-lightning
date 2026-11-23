//! Filter predicate for the Mods -> Logs page (source, minimum level, and substring search).

use crate::plumbing::logs::{Entry, Level, Source};

/// Filter applied to the Logs list (empty source/level means "any").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub source: String,
    pub level: String,
    pub search: String,
}

impl Filter {
    pub fn picked_source(&self) -> Option<Source> {
        Source::from_id(&self.source)
    }

    pub fn least_level(&self) -> Option<Level> {
        Level::from_id(&self.level)
    }

    pub fn is_active(&self) -> bool {
        self.picked_source().is_some()
            || self.least_level().is_some()
            || !self.search.trim().is_empty()
    }

    pub fn keeps(&self, entry: &Entry) -> bool {
        if self
            .picked_source()
            .is_some_and(|source| source != entry.source)
        {
            return false;
        }
        if self.least_level().is_some_and(|level| entry.level < level) {
            return false;
        }
        let wanted = self.search.trim().to_lowercase();
        if wanted.is_empty() {
            return true;
        }
        let haystack = format!(
            "{} {} {} {}",
            entry.source_label(),
            entry.level.label(),
            entry.text,
            entry.origin
        )
        .to_lowercase();
        haystack.contains(&wanted)
    }
}
