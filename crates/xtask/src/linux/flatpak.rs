use crate::common::{
    Result, chmod_executable, copy_dir_recursive, copy_file, program_exists, remove_dir_if_exists,
    remove_file_if_exists, root, run_program, write_file,
};
use crate::{
    LINUX_DESKTOP_ID, LINUX_FLATPAK, LINUX_FLATPAK_ID, LINUX_FLATPAK_RUNTIME,
    LINUX_FLATPAK_RUNTIME_VERSION, LINUX_FLATPAK_SDK, NATIVE_BIN, PACKAGE_ID,
};
use std::{env, ffi::OsString, fs, path::Path};

use super::glibc::validate_flatpak_glibc_symbols;
use super::stage::prepare_linux_appdir;

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
    let manifest = root.join(format!("flatpak/{LINUX_FLATPAK_ID}.json"));

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

fn prepare_linux_flatpak_payload(appdir: &Path, payload_dir: &Path) -> Result<()> {
    let root = root();
    let files_dir = payload_dir.join("files");
    let bin_dir = files_dir.join("bin");
    let install_bin = files_dir.join(format!("lib/{PACKAGE_ID}/{NATIVE_BIN}"));
    let applications_dir = files_dir.join("share/applications");
    let icons_dir = files_dir.join("share/icons/hicolor/128x128/apps");
    let metainfo_dir = files_dir.join("share/metainfo");

    fs::create_dir_all(&bin_dir)?;
    fs::create_dir_all(&applications_dir)?;
    fs::create_dir_all(&icons_dir)?;
    fs::create_dir_all(&metainfo_dir)?;

    copy_dir_recursive(appdir.join("usr/lib"), files_dir.join("lib"))?;
    copy_file(appdir.join(format!("usr/bin/{NATIVE_BIN}")), &install_bin)?;
    write_file(bin_dir.join(PACKAGE_ID), linux_flatpak_launcher_script())?;
    chmod_executable(bin_dir.join(PACKAGE_ID))?;
    chmod_executable(&install_bin)?;
    for bin in ["stream-server", "ffmpeg", "ffprobe"] {
        chmod_executable(files_dir.join(format!("lib/{PACKAGE_ID}/resources/{bin}")))?;
    }

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
        "[Application]\nname={LINUX_FLATPAK_ID}\nruntime={LINUX_FLATPAK_RUNTIME}/x86_64/{LINUX_FLATPAK_RUNTIME_VERSION}\nsdk={LINUX_FLATPAK_SDK}/x86_64/{LINUX_FLATPAK_RUNTIME_VERSION}\ncommand={PACKAGE_ID}\n\n[Context]\nshared=ipc;network;\nsockets=x11;pulseaudio;\ndevices=dri;\n\n[Session Bus Policy]\norg.freedesktop.Notifications=talk\n{LINUX_FLATPAK_ID}=own\n"
    )
}

fn linux_flatpak_launcher_script() -> String {
    format!(
        "#!/bin/sh\nset -eu\nexport LD_LIBRARY_PATH=\"/app/lib${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\nexport STREMIO_LIGHTNING_BUNDLE_DIR=\"/app/lib/{PACKAGE_ID}\"\nexec /app/lib/{PACKAGE_ID}/{NATIVE_BIN} \"$@\"\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatpak_launcher_includes_required_environment() {
        let flatpak = linux_flatpak_launcher_script();
        assert!(flatpak.contains("STREMIO_LIGHTNING_BUNDLE_DIR=\"/app/lib/stremio-lightning\""));
        assert!(flatpak.contains("${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"));
        assert!(!flatpak.contains("$LD_LIBRARY_PATH}}"));
        assert!(!flatpak.contains("WEBKIT_"));
        assert!(flatpak.contains("exec /app/lib/stremio-lightning/stremio-lightning \"$@\""));
    }

    #[test]
    fn flatpak_builder_manifest_matches_xtask_constants_and_assets() {
        let manifest =
            std::fs::read_to_string(root().join(format!("flatpak/{LINUX_FLATPAK_ID}.json")))
                .expect("Flatpak Builder manifest should be readable");

        assert!(manifest.contains(&format!("\"runtime\": \"{LINUX_FLATPAK_RUNTIME}\"")));
        assert!(manifest.contains(&format!(
            "\"runtime-version\": \"{LINUX_FLATPAK_RUNTIME_VERSION}\""
        )));
        assert!(manifest.contains(&format!("\"sdk\": \"{LINUX_FLATPAK_SDK}\"")));
        assert!(!manifest.contains("node22"));
        assert!(manifest.contains("cargo --offline build --release -p stremio-lightning"));
        assert!(manifest.contains(
            "install -Dm755 target/release/stremio-lightning /app/lib/stremio-lightning/stremio-lightning"
        ));
        assert!(manifest.contains(r#"exec /app/lib/stremio-lightning/stremio-lightning \"$@\""#));
        assert!(!manifest.contains("stremio-lightning-linux"));
        assert!(manifest.contains("\"cargo-sources.json\""));
        assert!(!manifest.contains("\"build-args\""));
        assert!(manifest.contains(&format!(
            "install -Dm644 assets/{LINUX_FLATPAK_ID}.desktop /app/share/applications/{LINUX_FLATPAK_ID}.desktop"
        )));
        assert!(manifest.contains(&format!(
            "install -Dm644 assets/{LINUX_FLATPAK_ID}.metainfo.xml /app/share/metainfo/{LINUX_FLATPAK_ID}.metainfo.xml"
        )));
        assert!(manifest.contains(
            "install -Dm755 stream-server-linux-amd64-headless /app/lib/stremio-lightning/resources/stream-server"
        ));
        assert!(manifest.contains("ln -s /usr/bin/ffmpeg"));
        assert!(manifest.contains("ln -s /usr/bin/ffprobe"));
        assert!(manifest.contains("mkdir -p /app/lib/codecs-extra"));
        assert!(manifest.contains("mkdir -p /app/lib/intel-vaapi-driver"));
        assert!(!manifest.contains("install -Dm755 crates/stremio-lightning/resources/ffmpeg"));
        assert!(!manifest.contains("install -Dm755 crates/stremio-lightning/resources/ffprobe"));
        assert!(!manifest.contains("LD_LIBRARY_PATH}}"));
    }
}
