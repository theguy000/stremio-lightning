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
    base_dir: PathBuf,
    kind: WindowsResourceLayoutKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsResourceLayoutKind {
    Development,
    Packaged,
}

impl WindowsResourceLayout {
    pub fn new(crate_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: crate_dir.into(),
            kind: WindowsResourceLayoutKind::Development,
        }
    }

    pub fn from_manifest_dir() -> Self {
        Self::new(env!("CARGO_MANIFEST_DIR"))
    }

    pub fn from_exe_dir(exe_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: exe_dir.into(),
            kind: WindowsResourceLayoutKind::Packaged,
        }
    }

    pub fn from_runtime() -> Self {
        runtime_layout_from_exe_path(std::env::current_exe().ok())
    }

    pub fn crate_dir(&self) -> &Path {
        &self.base_dir
    }

    pub fn resources_dir(&self) -> PathBuf {
        self.base_dir.join(RESOURCES_DIR_NAME)
    }

    pub fn mpv_dev_dir(&self) -> PathBuf {
        match self.kind {
            WindowsResourceLayoutKind::Development => self.base_dir.join(MPV_DEV_DIR_NAME),
            WindowsResourceLayoutKind::Packaged => self.base_dir.clone(),
        }
    }

    pub fn libmpv_dll(&self) -> PathBuf {
        match self.kind {
            WindowsResourceLayoutKind::Development => self.resources_dir().join(LIBMPV_DLL_NAME),
            WindowsResourceLayoutKind::Packaged => self.base_dir.join(LIBMPV_DLL_NAME),
        }
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

fn runtime_layout_from_exe_path(exe_path: Option<PathBuf>) -> WindowsResourceLayout {
    if let Some(exe_dir) = exe_path
        .as_deref()
        .and_then(Path::parent)
        .filter(|exe_dir| exe_dir.join(RESOURCES_DIR_NAME).is_dir())
    {
        return WindowsResourceLayout::from_exe_dir(exe_dir);
    }

    WindowsResourceLayout::from_manifest_dir()
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

    #[test]
    fn resolves_packaged_resources_beside_executable() {
        let layout = WindowsResourceLayout::from_exe_dir("dist/stremio-lightning-windows");

        assert_eq!(
            layout.resources_dir(),
            PathBuf::from("dist/stremio-lightning-windows/resources")
        );
        assert_eq!(
            layout.libmpv_dll(),
            PathBuf::from("dist/stremio-lightning-windows/libmpv-2.dll")
        );
    }
}
