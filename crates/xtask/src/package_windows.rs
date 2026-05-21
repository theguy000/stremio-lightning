use crate::common::{
    Result, copy_file, inno_path, package_version, program_exists, remove_dir_if_exists,
    remove_file_if_exists, required_file, root, run_program, run_program_in, write_file,
};
use crate::{APP_ID, APP_NAME, WINDOWS_BIN, WINDOWS_INSTALLER, WINDOWS_TARGET, WINDOWS_ZIP};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn package_windows() -> Result<()> {
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

pub fn package_windows_installer() -> Result<()> {
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
