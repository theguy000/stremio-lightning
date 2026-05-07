use serde::{Deserialize, Serialize};

#[cfg(windows)]
const MUTEX_NAME: &str = "Local\\StremioLightning.SingleInstance";
#[cfg(windows)]
const PIPE_NAME: &str = r"\\.\pipe\StremioLightning.SingleInstance";
#[cfg(windows)]
const MAX_LAUNCH_INTENT_BYTES: usize = 4096;

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
    pub fn open_media_value(&self) -> Option<String> {
        match self {
            Self::Focus => None,
            Self::FilePath(value) | Self::Torrent(value) => Some(normalize_file_argument(value)),
            Self::StremioDeepLink(value) | Self::Magnet(value) => Some(value.clone()),
        }
    }
}

fn normalize_file_argument(value: &str) -> String {
    if std::path::Path::new(value).exists() {
        format!("file:///{}", value.replace('\\', "/"))
    } else {
        value.to_string()
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

pub fn classify_launch_argument(argument: &str) -> Option<LaunchIntent> {
    let lower = argument.to_ascii_lowercase();
    if lower.starts_with("stremio://") {
        Some(LaunchIntent::StremioDeepLink(argument.to_string()))
    } else if lower.starts_with("magnet:") {
        Some(LaunchIntent::Magnet(argument.to_string()))
    } else if lower.ends_with(".torrent") {
        Some(LaunchIntent::Torrent(argument.to_string()))
    } else if argument.starts_with('-') {
        None
    } else {
        Some(LaunchIntent::FilePath(argument.to_string()))
    }
}

#[cfg(windows)]
pub use platform::{SingleInstanceGuard, SingleInstanceRole};

#[cfg(windows)]
mod platform {
    use super::{LaunchIntent, MAX_LAUNCH_INTENT_BYTES, MUTEX_NAME, PIPE_NAME};
    use std::sync::{mpsc, Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_PIPE_BUSY,
        ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_WRITE,
        FILE_SHARE_MODE, OPEN_EXISTING, PIPE_ACCESS_INBOUND,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, WaitNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
        PIPE_WAIT,
    };
    use windows::Win32::System::Threading::CreateMutexW;

    pub enum SingleInstanceRole {
        Primary(SingleInstanceGuard),
        SecondaryDelivered,
    }

    pub struct SingleInstanceGuard {
        mutex: HANDLE,
    }

    impl SingleInstanceGuard {
        pub fn acquire(intent: LaunchIntent) -> Result<SingleInstanceRole, String> {
            let mutex_name = to_wide_null(MUTEX_NAME);
            let mutex = unsafe { CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr())) }
                .map_err(|error| format!("Failed to create single-instance mutex: {error}"))?;

            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe {
                    let _ = CloseHandle(mutex);
                }
                deliver_to_primary(&intent)?;
                Ok(SingleInstanceRole::SecondaryDelivered)
            } else {
                Ok(SingleInstanceRole::Primary(Self { mutex }))
            }
        }

        pub fn start_listener(
            &self,
            notifier: Arc<Mutex<Option<crate::window::UiThreadNotifier>>>,
            initial_intent: LaunchIntent,
        ) -> mpsc::Receiver<LaunchIntent> {
            let (tx, rx) = mpsc::channel();
            if initial_intent != LaunchIntent::Focus {
                let _ = tx.send(initial_intent);
            }
            thread::spawn(move || loop {
                match receive_one_intent() {
                    Ok(intent) => {
                        let _ = tx.send(intent);
                        let notifier = notifier.lock().ok().and_then(|notifier| *notifier);
                        if let Some(notifier) = notifier {
                            let _ = notifier.notify();
                        }
                    }
                    Err(error) => {
                        eprintln!("[StremioLightning] Single-instance pipe failed: {error}")
                    }
                }
            });
            rx
        }
    }

    impl Drop for SingleInstanceGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.mutex);
            }
        }
    }

    fn deliver_to_primary(intent: &LaunchIntent) -> Result<(), String> {
        let pipe_name = to_wide_null(PIPE_NAME);
        let pipe = open_primary_pipe(&pipe_name)?;

        let result = write_intent(pipe, intent);
        unsafe {
            let _ = CloseHandle(pipe);
        }
        result
    }

    fn open_primary_pipe(pipe_name: &[u16]) -> Result<HANDLE, String> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match unsafe {
                CreateFileW(
                    PCWSTR(pipe_name.as_ptr()),
                    FILE_GENERIC_WRITE.0,
                    FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            } {
                Ok(pipe) => return Ok(pipe),
                Err(error) => {
                    let last_error = unsafe { GetLastError() };
                    if Instant::now() >= deadline {
                        return Err(format!(
                            "Failed to connect to primary Windows shell: {error}"
                        ));
                    }
                    if last_error == ERROR_PIPE_BUSY {
                        let _ = unsafe { WaitNamedPipeW(PCWSTR(pipe_name.as_ptr()), 100) };
                    } else if last_error == ERROR_FILE_NOT_FOUND {
                        thread::sleep(Duration::from_millis(25));
                    } else {
                        return Err(format!(
                            "Failed to connect to primary Windows shell: {error}"
                        ));
                    }
                }
            }
        }
    }

    fn receive_one_intent() -> Result<LaunchIntent, String> {
        let pipe_name = to_wide_null(PIPE_NAME);
        let pipe = unsafe {
            CreateNamedPipeW(
                PCWSTR(pipe_name.as_ptr()),
                PIPE_ACCESS_INBOUND,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                1,
                MAX_LAUNCH_INTENT_BYTES as u32,
                MAX_LAUNCH_INTENT_BYTES as u32,
                0,
                None,
            )
        };
        if pipe == INVALID_HANDLE_VALUE {
            return Err("Failed to create single-instance pipe".to_string());
        }

        let result = match unsafe { ConnectNamedPipe(pipe, None) } {
            Ok(()) => read_intent(pipe),
            Err(_) if unsafe { GetLastError() } == ERROR_PIPE_CONNECTED => read_intent(pipe),
            Err(error) => Err(format!("Failed to accept secondary instance pipe: {error}")),
        };
        unsafe {
            let _ = CloseHandle(pipe);
        }
        result
    }

    fn write_intent(pipe: HANDLE, intent: &LaunchIntent) -> Result<(), String> {
        let body = serde_json::to_vec(intent)
            .map_err(|error| format!("Failed to serialize launch intent: {error}"))?;
        if body.len() > MAX_LAUNCH_INTENT_BYTES {
            return Err("Launch intent payload is too large".to_string());
        }
        let len = (body.len() as u32).to_le_bytes();
        write_all(pipe, &len)?;
        write_all(pipe, &body)
    }

    fn read_intent(pipe: HANDLE) -> Result<LaunchIntent, String> {
        let mut len = [0_u8; 4];
        read_exact(pipe, &mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        if len > MAX_LAUNCH_INTENT_BYTES {
            return Err("Launch intent payload is too large".to_string());
        }
        let mut body = vec![0_u8; len];
        read_exact(pipe, &mut body)?;
        serde_json::from_slice(&body)
            .map_err(|error| format!("Invalid launch intent payload: {error}"))
    }

    fn write_all(pipe: HANDLE, mut bytes: &[u8]) -> Result<(), String> {
        while !bytes.is_empty() {
            let mut written = 0;
            unsafe { WriteFile(pipe, Some(bytes), Some(&mut written), None) }
                .map_err(|error| format!("Failed to write launch intent: {error}"))?;
            bytes = &bytes[written as usize..];
        }
        Ok(())
    }

    fn read_exact(pipe: HANDLE, mut bytes: &mut [u8]) -> Result<(), String> {
        while !bytes.is_empty() {
            let mut read = 0;
            unsafe { ReadFile(pipe, Some(bytes), Some(&mut read), None) }
                .map_err(|error| format!("Failed to read launch intent: {error}"))?;
            let read = read as usize;
            if read == 0 {
                return Err("Single-instance pipe closed before launch intent was read".to_string());
            }
            let (_, remaining) = std::mem::take(&mut bytes).split_at_mut(read);
            bytes = remaining;
        }
        Ok(())
    }

    fn to_wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_supported_launch_arguments() {
        assert_eq!(
            classify_launch_argument("stremio://detail/movie/foo"),
            Some(LaunchIntent::StremioDeepLink(
                "stremio://detail/movie/foo".to_string()
            ))
        );
        assert_eq!(
            classify_launch_argument("magnet:?xt=urn:btih:test"),
            Some(LaunchIntent::Magnet("magnet:?xt=urn:btih:test".to_string()))
        );
        assert_eq!(
            classify_launch_argument("movie.torrent"),
            Some(LaunchIntent::Torrent("movie.torrent".to_string()))
        );
        assert_eq!(
            classify_launch_argument("--webui-url=https://example.com"),
            None
        );
    }

    #[test]
    fn uses_focus_when_no_open_argument_is_present() {
        assert_eq!(
            launch_intent_from_args(["--streaming-server-disabled"]),
            LaunchIntent::Focus
        );
    }

    #[test]
    fn converts_launch_intent_to_shell_transport_open_media_value() {
        assert_eq!(
            LaunchIntent::Magnet("magnet:?xt=urn:btih:test".to_string()).open_media_value(),
            Some("magnet:?xt=urn:btih:test".to_string())
        );
        assert_eq!(LaunchIntent::Focus.open_media_value(), None);
    }

    #[test]
    fn existing_file_paths_are_normalized_like_shell_ng() {
        let path = std::env::temp_dir().join("stremio-lightning-open-media-test.torrent");
        std::fs::write(&path, b"test").unwrap();
        let path = path.to_string_lossy().to_string();

        assert_eq!(
            LaunchIntent::Torrent(path.clone()).open_media_value(),
            Some(format!("file:///{}", path.replace('\\', "/")))
        );

        let _ = std::fs::remove_file(path);
    }
}
