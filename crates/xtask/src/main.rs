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
const WINDOWS_BIN: &str = "stremio-lightning-windows";
const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";
const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";
const LINUX_APPIMAGE: &str = "Stremio_Lightning_Linux-x86_64.AppImage";
const WINDOWS_ZIP: &str = "stremio-lightning-windows-portable.zip";

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
        "build-linux-appimage" => build_linux_appimage()?,
        "package-windows" => package_windows()?,
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
  cargo xtask setup                  Download native shell dependencies for this OS\n\
  cargo xtask setup-linux            Download Linux shell dependencies\n\
  cargo xtask setup-windows          Download Windows shell dependencies\n\
  cargo xtask build-ui               Build the Svelte/Vite UI bundle\n\
  cargo xtask test-ui                Run frontend tests\n\
  cargo xtask build-linux-appimage   Build dist/{LINUX_APPIMAGE}\n\
  cargo xtask package-windows        Build and zip the Windows portable artifact\n"
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
        "bash",
        &[root().join("scripts/download-linux-shell-deps.sh")],
    )
}

fn setup_windows() -> Result<()> {
    run_program(
        "bash",
        &[root().join("scripts/download-windows-shell-deps.sh")],
    )
}

fn build_linux_appimage() -> Result<()> {
    let root = root();
    let linux_dir = root.join("crates/stremio-lightning-linux");
    let appdir = root.join(format!("target/appimage/{APP_ID}.AppDir"));
    let dist_dir = root.join("dist");
    let output = dist_dir.join(LINUX_APPIMAGE);
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
    command.env("ARCH", "x86_64").arg(&appdir).arg(&output);
    run_command(&mut command)?;
    chmod_executable(&output)?;

    println!(
        "==> AppImage ready: {}",
        output.strip_prefix(&root).unwrap_or(&output).display()
    );
    Ok(())
}

fn package_windows() -> Result<()> {
    let root = root();
    let windows_dir = root.join("crates/stremio-lightning-windows");
    let dist_dir = root.join("dist");
    let portable_dir = dist_dir.join("stremio-lightning-windows-portable");
    let zip_path = dist_dir.join(WINDOWS_ZIP);

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

    println!("==> Building native Windows shell crate...");
    build_windows_shell()?;

    remove_dir_if_exists(&portable_dir)?;
    fs::create_dir_all(&portable_dir)?;
    fs::create_dir_all(&dist_dir)?;

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
                "cargo xtask package-windows cross-builds the MSVC target with cargo-xwin off Windows.\n       Install it with: cargo install cargo-xwin"
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
    run_program("npm", args)
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
