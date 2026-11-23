use crate::common::{
    Result, chmod_executable, copy_dir_recursive, copy_file, is_executable_file, package_version,
    remove_dir_if_exists, remove_file_if_exists, require_program, root, run_command, run_program,
    write_file,
};
use crate::{LINUX_APPIMAGE, LINUX_DEB, LINUX_DESKTOP_ID, NATIVE_BIN, PACKAGE_ID};
use std::{env, ffi::OsStr, fs, path::PathBuf, process::Command};

use super::stage::prepare_linux_appdir;

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
    require_program(
        "dpkg-deb",
        "install dpkg tooling (`dpkg-deb`), then rerun: cargo xtask package-linux-deb",
    )?;

    let root = root();
    let dist_dir = root.join("dist");
    let output = dist_dir.join(LINUX_DEB);
    let appdir = prepare_linux_appdir()?;
    let deb_root = root.join("target/deb/stremio-lightning");
    let debian_dir = deb_root.join("DEBIAN");
    let install_root = deb_root.join(format!("usr/lib/{PACKAGE_ID}"));
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
        appdir.join(format!("usr/bin/{NATIVE_BIN}")),
        install_root.join(NATIVE_BIN),
    )?;
    copy_dir_recursive(
        appdir.join(format!("usr/lib/{PACKAGE_ID}/resources")),
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
        if entry.file_name() == OsStr::new(PACKAGE_ID) {
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
        deb_root.join(format!("usr/bin/{PACKAGE_ID}")),
        linux_deb_launcher_script(),
    )?;
    chmod_executable(deb_root.join(format!("usr/bin/{PACKAGE_ID}")))?;
    chmod_executable(install_root.join(NATIVE_BIN))?;
    for bin in ["stream-server", "ffmpeg", "ffprobe"] {
        chmod_executable(install_root.join(format!("resources/{bin}")))?;
    }

    copy_file(
        root.join(format!("assets/{LINUX_DESKTOP_ID}.desktop")),
        deb_root.join(format!("usr/share/applications/{LINUX_DESKTOP_ID}.desktop")),
    )?;
    write_file(
        debian_dir.join("control"),
        format!(
            "Package: {PACKAGE_ID}\nVersion: {}\nSection: video\nPriority: optional\nArchitecture: amd64\nRecommends: ca-certificates\nMaintainer: Stremio Lightning Maintainers <noreply@example.com>\nDescription: Lightweight native Stremio client\n Stremio Lightning packages a native Linux client with bundled runtime resources.\n",
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

pub(super) fn linux_appimage_launcher_script() -> String {
    format!(
        "#!/bin/bash\nset -euo pipefail\nHERE=$(dirname \"$(readlink -f \"$0\")\")\nexport LD_LIBRARY_PATH=\"$HERE/usr/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport STREMIO_LIGHTNING_BUNDLE_DIR=\"$HERE/usr/lib/{PACKAGE_ID}\"\nexec \"$HERE/usr/bin/{NATIVE_BIN}\" \"$@\"\n"
    )
}

fn linux_deb_launcher_script() -> String {
    format!(
        "#!/bin/sh\nset -eu\nexport LD_LIBRARY_PATH=\"/usr/lib/{PACKAGE_ID}/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport STREMIO_LIGHTNING_BUNDLE_DIR=\"/usr/lib/{PACKAGE_ID}\"\nexec \"/usr/lib/{PACKAGE_ID}/{NATIVE_BIN}\" \"$@\"\n"
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appimage_and_deb_launchers_include_required_environment() {
        let appimage = linux_appimage_launcher_script();
        assert!(appimage.contains("set -euo pipefail"));
        assert!(
            appimage.contains("STREMIO_LIGHTNING_BUNDLE_DIR=\"$HERE/usr/lib/stremio-lightning\"")
        );
        assert!(!appimage.contains("WEBKIT_"));
        assert!(appimage.contains("exec \"$HERE/usr/bin/stremio-lightning\""));

        let deb = linux_deb_launcher_script();
        assert!(deb.contains("/usr/lib/stremio-lightning/lib"));
        assert!(deb.contains("exec \"/usr/lib/stremio-lightning/stremio-lightning\""));
    }
}
