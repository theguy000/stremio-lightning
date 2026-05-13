use std::{
    collections::BTreeSet,
    env,
    error::Error,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const APP_ID: &str = "stremio-lightning";
const APP_NAME: &str = "Stremio Lightning";
const LINUX_BIN: &str = "stremio-lightning-linux";
const MACOS_BIN: &str = "stremio-lightning-macos";
const WINDOWS_BIN: &str = "stremio-lightning-windows";
const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";
const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";
const LINUX_APPIMAGE: &str = "Stremio_Lightning_Linux-x86_64.AppImage";
const LINUX_DEB: &str = "stremio-lightning-linux-amd64.deb";
const LINUX_FLATPAK: &str = "Stremio_Lightning_Linux-x86_64.flatpak";
const LINUX_FLATPAK_ID: &str = "io.github.theguy000.StremioLightning";
const LINUX_FLATPAK_RUNTIME: &str = "org.freedesktop.Platform";
const LINUX_FLATPAK_SDK: &str = "org.freedesktop.Sdk";
const LINUX_FLATPAK_RUNTIME_VERSION: &str = "25.08";
const MACOS_APP_BUNDLE: &str = "Stremio Lightning.app";
const WINDOWS_ZIP: &str = "stremio-lightning-windows-portable.zip";
const WINDOWS_INSTALLER: &str = "stremio-lightning-windows-setup.exe";

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
        "setup" => setup_current_platform()?,
        "setup-linux" | "setup:linux" | "setup-linux-shell" => setup_linux()?,
        "setup-windows" | "setup:windows" | "setup-windows-shell" => setup_windows()?,
        "build-ui" => run_npm(&["run", "build:ui"])?,
        "test-ui" => run_npm(&["run", "test:ui"])?,
        "package-linux-appimage" => build_linux_appimage()?,
        "package-linux-deb" => package_linux_deb()?,
        "package-linux-flatpak" => package_linux_flatpak()?,
        "package-macos" => package_macos()?,
        "package-windows-portable" => package_windows()?,
        "package-windows-installer" => package_windows_installer()?,
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
  cargo xtask package-linux-appimage      Build dist/{LINUX_APPIMAGE}\n\
  cargo xtask package-linux-deb           Build dist/{LINUX_DEB}\n\
    cargo xtask package-linux-flatpak       Build dist/{LINUX_FLATPAK}\n\
  cargo xtask package-macos               Build dist/{MACOS_APP_BUNDLE}\n\
  cargo xtask package-windows-portable    Build dist/{WINDOWS_ZIP}\n\
  cargo xtask package-windows-installer   Build dist/{WINDOWS_INSTALLER}\n"
    );
}

fn setup_current_platform() -> Result<()> {
    match env::consts::OS {
        "linux" => setup_linux(),
        "windows" => setup_windows(),
        os => Err(format!("unsupported platform for native shell setup: {os}").into()),
    }
}

fn setup_linux() -> Result<()> {
    run_program(
        bash_program(),
        &[root().join("scripts/download-linux-shell-deps.sh")],
    )
}

fn setup_windows() -> Result<()> {
    run_program(
        bash_program(),
        &[root().join("scripts/download-windows-shell-deps.sh")],
    )
}

fn build_linux_appimage() -> Result<()> {
    let root = root();
    let dist_dir = root.join("dist");
    let output = dist_dir.join(LINUX_APPIMAGE);
    let appdir = prepare_linux_appdir()?;

    let appimage_tool = appimage_tool_path()?;
    if !is_executable_file(&appimage_tool) {
        return Err(format!(
            "AppImage tool not found or not executable: {}\n       Set APPIMAGE_TOOL=/path/to/appimagetool or download appimagetool to the default cache path.",
            appimage_tool.display()
        )
        .into());
    }

    println!("==> Packaging AppImage...");
    remove_file_if_exists(&output)?;
    let mut command = Command::new(&appimage_tool);
    command
        .env("ARCH", "x86_64")
        .env("APPIMAGE_EXTRACT_AND_RUN", "1")
        .arg(&appdir)
        .arg(&output);
    run_command(&mut command)?;
    chmod_executable(&output)?;

    println!(
        "==> AppImage ready: {}",
        output.strip_prefix(&root).unwrap_or(&output).display()
    );
    Ok(())
}

