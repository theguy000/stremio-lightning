//! Launches a stream URL in an external player binary (VLC or MPV) or as a temporary M3U
//! playlist opened by the system default handler.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalPlayerKind {
    Vlc,
    Mpv,
    M3u,
}

impl ExternalPlayerKind {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "vlc" => Some(Self::Vlc),
            "mpv" => Some(Self::Mpv),
            "m3u" => Some(Self::M3u),
            _ => None,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Vlc => "VLC",
            Self::Mpv => "MPV",
            Self::M3u => "M3U Playlist",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalLaunch {
    pub kind: ExternalPlayerKind,
    pub url: String,
}

type Launcher = Box<dyn Fn(&ExternalLaunch) -> Result<(), String> + Send + Sync>;

static LAUNCHER: Mutex<Option<Launcher>> = Mutex::new(None);

/// Intercepts external player launches during tests so no real player process starts.
pub fn watch(launcher: Option<Launcher>) {
    *LAUNCHER.lock().unwrap_or_else(|e| e.into_inner()) = launcher;
}

/// Validates that `url` is a clean http(s) URL with a host and no control characters.
pub fn validate_stream_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("stream URL is invalid".to_owned());
    }
    let parsed = url::Url::parse(trimmed).map_err(|_| "stream URL is invalid".to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err("external player only supports http and https stream links".to_owned());
    }
    Ok(parsed.to_string())
}

/// Formats the exact 3-line M3U playlist Stremio Desktop writes.
pub fn format_m3u(url: &str) -> Result<String, String> {
    let valid = validate_stream_url(url)?;
    Ok(format!("#EXTM3U\n#EXTINF:0\n{valid}\n"))
}

pub fn open(launch: &ExternalLaunch) -> Result<(), String> {
    let valid_url = validate_stream_url(&launch.url)?;
    let checked = ExternalLaunch {
        kind: launch.kind,
        url: valid_url,
    };
    if let Some(spy) = LAUNCHER.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return spy(&checked);
    }
    match checked.kind {
        ExternalPlayerKind::Vlc => spawn_player(ExternalPlayerKind::Vlc, &checked.url),
        ExternalPlayerKind::Mpv => spawn_player(ExternalPlayerKind::Mpv, &checked.url),
        ExternalPlayerKind::M3u => open_m3u(&checked.url),
    }
}

fn spawn_player(kind: ExternalPlayerKind, url: &str) -> Result<(), String> {
    let bin = find_binary(kind).ok_or_else(|| {
        format!(
            "{} was not found on this computer. Install it and try again.",
            kind.display_name()
        )
    })?;
    let mut cmd = Command::new(&bin);
    cmd.arg("--").arg(url);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        cmd.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
    }
    cmd.spawn()
        .map(|_| ())
        .map_err(|err| format!("could not launch {}: {err}", kind.display_name()))
}

fn open_m3u(url: &str) -> Result<(), String> {
    let body = format_m3u(url)?;
    let dir = dirs::download_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Stremio");
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not create playlist folder: {e}"))?;
    let path = dir.join("playlist.m3u");
    std::fs::write(&path, body).map_err(|e| format!("could not write playlist.m3u: {e}"))?;
    open_local_file(&path)
}

#[cfg(target_os = "windows")]
fn open_local_file(path: &std::path::Path) -> Result<(), String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let op = HSTRING::from("open");
    let target = HSTRING::from(path.as_os_str());
    // SAFETY: both HSTRING buffers remain valid for the duration of ShellExecuteW.
    let res = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if res.0 as usize <= 32 {
        return Err(format!(
            "could not open playlist.m3u (code {})",
            res.0 as usize
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_local_file(path: &std::path::Path) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open playlist.m3u: {e}"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_local_file(path: &std::path::Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not open playlist.m3u: {e}"))
}

fn find_binary(kind: ExternalPlayerKind) -> Option<PathBuf> {
    for candidate in platform_candidates(kind) {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    let name = match kind {
        ExternalPlayerKind::Vlc => {
            if cfg!(windows) {
                "vlc.exe"
            } else {
                "vlc"
            }
        }
        ExternalPlayerKind::Mpv => {
            if cfg!(windows) {
                "mpv.exe"
            } else {
                "mpv"
            }
        }
        ExternalPlayerKind::M3u => return None,
    };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|p| p.is_file())
    })
}

fn platform_candidates(kind: ExternalPlayerKind) -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let pf = std::env::var_os("ProgramFiles").map(PathBuf::from);
        let pf86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from);
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        match kind {
            ExternalPlayerKind::Vlc => [pf, pf86]
                .into_iter()
                .flatten()
                .map(|r| r.join("VideoLAN").join("VLC").join("vlc.exe"))
                .collect(),
            ExternalPlayerKind::Mpv => [pf, local]
                .into_iter()
                .flatten()
                .flat_map(|r| {
                    [
                        r.join("mpv").join("mpv.exe"),
                        r.join("mpv.net").join("mpvnet.exe"),
                    ]
                })
                .collect(),
            ExternalPlayerKind::M3u => Vec::new(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        match kind {
            ExternalPlayerKind::Vlc => {
                vec![PathBuf::from("/Applications/VLC.app/Contents/MacOS/VLC")]
            }
            ExternalPlayerKind::Mpv => vec![
                PathBuf::from("/Applications/mpv.app/Contents/MacOS/mpv"),
                PathBuf::from("/opt/homebrew/bin/mpv"),
                PathBuf::from("/usr/local/bin/mpv"),
            ],
            ExternalPlayerKind::M3u => Vec::new(),
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = kind;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_urls_and_formats_three_line_m3u() {
        assert!(validate_stream_url("https://example.com/movie.mkv").is_ok());
        assert!(validate_stream_url("http://127.0.0.1:11470/abc/0").is_ok());
        assert!(validate_stream_url("ftp://example.com/movie.mkv").is_err());
        assert!(validate_stream_url("https://example.com/a\nb").is_err());
        assert!(validate_stream_url("vlc://https://example.com/a").is_err());

        let m3u = format_m3u("http://127.0.0.1:11470/abc/0").unwrap();
        assert_eq!(m3u, "#EXTM3U\n#EXTINF:0\nhttp://127.0.0.1:11470/abc/0\n");
    }

    #[test]
    fn parses_known_external_player_kinds() {
        assert_eq!(
            ExternalPlayerKind::parse("vlc"),
            Some(ExternalPlayerKind::Vlc)
        );
        assert_eq!(
            ExternalPlayerKind::parse("MPV"),
            Some(ExternalPlayerKind::Mpv)
        );
        assert_eq!(
            ExternalPlayerKind::parse("m3u"),
            Some(ExternalPlayerKind::M3u)
        );
        assert_eq!(ExternalPlayerKind::parse(""), None);
    }
}
