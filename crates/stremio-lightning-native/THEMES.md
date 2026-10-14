# Themes in the native app

The native app reads `*.theme.css` files and restyles its Slint UI by stremio-web class name.
Each component that has a web class name uses the same name as its element id in `ui/*.slint`.

## Where the files go

The same folder the other shells use, `<local app data>/stremio-lightning/themes/`:

- Windows: `%LOCALAPPDATA%\stremio-lightning\themes`
- Linux: `$XDG_DATA_HOME/stremio-lightning/themes`, or `~/.local/share/stremio-lightning/themes`
- macOS: `~/Library/Application Support/stremio-lightning/themes`

Files load in file name order when the app starts. Every file in the folder is applied; there is
no way yet to pick one. A later declaration wins over an earlier one. Specificity is not computed.

## What a theme can say

- Selectors: `.name`, `[class*="name"]`, `[class^="name"]`, `[class="name"]`, comma separated.
  The `[class...]` forms match the class name without the hash the web build adds.
- Properties: `color`, `background` / `background-color` (one colour, no gradient), `opacity`,
  `visibility: hidden | visible`, `display: none`. Hiding keeps the layout space.
- Colours: `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()`, `transparent`.

Anything else is printed on stderr (`theme <file>: ...`) and skipped. That includes `@media`,
descendant selectors such as `.a .b`, sizes, margins, gradients and hover states.

## Classes

| Class | Element | `color` | `background` | `opacity` | hide |
| --- | --- | --- | --- | --- | --- |
| `board-row` | one catalog row | | yes | yes | yes |
| `header-container` | the row's title line | | yes | yes | yes |
| `title-container` | the row title text | yes | | yes | yes |
| `see-all-container` | the "See All" pill (text and chevron) | yes | yes | yes | yes |
| `meta-items-container` | the strip of cards | | yes | yes | yes |
| `meta-item-link` | one card | | yes | yes | yes |
| `poster-container` | the poster box, behind the image | | yes | yes | yes |
| `poster-image` | the poster image | | | yes | yes |
| `title-bar-container` | the strip under the poster | | yes | yes | yes |
| `title-label` | the card title text | yes | | yes | yes |
| `horizontal-nav-bar-container`, `horizontal-nav-bar` | the top bar | | yes | yes | yes |
| `logo-container` | the logo's box | | yes | yes | yes |
| `logo` | the logo | | | yes | yes |
| `search-bar-container`, `search-bar` | the search box | | yes | yes | yes |
| `search-input` | the text typed in the search box | yes | | | |
| `placeholder-label` | "Search or paste link" | yes | | | |
| `submit-button-container` | the search icon's button | | yes | yes | yes |
| `buttons-container` | the fullscreen and menu buttons together | | | yes | yes |
| `button-container` | the fullscreen button. `submit-button-container` and `menu-toggle` carry this class too | | yes | yes | yes |
| `menu-toggle`, `label-container`, `nav-menu-popup-label`, `menu-button-container`, `button-container` | the avatar menu button | | yes | yes | yes |
| `avatar` | the avatar circle | | yes | yes | yes |
| `vertical-nav-bar-container`, `vertical-nav-bar` | the side nav | | yes | yes | yes |
| `nav-tab-button-container`, `nav-tab-button` | one side nav tab | | yes | yes | yes |
| `icon` | the unselected nav icons, the search icon and the fullscreen icon | yes | | | yes |

The Board, the top bar and the side nav are wired. Details, Player and the other sections have element
ids but no class styles yet. The `selected` and `active` states, hover and focus are not themable, and
`icon` does not reach the selected nav icon or the caret next to the avatar. `.plugin.js` plugins do not
run in the native app.

## Elements with several web classes

Some web elements carry more than one class. The menu button next to the avatar is
`label-container nav-menu-popup-label menu-toggle menu-button-container button-container`. A rule that
names any of them applies to it. All supported selectors weigh the same, so when two rules reach the same
element the later one in the file wins, whichever of its classes each rule names. The table lists every
class an element carries. The first one is its id in `ui/*.slint`.
