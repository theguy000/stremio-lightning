use crate::app_integration::{BUNDLE_EXECUTABLE, BUNDLE_NAME};
use std::fs;
use std::path::{Path, PathBuf};

pub const APP_BUNDLE_NAME: &str = "Stremio Lightning.app";
pub const CONTENTS_DIR: &str = "Contents";
pub const MACOS_DIR: &str = "MacOS";
pub const RESOURCES_DIR: &str = "Resources";
pub const FRAMEWORKS_DIR: &str = "Frameworks";
pub const ENTITLEMENTS_FILE: &str = "entitlements.plist";
pub const SERVER_RUNTIME_NAME: &str = "stremio-runtime-macos";
pub const SERVER_SCRIPT_NAME: &str = "server.cjs";
pub const FFMPEG_NAME: &str = "ffmpeg";
pub const FFPROBE_NAME: &str = "ffprobe";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleLayout {
    pub bundle_root: PathBuf,
    pub contents_dir: PathBuf,
    pub executable_dir: PathBuf,
    pub resources_dir: PathBuf,
    pub frameworks_dir: PathBuf,
    pub executable_path: PathBuf,
    pub info_plist_path: PathBuf,
    pub entitlements_path: PathBuf,
}

impl BundleLayout {
    pub fn new(bundle_root: impl Into<PathBuf>) -> Self {
        let bundle_root = bundle_root.into();
        let contents_dir = bundle_root.join(CONTENTS_DIR);
        let executable_dir = contents_dir.join(MACOS_DIR);
        let resources_dir = contents_dir.join(RESOURCES_DIR);
        let frameworks_dir = contents_dir.join(FRAMEWORKS_DIR);
        Self {
            executable_path: executable_dir.join(BUNDLE_EXECUTABLE),
            info_plist_path: contents_dir.join("Info.plist"),
            entitlements_path: resources_dir.join(ENTITLEMENTS_FILE),
            bundle_root,
            contents_dir,
            executable_dir,
            resources_dir,
            frameworks_dir,
        }
    }

    pub fn default_in_dist(root: impl AsRef<Path>) -> Self {
        Self::new(root.as_ref().join("dist").join(APP_BUNDLE_NAME))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.bundle_root.file_name().and_then(|name| name.to_str()) != Some(APP_BUNDLE_NAME) {
            return Err(format!("macOS app bundle must be named {APP_BUNDLE_NAME}"));
        }
        if self
            .executable_path
            .file_name()
            .and_then(|name| name.to_str())
            != Some(BUNDLE_EXECUTABLE)
        {
            return Err("macOS bundle executable path must match Info.plist".to_string());
        }
        if self.info_plist_path != self.contents_dir.join("Info.plist") {
            return Err("macOS Info.plist must live directly under Contents".to_string());
        }
        Ok(())
    }
}

