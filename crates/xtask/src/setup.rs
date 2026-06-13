use crate::common::{Result, bash_program, require_program, root, run_program};
use crate::package_macos::MacosArch;
use std::env;
use std::ffi::OsString;

pub fn setup_current_platform() -> Result<()> {
    match env::consts::OS {
        "linux" => setup_linux(),
        "macos" => setup_macos(MacosArch::host()?),
        "windows" => setup_windows(),
        os => Err(format!("unsupported platform for native shell setup: {os}").into()),
    }
}

pub fn setup_linux() -> Result<()> {
    require_program("bash", "install bash, then rerun: cargo xtask setup-linux")?;
    require_program(
        "gh",
        "install GitHub CLI (`gh`), then rerun: cargo xtask setup-linux",
    )?;
    require_program(
        "dpkg-deb",
        "install dpkg tooling (`dpkg-deb`), then rerun: cargo xtask setup-linux",
    )?;
    require_program("curl", "install curl, then rerun: cargo xtask setup-linux")?;
    require_program("tar", "install tar, then rerun: cargo xtask setup-linux")?;

    run_program(
        bash_program(),
        &[root().join("scripts/download-linux-shell-deps.sh")],
    )
}

pub fn setup_macos(arch: MacosArch) -> Result<()> {
    require_program("bash", "install bash, then rerun: cargo xtask setup-macos")?;
    require_program(
        "gh",
        "install GitHub CLI (`gh`), then rerun: cargo xtask setup-macos",
    )?;
    require_program("curl", "install curl, then rerun: cargo xtask setup-macos")?;
    require_program("tar", "install tar, then rerun: cargo xtask setup-macos")?;
    require_program(
        "unzip",
        "install unzip, then rerun: cargo xtask setup-macos",
    )?;

    run_program(
        bash_program(),
        [
            root()
                .join("scripts/download-macos-shell-deps.sh")
                .into_os_string(),
            OsString::from(arch.name()),
        ],
    )
}

pub fn setup_windows() -> Result<()> {
    run_program(
        bash_program(),
        &[root().join("scripts/download-windows-shell-deps.sh")],
    )
}
