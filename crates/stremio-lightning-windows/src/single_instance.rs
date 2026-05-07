#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchArgument {
    FilePath(String),
    StremioDeepLink(String),
    Magnet(String),
    Torrent(String),
}

pub fn classify_launch_argument(argument: &str) -> Option<LaunchArgument> {
    if argument.starts_with("stremio://") {
        Some(LaunchArgument::StremioDeepLink(argument.to_string()))
    } else if argument.starts_with("magnet:") {
        Some(LaunchArgument::Magnet(argument.to_string()))
    } else if argument.ends_with(".torrent") {
        Some(LaunchArgument::Torrent(argument.to_string()))
    } else if argument.starts_with('-') {
        None
    } else {
        Some(LaunchArgument::FilePath(argument.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_launch_arguments() {
        assert_eq!(
            classify_launch_argument("stremio://detail/movie/foo"),
            Some(LaunchArgument::StremioDeepLink(
                "stremio://detail/movie/foo".to_string()
            ))
        );
        assert_eq!(
            classify_launch_argument("magnet:?xt=urn:btih:test"),
            Some(LaunchArgument::Magnet(
                "magnet:?xt=urn:btih:test".to_string()
            ))
        );
        assert_eq!(
            classify_launch_argument("movie.torrent"),
            Some(LaunchArgument::Torrent("movie.torrent".to_string()))
        );
        assert_eq!(
            classify_launch_argument("--webui-url=https://example.com"),
            None
        );
    }
}
