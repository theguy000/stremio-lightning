# Native client: give it a shape that can grow

Status: proposed.

## Problem Statement

The native app works, but it is built as one big file with a few side files, and every new
screen makes that worse.

Almost everything lives in one 1,586-line file. That file starts the app, holds the app's
state, decides what each screen shows, loads and caches images, starts the streaming server,
keeps track of a browser sign-in, and wires up around twenty window callbacks. Reading it to
answer one question ("what happens when I click a stream?") means scrolling past matters that
have nothing to do with streams.

The side files are the same problem in smaller pieces. The settings file holds the option
lists, works out what each setting should show, changes settings, and also opens web links and
starts browsers. The theme file is a CSS parser, a table of UI elements, and the code that
paints them, all in one.

The practical cost:

- Adding a screen means editing the big file in five places and hoping none of them is missed.
- Nothing can be tested without a window. The app is a program, not a library, so the tests
  cannot call the app's own code. Two of the test files work around this by copying the code
  they want to test: one keeps its own copy of the error-message wording, another its own copy
  of the CSS parser. Those copies prove the copy works, not the app. They will drift, and when
  they drift, the tests will keep passing while the app misbehaves.
- A contributor cannot tell which file a change belongs in, because the answer is "the big
  one".

This is the wrong foundation for the work ahead. The Library screen, the Calendar screen, a
real player with controls, search, downloads, and notifications are all still to come, and
each of them lands in this same file.

The web side of the repository is being retired in favour of this native client, so the native
client is now the part that has to be pleasant to work in.

## Solution

Break the native client up by what it is for, not by what kind of code it is.

Three kinds of code, in three places:

1. **Screen data** — plain functions that take the app's current state and return a plain
   description of what a screen shows. No window, no Slint types, no async. These are the part
   worth testing, and they become the app's test doorway.
2. **Wiring** — the thin layer that turns a Slint callback into a named action, and a named
   action into a call on Stremio's core. It does no thinking of its own.
3. **Plumbing** — the outside world: mpv, the streaming server, image downloading, browser
   links, theme files. Each gets one small module that hides its mess behind a couple of
   functions.

Each screen gets its own small module holding its screen data and its wiring, so "change
Discover" means opening one folder. A screen module grows into a folder when it outgrows a
file; no screen starts as a folder.

The app becomes a library with a tiny launcher, so tests can call the app's real code instead
of a copy of it.

Nothing the person using the app sees changes. This is a move, not a feature.

## User Stories

1. As a contributor, I want to find the code for a screen in one place, so that I do not have to
   read the whole app to change one thing.
2. As a contributor adding a screen, I want a small checklist of files to touch, so that adding
   Library or Calendar does not depend on remembering every place the big file had to change.
3. As a contributor, I want the app's startup to be short enough to read in one sitting, so that
   I can understand what the program does before it shows a window.
4. As a contributor, I want each screen's "what does this show" logic to be a function I can
   read top to bottom, so that I can check it against Stremio's web client without hunting.
5. As a contributor, I want the window callbacks to be one line each that names an action, so
   that a callback never hides a decision.
6. As a contributor, I want all the decisions to live in named functions next to the screen they
   belong to, so that I can change a decision without touching the wiring.
7. As a contributor, I want the player, the streaming server, image loading, link opening and
   themes each behind a small module, so that changing one of them cannot break a screen.
8. As a contributor, I want the player module to own every conversation with mpv, so that a
   future player screen with controls does not need to learn mpv's details.
9. As a contributor, I want the streaming server module to report a status rather than a
   sentence, so that the screen decides how to word it and the module stays about servers.
10. As a contributor, I want the image cache to take a key and give back an image, so that a new
    screen that shows pictures does not reimplement downloading, resizing or de-duplication.
11. As a contributor, I want a new kind of image (a logo, a small blurred background, a poster)
    to be one line in one table, so that I do not copy the fetch code again.
12. As a contributor, I want the theme code split into "read the file" and "paint the window", so
    that I can fix a CSS parsing bug without reading Slint code.
13. As a contributor, I want the settings' option lists, the settings' current values and the
    act of changing a setting to sit together, so that adding a Stremio setting is one obvious
    edit instead of five scattered ones.
14. As a contributor, I want opening a web link to be one function used everywhere, so that the
    rules for which links are allowed live in one place.
15. As a contributor, I want the sign-in flow to be one module that owns the "is this attempt
    still wanted" guard, so that a future sign-in provider does not have to rediscover it.
