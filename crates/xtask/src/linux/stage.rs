use crate::common::{
    Result, chmod_executable, copy_file, program_exists, remove_dir_if_exists,
    required_executable_file, required_file, root, run_program, stage_runtime_bins, write_file,
};
use crate::{LINUX_DESKTOP_ID, NATIVE_BIN, PACKAGE_ID};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use super::appimage_deb::linux_appimage_launcher_script;

pub(super) const LINUX_SYSTEM_LIBRARY_DENYLIST: &[&str] = &[
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

pub(super) fn prepare_linux_appdir() -> Result<PathBuf> {
    let root = root();
    let native_dir = root.join("crates/stremio-lightning");
    let appdir = root.join(format!("target/appimage/{PACKAGE_ID}.AppDir"));
    let dist_dir = root.join("dist");
    let resources = native_dir.join("resources");
    let icon = root.join("assets/icons/128x128.png");
    let desktop_source = root.join(format!("assets/{LINUX_DESKTOP_ID}.desktop"));
    let app_resources = appdir.join(format!("usr/lib/{PACKAGE_ID}/resources"));
    let app_lib = appdir.join("usr/lib");
    let desktop_file = appdir.join(format!("{LINUX_DESKTOP_ID}.desktop"));

    for bin in ["stream-server", "ffmpeg", "ffprobe"] {
        required_executable_file(resources.join(bin), "cargo xtask setup-linux")?;
    }
    required_file(&icon, "restore assets/icons/128x128.png")?;
    required_file(&desktop_source, "restore the Linux desktop asset")?;

    println!("==> Building native Linux client...");
    run_program("cargo", ["build", "-p", NATIVE_BIN, "--release"])?;

    remove_dir_if_exists(&appdir)?;
    fs::create_dir_all(appdir.join("usr/bin"))?;
    fs::create_dir_all(&app_lib)?;
    fs::create_dir_all(appdir.join("usr/share/applications"))?;
    fs::create_dir_all(appdir.join("usr/share/icons/hicolor/128x128/apps"))?;
    fs::create_dir_all(&dist_dir)?;

    copy_file(
        root.join(format!("target/release/{NATIVE_BIN}")),
        appdir.join(format!("usr/bin/{NATIVE_BIN}")),
    )?;
    stage_runtime_bins(
        &resources,
        &app_resources,
        &["stream-server", "ffmpeg", "ffprobe"],
        true,
    )?;
    copy_file(&icon, appdir.join(format!("{LINUX_DESKTOP_ID}.png")))?;
    copy_file(
        appdir.join(format!("{LINUX_DESKTOP_ID}.png")),
        appdir.join(format!(
            "usr/share/icons/hicolor/128x128/apps/{LINUX_DESKTOP_ID}.png"
        )),
    )?;

    let desktop_entry = fs::read_to_string(&desktop_source)?;
    let installed_exec = format!("Exec={PACKAGE_ID} %U");
    let appimage_exec = format!("Exec={NATIVE_BIN} %U");
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
    chmod_executable(appdir.join(format!("usr/bin/{NATIVE_BIN}")))?;

    bundle_linux_shared_libraries(
        &appdir.join(format!("usr/bin/{NATIVE_BIN}")),
        &app_lib,
        "cargo xtask setup-linux",
    )?;
    // The streaming server links libssl and libcrypto, which not every distro ships.
    bundle_linux_shared_libraries(
        &app_resources.join("stream-server"),
        &app_lib,
        "cargo xtask setup-linux",
    )?;

    patch_absolute_needed_paths(&app_lib)?;
    patch_absolute_needed_paths(&appdir.join(format!("usr/bin/{NATIVE_BIN}")))?;

    required_executable_file(appdir.join("AppRun"), "cargo xtask setup-linux")?;
    required_executable_file(
        appdir.join(format!("usr/bin/{NATIVE_BIN}")),
        "cargo xtask setup-linux",
    )?;
    for bin in ["stream-server", "ffmpeg", "ffprobe"] {
        required_executable_file(app_resources.join(bin), "cargo xtask setup-linux")?;
    }

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
        if let Some(name) = lib.file_name() {
            copy_file(&lib, app_lib.join(name))?;
        }
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
            let subpath = entry?.path();
            if subpath.is_file()
                && subpath
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| name.contains(".so"))
            {
                patch_file_absolute_needed_paths(&subpath)?;
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
                .args(["--replace-needed", line, filename])
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
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| !LINUX_SYSTEM_LIBRARY_DENYLIST.contains(&name))
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
            "/usr/lib/x86_64-linux-gnu/libmpv.so.2"
        )));
        assert!(should_bundle_linux_library(Path::new(
            "/usr/lib/x86_64-linux-gnu/libssl.so.3"
        )));
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
}
