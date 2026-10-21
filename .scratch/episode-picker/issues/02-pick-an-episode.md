# 02: Pick an Episode and see its Streams

**What to build:** Clicking an Episode row asks the core for that Video's Streams and swaps the body from the Episode Picker to the Streams List for that Episode. The Streams List carries a `‹` control and the Episode's `S1E2 Title`, so what is loaded is never ambiguous; `‹` returns to the Episode Picker with the same Season still in view. Picking a different Episode replaces the previous Streams rather than adding to them.

The Episode is identified by its Video id, not by its position in the list, so a refresh landing between render and click still resolves to the Episode the user pointed at.

**Read 01 first:** the picker currently shows whenever the title has Seasons, because the core guesses a stream path for a series carrying a `behavior_hints.default_video_id` — so "a stream path exists" cannot mean "the user picked something". This ticket has to introduce the pick as its own piece of state (the Video id, held beside the browsed Season) and let *that* hide the picker and swap the body. Deciding whether the pick survives a title reload is part of this ticket, not a detail to leave standing.

Vocabulary is in `GLOSSARY.md`: Episode, Season, Streams List, Details Screen.

**Blocked by:** 01 — Show the Seasons and Episodes

**Status:** ready-for-agent

- [ ] Clicking an Episode swaps the body to that Episode's Streams List.
- [ ] The Streams List names the Episode as `S1E2 Title`.
- [ ] `‹` returns to the Episode Picker showing the same Season that was open.
- [ ] Picking a second Episode replaces the Streams List; nothing from the first Episode remains.
- [ ] The pick is carried by Video id, and a list that changed since render cannot resolve to the wrong Episode.
- [ ] A window-level test drives a real click and asserts the pick reaches the core with the right Video id.