fn package_linux_deb() -> Result<()> {
    if env::consts::OS != "linux" {
        return Err("cargo xtask package-linux-deb must be run on Linux".into());
    }

    let root = root();
    let dist_dir = root.join("dist");
    let output = dist_dir.join(LINUX_DEB);
    let appdir = prepare_linux_appdir()?;
    let deb_root = root.join("target/deb/stremio-lightning");
    let debian_dir = deb_root.join("DEBIAN");
    let install_root = deb_root.join(format!("usr/lib/{APP_ID}"));
    let bundled_lib_dir = install_root.join("lib");

    remove_dir_if_exists(&deb_root)?;
    fs::create_dir_all(&debian_dir)?;
    fs::create_dir_all(deb_root.join("usr/bin"))?;
    fs::create_dir_all(&install_root)?;
    fs::create_dir_all(&bundled_lib_dir)?;
    fs::create_dir_all(deb_root.join("usr/share/applications"))?;
    fs::create_dir_all(deb_root.join("usr/share/icons/hicolor/128x128/apps"))?;
    fs::create_dir_all(&dist_dir)?;

    copy_file(
        appdir.join(format!("usr/bin/{LINUX_BIN}")),
        install_root.join(LINUX_BIN),
    )?;
    copy_dir_recursive(
        appdir.join(format!("usr/lib/{APP_ID}/binaries")),
        install_root.join("binaries"),
    )?;
    copy_dir_recursive(
        appdir.join(format!("usr/lib/{APP_ID}/resources")),
        install_root.join("resources"),
    )?;
    copy_file(
        appdir.join(format!("usr/share/icons/hicolor/128x128/apps/{APP_ID}.png")),
        deb_root.join(format!("usr/share/icons/hicolor/128x128/apps/{APP_ID}.png")),
    )?;

    for entry in fs::read_dir(appdir.join("usr/lib"))? {
        let entry = entry?;
        if entry.file_name() == OsStr::new(APP_ID) {
            continue;
        }

        let path = entry.path();
        let destination = bundled_lib_dir.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(path, destination)?;
        } else if path.is_file() {
            copy_file(path, destination)?;
        }
    }

    write_file(
        deb_root.join(format!("usr/bin/{APP_ID}")),
        format!(
            "#!/bin/sh\nset -eu\nexport LD_LIBRARY_PATH=\"/usr/lib/{APP_ID}/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport STREMIO_LIGHTNING_BUNDLE_DIR=\"/usr/lib/{APP_ID}\"\nexec \"/usr/lib/{APP_ID}/{LINUX_BIN}\" \"$@\"\n"
        ),
    )?;
    chmod_executable(deb_root.join(format!("usr/bin/{APP_ID}")))?;
    chmod_executable(install_root.join(LINUX_BIN))?;
    chmod_executable(install_root.join(format!("binaries/stremio-runtime-{LINUX_TARGET}")))?;
    chmod_executable(install_root.join("resources/ffmpeg"))?;
    chmod_executable(install_root.join("resources/ffprobe"))?;

    write_file(
        deb_root.join(format!("usr/share/applications/{APP_ID}.desktop")),
        format!(
            "[Desktop Entry]\nType=Application\nName={APP_NAME}\nExec={APP_ID}\nIcon={APP_ID}\nCategories=AudioVideo;Video;Player;\nTerminal=false\n"
        ),
    )?;
    write_file(
        debian_dir.join("control"),
        format!(
            "Package: {APP_ID}\nVersion: {}\nSection: video\nPriority: optional\nArchitecture: amd64\nMaintainer: Stremio Lightning Maintainers <noreply@example.com>\nDescription: Lightweight native Stremio shell\n Stremio Lightning packages a native Linux shell with bundled runtime resources.\n",
            package_version()?
        ),
    )?;

    remove_file_if_exists(&output)?;
    run_program(
        "dpkg-deb",
        [
            "--root-owner-group",
            "--build",
            &deb_root.to_string_lossy(),
            &output.to_string_lossy(),
        ],
    )?;
    println!("==> Linux deb ready: {}", output.display());

    Ok(())
}

fn package_linux_flatpak() -> Result<()> {
    if env::consts::OS != "linux" {
        return Err("cargo xtask package-linux-flatpak must be run on Linux".into());
    }
    if !program_exists("flatpak") {
        return Err("missing flatpak. Install Flatpak tooling, then retry.".into());
    }

    let root = root();
    let dist_dir = root.join("dist");
    let output = dist_dir.join(LINUX_FLATPAK);
    let appdir = prepare_linux_appdir()?;
    let flatpak_dir = root.join("target/flatpak");
    let payload_dir = flatpak_dir.join("payload");
    let repo_dir = flatpak_dir.join("repo");

    remove_dir_if_exists(&payload_dir)?;
    remove_dir_if_exists(&repo_dir)?;
    fs::create_dir_all(&payload_dir)?;
    fs::create_dir_all(&dist_dir)?;
    prepare_linux_flatpak_payload(&appdir, &payload_dir)?;
    validate_flatpak_glibc_symbols(&payload_dir)?;
    write_file(payload_dir.join("metadata"), linux_flatpak_metadata())?;

    println!("==> Finalizing Flatpak payload...");
    run_program(
        "flatpak",
        [
            OsString::from("build-finish"),
            OsString::from("--no-exports"),
            payload_dir.as_os_str().to_os_string(),
        ],
    )?;

    println!("==> Building Flatpak repository...");
    run_program(
        "flatpak",
        [
            OsString::from("build-export"),
            OsString::from("--arch=x86_64"),
            repo_dir.as_os_str().to_os_string(),
            payload_dir.as_os_str().to_os_string(),
            OsString::from("stable"),
        ],
    )?;

    println!("==> Exporting Flatpak bundle...");
    remove_file_if_exists(&output)?;
    run_program(
        "flatpak",
        [
            OsString::from("build-bundle"),
            repo_dir.as_os_str().to_os_string(),
            output.as_os_str().to_os_string(),
            OsString::from(LINUX_FLATPAK_ID),
            OsString::from("stable"),
        ],
    )?;

    println!(
        "==> Linux Flatpak ready: {}",
        output.strip_prefix(&root).unwrap_or(&output).display()
    );
    Ok(())
}

