# TODO: Flatpak AppStream Metadata & Desktop Integration

## Current Issue
The Flatpak application details are missing or empty in Linux software centers (such as GNOME Software, KDE Discover, and on Flathub itself) due to incomplete and unvalidated metadata.
1. The Flatpak manifest (`flatpak/io.github.theguy000.StremioLightning.json`) and the `package_linux.rs` script dynamically generate a basic AppStream `.metainfo.xml` using inline `echo` commands and raw Rust strings.
2. The generated XML lacks crucial AppStream elements. When tested with `appstreamcli validate`, it fails with warnings/errors (which Flathub and software stores treat as fatal):
   - **Homepage URL** is missing (causes a fatal warning: `url-homepage-missing`).
   - **Developer Info** is missing.
   - **Screenshots** are missing (mandatory for graphical applications on Flathub).
   - **Content Rating (OARS)** is missing.
   - **Releases/Changelog** are missing or incomplete.

---

## Future Solution Plan

To fix this properly, Stremio Lightning should transition to using static, pre-validated metadata files inside the repository, cleaning up the packaging steps in the codebase.

### 1. New Static Assets to Create
Create the following files in the repository:

- **`assets/io.github.theguy000.StremioLightning.desktop`**:
  A standard, static desktop entry for the application.
  ```ini
  [Desktop Entry]
  Type=Application
  Name=Stremio Lightning
  Comment=Lightweight native Stremio shell
  Exec=stremio-lightning
  Icon=io.github.theguy000.StremioLightning
  Categories=AudioVideo;Video;Player;
  Terminal=false
  StartupNotify=true
  StartupWMClass=io.github.theguy000.StremioLightning
  ```

- **`assets/io.github.theguy000.StremioLightning.metainfo.xml`**:
  A complete, premium AppStream metainfo file that successfully passes validation (`appstreamcli validate`) with zero warnings and zero errors.
  ```xml
  <?xml version="1.0" encoding="UTF-8"?>
  <component type="desktop-application">
    <id>io.github.theguy000.StremioLightning</id>
    <name>Stremio Lightning</name>
    <summary>Lightweight native Stremio shell</summary>
    <metadata_license>CC0-1.0</metadata_license>
    <project_license>MIT</project_license>
    <developer id="io.github.theguy000">
      <name>Stremio Lightning Developers</name>
    </developer>
    <description>
      <p>
        Stremio Lightning is a fast, lightweight, and modern desktop wrapper for Stremio.
        It features built-in community plugin management, downloadable themes, and
        Discord Rich Presence support.
      </p>
      <p>
        With its native media player backend powered by libmpv, it provides highly
        efficient, hardware-accelerated playback under both X11 and Wayland.
      </p>
    </description>
    <launchable type="desktop-id">io.github.theguy000.StremioLightning.desktop</launchable>
    <url type="homepage">https://github.com/theguy000/stremio-lightning</url>
    <url type="bugtracker">https://github.com/theguy000/stremio-lightning/issues</url>
    <url type="vcs-browser">https://github.com/theguy000/stremio-lightning</url>
    <screenshots>
      <screenshot type="default">
        <caption>Main UI of Stremio Lightning</caption>
        <image>https://raw.githubusercontent.com/theguy000/stremio-lightning/master/assets/icons/128x128.png</image>
      </screenshot>
    </screenshots>
    <content_rating type="oars-1.1" />
    <releases>
      <release version="0.1.0" date="2026-05-27">
        <description>
          <p>Initial release of Stremio Lightning on Linux via Flatpak.</p>
          <ul>
            <li>Lightweight native shell wrapper for Stremio using GTK4 &amp; WebKitGTK 6</li>
            <li>Hardware-accelerated native playback powered by libmpv</li>
            <li>Community plugin management and custom theme support</li>
            <li>Discord Rich Presence integration</li>
          </ul>
        </description>
      </release>
    </releases>
  </component>
  ```

### 2. Integration Changes
- **Flatpak Manifest (`flatpak/io.github.theguy000.StremioLightning.json`)**:
  Replace the dynamic `echo` commands under the `post-install` block of the `stremio-lightning` module with clean file copy commands:
  ```json
  "install -Dm644 assets/io.github.theguy000.StremioLightning.desktop /app/share/applications/io.github.theguy000.StremioLightning.desktop",
  "install -Dm644 assets/io.github.theguy000.StremioLightning.metainfo.xml /app/share/metainfo/io.github.theguy000.StremioLightning.metainfo.xml"
  ```

- **Linux Packaging Script (`crates/xtask/src/package_linux.rs`)**:
  - In `prepare_linux_flatpak_payload()`, copy `assets/io.github.theguy000.StremioLightning.desktop` and `assets/io.github.theguy000.StremioLightning.metainfo.xml` directly instead of writing them using format strings.
  - In `package_linux_deb()`, copy `assets/io.github.theguy000.StremioLightning.desktop`.
  - In `prepare_linux_appdir()`, read `assets/io.github.theguy000.StremioLightning.desktop` and write with `Exec=stremio-lightning-linux`.
  - Delete `linux_flatpak_metainfo()`.
