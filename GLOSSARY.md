# Stremio Lightning

A native desktop Stremio client, built on `stremio-core`. This is the language used to talk about titles, their playable entries, and the sources those entries can be watched from.

## Language

**Details Screen**:
The screen for one title: its description, and either the Episodes to choose from or the Streams of the chosen Video.
_Avoid_: Meta preview, meta details, detail page

**Video**:
One playable entry in a title's catalogue — an Episode of a series, or the single entry for a film.
_Avoid_: Item, media, episode (when a film is meant)

**Episode**:
A Video belonging to a series.
_Avoid_: Video (when a series is meant)

**Season**:
A numbered group of Episodes. Season 0 holds the Specials.
_Avoid_: Series, volume

**Special**:
A Video in season 0, labelled SPECIAL and ordered after every numbered season.
_Avoid_: Extra, bonus, season-0 episode

**Episode Picker**:
The season selector and Episode list on the Details Screen.
_Avoid_: Video list, seasons bar, episode selector

**Streams List**:
The sources the addons offer for the chosen Video.
_Avoid_: Sources, links, results

**Next Video**:
The Video that follows a given Video in the title itself — unreleased Videos are skipped, the last Episode of a season continues into the next season, and the last Episode of the final season has no Next Video.
_Avoid_: Upcoming, following episode

**Next Episode**:
The label and control for moving to the Next Video.
_Avoid_: Next video (as a label)
