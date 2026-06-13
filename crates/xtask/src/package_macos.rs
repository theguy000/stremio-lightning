use crate::common::{
    Result, chmod_executable, copy_file, remove_dir_if_exists, remove_file_if_exists,
    required_executable_file, required_file, root, run_program,
};
use crate::{APP_NAME, MACOS_APP_BUNDLE, MACOS_BIN};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::process::Command;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacosArch {
    Arm64,
    X86_64,
}

impl MacosArch {
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "arm64" | "aarch64" => Ok(Self::Arm64),
            "x86_64" | "x64" | "amd64" | "intel" => Ok(Self::X86_64),
            other => {
                Err(format!("unsupported macOS architecture '{other}'. Use arm64 or x86_64").into())
            }
        }
    }

    pub fn host() -> Result<Self> {
        Self::parse(env::consts::ARCH)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::X86_64 => "x86_64",
        }
    }

    pub fn rust_target(self) -> &'static str {
        match self {
            Self::Arm64 => "aarch64-apple-darwin",
            Self::X86_64 => "x86_64-apple-darwin",
        }
    }

    pub fn homebrew_prefix(self) -> &'static str {
        match self {
            Self::Arm64 => "/opt/homebrew",
            Self::X86_64 => "/usr/local",
        }
    }

    pub fn dmg_file_name(self) -> String {
        format!("Stremio_Lightning_macOS-{}.dmg", self.name())
    }
}

pub fn package_macos(arch: MacosArch) -> Result<()> {
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
        "run: cargo xtask setup-macos",
    )?;
    for name in ["server.cjs", "ffmpeg", "ffprobe"] {
        required_file(
            &macos_dir.join(format!("resources/{name}")),
            "run: cargo xtask setup-macos",
        )?;
    }
    let mpv_library = macos_mpv_library(arch)?;

    println!("==> Building native macOS shell crate ({})...", arch.name());
    run_program(
        "cargo",
        [
            "build",
            "-p",
            MACOS_BIN,
            "--release",
            "--target",
            arch.rust_target(),
        ],
    )?;

    remove_dir_if_exists(&bundle)?;
    fs::create_dir_all(&executable_dir)?;
    fs::create_dir_all(&resources_dir)?;
    fs::create_dir_all(&frameworks_dir)?;
    fs::create_dir_all(&dist_dir)?;

    copy_file(
        root.join(format!("target/{}/release/{MACOS_BIN}", arch.rust_target())),
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

    println!("==> Bundling libmpv and its dependencies into Contents/Frameworks...");
    let bundled_dylibs = bundle_dylibs(&executable, &mpv_library, &frameworks_dir)?;

    println!("==> Rewriting macOS bundle rpaths...");
    run_install_name_tool([
        "-add_rpath".to_string(),
        "@executable_path/../Frameworks".to_string(),
        executable.to_string_lossy().into_owned(),
    ])?;

    verify_bundle(arch, &bundle, &executable, &resources_dir, &bundled_dylibs)?;

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

    println!(
        "==> macOS app bundle ready ({}): {}",
        arch.name(),
        bundle.display()
    );
    Ok(())
}

pub fn package_macos_dmg(arch: MacosArch) -> Result<()> {
    package_macos(arch)?;

    let root = root();
    let dist_dir = root.join("dist");
    let bundle = dist_dir.join(MACOS_APP_BUNDLE);
    let staging = dist_dir.join(format!("dmg-staging-{}", arch.name()));
    let dmg_path = dist_dir.join(arch.dmg_file_name());

    remove_dir_if_exists(&staging)?;
    remove_file_if_exists(&dmg_path)?;
    fs::create_dir_all(&staging)?;

    // ditto preserves code signatures, permissions and extended attributes.
    run_program(
        "ditto",
        [
            bundle.clone().into_os_string(),
            staging.join(MACOS_APP_BUNDLE).into_os_string(),
        ],
    )?;
    run_program(
        "ln",
        [
            OsString::from("-s"),
            OsString::from("/Applications"),
            staging.join("Applications").into_os_string(),
        ],
    )?;

    println!("==> Creating DMG {}...", dmg_path.display());
    run_program(
        "hdiutil",
        [
            OsString::from("create"),
            OsString::from("-volname"),
            OsString::from(APP_NAME),
            OsString::from("-srcfolder"),
            staging.clone().into_os_string(),
            OsString::from("-ov"),
            OsString::from("-format"),
            OsString::from("UDZO"),
            dmg_path.clone().into_os_string(),
        ],
    )?;

    remove_dir_if_exists(&staging)?;
    required_file(&dmg_path, "run: cargo xtask package-macos-dmg")?;

    println!(
        "==> macOS DMG ready ({}): {}",
        arch.name(),
        dmg_path.display()
    );
    Ok(())
}

