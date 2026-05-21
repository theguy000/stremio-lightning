use crate::common::{
    Result, chmod_executable, copy_file, required_executable_file, required_file, root, run_program,
};
use crate::{MACOS_APP_BUNDLE, MACOS_BIN};
use std::{env, fs, path::PathBuf};

pub fn package_macos() -> Result<()> {
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

    crate::common::remove_dir_if_exists(&bundle)?;
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
