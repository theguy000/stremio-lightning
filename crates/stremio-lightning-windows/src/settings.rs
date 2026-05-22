pub const DEFAULT_WEBUI_URL: &str = "https://web.stremio.com/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WebviewEngine {
    #[default]
    WebView2,
    Servo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsShellSettings {
    pub webui_url: String,
    pub streaming_server_disabled: bool,
    pub devtools: bool,
    pub engine: WebviewEngine,
}

pub type ShellSettings = WindowsShellSettings;

impl Default for WindowsShellSettings {
    fn default() -> Self {
        let mut settings = Self {
            webui_url: DEFAULT_WEBUI_URL.to_string(),
            streaming_server_disabled: false,
            devtools: true,
            engine: WebviewEngine::default(),
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
        let args_vec = args
            .into_iter()
            .map(|a| a.as_ref().to_string())
            .collect::<Vec<String>>();
        let mut i = 0;
        while i < args_vec.len() {
            let arg = &args_vec[i];
            if let Some(url) = arg.strip_prefix("--webui-url=") {
                self.webui_url = url.to_string();
            } else if arg == "--streaming-server-disabled" {
                self.streaming_server_disabled = true;
            } else if arg == "--devtools" {
                self.devtools = true;
            } else if arg == "--engine" {
                if i + 1 < args_vec.len() {
                    self.engine = match args_vec[i + 1].as_str() {
                        "servo" => WebviewEngine::Servo,
                        _ => WebviewEngine::WebView2,
                    };
                    i += 1;
                }
            } else if let Some(eng) = arg.strip_prefix("--engine=") {
                self.engine = match eng {
                    "servo" => WebviewEngine::Servo,
                    _ => WebviewEngine::WebView2,
                };
            }
            i += 1;
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
            devtools: true,
            engine: WebviewEngine::default(),
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

    #[test]
    fn enables_devtools_by_default() {
        let settings = WindowsShellSettings::from_args([] as [&str; 0]);

        assert!(settings.devtools);
    }

    #[test]
    fn parses_devtools_flag() {
        let settings = WindowsShellSettings::from_args(["--devtools"]);

        assert!(settings.devtools);
    }

    #[test]
    fn parses_engine_selection() {
        let settings = WindowsShellSettings::from_args(["--engine", "servo"]);
        assert_eq!(settings.engine, WebviewEngine::Servo);

        let settings = WindowsShellSettings::from_args(["--engine=servo"]);
        assert_eq!(settings.engine, WebviewEngine::Servo);

        let settings = WindowsShellSettings::from_args(["--engine=webview2"]);
        assert_eq!(settings.engine, WebviewEngine::WebView2);

        let settings = WindowsShellSettings::from_args([] as [&str; 0]);
        assert_eq!(settings.engine, WebviewEngine::WebView2);
    }
}