fn prepare_linux_flatpak_payload(appdir: &Path, payload_dir: &Path) -> Result<()> {
    let files_dir = payload_dir.join("files");
    let bin_dir = files_dir.join("bin");
    let applications_dir = files_dir.join("share/applications");
    let icons_dir = files_dir.join("share/icons/hicolor/128x128/apps");
    let metainfo_dir = files_dir.join("share/metainfo");

    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&applications_dir)?;
    fs::create_dir_all(&icons_dir)?;
    fs::create_dir_all(&metainfo_dir)?;

    copy_dir_recursive(appdir.join("usr/lib"), files_dir.join("lib"))?;
    copy_file(
        appdir.join(format!("usr/bin/{LINUX_BIN}")),
        bin_dir.join(LINUX_BIN),
    )?;
    write_file(
        bin_dir.join(APP_ID),
        format!(
            "#!/bin/sh\nset -eu\nexport LD_LIBRARY_PATH=\"/app/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport STREMIO_LIGHTNING_BUNDLE_DIR=\"/app/lib/{APP_ID}\"\nexec /app/bin/{LINUX_BIN} \"$@\"\n"
        ),
    )?;
    chmod_executable(bin_dir.join(APP_ID))?;
    chmod_executable(bin_dir.join(LINUX_BIN))?;
    chmod_executable(files_dir.join(format!(
        "lib/{APP_ID}/binaries/stremio-runtime-{LINUX_TARGET}"
    )))?;
    chmod_executable(files_dir.join(format!("lib/{APP_ID}/resources/ffmpeg")))?;
    chmod_executable(files_dir.join(format!("lib/{APP_ID}/resources/ffprobe")))?;

    write_file(
        applications_dir.join(format!("{LINUX_FLATPAK_ID}.desktop")),
        format!(
            "[Desktop Entry]\nType=Application\nName={APP_NAME}\nExec={APP_ID}\nIcon={LINUX_FLATPAK_ID}\nCategories=AudioVideo;Video;Player;\nTerminal=false\nStartupNotify=true\n"
        ),
    )?;
    copy_file(
        appdir.join(format!("{APP_ID}.png")),
        icons_dir.join(format!("{LINUX_FLATPAK_ID}.png")),
    )?;
    write_file(
        metainfo_dir.join(format!("{LINUX_FLATPAK_ID}.metainfo.xml")),
        linux_flatpak_metainfo(),
    )?;

    Ok(())
}

fn linux_flatpak_metadata() -> String {
    format!(
        "[Application]\nname={LINUX_FLATPAK_ID}\nruntime={LINUX_FLATPAK_RUNTIME}/x86_64/{LINUX_FLATPAK_RUNTIME_VERSION}\nsdk={LINUX_FLATPAK_SDK}/x86_64/{LINUX_FLATPAK_RUNTIME_VERSION}\ncommand={APP_ID}\n\n[Context]\nshared=ipc;network;\nsockets=x11;pulseaudio;\ndevices=dri;\n\n[Session Bus Policy]\norg.freedesktop.Notifications=talk\n"
    )
}

fn linux_flatpak_metainfo() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>{LINUX_FLATPAK_ID}</id>
  <name>{APP_NAME}</name>
  <summary>Lightweight native Stremio shell</summary>
  <metadata_license>MIT</metadata_license>
  <project_license>MIT</project_license>
  <description>
    <p>Stremio Lightning packages a native Linux shell with plugin management, theme support, and MPV-powered playback.</p>
  </description>
  <launchable type="desktop-id">{LINUX_FLATPAK_ID}.desktop</launchable>
  <categories>
    <category>AudioVideo</category>
    <category>Video</category>
    <category>Player</category>
  </categories>
  <releases>
    <release version="{}" date="2026-05-12" />
  </releases>
</component>
"#,
        package_version().unwrap_or_else(|_| "0.1.0".to_string())
    )
}

