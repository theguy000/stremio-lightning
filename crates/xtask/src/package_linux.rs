use crate::common::{
    Result, chmod_executable, copy_dir_recursive, copy_file, is_executable_file, package_version,
    program_exists, remove_dir_if_exists, remove_file_if_exists, required_executable_file,
    required_file, root, run_command, run_program, write_file,
};
use crate::{
    APP_ID, LINUX_APPIMAGE, LINUX_BIN, LINUX_DEB, LINUX_DESKTOP_ID, LINUX_FLATPAK,
    LINUX_FLATPAK_ID, LINUX_FLATPAK_RUNTIME, LINUX_FLATPAK_RUNTIME_VERSION, LINUX_FLATPAK_SDK,
    LINUX_TARGET,
};
use std::{
    collections::BTreeSet,
    env,
    ffi::{OsStr, OsString},
    fmt, fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn build_linux_appimage() -> Result<()> {
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

pub fn package_linux_deb() -> Result<()> {
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
        appdir.join(format!(
            "usr/share/icons/hicolor/128x128/apps/{LINUX_DESKTOP_ID}.png"
        )),
        deb_root.join(format!(
            "usr/share/icons/hicolor/128x128/apps/{LINUX_DESKTOP_ID}.png"
        )),
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
        linux_deb_launcher_script(),
    )?;
    chmod_executable(deb_root.join(format!("usr/bin/{APP_ID}")))?;
    chmod_executable(install_root.join(LINUX_BIN))?;
    chmod_executable(install_root.join(format!("binaries/stremio-runtime-{LINUX_TARGET}")))?;
    chmod_executable(install_root.join("resources/ffmpeg"))?;
    chmod_executable(install_root.join("resources/ffprobe"))?;

    copy_file(
        root.join(format!("assets/{LINUX_DESKTOP_ID}.desktop")),
        deb_root.join(format!("usr/share/applications/{LINUX_DESKTOP_ID}.desktop")),
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

pub fn package_linux_flatpak() -> Result<()> {
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
    let root = root();
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
    write_file(bin_dir.join(APP_ID), linux_flatpak_launcher_script())?;
    chmod_executable(bin_dir.join(APP_ID))?;
    chmod_executable(bin_dir.join(LINUX_BIN))?;
    chmod_executable(files_dir.join(format!(
        "lib/{APP_ID}/binaries/stremio-runtime-{LINUX_TARGET}"
    )))?;
    chmod_executable(files_dir.join(format!("lib/{APP_ID}/resources/ffmpeg")))?;
    chmod_executable(files_dir.join(format!("lib/{APP_ID}/resources/ffprobe")))?;

    copy_file(
        root.join(format!("assets/{LINUX_FLATPAK_ID}.desktop")),
        applications_dir.join(format!("{LINUX_FLATPAK_ID}.desktop")),
    )?;
    copy_file(
        appdir.join(format!("{LINUX_DESKTOP_ID}.png")),
        icons_dir.join(format!("{LINUX_FLATPAK_ID}.png")),
    )?;
    copy_file(
        root.join(format!("assets/{LINUX_FLATPAK_ID}.metainfo.xml")),
        metainfo_dir.join(format!("{LINUX_FLATPAK_ID}.metainfo.xml")),
    )?;

    Ok(())
}

fn linux_flatpak_metadata() -> String {
    format!(
        "[Application]\nname={LINUX_FLATPAK_ID}\nruntime={LINUX_FLATPAK_RUNTIME}/x86_64/{LINUX_FLATPAK_RUNTIME_VERSION}\nsdk={LINUX_FLATPAK_SDK}/x86_64/{LINUX_FLATPAK_RUNTIME_VERSION}\ncommand={APP_ID}\n\n[Context]\nshared=ipc;network;\nsockets=x11;pulseaudio;\ndevices=dri;\n\n[Session Bus Policy]\norg.freedesktop.Notifications=talk\n{LINUX_FLATPAK_ID}=own\n"
    )
}

fn linux_deb_launcher_script() -> String {
    format!(
        "#!/bin/sh\nset -eu\nexport LD_LIBRARY_PATH=\"/usr/lib/{APP_ID}/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport STREMIO_LIGHTNING_BUNDLE_DIR=\"/usr/lib/{APP_ID}\"\nexec \"/usr/lib/{APP_ID}/{LINUX_BIN}\" \"$@\"\n"
    )
}

fn linux_flatpak_launcher_script() -> String {
    format!(
        "#!/bin/sh\nset -eu\nexport LD_LIBRARY_PATH=\"/app/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport STREMIO_LIGHTNING_BUNDLE_DIR=\"/app/lib/{APP_ID}\"\nexport WEBKIT_EXEC_PATH=\"/app/lib/webkitgtk-6.0\"\nexport WEBKIT_INJECTED_BUNDLE_PATH=\"/app/lib/webkitgtk-6.0/injected-bundle\"\nexec /app/bin/{LINUX_BIN} \"$@\"\n"
    )
}

fn linux_appimage_launcher_script() -> String {
    format!(
        "#!/bin/bash\nset -euo pipefail\nHERE=$(dirname \"$(readlink -f \"$0\")\")\nexport LD_LIBRARY_PATH=\"$HERE/usr/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport STREMIO_LIGHTNING_BUNDLE_DIR=\"$HERE/usr/lib/{APP_ID}\"\nexport WEBKIT_EXEC_PATH=\"$HERE/usr/lib/webkitgtk-6.0\"\nexport WEBKIT_INJECTED_BUNDLE_PATH=\"$HERE/usr/lib/webkitgtk-6.0/injected-bundle\"\nexec \"$HERE/usr/bin/{LINUX_BIN}\" \"$@\"\n"
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct GlibcVersion {
    major: u16,
    minor: u16,
    patch: u16,
}

impl GlibcVersion {
    const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }
}

impl fmt::Display for GlibcVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.patch == 0 {
            write!(f, "GLIBC_{}.{}", self.major, self.minor)
        } else {
            write!(f, "GLIBC_{}.{}.{}", self.major, self.minor, self.patch)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FlatpakGlibcPolicy {
    max_supported: GlibcVersion,
}

fn current_flatpak_glibc_policy() -> Result<FlatpakGlibcPolicy> {
    flatpak_glibc_policy(LINUX_FLATPAK_RUNTIME, LINUX_FLATPAK_RUNTIME_VERSION).ok_or_else(|| {
        format!(
            "missing GLIBC compatibility policy for Flatpak runtime {LINUX_FLATPAK_RUNTIME}//{LINUX_FLATPAK_RUNTIME_VERSION}; update xtask before packaging"
        )
        .into()
    })
}

fn flatpak_glibc_policy(runtime: &str, runtime_version: &str) -> Option<FlatpakGlibcPolicy> {
    match (runtime, runtime_version) {
        ("org.gnome.Platform", "50") | ("org.freedesktop.Platform", "25.08") => {
            Some(FlatpakGlibcPolicy {
                max_supported: GlibcVersion::new(2, 42, 0),
            })
        }
        _ => None,
    }
}

fn glibc_symbols_newer_than(
    readelf_version_info: &str,
    policy: FlatpakGlibcPolicy,
) -> Vec<GlibcVersion> {
    let mut symbols = BTreeSet::new();
    let mut remaining = readelf_version_info;

    while let Some(index) = remaining.find("GLIBC_") {
        let symbol_tail = &remaining[index + "GLIBC_".len()..];
        let raw_version = symbol_tail
            .split(|character: char| !(character.is_ascii_digit() || character == '.'))
            .next()
            .unwrap_or_default();

        if let Some(version) = parse_glibc_version(raw_version)
            && version > policy.max_supported
        {
            symbols.insert(version);
        }

        let consumed = raw_version.len().max(1).min(symbol_tail.len());
        remaining = &symbol_tail[consumed..];
    }

    symbols.into_iter().collect()
}

fn parse_glibc_version(raw: &str) -> Option<GlibcVersion> {
    let mut parts = raw.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().map(str::parse).transpose().ok()?.unwrap_or(0);
    if parts.next().is_some() {
        return None;
    }

    Some(GlibcVersion::new(major, minor, patch))
}

fn validate_flatpak_glibc_symbols(payload_dir: &Path) -> Result<()> {
    if !program_exists("readelf") {
        return Err("missing readelf. Install binutils before packaging the Flatpak.".into());
    }

    let files_dir = payload_dir.join("files");
    if !files_dir.is_dir() {
        return Err(format!(
            "Flatpak payload is missing files directory for GLIBC validation: {}",
            files_dir.display()
        )
        .into());
    }

    let policy = current_flatpak_glibc_policy()?;
    let mut offenders = Vec::new();
    collect_flatpak_glibc_symbol_offenders(&files_dir, policy, &mut offenders)?;

    if offenders.is_empty() {
        return Ok(());
    }

    if env::var("IGNORE_GLIBC").is_ok() || env::var("STREMIO_LIGHTNING_IGNORE_GLIBC").is_ok() {
        println!("WARNING: Ignoring GLIBC symbol compatibility error as requested.");
        return Ok(());
    }

    Err(format!(
        "Flatpak payload contains libraries requiring GLIBC newer than {LINUX_FLATPAK_RUNTIME}//{LINUX_FLATPAK_RUNTIME_VERSION} (maximum supported symbol: {}):\n       {}\n       Build the Linux payload with a distro/SDK whose GLIBC is compatible with the target Flatpak runtime.",
        policy.max_supported,
        offenders.join("\n       ")
    )
    .into())
}

fn collect_flatpak_glibc_symbol_offenders(
    path: &Path,
    policy: FlatpakGlibcPolicy,
    offenders: &mut Vec<String>,
) -> Result<()> {
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_flatpak_glibc_symbol_offenders(&entry?.path(), policy, offenders)?;
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
    let newer_symbols = glibc_symbols_newer_than(&stdout, policy);
    if !newer_symbols.is_empty() {
        let symbols = newer_symbols
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        offenders.push(format!("{} requires {symbols}", path.display()));
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
    let desktop_source = root.join(format!("assets/{LINUX_DESKTOP_ID}.desktop"));
    let app_resources = appdir.join(format!("usr/lib/{APP_ID}/resources"));
    let app_binaries = appdir.join(format!("usr/lib/{APP_ID}/binaries"));
    let app_lib = appdir.join("usr/lib");
    let desktop_file = appdir.join(format!("{LINUX_DESKTOP_ID}.desktop"));

    required_executable_file(&runtime, "cargo xtask setup-linux")?;
    required_file(&server, "cargo xtask setup-linux")?;
    required_executable_file(&ffmpeg, "cargo xtask setup-linux")?;
    required_executable_file(&ffprobe, "cargo xtask setup-linux")?;
    required_file(&icon, "restore assets/icons/128x128.png")?;
    required_file(&desktop_source, "restore the Linux desktop asset")?;

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
    copy_file(&icon, appdir.join(format!("{LINUX_DESKTOP_ID}.png")))?;
    copy_file(
        appdir.join(format!("{LINUX_DESKTOP_ID}.png")),
        appdir.join(format!(
            "usr/share/icons/hicolor/128x128/apps/{LINUX_DESKTOP_ID}.png"
        )),
    )?;

    let desktop_entry = fs::read_to_string(&desktop_source)?;
    let installed_exec = format!("Exec={APP_ID}");
    let appimage_exec = format!("Exec={LINUX_BIN}");
    if !desktop_entry.lines().any(|line| line == installed_exec) {
        return Err(format!("{} must contain {installed_exec}", desktop_source.display()).into());
    }
    write_file(
        &desktop_file,
        desktop_entry.replacen(&installed_exec, &appimage_exec, 1),
    )?;
    copy_file(
        &desktop_file,
        appdir.join(format!("usr/share/applications/{LINUX_DESKTOP_ID}.desktop")),
    )?;
    write_file(appdir.join("AppRun"), linux_appimage_launcher_script())?;

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

    bundle_webkitgtk_helpers(&appdir)?;

    patch_absolute_needed_paths(&app_lib)?;
    patch_absolute_needed_paths(&appdir.join(format!("usr/bin/{LINUX_BIN}")))?;

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

        if let Some(path) = resolved_ldd_path(line).filter(|p| should_bundle_linux_library(p)) {
            libs.insert(path);
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

fn bundle_webkitgtk_helpers(appdir: &Path) -> Result<()> {
    let host_paths = [
        "/usr/lib/x86_64-linux-gnu/webkitgtk-6.0",
        "/usr/lib/webkitgtk-6.0",
        "/usr/lib/webkit2gtk-6.0",
    ];

    let mut helper_dir = None;
    for path in &host_paths {
        let p = Path::new(path);
        if p.join("WebKitNetworkProcess").is_file() {
            helper_dir = Some(p);
            break;
        }
    }

    let Some(host_helper_dir) = helper_dir else {
        println!(
            "WARNING: WebKitNetworkProcess not found on host. WebKitGTK helper bundling skipped."
        );
        return Ok(());
    };

    println!(
        "==> Bundling WebKitGTK helper processes from {}...",
        host_helper_dir.display()
    );
    let dest_dir = appdir.join("usr/lib/webkitgtk-6.0");
    fs::create_dir_all(&dest_dir)?;

    for name in [
        "WebKitNetworkProcess",
        "WebKitWebProcess",
        "WebKitGPUProcess",
    ] {
        let source = host_helper_dir.join(name);
        if source.is_file() {
            let destination = dest_dir.join(name);
            copy_file(&source, &destination)?;
            chmod_executable(&destination)?;
        }
    }

    let source_injected = host_helper_dir.join("injected-bundle");
    if source_injected.is_dir() {
        copy_dir_recursive(&source_injected, dest_dir.join("injected-bundle"))?;
    }

    Ok(())
}

fn patch_absolute_needed_paths(path: &Path) -> Result<()> {
    if !program_exists("patchelf") {
        return Ok(());
    }

    if path.is_file() {
        return patch_file_absolute_needed_paths(path);
    }

    if path.is_dir() {
        println!(
            "==> Patching absolute DT_NEEDED paths with patchelf in {}...",
            path.display()
        );
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let subpath = entry.path();
            if subpath.is_file() {
                let filename = subpath.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if filename.contains(".so") {
                    patch_file_absolute_needed_paths(&subpath)?;
                }
            }
        }
    }

    Ok(())
}

fn patch_file_absolute_needed_paths(file_path: &Path) -> Result<()> {
    let output = Command::new("patchelf")
        .arg("--print-needed")
        .arg(file_path)
        .output()?;
    if !output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(filename) = absolute_needed_filename(line) {
            println!(
                "    Fixing absolute dependency in {}: {} -> {}",
                file_path.display(),
                line,
                filename
            );
            let status = Command::new("patchelf")
                .arg("--replace-needed")
                .arg(line)
                .arg(filename)
                .arg(file_path)
                .status()?;
            if !status.success() {
                return Err(format!(
                    "patchelf failed to replace required absolute dependency in {}: {} -> {} (status: {status})",
                    file_path.display(),
                    line,
                    filename
                )
                .into());
            }
        }
    }

    Ok(())
}

fn absolute_needed_filename(needed: &str) -> Option<&str> {
    if !needed.starts_with('/') {
        return None;
    }

    Path::new(needed).file_name().and_then(|name| name.to_str())
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

    !LINUX_SYSTEM_LIBRARY_DENYLIST.contains(&name)
}

const LINUX_SYSTEM_LIBRARY_DENYLIST: &[&str] = &[
    "ld-linux-x86-64.so.2",
    "libc.so.6",
    "libdl.so.2",
    "libgcc_s.so.1",
    "libm.so.6",
    "libpthread.so.0",
    "libresolv.so.2",
    "librt.so.1",
    "libstdc++.so.6",
];

fn appimage_tool_path() -> Result<PathBuf> {
    if let Some(path) = env::var_os("APPIMAGE_TOOL") {
        return Ok(PathBuf::from(path));
    }

    let home = env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .ok_or("HOME/USERPROFILE is not set; set APPIMAGE_TOOL explicitly")?;
    Ok(PathBuf::from(home).join(".cache/appimage/appimagetool-x86_64.AppImage"))
}

pub fn package_linux_flatpak_builder() -> Result<()> {
    if env::consts::OS != "linux" {
        return Err("cargo xtask package-linux-flatpak-builder must be run on Linux".into());
    }
    if !program_exists("flatpak-builder") {
        return Err("missing flatpak-builder. Install flatpak-builder, then retry.".into());
    }

    let root = root();
    let dist_dir = root.join("dist");
    let output = dist_dir.join(LINUX_FLATPAK);
    let build_dir = root.join("target/flatpak-builder-build");
    let repo_dir = root.join("target/flatpak-builder-repo");
    let manifest = root.join("flatpak/io.github.theguy000.StremioLightning.json");

    println!("==> Cleaning previous Flatpak Builder directories...");
    remove_dir_if_exists(&build_dir)?;
    remove_dir_if_exists(&repo_dir)?;
    fs::create_dir_all(&dist_dir)?;

    println!("==> Running flatpak-builder...");
    run_program(
        "flatpak-builder",
        [
            "--force-clean",
            "--ccache",
            "--disable-rofiles-fuse",
            &format!("--repo={}", repo_dir.display()),
            &build_dir.to_string_lossy(),
            &manifest.to_string_lossy(),
        ],
    )?;

    validate_flatpak_glibc_symbols(&build_dir)?;

    println!("==> Exporting Flatpak bundle...");
    remove_file_if_exists(&output)?;
    run_program(
        "flatpak",
        [
            OsString::from("build-bundle"),
            repo_dir.as_os_str().to_os_string(),
            output.as_os_str().to_os_string(),
            OsString::from(LINUX_FLATPAK_ID),
        ],
    )?;

    println!(
        "==> Hermetic Flatpak ready: {}",
        output.strip_prefix(&root).unwrap_or(&output).display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_ldd_path_handles_common_formats() {
        assert_eq!(
            resolved_ldd_path("libgtk-4.so.1 => /lib/x86_64-linux-gnu/libgtk-4.so.1 (0x00007f00)"),
            Some(PathBuf::from("/lib/x86_64-linux-gnu/libgtk-4.so.1"))
        );
        assert_eq!(
            resolved_ldd_path("/lib64/ld-linux-x86-64.so.2 (0x00007f00)"),
            Some(PathBuf::from("/lib64/ld-linux-x86-64.so.2"))
        );
        assert_eq!(resolved_ldd_path("libmissing.so => not found"), None);
        assert_eq!(resolved_ldd_path("linux-vdso.so.1 (0x00007fff)"), None);
    }

    #[test]
    fn should_bundle_linux_library_excludes_system_runtime_libraries() {
        for name in LINUX_SYSTEM_LIBRARY_DENYLIST {
            assert!(!should_bundle_linux_library(&PathBuf::from(format!(
                "/lib/x86_64-linux-gnu/{name}"
            ))));
        }

        assert!(should_bundle_linux_library(Path::new(
            "/lib/x86_64-linux-gnu/libwebkitgtk-6.0.so.4"
        )));
        assert!(should_bundle_linux_library(Path::new(
            "/usr/lib/x86_64-linux-gnu/libmpv.so.2"
        )));
    }

    #[test]
    fn current_flatpak_runtime_has_glibc_policy() {
        let policy = flatpak_glibc_policy(LINUX_FLATPAK_RUNTIME, LINUX_FLATPAK_RUNTIME_VERSION)
            .expect("current Flatpak runtime must have a GLIBC policy");

        assert_eq!(policy.max_supported, GlibcVersion::new(2, 42, 0));
    }

    #[test]
    fn glibc_policy_reports_symbols_newer_than_runtime_limit() {
        let policy = FlatpakGlibcPolicy {
            max_supported: GlibcVersion::new(2, 42, 0),
        };
        let symbols = glibc_symbols_newer_than(
            "Name: GLIBC_2.34\n0x00 0x00 4 (GLIBC_2.43)\nName: GLIBC_2.2.5\nName: GLIBC_2.44\n",
            policy,
        );

        assert_eq!(
            symbols,
            vec![GlibcVersion::new(2, 43, 0), GlibcVersion::new(2, 44, 0)]
        );
    }

    #[test]
    fn launcher_scripts_include_required_environment() {
        let appimage = linux_appimage_launcher_script();
        assert!(appimage.contains("set -euo pipefail"));
        assert!(appimage.contains("WEBKIT_EXEC_PATH"));
        assert!(appimage.contains(LINUX_BIN));

        let flatpak = linux_flatpak_launcher_script();
        assert!(flatpak.contains("STREMIO_LIGHTNING_BUNDLE_DIR=\"/app/lib/stremio-lightning\""));
        assert!(flatpak.contains("${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"));
        assert!(!flatpak.contains("$LD_LIBRARY_PATH}}"));

        let deb = linux_deb_launcher_script();
        assert!(deb.contains("/usr/lib/stremio-lightning/lib"));
        assert!(deb.contains("exec \"/usr/lib/stremio-lightning/stremio-lightning-linux\""));
    }

    #[test]
    fn absolute_needed_filename_only_rewrites_absolute_paths() {
        assert_eq!(
            absolute_needed_filename("/usr/lib/x86_64-linux-gnu/libfoo.so.1"),
            Some("libfoo.so.1")
        );
        assert_eq!(absolute_needed_filename("libfoo.so.1"), None);
        assert_eq!(absolute_needed_filename("/"), None);
    }

    #[test]
    fn flatpak_builder_manifest_matches_xtask_constants_and_assets() {
        let manifest = std::fs::read_to_string(
            root().join("flatpak/io.github.theguy000.StremioLightning.json"),
        )
        .expect("Flatpak Builder manifest should be readable");

        assert!(manifest.contains(&format!("\"runtime\": \"{LINUX_FLATPAK_RUNTIME}\"")));
        assert!(manifest.contains(&format!(
            "\"runtime-version\": \"{LINUX_FLATPAK_RUNTIME_VERSION}\""
        )));
        assert!(manifest.contains(&format!("\"sdk\": \"{LINUX_FLATPAK_SDK}\"")));
        assert!(manifest.contains(&format!(
            "install -Dm644 assets/{LINUX_FLATPAK_ID}.desktop /app/share/applications/{LINUX_FLATPAK_ID}.desktop"
        )));
        assert!(manifest.contains(&format!(
            "install -Dm644 assets/{LINUX_FLATPAK_ID}.metainfo.xml /app/share/metainfo/{LINUX_FLATPAK_ID}.metainfo.xml"
        )));
        assert!(!manifest.contains("LD_LIBRARY_PATH}}"));
    }
}
