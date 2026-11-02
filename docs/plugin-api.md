# Plugin API

A plugin is a JavaScript file in the app's plugins folder, `<app data>/stremio-lightning/plugins/`,
named `name.plugin.js`. The Mods screen loads it when the plugin is switched on.

The native app has no web page, so a plugin is not a patch of stremio-web any more. A plugin is a
script the app runs in a small JavaScript engine (QuickJS) and gives a handful of functions: it
can read and save its own settings, register the settings page the user fills in, log lines that
show up on the Logs page, restyle the window the way a theme file does, set timers, and subscribe
to what the app says about playback, the window, the streaming server and catalogs. That is the
whole of what it can do.

This page is the contract. A plugin that uses only the names below keeps working as the app
grows. A plugin that reaches for something else gets an error the moment it runs.

## What a plugin can do

- Read and save its own settings, and register the form the Plugin settings page draws.
- Write log lines the Logs page shows, at four levels.
- Restyle the window with the same rules a theme file uses.
- Run code later with timers, and hear about playback, fullscreen, the streaming server and
  catalogs as they change.
- Change its own interval of work when its settings are saved.

## What a plugin cannot do

- **No page, no DOM.** There is no `document`, no `window`, no elements to patch, no CSS to
  inject. A plugin that patched stremio-web has nothing to patch here. The only way to change the
  window is `applyTheme`, and the only elements that can be reached are the ones a theme file can
  reach, listed in `THEMES.md`.
- **No network.** There is no `fetch`, no `XMLHttpRequest`, no WebSocket, and no `require`. A
  plugin that wants a file from the internet asks the user to install it through the Marketplace.
- **No files.** A plugin cannot read or write anywhere except through `getSetting` and
  `saveSetting`, which reach its own settings file and nothing else.
- **No other plugins and no app state beyond the events below.** `getSetting` reads the
  plugin's own settings, never another plugin's and never the app's profile.