fn validate_flatpak_glibc_symbols(payload_dir: &Path) -> Result<()> {
    if !program_exists("readelf") {
        return Err("missing readelf. Install binutils before packaging the Flatpak.".into());
    }

    let mut offenders = Vec::new();
    collect_flatpak_glibc_symbol_offenders(&payload_dir.join("files"), &mut offenders)?;

    if offenders.is_empty() {
        return Ok(());
    }

    Err(format!(
        "Flatpak payload contains libraries requiring GLIBC newer than {LINUX_FLATPAK_RUNTIME}//{LINUX_FLATPAK_RUNTIME_VERSION}:\n       {}\n       Build the Linux payload with a distro/SDK whose GLIBC is compatible with the target Flatpak runtime.",
        offenders.join("\n       ")
    )
    .into())
}

fn collect_flatpak_glibc_symbol_offenders(path: &Path, offenders: &mut Vec<String>) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_flatpak_glibc_symbol_offenders(&entry?.path(), offenders)?;
        }
        return Ok(());
    }

    if !path.is_file() || !is_elf_file(path)? {
        return Ok(());
    }

    let output = Command::new("readelf")
        .arg("--version-info")
        .arg(path)
        .output()?;
    if !output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("GLIBC_2.43") {
        offenders.push(format!("{} requires GLIBC_2.43", path.display()));
    }

    Ok(())
}

fn is_elf_file(path: &Path) -> Result<bool> {
    let bytes = fs::read(path)?;
    Ok(bytes.starts_with(b"\x7fELF"))
}

fn prepare_linux_appdir() -> Result<PathBuf> {
    let root = root();
    let linux_dir = root.join("crates/stremio-lightning-linux");
    let appdir = root.join(format!("target/appimage/{APP_ID}.AppDir"));
    let dist_dir = root.join("dist");
    let runtime = linux_dir.join(format!("binaries/stremio-runtime-{LINUX_TARGET}"));
    let server = linux_dir.join("resources/server.cjs");
    let ffmpeg = linux_dir.join("resources/ffmpeg");
    let ffprobe = linux_dir.join("resources/ffprobe");
    let icon = root.join("assets/icons/128x128.png");
    let app_resources = appdir.join(format!("usr/lib/{APP_ID}/resources"));
    let app_binaries = appdir.join(format!("usr/lib/{APP_ID}/binaries"));
    let app_lib = appdir.join("usr/lib");
    let desktop_file = appdir.join(format!("{APP_ID}.desktop"));

    required_executable_file(&runtime, "cargo xtask setup-linux")?;
    required_file(&server, "cargo xtask setup-linux")?;
    required_executable_file(&ffmpeg, "cargo xtask setup-linux")?;
    required_executable_file(&ffprobe, "cargo xtask setup-linux")?;
    required_file(&icon, "restore assets/icons/128x128.png")?;

    println!("==> Building native Linux shell crate...");
    run_program("cargo", ["build", "-p", LINUX_BIN, "--release"])?;

    remove_dir_if_exists(&appdir)?;
    fs::create_dir_all(appdir.join("usr/bin"))?;
    fs::create_dir_all(&app_lib)?;
    fs::create_dir_all(&app_binaries)?;
    fs::create_dir_all(&app_resources)?;
    fs::create_dir_all(appdir.join("usr/share/applications"))?;
    fs::create_dir_all(appdir.join("usr/share/icons/hicolor/128x128/apps"))?;
    fs::create_dir_all(&dist_dir)?;

    copy_file(
        root.join(format!("target/release/{LINUX_BIN}")),
        appdir.join(format!("usr/bin/{LINUX_BIN}")),
    )?;
    copy_file(
        &runtime,
        app_binaries.join(format!("stremio-runtime-{LINUX_TARGET}")),
    )?;
    copy_file(&server, app_resources.join("server.cjs"))?;
    copy_file(&ffmpeg, app_resources.join("ffmpeg"))?;
    copy_file(&ffprobe, app_resources.join("ffprobe"))?;
    copy_file(&icon, appdir.join(format!("{APP_ID}.png")))?;
    copy_file(
        appdir.join(format!("{APP_ID}.png")),
        appdir.join(format!("usr/share/icons/hicolor/128x128/apps/{APP_ID}.png")),
    )?;

    write_file(
        &desktop_file,
        format!(
            "[Desktop Entry]\nType=Application\nName={APP_NAME}\nExec={LINUX_BIN}\nIcon={APP_ID}\nCategories=AudioVideo;Video;Player;\nTerminal=false\n"
        ),
    )?;
    copy_file(
        &desktop_file,
        appdir.join(format!("usr/share/applications/{APP_ID}.desktop")),
    )?;
    write_file(
        appdir.join("AppRun"),
        format!(
            "#!/bin/bash\nset -euo pipefail\nHERE=$(dirname \"$(readlink -f \"$0\")\")\nexport LD_LIBRARY_PATH=\"$HERE/usr/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport STREMIO_LIGHTNING_BUNDLE_DIR=\"$HERE/usr/lib/{APP_ID}\"\nexec \"$HERE/usr/bin/{LINUX_BIN}\" \"$@\"\n"
        ),
    )?;

    chmod_executable(appdir.join("AppRun"))?;
    chmod_executable(appdir.join(format!("usr/bin/{LINUX_BIN}")))?;
    chmod_executable(app_binaries.join(format!("stremio-runtime-{LINUX_TARGET}")))?;
    chmod_executable(app_resources.join("ffmpeg"))?;
    chmod_executable(app_resources.join("ffprobe"))?;

    bundle_linux_shared_libraries(
        &appdir.join(format!("usr/bin/{LINUX_BIN}")),
        &app_lib,
        "cargo xtask setup-linux",
    )?;

    required_executable_file(appdir.join("AppRun"), "cargo xtask setup-linux")?;
    required_executable_file(
        appdir.join(format!("usr/bin/{LINUX_BIN}")),
        "cargo xtask setup-linux",
    )?;
    required_executable_file(
        app_binaries.join(format!("stremio-runtime-{LINUX_TARGET}")),
        "cargo xtask setup-linux",
    )?;
    required_file(&app_resources.join("server.cjs"), "cargo xtask setup-linux")?;
    required_executable_file(app_resources.join("ffmpeg"), "cargo xtask setup-linux")?;
    required_executable_file(app_resources.join("ffprobe"), "cargo xtask setup-linux")?;

    Ok(appdir)
}

