# Third-party notices

The native app's code is GPL-3.0-only (see the repository `LICENSE`). The assets below keep
their own licences.

## Stremio icons (`ui/icons/*.svg`)

- Source: `@stremio/stremio-icons` 5.16.0, https://github.com/Stremio/stremio-icons
- Author named by the package: Smart Code Ltd.
- Licence: MIT, as declared in the package's `package.json` on npm. The GitHub repository has no
  `LICENSE` file and the package states no copyright year, so none is given here.
- 33 of the 138 icons. In 29 of them the only change is that `currentcolor` is written `currentColor`, because the lowercase form drew no strokes in the Slint window. The other 4 are unmodified.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Stremio colours (`ui/theme.slint`)

- Source: `@stremio/stremio-colors` 5.2.0, https://github.com/Stremio/stremio-colors
- Author named by the package: Smart Code Ltd. Licence: MIT, as declared in `package.json`
  (same terms as above).
- The HSL values in `colors.json` are converted to hex.

## Plus Jakarta Sans (`ui/fonts/`)

- Source: https://github.com/tokotype/PlusJakartaSans (the upstream project, not stremio-web).
- Copyright 2020 The Plus Jakarta Sans Project Authors.
- Licence: SIL Open Font License 1.1. The full text is in `ui/fonts/OFL.txt`.
- The Regular, Medium, SemiBold and Bold TrueType files are unmodified.

## stremio-web (layout, styles and assets taken into `ui/`)

- Source: https://github.com/Stremio/stremio-web. Its source files carry the line
  `Copyright (C) 2017-2026 Smart code 203358507` (the end year differs per file).
- What upstream says about its licence, checked on 2026-10-14 at commit `9d1d526`:
  - `LICENSE.md` is the GNU GPL version 2 text.
  - `package.json` has `"license": "gpl-2.0"`. The README says "Released under the GPL-2.0 license".
  - None of the 677 source files has a licence grant in its header, only the copyright line.
    No file says "only" and no file says "or later".
- Position taken by this project: upstream is **GPL-2.0 or later**, because it names version 2 and
  never restricts it to that version. The "any later version" clause of GPL-2.0 then lets this
  project use the code under **GPL-3.0 or any later version**, which fits this repository's
  GPL-3.0 licence.
- This position is **not confirmed by Smart Code**. The text above does not state "or later" either.
  If upstream says it is GPL-2.0-only, this position is wrong and the files taken from stremio-web
  must be removed or relicensed with Smart Code's permission.
- Every file taken from stremio-web keeps upstream's copyright line and names its source path.
- Taken so far: `ui/board.slint` ports the layout numbers of `src/components/MetaRow/styles.less`,
  `src/components/MetaItem/styles.less`, `src/routes/Board/styles.less` and the root values in
  `src/App/styles.less` (poster ratios, font-size steps, breakpoints), and the row title format of
  `catalogTitle` in `src/common/useTranslate.js` (without translations). It is hand-written Slint.
  No upstream file is copied verbatim.
