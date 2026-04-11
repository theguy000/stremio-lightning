use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread;
use tauri::{AppHandle, Manager, Window};

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

pub const MAIN_APP_LABEL: &str = "main";
pub const PLAYER_HOST_LABEL: &str = MAIN_APP_LABEL;

const FLOAT_PROPERTIES: &[&str] = &[
    "time-pos",
    "duration",
    "volume",
    "speed",
    "sub-pos",
    "sub-scale",
    "sub-delay",
    "cache-buffering-state",
];
const BOOL_PROPERTIES: &[&str] = &[
    "pause",
    "paused-for-cache",
    "seeking",
    "eof-reached",
    "osc",
    "input-default-bindings",
    "input-vo-keyboard",
    "mute",
];
const INT_PROPERTIES: &[&str] = &["aid", "vid", "sid"];
const STRING_PROPERTIES: &[&str] = &[
    "path",
    "mpv-version",
    "ffmpeg-version",
    "hwdec",
    "vo",
    "track-list",
    "video-params",
    "metadata",
    "sub-color",
    "sub-back-color",
    "sub-border-color",
];
const JSON_STRING_PROPERTIES: &[&str] = &["track-list", "video-params", "metadata"];

pub struct PlayerState {
    backend: Mutex<Option<PlayerBackend>>,
    /// Tracks whether the native MPV player is currently paused.
    /// Updated from the MPV event loop whenever a "pause" property change event fires.
    /// Initialized to `true` because no content is playing at startup (effectively paused).
    /// Uses `AtomicBool` so it can be read from the window event callback thread
    /// without locking the backend mutex.
    pub is_paused: AtomicBool,
    /// Flag indicating that we (the auto-pause feature) were the ones who paused playback
    /// when the window lost focus. This is critical for the resume logic: we should only
    /// auto-resume on refocus if we auto-paused — if the user paused manually, we must
    /// not override their intent. Cleared when we resume, or when the user disables the feature.
    pub auto_paused_on_unfocus: AtomicBool,
    /// Whether the auto-pause-on-unfocus feature is enabled (user setting).
    /// Toggled via the `set_auto_pause` Tauri command from the settings UI.
    /// When disabled, both `auto_pause_on_unfocus` and `auto_resume_on_focus` become no-ops.
    pub auto_pause_enabled: AtomicBool,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            backend: Mutex::new(None),
            is_paused: AtomicBool::new(true),
            auto_paused_on_unfocus: AtomicBool::new(false),
            auto_pause_enabled: AtomicBool::new(true),
        }
    }
}

#[derive(Serialize)]
pub struct NativePlayerStatus {
    enabled: bool,
    initialized: bool,
    backend: &'static str,
    host_window_label: Option<String>,
    host_window_visible: bool,
}

struct PlayerBackend {
    command_sender: Sender<PlayerCommand>,
    window: Window,
}

enum PlayerCommand {
    Observe(String),
    SetProperty { name: String, value: Value },
    Command { name: String, args: Vec<String> },
}

#[cfg(windows)]
mod platform {
    use super::{
        BOOL_PROPERTIES, FLOAT_PROPERTIES, INT_PROPERTIES, JSON_STRING_PROPERTIES,
        PlayerState, STRING_PROPERTIES,
    };
    use libmpv2::events::{Event, PropertyData};
    use libmpv2::{mpv_end_file_reason, EndFileReason, Format, Mpv, Result as MpvResult};
    use serde::Serialize;
    use serde_json::{json, Value};
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::Receiver;
    use tauri::{AppHandle, Manager};

    use crate::shell_transport;

    use super::PlayerCommand;

    #[derive(Serialize)]
    struct PlayerPropertyChange {
        name: String,
        data: Value,
    }

    #[derive(Serialize)]
    struct PlayerEndedError {
        message: String,
        critical: bool,
    }

    #[derive(Serialize)]
    struct PlayerEnded {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<PlayerEndedError>,
    }

