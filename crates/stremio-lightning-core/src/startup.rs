//! Startup options every desktop shell parses the same way: the page to load
//! and the developer flags. Each shell adds its own flags on top.

pub const DEFAULT_URL: &str = "http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/";
pub const STREMIO_WEB_URL: &str = "https://web.stremio.com/";

/// Sends the bare Stremio web URL through the local streaming-server proxy, which
/// the page needs to reach the server. Any other URL is kept as given.
#[must_use]
pub fn normalize_startup_url(url: &str) -> String {
    if url.trim_end_matches('/') == STREMIO_WEB_URL.trim_end_matches('/') {
        DEFAULT_URL.to_string()
    } else {
        url.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupOptions {
    pub url: String,
    pub devtools: bool,
    pub headless_bootstrap: bool,
}

impl Default for StartupOptions {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.to_string(),
            devtools: true,
            headless_bootstrap: false,
        }
    }
}

impl StartupOptions {
    /// Applies `arg` if it is one of the shared flags, taking the value of
    /// `--url` from `rest`. Returns `false` for anything else, so the shell can
    /// try its own flags.
    pub fn apply_arg(&mut self, arg: &str, rest: &mut impl Iterator<Item = String>) -> bool {
        if arg == "--url" {
            if let Some(url) = rest.next() {
                self.url = normalize_startup_url(&url);
            }
        } else if let Some(url) = arg.strip_prefix("--url=") {
            self.url = normalize_startup_url(url);
        } else if arg == "--devtools" {
            self.devtools = true;
        } else if arg == "--headless-bootstrap" {
            self.headless_bootstrap = true;
        } else {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> (StartupOptions, Vec<String>) {
        let mut options = StartupOptions::default();
        let mut rest = args.iter().map(ToString::to_string);
        let mut unhandled = Vec::new();
        while let Some(arg) = rest.next() {
            if !options.apply_arg(&arg, &mut rest) {
                unhandled.push(arg);
            }
        }
        (options, unhandled)
    }

    #[test]
    fn defaults_to_the_streaming_server_proxy_with_devtools() {
        let (options, unhandled) = parse(&[]);
        assert_eq!(options.url, DEFAULT_URL);
        assert!(options.devtools);
        assert!(!options.headless_bootstrap);
        assert!(unhandled.is_empty());
    }

    #[test]
    fn accepts_a_developer_url_in_both_spellings() {
        assert_eq!(
            parse(&["--url", "file:///tmp/smoke.html"]).0.url,
            "file:///tmp/smoke.html"
        );
        assert_eq!(
            parse(&["--url=https://localhost:5173/"]).0.url,
            "https://localhost:5173/"
        );
    }

    #[test]
    fn url_flag_without_a_value_keeps_the_default() {
        let (options, unhandled) = parse(&["--url"]);
        assert_eq!(options.url, DEFAULT_URL);
        assert!(unhandled.is_empty());
    }

    #[test]
    fn bare_stremio_web_url_goes_through_the_local_proxy() {
        for url in ["https://web.stremio.com/", "https://web.stremio.com"] {
            assert_eq!(parse(&["--url", url]).0.url, DEFAULT_URL);
        }
        assert_eq!(
            normalize_startup_url("https://web.stremio.com/#/player"),
            "https://web.stremio.com/#/player"
        );
    }

    #[test]
    fn accepts_devtools_and_headless_bootstrap() {
        let (options, _) = parse(&["--devtools", "--headless-bootstrap"]);
        assert!(options.devtools);
        assert!(options.headless_bootstrap);
    }

    #[test]
    fn leaves_other_arguments_to_the_shell() {
        let (_, unhandled) = parse(&["--no-streaming-server", "stremio://x", "--devtools"]);
        assert_eq!(unhandled, ["--no-streaming-server", "stremio://x"]);
    }
}
