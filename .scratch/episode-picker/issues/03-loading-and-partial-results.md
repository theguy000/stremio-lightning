# 03: Loading and partial results on the Streams List

**What to build:** Between picking an Episode and the first addon answering, the Streams List shows two placeholder rows shaped like a source — a name bar and two description bars, in the dim surface colour. They vanish the moment the first real source arrives. While some addons are still out, a footer reports how many are loading and disappears when the last one answers. The placeholders are static: no spinner, no shimmer, no animation of any kind.

This matches how Stremio itself behaves: nothing Ready means placeholders, and a loading count sits under the list until the slow addons finish.

Vocabulary is in `GLOSSARY.md`: Episode, Streams List.

**Blocked by:** 02 — Pick an Episode and see its Streams

**Status:** done

- [x] Two placeholder rows appear immediately after an Episode is picked, before any source has arrived.
- [x] Placeholders are replaced by real rows as soon as the first addon answers.
- [x] Once real rows exist, a footer counts the addons still loading and disappears when the last one answers. It does not appear beside the placeholders — the placeholders are the whole story until something lands.
- [x] Placeholders are static — no repeated animation, no added timer.
- [x] An Episode whose sources all fail still reports the existing status line rather than being covered by placeholders forever.

## Delivered

Two static placeholders while nothing is Ready, and the addons-loading footer only once real rows
exist — the web's branch order, not an animation. Rows are web's `Stream`: the addon that answered,
the stream's name over its description, and a play circle that fades in with the hover.
