pub const DEFAULT_WEBUI_URL: &str = "https://web.stremio.com/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsShellSettings {
    pub webui_url: String,
    pub streaming_server_disabled: bool,
}

impl Default for WindowsShellSettings {
    fn default() -> Self {
        let mut settings = Self {
            webui_url: DEFAULT_WEBUI_URL.to_string(),
            streaming_server_disabled: false,
        };

        settings.apply_args(std::env::args().skip(1));
        settings
    }
}

impl WindowsShellSettings {
    pub fn apply_args<I>(&mut self, args: I)
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        for arg in args {
            let arg = arg.as_ref();
            if let Some(url) = arg.strip_prefix("--webui-url=") {
                self.webui_url = url.to_string();
            } else if arg == "--streaming-server-disabled" {
                self.streaming_server_disabled = true;
            }
        }
    }

    pub fn from_args<I>(args: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        Self {
            webui_url: DEFAULT_WEBUI_URL.to_string(),
            streaming_server_disabled: false,
        }
        .with_args(args)
    }

    fn with_args<I>(mut self, args: I) -> Self
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        self.apply_args(args);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_webui_url_override() {
        let settings = WindowsShellSettings::from_args(["--webui-url=http://127.0.0.1:5173/"]);

        assert_eq!(settings.webui_url, "http://127.0.0.1:5173/");
    }
}
