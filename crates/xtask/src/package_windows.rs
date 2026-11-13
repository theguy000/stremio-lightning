use crate::common::{
    Result, copy_file, inno_path, package_version, program_exists, remove_dir_if_exists,
    remove_file_if_exists, required_file, root, run_program, run_program_in, write_file,
};
use crate::{
    APP_ID, APP_NAME, NATIVE_BIN, PACKAGE_ID, WINDOWS_INSTALLER, WINDOWS_TARGET, WINDOWS_ZIP,
};
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
    let native_dir = root.join("crates/stremio-lightning");
    let dist_dir = root.join("dist");
    let portable_dir = dist_dir.join("stremio-lightning-windows-portable");

    required_file(
        &native_dir.join("resources/stream-server.exe"),
        "cargo xtask setup-windows",
    )?;
    required_file(
        &native_dir.join("resources/ffmpeg.exe"),
        "cargo xtask setup-windows",
    )?;
    required_file(
        &native_dir.join("resources/ffprobe.exe"),
        "cargo xtask setup-windows",
    )?;
    required_file(
        &native_dir.join("resources/libmpv-2.dll"),
        "cargo xtask setup-windows",
    )?;
    required_file(
        &root.join("assets/icons/favicon.ico"),
        "restore assets/icons/favicon.ico for the Windows executable icon",
    )?;

    println!("==> Building native Windows client...");
    build_windows_client()?;

    remove_dir_if_exists(&portable_dir)?;
    fs::create_dir_all(&dist_dir)?;
    fs::create_dir_all(&portable_dir)?;

    copy_file(
        root.join(format!("target/{WINDOWS_TARGET}/release/{NATIVE_BIN}.exe")),
        portable_dir.join(format!("{NATIVE_BIN}.exe")),
    )?;

    // libmpv-2.dll goes flat beside the exe (Packaged layout expects base_dir/libmpv-2.dll)
    copy_file(
        native_dir.join("resources/libmpv-2.dll"),
        portable_dir.join("libmpv-2.dll"),
    )?;

    // Engine/runtime files go into a resources/ subdirectory
    // (Packaged layout expects base_dir/resources/stream-server.exe etc.)
    let portable_resources = portable_dir.join("resources");
    fs::create_dir_all(&portable_resources)?;

    for name in ["stream-server.exe", "ffmpeg.exe", "ffprobe.exe"] {
        copy_file(
            native_dir.join(format!("resources/{name}")),
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
    let icon = root.join("assets/icons/favicon.ico");

    required_file(
        &icon,
        "restore assets/icons/favicon.ico for the Windows installer icon",
    )?;

    remove_file_if_exists(&installer_output)?;
    write_file(
        &installer_script,
        windows_inno_script(&package_version()?, &icon, &dist_dir, &portable_dir),
    )?;

    run_program("iscc", [&installer_script])?;
    required_file(&installer_output, "cargo xtask package-windows-installer")?;
    println!(
        "==> Windows installer ready: {}",
        installer_output.display()
    );

    Ok(())
}

fn windows_inno_script(version: &str, icon: &Path, dist_dir: &Path, portable_dir: &Path) -> String {
    format!(
        r#"#define MyAppName "{APP_NAME}"
#define MyAppVersion "{version}"
#define MyAppPublisher "Stremio Lightning"
#define MyAppExeName "{NATIVE_BIN}.exe"
#define MyAppUserModelID "{APP_ID}"
#define MyAppIcon "{}"

[Setup]
AppId={PACKAGE_ID}
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
Name: "{{group}}\{{#MyAppName}}"; Filename: "{{app}}\{{#MyAppExeName}}"; IconFilename: "{{app}}\{{#MyAppExeName}}"; AppUserModelID: "{{#MyAppUserModelID}}"
Name: "{{autodesktop}}\{{#MyAppName}}"; Filename: "{{app}}\{{#MyAppExeName}}"; IconFilename: "{{app}}\{{#MyAppExeName}}"; AppUserModelID: "{{#MyAppUserModelID}}"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Classes\stremio"; ValueType: string; ValueName: ""; ValueData: "URL:Stremio Protocol"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\stremio"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\stremio\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{{app}}\{{#MyAppExeName}},0"
Root: HKCU; Subkey: "Software\Classes\stremio\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{{app}}\{{#MyAppExeName}}"" ""%1"""

[Run]
Filename: "{{app}}\{{#MyAppExeName}}"; Description: "Launch {{#MyAppName}}"; Flags: nowait postinstall skipifsilent
"#,
        inno_path(icon),
        inno_path(dist_dir),
        WINDOWS_INSTALLER.trim_end_matches(".exe"),
        inno_path(portable_dir)
    )
}

fn build_windows_client() -> Result<()> {
    if env::consts::OS == "windows" {
        run_program(
            "cargo",
            [
                "build",
                "-p",
                NATIVE_BIN,
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
                NATIVE_BIN,
                "--release",
                "--target",
                WINDOWS_TARGET,
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inno_script_targets_native_binary_and_shared_icon() {
        let script = windows_inno_script(
            "0.1.5",
            Path::new("assets/icons/favicon.ico"),
            Path::new("dist"),
            Path::new("dist/stremio-lightning-windows-portable"),
        );
        assert!(script.contains("#define MyAppExeName \"stremio-lightning.exe\""));
        assert!(script.contains("#define MyAppIcon \"assets/icons/favicon.ico\""));
        assert!(script.contains("#define MyAppVersion \"0.1.5\""));
        assert!(!script.contains("stremio-lightning-windows.exe"));
    }
}