16. As a contributor, I want the wording of a failed sign-in to be a single function, so that
    the tests can check the wording against the real function rather than a copy of it.
17. As a contributor, I want the app's state to be one clearly named type with one place that
    builds it, so that I know where to look when a screen shows stale data.
18. As a contributor, I want one place that turns a changed app state into "refresh the window",
    so that a new screen is refreshed by being added to a list and not by editing the refresh
    code.
19. As a contributor, I want the shared bits of the window (which screen is showing, which
    overlay is open, whether video is playing) to be written down as belonging to the shell, so
    that a screen cannot quietly take over another screen's flags.
20. As a contributor, I want the Slint UI files and the Rust modules to line up one-for-one, so
    that finding the Rust side of a screen is not a guess.
21. As a contributor, I want the build to stay a single crate, so that I am not learning a
    multi-crate layout to change one screen.
22. As a contributor, I want the app's own code to be callable from a test, so that tests test
    the app.
23. As a tester, I want to check what a screen would show without opening a window, so that
    screen tests run fast and do not need a display.
24. As a tester, I want the error wording, the link rules and the theme parser to be tested
    against the real code, so that the tests cannot pass while the app is broken.
25. As a tester, I want the tests that copy production code deleted, so that the suite does not
    give false confidence or double maintenance.
26. As a tester, I want the window-level tests that do something real (does clicking open the
    details view) kept, so that the wiring is still covered.
27. As a tester, I want one shared way to start a headless Slint window, so that a new window
    test does not reinvent it.
28. As a reviewer, I want the review of a screen change to be readable in one folder, so that I
    can judge the whole change in one sitting.
29. As a reviewer, I want modules to expose the fewest things possible, so that I can reason
    about what a change can affect.
30. As a reviewer, I want no behaviour change in this reorganisation, so that I can compare test
    results before and after and treat any difference as a mistake.
31. As a packaging maintainer, I want the produced binary and its command line to stay exactly
    as they are, so that the Windows, Linux and Flatpak packages do not change.
32. As the person using the app, I want it to start, look and behave exactly as it does now, so
    that a refactor never becomes a surprise regression.
33. As the person using the app, I want the next screens to arrive without the app getting
    slower or less stable, so that growth does not cost quality.
34. As the owner, I want the documentation to describe the native client, so that the project's
    docs stop describing a web shell that is being retired.
35. As a contributor, I want one stated rule for where a new file goes, so that the layout does
    not drift back into a pile.

## Implementation Decisions

### The app becomes a library with a launcher

The crate keeps one package and one binary, but gains a library root. The binary does nothing
but start the tokio runtime, prepare the app's data folder, and call into the library. This is
what lets tests reach the real code, and it is the only decision here that changes the build
rather than just moving text around.

### Three kinds of code, kept apart

- **Screen data.** Plain structs describing what a screen shows, produced by plain functions
  that take the app state. These never import Slint and never start work.
- **Actions.** Named functions for everything the window can ask for — open an item, play a
  stream, change a setting, start a sign-in. Each takes only the pieces it needs, so it can be
  called from a test with no window.
- **Plumbing.** mpv, the streaming server, images, links, themes. Each hides its details behind
  a couple of functions and reports plain values, never Slint types and never window
  properties.

The rule that keeps this honest: a single file belongs to exactly one of the three. A callback
wiring file may mention screens and actions but must not decide anything; a screen's data
function may not mention the window.

### Shape

Each screen module owns its data and the actions that belong to it. The shell module owns which
screen is showing, which overlay is open and whether video is playing, and it owns the wiring of
every window callback to an action. Startup, the app state type, the event loop that refreshes
the window, and the refresh itself are their own small modules.

The intended shape, which the work should reach without inventing anything beyond it:

- A launcher, and a library root that only declares the modules.
- An app area: the state type and how it is built, the runtime event pump, the refresh that
  turns state into window content, the shell state, and one wiring function per screen.
- A screens area with one module per screen (board, discover, details, addons, settings,
  auth). Settings and auth are folders, because each already carries a parser-like body of
  tables and a body of flow logic; the rest are single files until they grow.
- Standalone plumbing modules for images, the streaming server, the player, theme files, and
  external links.

Shared Slint widgets (the multiselect menu, the scroll bar, the logged-out placeholder) have no
Rust module; they are already Slint-only.

### Settings stop being five parallel lists

Today one setting is described in five places: an option table, a value lookup, a match arm
that writes it, a field on the view struct, and a property in the Slint file. Only the Slint
property is genuinely separate.

