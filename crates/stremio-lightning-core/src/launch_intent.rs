use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub enum LaunchIntent {
    Focus,
    FilePath(String),
    StremioDeepLink(String),
    Magnet(String),
    Torrent(String),
}

impl LaunchIntent {
    #[must_use]
    pub fn open_media_value(&self) -> Option<String> {
        match self {
            Self::Focus => None,
            Self::FilePath(value) | Self::Torrent(value) => Some(normalize_file_argument(value)),
            Self::StremioDeepLink(value) | Self::Magnet(value) => Some(value.clone()),
        }
    }
}

fn normalize_file_argument(value: &str) -> String {
    if !std::path::Path::new(value).exists() {
        return value.to_string();
    }
    if cfg!(windows) {
        format!("file:///{}", value.replace('\\', "/"))
    } else {
        format!("file://{value}")
    }
}

pub fn launch_intent_from_args<I>(args: I) -> LaunchIntent
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    args.into_iter()
        .find_map(|argument| classify_launch_argument(argument.as_ref()))
        .unwrap_or(LaunchIntent::Focus)
}

#[must_use]
pub fn classify_launch_argument(argument: &str) -> Option<LaunchIntent> {
    let starts_with_ignore_ascii_case = |prefix: &str| {
        argument
            .get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    };

    if starts_with_ignore_ascii_case("stremio://") {
        Some(LaunchIntent::StremioDeepLink(argument.to_string()))
    } else if starts_with_ignore_ascii_case("magnet:") {
        Some(LaunchIntent::Magnet(argument.to_string()))
    } else if argument
        .get(argument.len().saturating_sub(".torrent".len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(".torrent"))
    {
        Some(LaunchIntent::Torrent(argument.to_string()))
    } else if argument.starts_with('-') {
        None
    } else {
        Some(LaunchIntent::FilePath(argument.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_arguments_case_insensitively() {
        assert_eq!(
            classify_launch_argument("STREMIO://detail/movie/foo"),
            Some(LaunchIntent::StremioDeepLink(
                "STREMIO://detail/movie/foo".into()
            ))
        );
        assert_eq!(
            classify_launch_argument("MAGNET:?xt=urn:btih:test"),
            Some(LaunchIntent::Magnet("MAGNET:?xt=urn:btih:test".into()))
        );
        assert_eq!(
            classify_launch_argument("MOVIE.TORRENT"),
            Some(LaunchIntent::Torrent("MOVIE.TORRENT".into()))
        );
        assert_eq!(
            classify_launch_argument("short"),
            Some(LaunchIntent::FilePath("short".into()))
        );
        assert_eq!(classify_launch_argument("--devtools"), None);
    }

    #[test]
    fn focus_when_no_open_argument() {
        assert_eq!(launch_intent_from_args(["--devtools"]), LaunchIntent::Focus);
        assert_eq!(LaunchIntent::Focus.open_media_value(), None);
    }

    #[test]
    fn existing_files_become_file_urls() {
        let path = std::env::temp_dir().join("stremio-lightning-open-media-test.torrent");
        std::fs::write(&path, b"test").unwrap();
        let path = path.to_string_lossy().to_string();
        let expected = if cfg!(windows) {
            format!("file:///{}", path.replace('\\', "/"))
        } else {
            format!("file://{path}")
        };
        assert_eq!(
            LaunchIntent::Torrent(path.clone()).open_media_value(),
            Some(expected)
        );
        let _ = std::fs::remove_file(path);
    }
}
