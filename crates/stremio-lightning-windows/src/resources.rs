use std::path::{Path, PathBuf};

pub const RESOURCES_DIR_NAME: &str = "resources";
pub const MPV_DEV_DIR_NAME: &str = "mpv-dev";
pub const LIBMPV_DLL_NAME: &str = "libmpv-2.dll";
pub const STREMIO_RUNTIME_NAME: &str = "stremio-runtime.exe";
pub const SERVER_SCRIPT_NAME: &str = "server.cjs";
pub const FFMPEG_NAME: &str = "ffmpeg.exe";
pub const FFPROBE_NAME: &str = "ffprobe.exe";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsResourceLayout {
    crate_dir: PathBuf,
}

impl WindowsResourceLayout {
    pub fn new(crate_dir: impl Into<PathBuf>) -> Self {
        Self {
            crate_dir: crate_dir.into(),
        }
    }

    pub fn from_manifest_dir() -> Self {
        Self::new(env!("CARGO_MANIFEST_DIR"))
    }

    pub fn crate_dir(&self) -> &Path {
        &self.crate_dir
    }

    pub fn resources_dir(&self) -> PathBuf {
        self.crate_dir.join(RESOURCES_DIR_NAME)
    }

    pub fn mpv_dev_dir(&self) -> PathBuf {
        self.crate_dir.join(MPV_DEV_DIR_NAME)
    }

    pub fn libmpv_dll(&self) -> PathBuf {
        self.resources_dir().join(LIBMPV_DLL_NAME)
    }

    pub fn stremio_runtime(&self) -> PathBuf {
        self.resources_dir().join(STREMIO_RUNTIME_NAME)
    }

    pub fn server_script(&self) -> PathBuf {
        self.resources_dir().join(SERVER_SCRIPT_NAME)
    }

    pub fn ffmpeg(&self) -> PathBuf {
        self.resources_dir().join(FFMPEG_NAME)
    }

    pub fn ffprobe(&self) -> PathBuf {
        self.resources_dir().join(FFPROBE_NAME)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_resources_under_windows_crate() {
        let layout = WindowsResourceLayout::new("crates/stremio-lightning-windows");

        assert_eq!(
            layout.libmpv_dll(),
            PathBuf::from("crates/stremio-lightning-windows/resources/libmpv-2.dll")
        );
        assert_eq!(
            layout.stremio_runtime(),
            PathBuf::from("crates/stremio-lightning-windows/resources/stremio-runtime.exe")
        );
        assert_eq!(
            layout.mpv_dev_dir(),
            PathBuf::from("crates/stremio-lightning-windows/mpv-dev")
        );
    }
}