fn package_macos() -> Result<()> {
    if env::consts::OS != "macos" {
        return Err("cargo xtask package-macos must be run on macOS so install_name_tool, codesign, and bundled dylibs match the host architecture".into());
    }

    let root = root();
    let macos_dir = root.join("crates/stremio-lightning-macos");
    let dist_dir = root.join("dist");
    let bundle = dist_dir.join(MACOS_APP_BUNDLE);
    let contents = bundle.join("Contents");
    let executable_dir = contents.join("MacOS");
    let resources_dir = contents.join("Resources");
    let frameworks_dir = contents.join("Frameworks");
    let executable = executable_dir.join(MACOS_BIN);
    let entitlements = resources_dir.join("entitlements.plist");

    required_executable_file(
        macos_dir.join("binaries/stremio-runtime-macos"),
        "provide crates/stremio-lightning-macos/binaries/stremio-runtime-macos",
    )?;
    for name in ["server.cjs", "ffmpeg", "ffprobe"] {
        required_file(
            &macos_dir.join(format!("resources/{name}")),
            "provide macOS streaming server resources under crates/stremio-lightning-macos/resources",
        )?;
    }
    let mpv_library = macos_mpv_library()?;

    println!("==> Building native macOS shell crate...");
    run_program("cargo", ["build", "-p", MACOS_BIN, "--release"])?;

    remove_dir_if_exists(&bundle)?;
    fs::create_dir_all(&executable_dir)?;
    fs::create_dir_all(&resources_dir)?;
    fs::create_dir_all(&frameworks_dir)?;
    fs::create_dir_all(&dist_dir)?;

    copy_file(
        root.join(format!("target/release/{MACOS_BIN}")),
        &executable,
    )?;
    copy_file(
        macos_dir.join("resources/Info.plist"),
        contents.join("Info.plist"),
    )?;
    copy_file(
        macos_dir.join("resources/entitlements.plist"),
        &entitlements,
    )?;

    let mpv_name = mpv_library
        .file_name()
        .ok_or("libmpv path has no file name")?
        .to_os_string();
    let bundled_mpv = frameworks_dir.join(&mpv_name);
    copy_file(&mpv_library, &bundled_mpv)?;

    let bundled_resource_root = resources_dir.join("resources");
    let bundled_binary_root = resources_dir.join("binaries");
    fs::create_dir_all(&bundled_resource_root)?;
    fs::create_dir_all(&bundled_binary_root)?;
    copy_file(
        macos_dir.join("binaries/stremio-runtime-macos"),
        bundled_binary_root.join("stremio-runtime-macos"),
    )?;
    for name in ["server.cjs", "ffmpeg", "ffprobe"] {
        copy_file(
            macos_dir.join(format!("resources/{name}")),
            bundled_resource_root.join(name),
        )?;
    }

    chmod_executable(&executable)?;
    chmod_executable(bundled_binary_root.join("stremio-runtime-macos"))?;
    chmod_executable(bundled_resource_root.join("ffmpeg"))?;
    chmod_executable(bundled_resource_root.join("ffprobe"))?;

    println!("==> Rewriting macOS bundle rpaths...");
    run_install_name_tool([
        "-add_rpath".to_string(),
        "@executable_path/../Frameworks".to_string(),
        executable.to_string_lossy().into_owned(),
    ])?;
    let mpv_file_name = bundled_mpv
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("libmpv.dylib");
    run_install_name_tool([
        "-id".to_string(),
        format!("@rpath/{mpv_file_name}"),
        bundled_mpv.to_string_lossy().into_owned(),
    ])?;

    println!("==> Ad-hoc signing macOS app bundle...");
    run_codesign([
        "--force".to_string(),
        "--deep".to_string(),
        "--options".to_string(),
        "runtime".to_string(),
        "--entitlements".to_string(),
        entitlements.to_string_lossy().into_owned(),
        "--sign".to_string(),
        "-".to_string(),
        bundle.to_string_lossy().into_owned(),
    ])?;

    println!("==> macOS app bundle ready: {}", bundle.display());
    Ok(())
}

