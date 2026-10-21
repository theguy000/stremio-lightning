# 01: Show the Seasons and Episodes

**What to build:** A series opens its Details Screen on the Episode Picker instead of the Streams List: a season bar with previous/next controls wrapped around the season dropdown, and beneath it a scrollable list of the chosen Season's Episodes. Each row carries a thumbnail, an `S1E2`-style label and the Episode title, in exactly the order the title supplies them — the picker never re-sorts. The picker opens on the Season holding the title's default video, else the first non-special Season, else Specials. Changing Season re-renders the list. A film is untouched: it still opens straight to its Streams List, with no picker and no season bar.

Season 0 is a Season like any other here, shown as `SPECIAL` and ordered last; its Episodes are labelled `SPECIAL 1`, never `S0E1`.

Vocabulary is in `GLOSSARY.md`: Details Screen, Video, Episode, Season, Special, Episode Picker.

**Blocked by:** None (can start immediately)

**Status:** done

- [x] A series opens on the Episode Picker, and a film still opens on its own Streams List with no picker.
- [x] Seasons read in ascending order with Specials last; previous/next and the dropdown both change Season and agree with each other.
- [x] The picker lands on the default video's Season, else the first non-special Season, else Specials.
- [x] Episode rows show thumbnail, `S1E2` label and title, in the title's order, with Specials labelled `SPECIAL n`.
- [x] A Season with many Episodes scrolls inside the picker without shrinking the rest of the screen.
- [x] Unit tests cover Season grouping, the default-Season choice and row projection.
- [x] A film renders no picker — covered by a test, not just by eye.

## Delivered, and one thing that changed

The picker is driven by "the title has Seasons", not by "no Streams are loaded". The original plan
had the swap keyed on `stream_path.is_some()`, but the core guesses a stream path of its own for a
series that carries a `behavior_hints.default_video_id` — so a stream path being present says
nothing about the user having picked anything, and that rule would have hidden the picker on
exactly the titles that have a default. What hides the picker is a pick; that lands in 02.

Rows are inert here on purpose: clicking one is 02's behaviour.
