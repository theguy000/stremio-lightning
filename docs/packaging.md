# Packaging

Packaging commands build release artifacts under `dist/`. Run them from the
repository root after installing frontend dependencies and the target
platform's native dependencies.

Use plain `cargo build` for compile checks. Use `cargo xtask package-*` when you
need a distributable artifact.

## Command Summary

| Artifact | Command | Output |
| --- | --- | --- |
| Linux AppImage | `cargo xtask package-linux-appimage` | `dist/Stremio_Lightning_Linux-x86_64.AppImage` |
| Linux Debian package | `cargo xtask package-linux-deb` | `dist/stremio-lightning-linux-amd64.deb` |
| Linux Flatpak, fast host build | `cargo xtask package-linux-flatpak` | `dist/Stremio_Lightning_Linux-x86_64.flatpak` |
| Linux Flatpak, hermetic build | `cargo xtask package-linux-flatpak-builder` | `dist/Stremio_Lightning_Linux-x86_64.flatpak` |
| macOS app bundle | `cargo xtask package-macos` | `dist/Stremio Lightning.app` |
| Windows portable zip | `cargo xtask package-windows-portable` | `dist/stremio-lightning-windows-portable.zip` |
| Windows installer | `cargo xtask package-windows-installer` | `dist/stremio-lightning-windows-setup.exe` |

## Linux

Complete the [Linux setup](platforms/linux.md) before packaging.

### AppImage and Debian

```bash
cargo xtask package-linux-appimage
cargo xtask package-linux-deb
```

If `appimagetool` is missing, install it at the default cache path:

```bash
mkdir -p "$HOME/.cache/appimage"
curl -L "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage" -o "$HOME/.cache/appimage/appimagetool-x86_64.AppImage"
chmod +x "$HOME/.cache/appimage/appimagetool-x86_64.AppImage"
```

Alternatively, point `APPIMAGE_TOOL` to another executable:

```bash
APPIMAGE_TOOL=/path/to/appimagetool cargo xtask package-linux-appimage
```

### Flatpak

Choose the Flatpak workflow based on the artifact's purpose:

| Workflow | Command | Use |
| --- | --- | --- |
| Fast host bundling | `cargo xtask package-linux-flatpak` | Local development; bundles host binaries and shared libraries. |
| Hermetic builder | `cargo xtask package-linux-flatpak-builder` | Release and CI; compiles inside the GNOME 50 sandbox. |

The fast workflow requires the `flatpak` CLI. The hermetic workflow requires
`flatpak-builder` and the GNOME 50 SDK. Install the required runtimes with:

```bash
flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.gnome.Platform//50 org.gnome.Sdk//50 org.freedesktop.Sdk.Extension.rust-stable//25.08 org.freedesktop.Sdk.Extension.node22//25.08
```

Install and run the resulting local bundle:

```bash
flatpak install --user --bundle dist/Stremio_Lightning_Linux-x86_64.flatpak
flatpak run io.github.theguy000.stremio-lightning
```

## macOS

Complete the [macOS setup](platforms/macos.md), then package on macOS:

```bash
cargo xtask package-macos
```

The command uses platform-specific Apple tools such as `install_name_tool` and
`codesign`, so it cannot produce a valid bundle from another operating system.

## Windows

Complete the [Windows setup](platforms/windows.md) before packaging.

Build the portable distribution:

```powershell
cargo xtask package-windows-portable
```

On Windows this uses the MSVC toolchain. Linux and macOS use `cargo-xwin` for
MSVC cross-compilation:

```bash
cargo install cargo-xwin
```

Cross-building can assemble the portable layout, but runtime validation still
requires Windows with WebView2 and MPV DLL loading available.

Build the installer on Windows with Inno Setup's `iscc` compiler in `PATH`:

```powershell
cargo xtask package-windows-installer
```

The installer command is Windows-only. It prepares the portable layout before
creating the setup executable.

## Package Validation

For Linux packaging changes, validate the relevant package. For example:

```bash
cargo xtask package-linux-appimage
./dist/Stremio_Lightning_Linux-x86_64.AppImage --appimage-help
```

For Windows packaging changes, run the relevant packaging command on a supported
host. Run installer validation only on Windows with Inno Setup installed.
