pub mod common;
pub mod package_linux;
pub mod package_macos;
pub mod package_windows;
pub mod setup;
pub mod validation;

use common::{Result, run_npm};
use std::env;

pub const APP_ID: &str = "stremio-lightning";
pub const APP_NAME: &str = "Stremio Lightning";
pub const LINUX_BIN: &str = "stremio-lightning-linux";
pub const MACOS_BIN: &str = "stremio-lightning-macos";
pub const WINDOWS_BIN: &str = "stremio-lightning-windows";
pub const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";
pub const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";
pub const LINUX_APPIMAGE: &str = "Stremio_Lightning_Linux-x86_64.AppImage";
pub const LINUX_DEB: &str = "stremio-lightning-linux-amd64.deb";
pub const LINUX_FLATPAK: &str = "Stremio_Lightning_Linux-x86_64.flatpak";
pub const LINUX_DESKTOP_ID: &str = "io.github.theguy000.StremioLightning";
pub const LINUX_FLATPAK_ID: &str = LINUX_DESKTOP_ID;
pub const LINUX_FLATPAK_RUNTIME: &str = "org.gnome.Platform";
pub const LINUX_FLATPAK_SDK: &str = "org.gnome.Sdk";
pub const LINUX_FLATPAK_RUNTIME_VERSION: &str = "50";
pub const MACOS_APP_BUNDLE: &str = "Stremio Lightning.app";
pub const WINDOWS_ZIP: &str = "stremio-lightning-windows-portable.zip";
pub const WINDOWS_INSTALLER: &str = "stremio-lightning-windows-setup.exe";

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        print_help();
        return Ok(());
    };

    match command.as_str() {
        "help" | "--help" | "-h" => print_help(),
        "setup" => setup::setup_current_platform()?,
        "setup-linux" | "setup:linux" | "setup-linux-shell" => setup::setup_linux()?,
        "setup-windows" | "setup:windows" | "setup-windows-shell" => setup::setup_windows()?,
        "build-ui" => run_npm(&["run", "build:ui"])?,
        "test-ui" => run_npm(&["run", "test:ui"])?,
        "validate" => validation::run_validation()?,
        "package-linux-appimage" => package_linux::build_linux_appimage()?,
        "package-linux-deb" => package_linux::package_linux_deb()?,
        "package-linux-flatpak" => package_linux::package_linux_flatpak()?,
        "package-linux-flatpak-builder" => package_linux::package_linux_flatpak_builder()?,
        "package-macos" => package_macos::package_macos()?,
        "package-windows-portable" => package_windows::package_windows()?,
        "package-windows-installer" => package_windows::package_windows_installer()?,
        other => {
            return Err(format!(
                "unknown xtask command '{other}'. Run `cargo xtask help` for usage."
            )
            .into());
        }
    }

    Ok(())
}

fn print_help() {
    println!(
        "Stremio Lightning xtask\n\n\
Usage:\n\
  cargo xtask setup                       Download native shell dependencies for this OS\n\
  cargo xtask setup-linux                 Download Linux shell dependencies\n\
  cargo xtask setup-windows               Download Windows shell dependencies\n\
  cargo xtask build-ui                    Build the Svelte/Vite UI bundle\n\
  cargo xtask test-ui                     Run frontend tests\n\
  cargo xtask validate                    Run all formatting, linting, tests and UI checks\n\
  cargo xtask package-linux-appimage      Build dist/{LINUX_APPIMAGE}\n\
  cargo xtask package-linux-deb           Build dist/{LINUX_DEB}\n\
  cargo xtask package-linux-flatpak       Build dist/{LINUX_FLATPAK} (Fast-Host bundling)\n\
  cargo xtask package-linux-flatpak-builder Build dist/{LINUX_FLATPAK} (Hermetic flatpak-builder)\n\
  cargo xtask package-macos               Build dist/{MACOS_APP_BUNDLE}\n\
  cargo xtask package-windows-portable    Build dist/{WINDOWS_ZIP}\n\
  cargo xtask package-windows-installer   Build dist/{WINDOWS_INSTALLER}\n"
    );
}