    pub fn create(window_handle: isize) -> Result<Mpv, String> {
        Mpv::with_initializer(|initializer| -> MpvResult<()> {
            initializer.set_property("wid", window_handle as i64)?;
            initializer.set_property("title", "Stremio Lightning")?;
            initializer.set_property("audio-client-name", "Stremio Lightning")?;
            initializer.set_property("terminal", "yes")?;
            #[cfg(debug_assertions)]
            initializer.set_property("msg-level", "all=no,cplayer=debug")?;
            #[cfg(not(debug_assertions))]
            initializer.set_property("msg-level", "all=no")?;
            initializer.set_property("quiet", "yes")?;
            initializer.set_property("hwdec", "auto")?;
            initializer.set_property("ytdl", "no")?;
            Ok(())
        })
        .map_err(to_string)
    }

    pub fn run_event_loop(app: AppHandle, mut mpv: Mpv, command_receiver: Receiver<PlayerCommand>) {
        if let Err(error) = mpv.disable_deprecated_events() {
            eprintln!("Failed to disable deprecated MPV events: {error}");
        }

        loop {
            for command in command_receiver.try_iter() {
                match command {
                    PlayerCommand::Observe(name) => observe_property(&mpv, &name),
                    PlayerCommand::SetProperty { name, value } => set_property(&mpv, &name, value),
                    PlayerCommand::Command { name, args } => send_command(&mpv, &name, &args),
                }
            }

            let event = match mpv.wait_event(0.1) {
                Some(Ok(event)) => event,
                Some(Err(error)) => {
                    eprintln!("[StremioLightning] MPV event error: {error}");
                    let payload = PlayerEnded {
                        reason: "error".to_string(),
                        error: Some(PlayerEndedError {
                            message: error.to_string(),
                            critical: true,
                        }),
                    };
                    let _ = shell_transport::emit_transport_event(
                        &app,
                        json!(["mpv-event-ended", payload]),
                    );
                    continue;
                }
                None => continue,
            };

            match event {
                Event::PropertyChange { name, change, .. } => {
                    if let Some(data) = property_data_to_json(name, change) {
                        // Mirror the MPV "pause" property into our AtomicBool so the
                        // window event callback can check pause state without locking the
                        // backend mutex. This is the single source-of-truth update for `is_paused`.
                        if name == "pause" {
                            if let Some(paused) = data.as_bool() {
                                if let Some(state) = app.try_state::<PlayerState>() {
                                    state.is_paused.store(paused, Ordering::Relaxed);
                                }
                            }
                        }
                        let payload = PlayerPropertyChange {
                            name: name.to_string(),
                            data,
                        };
                        let _ = shell_transport::emit_transport_event(
                            &app,
                            json!(["mpv-prop-change", payload]),
                        );
                    }
                }
                Event::EndFile(reason) => {
                    eprintln!(
                        "[StremioLightning] MPV EndFile: reason={} (raw={})",
                        end_reason_string(reason),
                        reason as i32
                    );
                    let error_msg = if reason == mpv_end_file_reason::Error {
                        let msg = format!(
                            "MPV playback error (end-file reason={})",
                            reason as i32
                        );
                        eprintln!("[StremioLightning] {}", msg);
                        Some(PlayerEndedError {
                            message: msg,
                            critical: true,
                        })
                    } else {
                        None
                    };
                    let payload = PlayerEnded {
                        reason: end_reason_string(reason).to_string(),
                        error: error_msg,
                    };
                    let _ = shell_transport::emit_transport_event(
                        &app,
                        json!(["mpv-event-ended", payload]),
                    );
                }
                Event::Shutdown => break,
                _ => {}
            }
        }
    }

    fn observe_property(mpv: &Mpv, name: &str) {
        let format = property_format(name);
        if let Some(format) = format {
            if let Err(error) = mpv.observe_property(name, format, 0) {
                eprintln!("Failed to observe MPV property {name}: {error}");
            }
        } else {
            eprintln!("Unsupported MPV property observation: {name}");
        }
    }

    fn send_command(mpv: &Mpv, name: &str, args: &[String]) {
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Err(error) = mpv.command(name, &arg_refs) {
            eprintln!("[StremioLightning] MPV command '{name}' failed: {error} (args: {args:?})");
        }
    }