The decision: each option-list setting is described once, as a table with its settings key and
its options; what the screen shows and what "option three means" both read that table. Toggles
keep a single list of the keys that are booleans. This removes the parallel match arms without
introducing a general settings framework — it is a lookup where there is currently a match.

Everything the app reads and writes must stay byte-for-byte the same on disk: the same JSON
keys, the same defaults, the same handling of "none" as a null.

### The streaming server reports state, not sentences

The server module answers "what is the server doing" with a small set of named states: not
started (with a reason), starting, ready, someone else's server already answering on the port,
started but not answering. The settings screen turns a state into its sentence. This moves the
three wordings out of the module that actually manages a child process, and makes the "a server
was already running" decision testable.

### Images become one small module

Taking a key, giving back an image, and fetching anything new in the background is already the
right shape; it simply lives inside the big file today. It moves out unchanged in behaviour: one
request per key, a table of key suffixes to sizes, cache-then-repaint.

### The sign-in flow owns its guard

The counter that decides whether a browser sign-in attempt is still wanted becomes part of the
sign-in module rather than a value passed through five callbacks. The phrasing of a failed
sign-in moves next to it as one function. A future provider is then a new entry, not a new copy
of the guard.

### Naming

The domain words stay the ones the code already uses and the Stremio models use: board,
discover, details, addons, settings, auth, player, stream server, theme, mod. New modules are
named after what they are for, not after their code shape, and no new word is introduced for a
concept that already has one.

### Documentation

The runtime architecture document describes only the web shells and does not mention the native
client at all. It is rewritten to lead with the native client and to state the three-kinds-of-code
rule, so that the next contributor meets the rule before meeting the files.

## Testing Decisions

A good test here asks a question a person would ask — "what does Discover show for this
profile?", "what wording does a wrong password produce?", "what address does this torrent play
from?" — and answers it by calling the app's own function. A test that asserts on a copy of the
code is worthless, because it cannot fail when the app breaks.

**The main doorway: screen data.** Every screen's data function is called directly with an app
state built from a profile, exactly as several existing unit tests already do in the addons and
settings files. This needs no window, no display and no network, and it is where the bulk of new
coverage belongs. It is one doorway for every screen, not one per screen.

**The second doorway: actions.** Actions that only need the runtime (open details, play a
stream, change a setting, sign in) take the pieces they need, so a test can drive them with a
real runtime and read the resulting state or events. The existing sign-in tests already work this
way and are the model for it.

**Existing coverage to keep.** The sign-in end-to-end suites, the theme and settings unit tests,
and the navigation test that builds a real window on the headless Slint backend. The headless
backend helper already exists and stays.

**Existing coverage to delete.** The two suites that re-implement production logic (the copied
error wording, the copied link rules, the copied CSS parser) are removed rather than ported, and
whatever in them is worth keeping is rewritten to call the real functions. A test that is a copy
of the thing it tests is worse than no test, because it will be trusted.

**The wiring is tested once, not per screen.** One window-level test per screen that the callback
reaches the right action is enough; anything deeper belongs in the screen data.

**Prior art.** The existing addons and settings unit tests for screen data; the sign-in suites for
runtime-driven actions; the navigation test for windows.

## Out of Scope

- Any change in what the app looks like or does. Screens, wording, styling and behaviour stay
  identical.
- New screens. Library, Calendar, search, downloads, notifications and a player with controls all
  stay future work; this spec only makes them cheap.
- The shortcut keys, the tabs of the Settings screen, and the details layout, which are the
  current rough edges and belong to their own work.
- Splitting the crate, or adding a crate for the UI or for any platform.
- The web shells, the Svelte Mods UI, the Vite setup and the `web/` folder. They are being
  retired; they are left exactly where they are and are not touched by this work.
- Packaging, release scripts and the build commands.
- Rewriting the Slint files. A Rust module per screen mirrors the Slint file that exists; the
  Slint side is not reorganised.

## Further Notes

- The move should be done screen by screen, in one-way steps, with the app building and the tests
  passing at every step. Each step is one screen or one plumbing module moving out of the big
  file, never a rearrangement that leaves the build broken for a while.
- The two windows with the most future work — the details screen and the player — are the ones
  most worth landing early, because their shape determines whether the next two screens are easy.
- Where the app already has behaviour that a move would make easy to "tidy", leave it. The value
  of this work is that a later comparison of behaviour is trustworthy.
- The `docs/` table of contents and the developer guide should point at this shape once it lands.
