# 04: Next Episode

**What to build:** On the Streams List, when the title has a Video after the picked Episode, a Next Episode control appears beside the Episode's name; choosing it selects that Video exactly as clicking the row in the Episode Picker would. The control is absent when there is nothing after the picked Episode: the last Episode of the final Season, a live title, or a following Episode that has not been released yet. The last Episode of a Season leads into the next Season. A Special is never what a regular Episode leads to, though inside Specials one Special does lead to the next.

The answer comes from the core rather than being computed here — the control exists iff the title can name a Next Video.

Vocabulary is in `GLOSSARY.md`: Next Video, Next Episode, Special, Season, Streams List.

**Blocked by:** 01 — Show the Seasons and Episodes, 02 — Pick an Episode and see its Streams

**Status:** ready-for-agent

- [ ] The control appears on the Streams List when the picked Episode has a Next Video.
- [ ] Choosing it loads the Next Video's Streams List, with its own name and sources.
- [ ] The last Episode of a Season leads into the next Season's first Episode.
- [ ] The control is absent at the last Episode of the final Season, for live titles, and when the next Episode is unreleased.
- [ ] A regular Episode never resolves to a Special as its Next Video.
- [ ] Unit tests cover each of those resolutions, including the season rollover and the absent cases.