    fn set_property(mpv: &Mpv, name: &str, value: Value) {
        let name = name.to_string();
        match value {
            Value::Bool(boolean) => {
                if let Err(error) = mpv.set_property(&name, boolean) {
                    eprintln!("Failed to set MPV property {name}: {error}");
                }
            }
            Value::Number(number) => {
                if let Some(integer) = number.as_i64() {
                    if let Err(error) = mpv.set_property(&name, integer) {
                        eprintln!("Failed to set MPV property {name}: {error}");
                    }
                } else if let Some(float) = number.as_f64() {
                    if let Err(error) = mpv.set_property(&name, float) {
                        eprintln!("Failed to set MPV property {name}: {error}");
                    }
                }
            }
            Value::String(mut string) => {
                if name == "vo" {
                    if !string.is_empty() && !string.ends_with(',') {
                        string.push(',');
                    }
                    string.push_str("gpu-next,");
                }

                if let Err(error) = mpv.set_property(&name, string) {
                    eprintln!("Failed to set MPV property {name}: {error}");
                }
            }
            other => {
                eprintln!("Unsupported MPV property value for {name}: {other}");
            }
        }
    }

    fn property_format(name: &str) -> Option<Format> {
        if FLOAT_PROPERTIES.contains(&name) {
            Some(Format::Double)
        } else if BOOL_PROPERTIES.contains(&name) {
            Some(Format::Flag)
        } else if INT_PROPERTIES.contains(&name) {
            Some(Format::Int64)
        } else if STRING_PROPERTIES.contains(&name) {
            Some(Format::String)
        } else {
            None
        }
    }

    fn property_data_to_json(name: &str, data: PropertyData) -> Option<Value> {
        match data {
            PropertyData::Flag(value) => Some(Value::Bool(value)),
            PropertyData::Int64(value) => Some(json!(value)),
            PropertyData::Double(value) => serde_json::Number::from_f64(value).map(Value::Number),
            PropertyData::OsdStr(value) => Some(Value::String(value.to_string())),
            PropertyData::Str(value) => {
                let value = value.to_string();
                if JSON_STRING_PROPERTIES.contains(&name) {
                    serde_json::from_str::<Value>(&value)
                        .ok()
                        .or(Some(Value::String(value)))
                } else {
                    Some(Value::String(value))
                }
            }
            // Catch-all for any future PropertyData variants added by libmpv2.
            #[allow(unreachable_patterns)]
            _ => {
                eprintln!("[StremioLightning] Unhandled MPV property data variant for {name}");
                None
            }
        }
    }

    fn end_reason_string(reason: EndFileReason) -> &'static str {
        match reason {
            mpv_end_file_reason::Error => "error",
            mpv_end_file_reason::Quit => "quit",
            _ => "other",
        }
    }

    fn to_string(error: impl std::fmt::Display) -> String {
        error.to_string()
    }
}

#[cfg(not(windows))]
mod platform {
    use std::sync::mpsc::Receiver;

    use tauri::AppHandle;

    use super::PlayerCommand;

    pub struct UnsupportedMpv;

    pub fn create(_window_handle: isize) -> Result<UnsupportedMpv, String> {
        Err("Native MPV is only implemented on Windows in this build".to_string())
    }

    pub fn run_event_loop(
        _app: AppHandle,
        _mpv: &UnsupportedMpv,
        _command_receiver: Receiver<PlayerCommand>,
    ) {
    }
}

pub fn native_player_enabled() -> bool {
    cfg!(windows)
        && std::env::var("STREMIO_LIGHTNING_NATIVE_PLAYER")
            .map(|value| {
                let normalized = value.trim().to_ascii_lowercase();
                normalized != "0" && normalized != "false" && normalized != "off"
            })
            .unwrap_or(true)
}

