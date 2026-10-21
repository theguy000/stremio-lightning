# 00: Port the Details Screen to stremio-web's MetaDetails

**What to build:** The Details Screen stops being a placeholder and becomes a translation of web's
`MetaDetails`: the `MetaPreview` panel inset by 4rem on the left, a flexible gap, and a 30rem column
for the Episodes or the Streams over 0.4 black. The nav bar grows the back control web puts on that
route, replacing the logo in its first slot.

Opened after ticket 01 shipped looking nothing like web. `main.slint` was the one screen file with
no `Web:` references: it inlined its own poster, title, description and "Streams" heading in
std-widgets, while `meta-preview.slint` beside it was a finished port nobody mounted. Ticket 01 was
therefore written against the placeholder's idiom and inherited it.

**Blocked by:** None — this is the prefactor the other tickets needed.

**Status:** done

- [x] The screen is web's grid, with the ported `MetaPreview` where the hand-rolled header was.
- [x] `videos-list.slint` ports `VideosList`, `SeasonsBar` and `Video` number for number.
- [x] The nav bar's back control is the way out of the screen.
- [x] Two web details are dropped, not faked: the column's backdrop blur, and the selected row's
      fading border (it stays accent-coloured).