pub fn create_bundle_directories(layout: &BundleLayout) -> std::io::Result<()> {
    fs::create_dir_all(&layout.executable_dir)?;
    fs::create_dir_all(&layout.resources_dir)?;
    fs::create_dir_all(&layout.frameworks_dir)?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledResource {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub executable: bool,
}

pub fn streaming_server_resources(
    source_root: impl AsRef<Path>,
    layout: &BundleLayout,
) -> Vec<BundledResource> {
    let source_root = source_root.as_ref();
    let binaries = source_root.join("binaries");
    let resources = source_root.join("resources");
    vec![
        BundledResource {
            source: binaries.join(SERVER_RUNTIME_NAME),
            destination: layout
                .resources_dir
                .join("binaries")
                .join(SERVER_RUNTIME_NAME),
            executable: true,
        },
        BundledResource {
            source: resources.join(SERVER_SCRIPT_NAME),
            destination: layout
                .resources_dir
                .join("resources")
                .join(SERVER_SCRIPT_NAME),
            executable: false,
        },
        BundledResource {
            source: resources.join(FFMPEG_NAME),
            destination: layout.resources_dir.join("resources").join(FFMPEG_NAME),
            executable: true,
        },
        BundledResource {
            source: resources.join(FFPROBE_NAME),
            destination: layout.resources_dir.join("resources").join(FFPROBE_NAME),
            executable: true,
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandInvocation {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandInvocation {
    pub fn new(
        program: impl Into<String>,
        args: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

pub fn rpath_commands(layout: &BundleLayout, dylibs: &[PathBuf]) -> Vec<CommandInvocation> {
    let mut commands = vec![CommandInvocation::new(
        "install_name_tool",
        [
            "-add_rpath".to_string(),
            "@executable_path/../Frameworks".to_string(),
            layout.executable_path.to_string_lossy().into_owned(),
        ],
    )];

    for dylib in dylibs {
        if let Some(name) = dylib.file_name().and_then(|name| name.to_str()) {
            commands.push(CommandInvocation::new(
                "install_name_tool",
                [
                    "-id".to_string(),
                    format!("@rpath/{name}"),
                    layout
                        .frameworks_dir
                        .join(name)
                        .to_string_lossy()
                        .into_owned(),
                ],
            ));
        }
    }

    commands
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SigningIdentity {
    AdHoc,
    DeveloperId(String),
}

impl SigningIdentity {
    fn codesign_value(&self) -> &str {
        match self {
            Self::AdHoc => "-",
            Self::DeveloperId(identity) => identity.as_str(),
        }
    }
}

pub fn codesign_command(layout: &BundleLayout, identity: SigningIdentity) -> CommandInvocation {
    CommandInvocation::new(
        "codesign",
        [
            "--force".to_string(),
            "--deep".to_string(),
            "--options".to_string(),
            "runtime".to_string(),
            "--entitlements".to_string(),
            layout.entitlements_path.to_string_lossy().into_owned(),
            "--sign".to_string(),
            identity.codesign_value().to_string(),
            layout.bundle_root.to_string_lossy().into_owned(),
        ],
    )
}

pub fn notarization_zip_command(
    layout: &BundleLayout,
    output_zip: impl AsRef<Path>,
) -> CommandInvocation {
    CommandInvocation::new(
        "ditto",
        [
            "-c".to_string(),
            "-k".to_string(),
            "--keepParent".to_string(),
            layout.bundle_root.to_string_lossy().into_owned(),
            output_zip.as_ref().to_string_lossy().into_owned(),
        ],
    )
}

pub fn bundle_environment_value(layout: &BundleLayout) -> String {
    layout.resources_dir.to_string_lossy().into_owned()
}

pub fn bundle_display_name() -> &'static str {
    BUNDLE_NAME
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn builds_expected_bundle_layout() {
        let layout = BundleLayout::default_in_dist("/repo");
        layout.validate().unwrap();
        assert_eq!(
            layout.bundle_root,
            PathBuf::from("/repo/dist/Stremio Lightning.app")
        );
        assert_eq!(
            layout.executable_path,
            PathBuf::from(
                "/repo/dist/Stremio Lightning.app/Contents/MacOS/stremio-lightning-macos"
            )
        );
        assert_eq!(
            layout.frameworks_dir,
            PathBuf::from("/repo/dist/Stremio Lightning.app/Contents/Frameworks")
        );
        assert_eq!(bundle_display_name(), "Stremio Lightning");
    }

    #[test]
    fn creates_bundle_directories_as_filesystem_output() {
        let root = std::env::temp_dir().join(format!(
            "stremio-lightning-macos-bundle-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let layout = BundleLayout::new(root.join(APP_BUNDLE_NAME));
        create_bundle_directories(&layout).unwrap();
        assert!(layout.executable_dir.is_dir());
        assert!(layout.resources_dir.is_dir());
        assert!(layout.frameworks_dir.is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn maps_streaming_server_resources_into_bundle_resources() {
        let layout = BundleLayout::default_in_dist("/repo");
        let resources = streaming_server_resources("/repo/crates/stremio-lightning-macos", &layout);
        assert_eq!(resources.len(), 4);
        assert_eq!(
            resources[0].source,
            PathBuf::from("/repo/crates/stremio-lightning-macos/binaries/stremio-runtime-macos")
        );
        assert_eq!(
            resources[0].destination,
            layout.resources_dir.join("binaries/stremio-runtime-macos")
        );
        assert!(resources[0].executable);
        assert_eq!(
            resources[1].destination,
            layout.resources_dir.join("resources/server.cjs")
        );
    }

    #[test]
    fn constructs_rpath_and_install_name_commands() {
        let layout = BundleLayout::default_in_dist("/repo");
        let commands = rpath_commands(
            &layout,
            &[PathBuf::from("/opt/homebrew/opt/mpv/lib/libmpv.dylib")],
        );
        assert_eq!(commands[0].program, "install_name_tool");
        assert_eq!(commands[0].args[0], "-add_rpath");
        assert_eq!(commands[0].args[1], "@executable_path/../Frameworks");
        assert_eq!(commands[1].args[0], "-id");
        assert_eq!(commands[1].args[1], "@rpath/libmpv.dylib");
        assert_eq!(
            commands[1].args[2],
            "/repo/dist/Stremio Lightning.app/Contents/Frameworks/libmpv.dylib"
        );
    }

    #[test]
    fn constructs_ad_hoc_codesign_command_with_hardened_runtime() {
        let layout = BundleLayout::default_in_dist("/repo");
        let command = codesign_command(&layout, SigningIdentity::AdHoc);
        assert_eq!(command.program, "codesign");
        assert!(command.args.contains(&"--deep".to_string()));
        assert!(command.args.contains(&"runtime".to_string()));
        assert!(command.args.contains(&"-".to_string()));
        assert!(command
            .args
            .contains(&layout.entitlements_path.to_string_lossy().into_owned()));
    }

    #[test]
    fn constructs_notarization_zip_command() {
        let layout = BundleLayout::default_in_dist("/repo");
        let command = notarization_zip_command(&layout, "/repo/dist/Stremio Lightning.zip");
        assert_eq!(command.program, "ditto");
        assert_eq!(command.args[0], "-c");
        assert!(command.args.contains(&"--keepParent".to_string()));
        assert_eq!(
            command.args.last().unwrap(),
            "/repo/dist/Stremio Lightning.zip"
        );
    }
}