pub fn initialize(app: &AppHandle) -> Result<(), String> {
    if !native_player_enabled() {
        return Ok(());
    }

    #[cfg(windows)]
    {
        // Tauri bundles resources in a "resources/" subdirectory next to the exe,
        // which is not on the default OS DLL search path. We must load
        // libmpv-2.dll explicitly by full path before the first libmpv2 FFI
        // call. The delay-load hook will then find the DLL already in memory.
        {
            let dll_path = app
                .path()
                .resource_dir()
                .map(|dir| dir.join("resources").join("libmpv-2.dll"))
                .map_err(|e| format!("Failed to resolve resource directory: {}", e))?;

            let wide: Vec<u16> = dll_path.as_os_str().encode_wide().collect();
            // LoadLibraryExW may not handle the \\?\ extended-length prefix,
            // and the path doesn't need it (well under MAX_PATH).
            let start = if wide.len() >= 4
                && wide[..4] == ['\\' as u16, '\\' as u16, '?' as u16, '\\' as u16]
            {
                4
            } else {
                0
            };
            let wide: Vec<u16> = wide[start..].iter().copied().chain(std::iter::once(0)).collect();
            unsafe {
                let handle = windows_sys::Win32::System::LibraryLoader::LoadLibraryExW(
                    wide.as_ptr(),
                    std::ptr::null_mut(),
                    0,
                );
                if handle.is_null() {
                    return Err(format!(
                        "Failed to load libmpv-2.dll from {}",
                        dll_path.display()
                    ));
                }
            }
        }

        let state = app.state::<PlayerState>();
        let mut backend = state.backend.lock().map_err(|e| e.to_string())?;
        if backend.is_some() {
            return Ok(());
        }

        let main_window = app
            .get_window(MAIN_APP_LABEL)
            .ok_or_else(|| "Main window not found".to_string())?;

        let player_hwnd = main_window.hwnd().map_err(|e| e.to_string())?;

        let mpv = platform::create(player_hwnd.0 as isize)?;
        let (command_sender, command_receiver) = mpsc::channel::<PlayerCommand>();
        let app_handle = app.clone();

        thread::spawn(move || {
            platform::run_event_loop(app_handle, mpv, command_receiver);
        });

        *backend = Some(PlayerBackend {
            command_sender,
            window: main_window.clone(),
        });

        eprintln!(
            "[StremioLightning] Native player initialized with backend=libmpv window={}",
            PLAYER_HOST_LABEL
        );
    }

    Ok(())
}

pub fn status(app: &AppHandle) -> NativePlayerStatus {
    let enabled = native_player_enabled();
    let (initialized, host_window_label, host_window_visible) =
        match app.state::<PlayerState>().backend.lock() {
            Ok(guard) => {
                if let Some(backend) = guard.as_ref() {
                    (
                        true,
                        Some(backend.window.label().to_string()),
                        backend.window.is_visible().unwrap_or(false),
                    )
                } else {
                    (false, None, false)
                }
            }
            Err(_) => (false, None, false),
        };

    NativePlayerStatus {
        enabled,
        initialized,
        backend: if cfg!(windows) { "libmpv" } else { "disabled" },
        host_window_label,
        host_window_visible,
    }
}

pub fn handle_transport(app: &AppHandle, method: &str, data: Option<Value>) -> Result<(), String> {
    let state = app.state::<PlayerState>();
    let backend = state.backend.lock().map_err(|e| e.to_string())?;
    let backend = backend
        .as_ref()
        .ok_or_else(|| "Native MPV backend is not initialized".to_string())?;

    match method {
        "mpv-observe-prop" => {
            let name = data
                .as_ref()
                .and_then(Value::as_str)
                .ok_or_else(|| "Invalid mpv-observe-prop payload".to_string())?;
            backend
                .command_sender
                .send(PlayerCommand::Observe(name.to_string()))
                .map_err(|e| e.to_string())
        }
        "mpv-set-prop" => {
            let pair = data
                .as_ref()
                .and_then(Value::as_array)
                .ok_or_else(|| "Invalid mpv-set-prop payload".to_string())?;
            let name = pair
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| "Missing mpv-set-prop name".to_string())?;
            let value = pair
                .get(1)
                .cloned()
                .ok_or_else(|| "Missing mpv-set-prop value".to_string())?;
            backend
                .command_sender
                .send(PlayerCommand::SetProperty {
                    name: name.to_string(),
                    value,
                })
                .map_err(|e| e.to_string())
        }
        "mpv-command" => {
            let args = data
                .as_ref()
                .and_then(Value::as_array)
                .ok_or_else(|| "Invalid mpv-command payload".to_string())?;
            let name = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| "Missing mpv-command name".to_string())?;
            let values = args
                .iter()
                .skip(1)
                .map(|value| match value {
                    Value::String(string) => Ok(string.clone()),
                    other => Ok(other.to_string()),
                })
                .collect::<Result<Vec<_>, String>>()?;

            if name == "loadfile" {
                eprintln!("[StremioLightning] MPV loadfile -> {:?}", values);
                let _ = backend.window.set_focus();
            } else if name == "stop" {
                eprintln!("[StremioLightning] MPV stop");
            }

            backend
                .command_sender
                .send(PlayerCommand::Command {
                    name: name.to_string(),
                    args: values,
                })
                .map_err(|e| e.to_string())
        }
        other => Err(format!("Unsupported MPV transport method: {other}")),
    }
}

