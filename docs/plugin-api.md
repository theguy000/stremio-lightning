# Mods, Built-in Plugins, and Themes

Stremio Lightning is a native Slint client with no embedded browser or JavaScript runtime.
Customization is split into two surfaces managed from the **Mods** section:

1. **Built-in plugins** (compiled Rust modules declared in `crates/stremio-lightning/src/screens/mods/builtin.rs`).
2. **Themes** (`.theme.css` files plus compiled-in themes such as Amoled; see `crates/stremio-lightning/THEMES.md`).

## Built-in Plugins

Built-in plugins are pure Rust descriptors (`screens::mods::builtin::Builtin`) paired with
plumbing implementations:

- **Declaration**: Each built-in plugin defines its stable `id` (for example `builtin:aniskip`),
  display metadata (`name`, `version`, `description`), toggle `Switch`, and declarative `Field`
  settings schema (`Text` or `Toggle`) with `Validation` rules (`None`, `RequiredUuid`,
  `NonEmptyDefault`).
- **Persistence**: Enabled switches and plugin field values are stored in
  `<app data>/stremio-lightning/settings.json` via `plumbing::local_settings::LocalExtras`.
- **UI**: The **Mods -> Plugins** page displays each built-in plugin card and toggle;
  **Mods -> Plugin settings** renders its declared schema fields generically and persists edits on
  Save without recreating focused text fields.

## Themes

User themes live in `<app data>/stremio-lightning/themes/` as `*.theme.css` files alongside
compiled-in themes (`Amoled`).

- Theme parsing lives in `plumbing/theme/parse.rs` and `plumbing/theme/color.rs`.
- Supported CSS selectors, tokens, and element names are documented in
  `crates/stremio-lightning/THEMES.md`.
- Only one theme is active at a time; `shell/theme_apply.rs` applies the active theme's rules to
  Slint's `Theme` and `ClassStyles` globals at startup and whenever the selection changes.