- **No way to run past its time.** Each step a plugin runs is given a fraction of a second (see
  [Time](#time)).
- **No modules or `import`.** A plugin is one file. Code it needs goes in that file.

## Install and metadata

A plugin keeps the comment convention every mod file has. The Mods screen reads it to build the
card:

```js
/**
 * @name Skip Intro
 * @description Adds a button that jumps past the opening titles.
 * @author Ada
 * @version 1.2.0
 */

console.log("Skip Intro loaded");
```

The file runs as soon as the plugin is switched on and again on every start of the app while it
stays switched on. A file that throws while loading is shown as failed on the Plugins page, with
the error, and never runs again until it is switched off and on.

## Time

Every step the app takes into a plugin, whether that step is loading the file, calling an
`onSettingsSaved` callback, calling an event callback, or firing a timer, is given 200
milliseconds of wall-clock time. A loop that runs past that is stopped by the engine, and the
plugin is closed. Working that needs longer has to be done in pieces with `setTimeout`, and each
piece gets its own 200 milliseconds.

A step may log at most 200 lines and arm at most 64 new timers. A plugin that goes past either is
closed the same way, and the reason is reported on the Logs page and on its card, because a plugin
whose output is unbounded freezes the Logs page instead of the window. The 64 is per step: a timer
that arms two more every time it fires is over it on the step it reaches 64, whatever the ones
before it did.

A plugin may have at most 256 timers waiting at once. Past that it is closed, and the reason is
reported the same way. A `setInterval` of a millisecond, or a timer that sets its own successor,
reaches this on its own; nothing else is likely to.

A plugin may use up to 8 MB of memory. Allocating past that throws inside the plugin.

A plugin may use 1 MB of stack. A function that calls itself without end throws
`InternalError: Maximum call stack size exceeded`, and the plugin is closed with that error. It is
the same for a value nested more than 64 levels deep: the level past 64 is stored as `null` rather
than read.

## Globals

Every name a plugin can see that is not part of the JavaScript language itself:

| Name | What it does | Returns |
| --- | --- | --- |
| `console.log(...values)` | One `info` line with every value, joined by spaces. A string prints as itself; anything else prints as its JSON, so `console.log("rows", [1,2])` says `rows [1,2]`. | nothing |
| `console.info(...values)` | Same as `console.log`. | nothing |
| `console.debug(...values)` | One `debug` line, values printed the same way. | nothing |
| `console.warn(...values)` | One `warn` line, values printed the same way. | nothing |
| `console.error(...values)` | One `error` line, values printed the same way. | nothing |
| `logger.debug(...values)` | One `debug` line, printed the same way as `console.debug`. | nothing |
| `logger.info(...values)` | One `info` line, printed the same way as `console.info`. | nothing |
| `logger.warn(...values)` | One `warn` line, printed the same way as `console.warn`. | nothing |
| `logger.error(...values)` | One `error` line, printed the same way as `console.error`. | nothing |
| `getSetting(key)` | The stored value of a setting of this plugin, or `null` when nothing is stored. | the value |
| `saveSetting(key, value)` | Stores a value in this plugin's settings file, `<name>.plugin.json`. The value may be a string, a number, a boolean, `null`, a list or an object of those. | nothing |
| `registerSettings(schema)` | Registers the settings page; see [Settings](#settings). The last call wins, so a plugin may register from its load code or from `onSettingsSaved`. | nothing |
| `onSettingsSaved(callback)` | Keeps `callback`. The app calls it with the settings file's contents (an object keyed by setting name) after the user saves the Plugin settings page for this plugin, and after `registerSettings` has recorded the new form. `getSetting` inside the callback already returns the new values. Called with anything but a function it does nothing and reports nothing: the callback is not kept, and the plugin is not told. | nothing |
| `applyTheme(rules)` | Restyles the window. `rules` is a list of `{ selector, property, value }`; the selector and property are the ones a theme file uses. A rule this app cannot read is left out, and the app does not report it back to the plugin, so a plugin cannot check its own rules. Rules replace the plugin's previous ones. An empty list takes the plugin's rules away. | nothing |
| `setTimeout(callback, milliseconds)` | Runs `callback` once, after at least that long, while the plugin stays on. `callback` is a function; a string is not accepted and no timer is armed. | the timer's id, a number, or `0` when `callback` was not a function |
| `setInterval(callback, milliseconds)` | Runs `callback` every that often, while the plugin stays on. `callback` is a function, as for `setTimeout`. | the timer's id |
| `clearTimeout(id)` | Stops a `setTimeout`. | nothing |
| `clearInterval(id)` | Stops a `setInterval`. | nothing |
| `on(event, callback)` | Keeps `callback` for one of the events below, and calls it with the event's value. | nothing |

Timers only fire while the plugin is loaded and running. The app looks in on the plugin every
tenth of a second or so, and fires whatever has come due then, so a timer fires within about a
tenth of a second of its time. The plugin cannot do anything while that look-in is happening, so
the wait is only as good as the step before it: a callback that runs to the 200 millisecond limit
delays the next timer by that much. A timer set for a long time is not a way to keep the app
awake: nothing runs while the app is closed.

## Events

`on(name, callback)` takes one of these names. The callback is handed the value in the third
column.

| Event | When it happens | Value |
| --- | --- | --- |
| `playback-started` | The Video on screen started playing: the file has opened in the player. A new stream in the same player is a second start with the new Video's id. | `{ videoId }`, the id of the Video that is playing |
| `playback-stopped` | The player closed, or took a file off the screen. | `null` |
| `fullscreen` | The window entered or left fullscreen, as the window reports it: a compositor that refuses a fullscreen press says nothing happened. | `{ on }`, a boolean |
| `server-running` | The streaming server finished starting, one way or another, with whether it is answering. | `{ running }`, a boolean |
| `catalog-loaded` | A catalog on the Board's first page arrived. A catalog reloaded is a second arrival, and one that failed and then arrived is an arrival. | `{ id, count }`: the catalog's id and how many items it brought |

Those five are all of them. A plugin that subscribes to another name is not told about anything:
the name is kept and never called, because the app has no such event to tell it about.

There is no event for the app's window being moved, and no function to move it, minimize it or
close it: those belong to the user and the operating system.

## Settings

`registerSettings` takes a list of fields. Each field has a `key`, a `label`, a `type`, and
optionally a `default`. The four types are the four controls the Plugin settings page has:

| Type | Control | Other keys | Default |
| --- | --- | --- | --- |
| `toggle` | A switch | none | `false` |
| `text` | A one-line text box | none | `""` |
| `select` | A menu of choices | `options`, a list of strings | the first option |
| `range` | A slider | `min`, `max`, `step` | `min` |

```js
registerSettings([
  { key: "enabled", label: "Skip the opening titles", type: "toggle", default: true },
  { key: "seconds", label: "How many seconds count as titles", type: "range", min: 5, max: 180, step: 5, default: 90 },
  { key: "marker", label: "What the marker says", type: "text", default: "Skip intro" },
  { key: "corner", label: "Where the marker sits", type: "select", options: ["top left", "top right"], default: "top right" }
]);
```

The user's values are saved only when the user presses Save on the Plugin settings page, one
`saveSetting` at a time, and then the plugin's `onSettingsSaved` callbacks run. `Revert` puts the
form back to what the settings file holds, and saves nothing.

A field of another type is left out of the page and reported on the Logs page. A schema that is
not a list is an error inside the plugin: `registerSettings` throws from the call that gave it,
so the plugin's load or its callback stops with that message, and the page draws no fields for it.
One that is a list of things that are not fields is drawn as the fields it can read and no more;
nothing is reported for the ones it cannot.
A key the plugin itself uses may not be `__schema`: that key holds the registered form.

## Worked example

A plugin that puts a marker on the player when a Video starts, and writes what it did to its
settings:

```js
/**
 * @name Marker
 * @description Writes down what played, and says so on the Logs page.
 * @author Ada
 * @version 1.0.0
 */

registerSettings([
  { key: "announce", label: "Say which Video started", type: "toggle", default: true }
]);

const seen = getSetting("seen") || 0;

on("playback-started", (event) => {
  if (getSetting("announce")) {
    logger.info("Playing " + event.videoId);
  }
  saveSetting("seen", seen + 1);
});

setInterval(() => {
  logger.debug("still here");
}, 60000);
```

## What changed from the web API

The web shells gave plugins `window.StremioEnhancedAPI`: window control, streaming server
control, mod management, settings, events and logging, all through a page. With no page, and no
way for a plugin to call into the app's own threads, that object does not exist here. This table
says where each part of it went.

| Web API | Here |
| --- | --- |
| Settings: get, save, register | `getSetting`, `saveSetting`, `registerSettings`, `onSettingsSaved` |
| Logging: debug, info, warn, error | `console.*` and `logger.*`, shown on the Logs page |
| Events: fullscreen, server state | `on("fullscreen")`, `on("server-running")`, and the three others above |
| Window: minimize, maximize, close, drag | Not implemented, and not planned: the window belongs to the user. |
| Streaming server: start, stop, restart, status | Not implemented. `server-running` only tells a plugin that the app started or stopped it; a plugin cannot start or stop it. |
| Mod management: list, download, delete, update | Not implemented. Installing mods is the Marketplace's business, and a plugin cannot install or delete one. |
| The page itself: DOM, styles the plugin injects | Not implemented. `applyTheme` reaches the classes in `THEMES.md`, and nothing else. |

Each "not implemented" row is left out of the globals table above, because this app does not
pretend to do it. A plugin written against the old API gets `ReferenceError: StremioEnhancedAPI is
not defined` as its load error, which the Plugins page shows.

## Where the plugin's files are

| What | Where |
| --- | --- |
| The plugin itself | `<app data>/stremio-lightning/plugins/<name>.plugin.js` |
| Its settings and registered form | `<app data>/stremio-lightning/plugins/<name>.plugin.json` |
| Its log lines | The Mods screen's Logs page, in memory, for this run of the app |

`<app data>` is the platform's data folder: `~/.local/share` on Linux, `%APPDATA%` on Windows,
`~/Library/Application Support` on macOS.
