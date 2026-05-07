pub const DEFAULT_WEBUI_URL: &str = "https://web.stremio.com/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsShellSettings {
    pub webui_url: String,
    pub streaming_server_disabled: bool,
}

impl Default for WindowsShellSettings {
    fn default() -> Self {
        Self {
            webui_url: DEFAULT_WEBUI_URL.to_string(),
            streaming_server_disabled: false,
        }
    }
}
