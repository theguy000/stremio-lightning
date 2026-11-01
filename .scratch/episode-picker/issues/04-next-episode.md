# 04: Next Episode

**What to build:** On the Streams List, when the title has a Video after the picked Episode, a Next Episode control appears beside the Episode's name; choosing it selects that Video exactly as clicking the row in the Episode Picker would. The control is absent when there is nothing after the picked Episode: the last Episode of the final Season, a live title, or a following Episode that has not been released yet. The last Episode of a Season leads into the next Season. A Special is never what a regular Episode leads to, though inside Specials one Special does lead to the next.

The answer comes from the core rather than being computed here — the control exists iff the title can name a Next Video.

Vocabulary is in `GLOSSARY.md`: Next Video, Next Episode, Special, Season, Streams List.

**Blocked by:** 01 — Show the Seasons and Episodes, 02 — Pick an Episode and see its Streams

**Status:** done

- [x] The control appears on the Streams List when the picked Episode has a Next Video.
- [x] Choosing it loads the Next Video's Streams List, with its own name and sources.
- [x] The last Episode of a Season leads into the next Season's first Episode.
- [x] The control is absent at the last Episode of the final Season, for live titles, and when the next Episode is unreleased.
- [x] A regular Episode never resolves to a Special as its Next Video.
- [x] Unit tests cover each of those resolutions, including the season rollover and the absent cases.

## Delivered

The control asks the core what follows rather than re-deriving it, so all four cases come from one
implementation: season rollover, the final episode, the Specials, and unaired Episodes. Tests build
a real `MetaItem` and assert against `next_video` itself.

The web has no such control — its equivalent is the player's countdown — so this is the one piece
of the screen that is this client's own, drawn in the web's pill shape and accent.