fn package_windows() -> Result<()> {
    let root = root();
    let dist_dir = root.join("dist");
    let portable_dir = prepare_windows_portable_layout(&root)?;
    let zip_path = dist_dir.join(WINDOWS_ZIP);

    remove_file_if_exists(&zip_path)?;
    if env::consts::OS == "windows" {
        run_program(
            "powershell",
            [
                "-NoProfile",
                "-Command",
                &format!(
                    "Compress-Archive -Path '{}' -DestinationPath '{}' -Force",
                    portable_dir.join("*").display(),
                    zip_path.display()
                ),
            ],
        )?;
    } else {
        run_program_in(
            &portable_dir,
            "zip",
            ["-r", &zip_path.to_string_lossy(), "."],
        )?;
    }
    println!("==> Windows portable zip ready: {}", zip_path.display());

    Ok(())
}

fn prepare_windows_portable_layout(root: &Path) -> Result<PathBuf> {
    let windows_dir = root.join("crates/stremio-lightning-windows");
    let dist_dir = root.join("dist");
    let portable_dir = dist_dir.join("stremio-lightning-windows-portable");

    required_file(
        &windows_dir.join("resources/stremio-runtime.exe"),
        "cargo xtask setup-windows",
    )?;
    required_file(
        &windows_dir.join("resources/server.cjs"),
        "cargo xtask setup-windows",
    )?;
    required_file(
        &windows_dir.join("resources/ffmpeg.exe"),
        "cargo xtask setup-windows",
    )?;
    required_file(
        &windows_dir.join("resources/ffprobe.exe"),
        "cargo xtask setup-windows",
    )?;
    required_file(
        &windows_dir.join("resources/libmpv-2.dll"),
        "cargo xtask setup-windows",
    )?;
    required_file(
        &root.join("src/favicon.ico"),
        "restore src/favicon.ico for the Windows executable icon",
    )?;

    println!("==> Building native Windows shell crate...");
    build_windows_shell()?;

    remove_dir_if_exists(&portable_dir)?;
    fs::create_dir_all(&dist_dir)?;
    fs::create_dir_all(&portable_dir)?;

    copy_file(
        root.join(format!("target/{WINDOWS_TARGET}/release/{WINDOWS_BIN}.exe")),
        portable_dir.join(format!("{WINDOWS_BIN}.exe")),
    )?;

    // libmpv-2.dll goes flat beside the exe (Packaged layout expects base_dir/libmpv-2.dll)
    copy_file(
        windows_dir.join("resources/libmpv-2.dll"),
        portable_dir.join("libmpv-2.dll"),
    )?;

    // Server/runtime files go into a resources/ subdirectory
    // (Packaged layout expects base_dir/resources/stremio-runtime.exe etc.)
    let portable_resources = portable_dir.join("resources");
    fs::create_dir_all(&portable_resources)?;

    for name in [
        "stremio-runtime.exe",
        "server.cjs",
        "ffmpeg.exe",
        "ffprobe.exe",
    ] {
        copy_file(
            windows_dir.join(format!("resources/{name}")),
            portable_resources.join(name),
        )?;
    }

    Ok(portable_dir)
}

