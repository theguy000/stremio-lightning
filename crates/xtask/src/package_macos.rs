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
    let resources_dir =