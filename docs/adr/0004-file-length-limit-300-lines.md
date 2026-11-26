# 300-line limit per source file

## Context

Large files mix unrelated responsibilities, slow down review, and make UI and state changes harder to isolate. ADR 0003 established a 300-line ceiling during the architecture refactor, and we want that ceiling recorded as a standalone rule for every file in the repository.

## Decision

Every source file (`.rs` and `.slint`) must stay at or below 300 lines.

A file may exceed 300 lines only when there is an explicit, documented reason (such as a static lookup table like `crates/stremio-lightning/src/screens/language_names.rs`). When a change would push a file past 300 lines, split the new component, helper, or view into a dedicated module in the same folder instead of growing the existing file.

## Consequences

- New UI sections (such as the Details hero episode view and action bar) live in small, focused `.slint` components under `ui/details/` or `ui/primitives/`.
- Code review checks file line counts alongside behavior and tests.