/// Recursively collects libmpv plus every non-system dylib it (or the app
/// executable) depends on, copies them into Contents/Frameworks, gives each a
/// stable @rpath id, and rewrites all references so the bundle is
/// self-contained.
fn bundle_dylibs(
    executable: &Path,
    mpv_library: &Path,
    frameworks_dir: &Path,
) -> Result<Vec<PathBuf>> {
    let mut bundled: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut queue: Vec<PathBuf> = vec![mpv_library.to_path_buf()];
    for dependency in dylib_dependencies(executable)? {
        if let Some(resolved) = resolve_bundlable_dependency(&dependency, executable) {
            queue.push(resolved);
        }
    }

    while let Some(source) = queue.pop() {
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("dylib path has no file name: {}", source.display()))?
            .to_string();
        if bundled.contains_key(&name) {
            continue;
        }
        let dependencies = dylib_dependencies(&source)?;
        for dependency in dependencies {
            if let Some(resolved) = resolve_bundlable_dependency(&dependency, &source) {
                queue.push(resolved);
            }
        }
        bundled.insert(name, source);
    }

    let mut destinations = Vec::new();
    for (name, source) in &bundled {
        let destination = frameworks_dir.join(name);
        copy_file(source, &destination)?;
        chmod_executable(&destination)?;
        run_install_name_tool([
            "-id".to_string(),
            format!("@rpath/{name}"),
            destination.to_string_lossy().into_owned(),
        ])?;
        destinations.push(destination);
    }

    let mut binaries: Vec<PathBuf> = vec![executable.to_path_buf()];
    binaries.extend(destinations.iter().cloned());
    for binary in &binaries {
        for dependency in dylib_dependencies(binary)? {
            let Some(name) = dependency_file_name(&dependency) else {
                continue;
            };
            if !bundled.contains_key(&name) {
                continue;
            }
            let replacement = format!("@rpath/{name}");
            if dependency == replacement {
                continue;
            }
            run_install_name_tool([
                "-change".to_string(),
                dependency.clone(),
                replacement,
                binary.to_string_lossy().into_owned(),
            ])?;
        }
    }

    Ok(destinations)
}

fn verify_bundle(
    arch: MacosArch,
    bundle: &Path,
    executable: &Path,
    resources_dir: &Path,
    bundled_dylibs: &[PathBuf],
) -> Result<()> {
    println!("==> Verifying macOS app bundle...");
    required_executable_file(executable, "run: cargo xtask package-macos")?;
    required_file(
        &bundle.join("Contents/Info.plist"),
        "run: cargo xtask package-macos",
    )?;
    required_file(
        &resources_dir.join("entitlements.plist"),
        "run: cargo xtask package-macos",
    )?;
    required_executable_file(
        resources_dir.join("binaries/stremio-runtime-macos"),
        "run: cargo xtask setup-macos",
    )?;
    for name in ["server.cjs", "ffmpeg", "ffprobe"] {
        required_file(
            &resources_dir.join(format!("resources/{name}")),
            "run: cargo xtask setup-macos",
        )?;
    }

    if bundled_dylibs.is_empty() {
        return Err(
            "no dylibs were bundled into Contents/Frameworks; expected at least libmpv".into(),
        );
    }
    let has_mpv = bundled_dylibs.iter().any(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("libmpv"))
    });
    if !has_mpv {
        return Err("Contents/Frameworks is missing libmpv".into());
    }

    verify_macho_arch(arch, executable)?;
    verify_macho_arch(arch, &resources_dir.join("binaries/stremio-runtime-macos"))?;
    verify_macho_arch(arch, &resources_dir.join("resources/ffmpeg"))?;
    verify_macho_arch(arch, &resources_dir.join("resources/ffprobe"))?;
    for dylib in bundled_dylibs {
        verify_macho_arch(arch, dylib)?;
    }
    Ok(())
}