fn package_windows_installer() -> Result<()> {
    if env::consts::OS != "windows" {
        return Err("cargo xtask package-windows-installer must be run on Windows with Inno Setup installed".into());
    }

    let root = root();
    let dist_dir = root.join("dist");
    let portable_dir = prepare_windows_portable_layout(&root)?;
    let installer_script = root.join("target/windows-installer/stremio-lightning.iss");
    let installer_output = dist_dir.join(WINDOWS_INSTALLER);
    let icon = root.join("src/favicon.ico");

    required_file(
        &icon,
        "restore src/favicon.ico for the Windows installer icon",
    )?;

    remove_file_if_exists(&installer_output)?;
    write_file(
        &installer_script,
        format!(
            r#"#define MyAppName "{APP_NAME}"
#define MyAppVersion "{}"
#define MyAppPublisher "Stremio Lightning"
#define MyAppExeName "{WINDOWS_BIN}.exe"
#define MyAppIcon "{}"

[Setup]
AppId={APP_ID}
AppName={{#MyAppName}}
AppVersion={{#MyAppVersion}}
AppPublisher={{#MyAppPublisher}}
DefaultDirName={{autopf}}\{APP_NAME}
DefaultGroupName={{#MyAppName}}
DisableProgramGroupPage=yes
OutputDir={}
OutputBaseFilename={}
Compression=lzma
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SetupIconFile={{#MyAppIcon}}
UninstallDisplayIcon={{app}}\{{#MyAppExeName}}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "{}\*"; DestDir: "{{app}}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{{group}}\{{#MyAppName}}"; Filename: "{{app}}\{{#MyAppExeName}}"; IconFilename: "{{app}}\{{#MyAppExeName}}"
Name: "{{autodesktop}}\{{#MyAppName}}"; Filename: "{{app}}\{{#MyAppExeName}}"; IconFilename: "{{app}}\{{#MyAppExeName}}"; Tasks: desktopicon

[Run]
Filename: "{{app}}\{{#MyAppExeName}}"; Description: "Launch {{#MyAppName}}"; Flags: nowait postinstall skipifsilent
"#,
            package_version()?,
            inno_path(&icon),
            inno_path(&dist_dir),
            WINDOWS_INSTALLER.trim_end_matches(".exe"),
            inno_path(&portable_dir)
        ),
    )?;

    run_program("iscc", [&installer_script])?;
    required_file(&installer_output, "cargo xtask package-windows-installer")?;
    println!(
        "==> Windows installer ready: {}",
        installer_output.display()
    );

    Ok(())
}

fn package_version() -> Result<String> {
    let raw = env::var("STREMIO_LIGHTNING_VERSION")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            if env::var("GITHUB_REF_TYPE").ok().as_deref() == Some("tag") {
                env::var("GITHUB_REF_NAME").ok()
            } else {
                None
            }
        })
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

    let version = raw.trim().trim_start_matches('v').to_string();
    if version.is_empty() {
        return Err("package version is empty".into());
    }
    if !version.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '+' | '~' | '-' | ':')
    }) {
        return Err(format!("package version contains unsupported characters: {raw}").into());
    }
    if !version
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        return Err(format!(
            "package version must start with a digit after optional 'v' prefix: {raw}"
        )
        .into());
    }

    Ok(version)
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask must live under crates/xtask")
        .to_path_buf()
}

fn build_windows_shell() -> Result<()> {
    if env::consts::OS == "windows" {
        run_program(
            "cargo",
            [
                "build",
                "-p",
                WINDOWS_BIN,
                "--release",
                "--target",
                WINDOWS_TARGET,
            ],
        )
    } else {
        if !program_exists("cargo-xwin") {
            return Err(
                "cargo xtask package-windows-portable cross-builds the MSVC target with cargo-xwin off Windows.\n       Install it with: cargo install cargo-xwin"
                    .into(),
            );
        }

        run_program(
            "cargo",
            [
                "xwin",
                "build",
                "-p",
                WINDOWS_BIN,
                "--release",
                "--target",
                WINDOWS_TARGET,
            ],
        )
    }
}

fn macos_mpv_library() -> Result<PathBuf> {
    let mut roots: Vec<PathBuf> = ["MPV_DIR", "STREMIO_LIGHTNING_MPV_DIR"]
        .into_iter()
        .filter_map(env::var_os)
        .map(PathBuf::from)
        .collect();
    let root = root();
    roots.extend([
        root.join("crates/stremio-lightning-macos/mpv-dev"),
        PathBuf::from("/opt/homebrew/opt/mpv"),
        PathBuf::from("/usr/local/opt/mpv"),
    ]);

    for candidate in roots {
        for name in ["libmpv.dylib", "libmpv.2.dylib"] {
            let path = candidate.join("lib").join(name);
            if path.is_file() {
                return Ok(path);
            }
        }
    }

    Err("missing libmpv.dylib for macOS bundle. Set MPV_DIR or STREMIO_LIGHTNING_MPV_DIR to an mpv prefix, or install mpv with Homebrew".into())
}

fn run_install_name_tool<I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    run_program("install_name_tool", args)
}

fn run_codesign<I>(args: I) -> Result<()>
where
    I: IntoIterator<Item = String>,
{
    run_program("codesign", args)
}

fn appimage_tool_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("APPIMAGE_TOOL") {
        return Ok(PathBuf::from(path));
    }

    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or("HOME/USERPROFILE is not set; set APPIMAGE_TOOL explicitly")?;
    Ok(PathBuf::from(home).join(".cache/appimage/appimagetool-x86_64.AppImage"))
}

fn required_file(path: &Path, setup_hint: &str) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|_| {
        format!(
            "missing required file: {}\n       Run: {setup_hint}",
            path.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!(
            "required file is empty or invalid: {}\n       Run: {setup_hint}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn required_executable_file(path: impl AsRef<Path>, setup_hint: &str) -> Result<()> {
    let path = path.as_ref();
    required_file(path, setup_hint)?;
    if !is_executable_file(path) {
        return Err(format!(
            "required file is not executable: {}\n       Run: {setup_hint}",
            path.display()
        )
        .into());
    }
    Ok(())
}

fn bundle_linux_shared_libraries(binary: &Path, app_lib: &Path, setup_hint: &str) -> Result<()> {
    if env::consts::OS != "linux" {
        return Ok(());
    }

    println!("==> Bundling Linux shared libraries...");
    let output = Command::new("ldd").arg(binary).output().map_err(|error| {
        format!(
            "failed to inspect Linux shared libraries for {}: {error}\n       Run: {setup_hint}",
            binary.display()
        )
    })?;
    if !output.status.success() {
        return Err(format!(
            "failed to inspect Linux shared libraries for {}\n       Run: {setup_hint}",
            binary.display()
        )
        .into());
    }

    let ldd = String::from_utf8_lossy(&output.stdout);
    let mut missing = Vec::new();
    let mut libs = BTreeSet::new();
    for line in ldd.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.contains("not found") {
            missing.push(line.to_string());
            continue;
        }

        if let Some(path) = resolved_ldd_path(line) {
            if should_bundle_linux_library(&path) {
                libs.insert(path);
            }
        }
    }

    if !missing.is_empty() {
        return Err(format!(
            "missing Linux shared libraries while preparing AppImage:\n       {}\n       Run: {setup_hint}",
            missing.join("\n       ")
        )
        .into());
    }

    for lib in libs {
        let Some(name) = lib.file_name() else {
            continue;
        };
        copy_file(&lib, app_lib.join(name))?;
    }

    Ok(())
}

fn resolved_ldd_path(line: &str) -> Option<PathBuf> {
    if let Some((_, rest)) = line.split_once("=>") {
        return rest
            .split_whitespace()
            .next()
            .filter(|path| path.starts_with('/'))
            .map(PathBuf::from);
    }

    line.split_whitespace()
        .next()
        .filter(|path| path.starts_with('/'))
        .map(PathBuf::from)
}

fn should_bundle_linux_library(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    !matches!(
        name,
        "ld-linux-x86-64.so.2"
            | "libc.so.6"
            | "libdl.so.2"
            | "libgcc_s.so.1"
            | "libm.so.6"
            | "libpthread.so.0"
            | "libresolv.so.2"
            | "librt.so.1"
            | "libstdc++.so.6"
    )
}

fn copy_file(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(from, to).map_err(|error| {
        format!(
            "failed to copy {} to {}: {error}",
            from.display(),
            to.display()
        )
    })?;
    Ok(())
}

fn copy_dir_recursive(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();
    fs::create_dir_all(to)?;

    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_dir_recursive(source, destination)?;
        } else if source.is_file() {
            copy_file(source, destination)?;
        }
    }

    Ok(())
}

fn inno_path(path: &Path) -> String {
    path.to_string_lossy().replace('"', "\"\"")
}

fn write_file(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn remove_dir_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn remove_file_if_exists(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn run_npm(args: &[&str]) -> Result<()> {
    let program = if cfg!(windows) { "npm.cmd" } else { "npm" };
    run_program(program, args)
}

fn bash_program() -> OsString {
    #[cfg(windows)]
    {
        let git_bash = Path::new(r"C:\Program Files\Git\bin\bash.exe");
        if git_bash.is_file() {
            return git_bash.as_os_str().to_os_string();
        }
    }

    OsString::from("bash")
}

fn run_program<I, S>(program: impl AsRef<OsStr>, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    run_program_in(root(), program, args)
}

fn run_program_in<I, S>(cwd: impl AsRef<Path>, program: impl AsRef<OsStr>, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut command = Command::new(program);
    command.args(args.into_iter().map(Into::into));
    run_command_in(&mut command, cwd)
}

fn run_command(command: &mut Command) -> Result<()> {
    run_command_in(command, root())
}

fn run_command_in(command: &mut Command, cwd: impl AsRef<Path>) -> Result<()> {
    command
        .current_dir(cwd)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let status = command.status()?;
    if !status.success() {
        return Err(format!("command failed with status {status:?}: {command:?}").into());
    }
    Ok(())
}

fn program_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn is_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn chmod_executable(path: impl AsRef<Path>) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = path.as_ref();
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
