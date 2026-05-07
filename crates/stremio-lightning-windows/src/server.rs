use crate::resources::WindowsResourceLayout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsServerConfig {
    pub disabled: bool,
    pub runtime_path: std::path::PathBuf,
    pub script_path: std::path::PathBuf,
}

impl WindowsServerConfig {
    pub fn from_resources(layout: &WindowsResourceLayout) -> Self {
        Self {
            disabled: false,
            runtime_path: layout.stremio_runtime(),
            script_path: layout.server_script(),
        }
    }
}