fn verify_macho_arch(arch: MacosArch, binary: &Path) -> Result<()> {
    let output = capture_stdout(
        "lipo",
        &["-archs".to_string(), binary.to_string_lossy().into_owned()],
    )?;
    if output
        .split_whitespace()
        .any(|candidate| candidate == arch.name())
    {
        return Ok(());
    }
    Err(format!(
        "{} is built for '{}', expected {}",
        binary.display(),
        output.trim(),
        arch.name()
    )
    .into())
}

fn dylib_dependencies(binary: &Path) -> Result<Vec<String>> {
    let output = capture_stdout(
        "otool",
        &["-L".to_string(), binary.to_string_lossy().into_owned()],
    )?;
    let mut dependencies = Vec::new();
    for line in output.lines().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let path = trimmed
            .split(" (compatibility")
            .next()
            .unwrap_or(trimmed)
            .trim();
        if path.is_empty() {
            continue;
        }
        dependencies.push(path.to_string());
    }
    Ok(dependencies)
}

fn resolve_bundlable_dependency(dependency: &str, referrer: &Path) -> Option<PathBuf> {
    if dependency.starts_with("/usr/lib/") || dependency.starts_with("/System/") {
        return None;
    }
    let referrer_dir = referrer.parent()?;
    let path = if let Some(rest) = dependency.strip_prefix("@loader_path/") {
        referrer_dir.join(rest)
    } else if let Some(rest) = dependency.strip_prefix("@rpath/") {
        // Heuristic: Homebrew keeps dependent dylibs flat in the same lib dir.
        referrer_dir.join(rest)
    } else if dependency.starts_with('@') {
        return None;
    } else {
        PathBuf::from(dependency)
    };
    let resolved = path.canonicalize().ok()?;
    if resolved.starts_with("/usr/lib") || resolved.starts_with("/System") {
        return None;
    }
    if !resolved.is_file() {
        return None;
    }
    Some(resolved)
}

fn dependency_file_name(dependency: &str) -> Option<String> {
    dependency
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
}

fn capture_stdout(program: &str, args: &[String]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root())
        .output()
        .map_err(|error| format!("failed to start {program}: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{program} {} failed with status {}: {}",
            args.join(" "),
            output.status,
            stderr.trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn macos_mpv_library(arch: MacosArch) -> Result<PathBuf> {
    let mut roots: Vec<PathBuf> = ["MPV_DIR", "STREMIO_LIGHTNING_MPV_DIR"]
        .into_iter()
        .filter_map(env::var_os)
        .map(PathBuf::from)
        .collect();
    let root = root();
    roots.extend([
        root.join("crates/stremio-lightning-macos/mpv-dev"),
        PathBuf::from(arch.homebrew_prefix()).join("opt/mpv"),
    ]);

    for candidate in roots {
        for name in ["libmpv.dylib", "libmpv.2.dylib"] {
            let path = candidate.join("lib").join(name);
            if path.is_file() {
                return Ok(path);
            }
        }
    }

    Err(format!(
        "missing libmpv.dylib for the {} macOS bundle. Set MPV_DIR or STREMIO_LIGHTNING_MPV_DIR to an mpv prefix, or install mpv with Homebrew under {}",
        arch.name(),
        arch.homebrew_prefix()
    )
    .into())
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