pub fn stop_and_hide(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<PlayerState>();
    let backend = state.backend.lock().map_err(|e| e.to_string())?;
    let Some(backend) = backend.as_ref() else {
        return Ok(());
    };

    backend
        .command_sender
        .send(PlayerCommand::Command {
            name: "stop".to_string(),
            args: Vec::new(),
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Send a pause/unpause command to the MPV backend via the command channel.
/// Returns `true` if the command was successfully enqueued, `false` if the
/// backend is not initialized, the mutex is poisoned, or the channel is closed.
/// This is the shared implementation used by both `auto_pause_on_unfocus` and
/// `auto_resume_on_focus` to avoid duplicating the lock-and-send boilerplate.
fn send_pause(app: &AppHandle, pause: bool) -> bool {
    let state = app.state::<PlayerState>();
    let backend = match state.backend.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    let Some(backend) = backend.as_ref() else {
        return false;
    };
    backend
        .command_sender
        .send(PlayerCommand::SetProperty {
            name: "pause".to_string(),
            value: Value::Bool(pause),
        })
        .is_ok()
}

/// Called from the window event callback when the window loses focus.
/// If the auto-pause feature is enabled and the player is currently playing,
/// sends a pause command to MPV and sets the `auto_paused_on_unfocus` flag
/// so that `auto_resume_on_focus` knows it should resume later.
pub fn auto_pause_on_unfocus(app: &AppHandle) {
    let state = app.state::<PlayerState>();

    // Feature disabled by user — do nothing
    if !state.auto_pause_enabled.load(Ordering::Relaxed) {
        return;
    }

    // Already paused (either by user or by us) — no need to pause again
    if state.is_paused.load(Ordering::Relaxed) {
        return;
    }

    // Attempt to send the pause command; on success, mark that we auto-paused
    if send_pause(app, true) {
        state.auto_paused_on_unfocus.store(true, Ordering::Relaxed);
        eprintln!("[StremioLightning] Auto-paused native player on unfocus");
    }
}

/// Called from the window event callback when the window regains focus.
/// Only resumes playback if the auto-pause feature is enabled AND we were the
/// ones who paused it (i.e. `auto_paused_on_unfocus` is true). This prevents
/// overriding a manual pause the user initiated while the window was unfocused.
pub fn auto_resume_on_focus(app: &AppHandle) {
    let state = app.state::<PlayerState>();

    // Feature disabled by user — do nothing
    if !state.auto_pause_enabled.load(Ordering::Relaxed) {
        return;
    }

    // We didn't auto-pause (user paused manually, or player was already paused) — don't resume
    if !state.auto_paused_on_unfocus.load(Ordering::Relaxed) {
        return;
    }

    // Clear the flag before resuming so we don't double-resume on a spurious focus event
    state.auto_paused_on_unfocus.store(false, Ordering::Relaxed);

    // Send the unpause command to MPV
    if send_pause(app, false) {
        eprintln!("[StremioLightning] Auto-resumed native player on focus");
    }
}
