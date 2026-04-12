use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use tauri::{AppHandle, Manager, Window};
use tauri::PhysicalPosition;
use tauri::PhysicalSize;

use crate::shell_transport;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

pub const MAIN_APP_LABEL: &str = "main";
pub const PLAYER_HOST_LABEL: &str = MAIN_APP_LABEL;

/// All properties observed as MPV_FORMAT_NODE.
const OBSERVED_PROPERTIES: &[&str] = &[
    "time-pos",
    "duration",
    "volume",
    "speed",
    "sub-pos",
    "sub-scale",
    "sub-delay",
    "cache-buffering-state",
    "pause",
    "paused-for-cache",
    "seeking",
    "eof-reached",
    "osc",
    "input-default-bindings",
    "input-vo-keyboard",
    "mute",
    "aid",
    "vid",
    "sid",
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
    /// Whether PiP mode should suppress auto-pause-on-unfocus.
    /// When enabled and PiP is active, the player won't auto-pause when the window
    /// loses focus — since PiP is always-on-top, the user is likely multitasking.
    pub pip_disables_auto_pause: AtomicBool,
    /// Whether Picture-in-Picture mode is currently active.
    /// When enabled, the window becomes borderless and always-on-top.
    /// The web UI handles the compact visual layout.
    pub is_pip_mode: AtomicBool,
    /// Saved window geometry before entering PiP mode.
    /// Used to restore the window size and position when exiting PiP.
    pub pre_pip_geometry: Mutex<Option<PrePipGeometry>>,
}

/// Stores the window size and position before PiP mode was activated,
/// so they can be restored when PiP is exited.
pub struct PrePipGeometry {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            backend: Mutex::new(None),
            is_paused: AtomicBool::new(true),
            auto_paused_on_unfocus: AtomicBool::new(false),
            auto_pause_enabled: AtomicBool::new(true),
            pip_disables_auto_pause: AtomicBool::new(true),
            is_pip_mode: AtomicBool::new(false),
            pre_pip_geometry: Mutex::new(None),
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
    wakeup: Arc<(Mutex<bool>, Condvar)>,
    window: Window,
}

impl PlayerBackend {
    fn signal(&self) {
        signal_wakeup(&self.wakeup);
    }
}

fn signal_wakeup(wakeup: &Arc<(Mutex<bool>, Condvar)>) {
    let (lock, cvar) = &**wakeup;
    if let Ok(mut flag) = lock.lock() {
        *flag = true;
        cvar.notify_all();
    }
}

enum PlayerCommand {
    Observe(String),
    SetProperty { name: String, value: Value },
    Command { name: String, args: Vec<String> },
}

#[cfg(windows)]
mod platform {
    use super::{OBSERVED_PROPERTIES, PlayerState, exit_pip_internal};
    use libmpv2::{mpv_end_file_reason, Format, Mpv, Result as MpvResult};
    use libmpv2_sys as sys;
    use serde::Serialize;
    use serde_json::{json, Value};
    use std::ffi::CStr;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::Receiver;
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::Duration;
    use tauri::{AppHandle, Manager};

    use crate::shell_transport;

    use super::{signal_wakeup, PlayerCommand};

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

    /// Recursively convert an `mpv_node` to a `serde_json::Value`.
    unsafe fn mpv_node_to_json(node: &sys::mpv_node) -> Value {
        match node.format {
            sys::mpv_format_MPV_FORMAT_NONE => Value::Null,
            sys::mpv_format_MPV_FORMAT_STRING
                | sys::mpv_format_MPV_FORMAT_OSD_STRING => {
                let s = CStr::from_ptr(node.u.string);
                Value::String(s.to_string_lossy().into_owned())
            }
            sys::mpv_format_MPV_FORMAT_FLAG => Value::Bool(node.u.flag != 0),
            sys::mpv_format_MPV_FORMAT_INT64 => json!(node.u.int64),
            sys::mpv_format_MPV_FORMAT_DOUBLE => {
                serde_json::Number::from_f64(node.u.double_)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)
            }
            sys::mpv_format_MPV_FORMAT_NODE_ARRAY => {
                let list = node.u.list;
                if list.is_null() {
                    return Value::Array(Vec::new());
                }
                let list = &*list;
                let mut arr = Vec::with_capacity(list.num as usize);
                for i in 0..list.num as usize {
                    arr.push(mpv_node_to_json(&*list.values.add(i)));
                }
                Value::Array(arr)
            }
            sys::mpv_format_MPV_FORMAT_NODE_MAP => {
                let list = node.u.list;
                if list.is_null() {
                    return Value::Object(serde_json::Map::new());
                }
                let list = &*list;
                let mut map = serde_json::Map::with_capacity(list.num as usize);
                for i in 0..list.num as usize {
                    let key = if !list.keys.is_null() {
                        let k = *list.keys.add(i);
                        if k.is_null() {
                            String::new()
                        } else {
                            CStr::from_ptr(k).to_string_lossy().into_owned()
                        }
                    } else {
                        String::new()
                    };
                    map.insert(key, mpv_node_to_json(&*list.values.add(i)));
                }
                Value::Object(map)
            }
            _ => Value::Null,
        }
    }

    pub fn create(window_handle: isize) -> Result<Mpv, String> {
        let mpv = Mpv::with_initializer(|initializer| -> MpvResult<()> {
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
        .map_err(to_string)?;

        // Cache/demuxer tuning (matching stremio-community-v5 defaults)
        // These must be set AFTER mpv_initialize() — they are not available
        // as pre-init options. Reduces stream load time by limiting the
        // probe/analyze phase and enabling a larger forward cache.
        fn apply_stream_tuning(mpv: &Mpv) {
            let props: &[(&str, &str)] = &[
                ("demuxer-lavf-probesize",      "524288"),
                ("demuxer-lavf-analyzeduration", "0.5"),
                ("demuxer-max-bytes",           "300000000"),
                ("demuxer-max-packets",          "150000000"),
                ("cache",                       "yes"),
                ("cache-pause",                 "no"),
                ("cache-secs",                  "60"),
                ("vd-lavc-threads",             "0"),
                ("ad-lavc-threads",             "0"),
                ("audio-fallback-to-null",      "yes"),
            ];
            for (name, value) in props {
                if let Err(e) = mpv.set_property(name, *value) {
                    eprintln!("[StremioLightning] Failed to set MPV property {name}={value}: {e}");
                }
            }
        }
        apply_stream_tuning(&mpv);

        Ok(mpv)
    }

    /// Run the MPV event loop, driven by a wakeup callback instead of polling.
    ///
    /// The wakeup callback fires from an MPV internal thread whenever new events
    /// are available. It signals a shared `Condvar` that this loop blocks on,
    /// giving **zero-latency** event delivery (vs. the previous 100ms polling).
    /// Commands from the Tauri IPC thread also signal the same condvar so they
    /// are processed immediately rather than waiting for the next poll tick.
    pub fn run_event_loop(
        app: AppHandle,
        mut mpv: Mpv,
        command_receiver: Receiver<PlayerCommand>,
        wakeup: Arc<(Mutex<bool>, Condvar)>,
    ) {
        if let Err(error) = mpv.disable_deprecated_events() {
            eprintln!("Failed to disable deprecated MPV events: {error}");
        }

        // Observe all properties upfront as MPV_FORMAT_NODE.
        for &name in OBSERVED_PROPERTIES {
            if let Err(error) = mpv.observe_property(name, Format::Node, 0) {
                eprintln!("Failed to observe MPV property {name}: {error}");
            }
        }

        // Set the wakeup callback to signal the condvar on new events.
        let wakeup_cb = wakeup.clone();
        mpv.set_wakeup_callback(move || {
            signal_wakeup(&wakeup_cb);
        });

        loop {
            // 1. Drain all pending commands from the Tauri IPC thread.
            for command in command_receiver.try_iter() {
                match command {
                    PlayerCommand::Observe(name) => {
                        // Re-observe triggers a fresh PropertyChange event with the
                        // current value, which the web UI needs to initialize its state.
                        if let Err(error) = mpv.observe_property(&name, Format::Node, 0) {
                            eprintln!("Failed to observe MPV property {name}: {error}");
                        }
                    }
                    PlayerCommand::SetProperty { name, value } => set_property(&mpv, &name, value),
                    PlayerCommand::Command { name, args } => send_command(&mpv, &name, &args),
                }
            }

            // 2. Drain all pending MPV events (non-blocking).
            loop {
                let event = unsafe { *sys::mpv_wait_event(mpv.ctx.as_ptr(), 0.0) };
                if event.event_id == sys::mpv_event_id_MPV_EVENT_NONE {
                    break;
                }

                if event.error < 0 {
                    let error_str = unsafe { CStr::from_ptr(sys::mpv_error_string(event.error)) };
                    eprintln!(
                        "[StremioLightning] MPV event error: {}",
                        error_str.to_string_lossy()
                    );
                }

                match event.event_id {
                    sys::mpv_event_id_MPV_EVENT_PROPERTY_CHANGE => {
                        let prop = unsafe { *(event.data as *mut sys::mpv_event_property) };
                        let name = unsafe { CStr::from_ptr(prop.name) };
                        let name_str = name.to_string_lossy();

                        if prop.format == sys::mpv_format_MPV_FORMAT_NONE || prop.data.is_null() {
                            continue;
                        }

                        if prop.format == sys::mpv_format_MPV_FORMAT_NODE {
                            let node = unsafe { &*(prop.data as *const sys::mpv_node) };
                            let data = unsafe { mpv_node_to_json(node) };

                            if name_str == "pause" {
                                if let Some(paused) = data.as_bool() {
                                    if let Some(state) = app.try_state::<PlayerState>() {
                                        state.is_paused.store(paused, Ordering::Relaxed);
                                    }
                                }
                            }

                            let payload = PlayerPropertyChange {
                                name: name_str.into_owned(),
                                data,
                            };
                            let _ = shell_transport::emit_transport_event(
                                &app,
                                json!(["mpv-prop-change", payload]),
                            );

                            // mpv_free_event_contents is not in the sys bindings,
                            // so we free the node data explicitly.
                            unsafe { sys::mpv_free_node_contents(prop.data as *mut sys::mpv_node) };
                        } else {
                            eprintln!(
                                "[StremioLightning] Unexpected property format {} for {}",
                                prop.format, name_str
                            );
                        }
                    }
                    sys::mpv_event_id_MPV_EVENT_END_FILE => {
                        let ef = unsafe { *(event.data as *mut sys::mpv_event_end_file) };
                        eprintln!(
                            "[StremioLightning] MPV EndFile: reason={} (raw={})",
                            end_reason_string(ef.reason as _),
                            ef.reason
                        );
                        // When a file ends, the player becomes idle (effectively paused).
                        // MPV does NOT fire a "pause" property change on EndFile, so we must
                        // update is_paused here to prevent auto-pause/resume from acting on
                        // a stale "playing" state when no content is active.
                        if let Some(state) = app.try_state::<PlayerState>() {
                            state.is_paused.store(true, Ordering::Relaxed);
                            state.auto_paused_on_unfocus.store(false, Ordering::Relaxed);

                            // Auto-exit PiP mode when playback ends
                            if state.is_pip_mode.load(Ordering::Relaxed) {
                                eprintln!("[StremioLightning] Playback ended, auto-exiting PiP mode");
                                drop(state);
                                let _ = exit_pip_internal(&app);
                            }
                        }
                        let error_msg = if ef.reason == mpv_end_file_reason::Error as u32 {
                            let msg = format!(
                                "MPV playback error (end-file reason={})",
                                ef.reason
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
                            reason: end_reason_string(ef.reason as _).to_string(),
                            error: error_msg,
                        };
                        let _ = shell_transport::emit_transport_event(
                            &app,
                            json!(["mpv-event-ended", payload]),
                        );
                    }
                    sys::mpv_event_id_MPV_EVENT_SHUTDOWN => {
                        // Free the MPV context
                        unsafe { sys::mpv_terminate_destroy(mpv.ctx.as_ptr()) };
                        // Prevent Drop from double-freeing
                        mpv.ctx = std::ptr::NonNull::dangling();
                        return;
                    }
                    _ => {}
                }
            }

            // 3. Wait for the next wakeup signal (from MPV or a command).
            //    A 50ms timeout ensures commands aren't stuck if the signal
            //    is missed, but the common path is instant wakeup.
            {
                let (lock, cvar) = &*wakeup;
                if let Ok(mut flag) = lock.lock() {
                    if !*flag {
                        flag = match cvar.wait_timeout(flag, Duration::from_millis(50)) {
                            Ok((guard, _)) => guard,
                            Err(e) => e.into_inner().0,
                        };
                    }
                    *flag = false;
                }
            }
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

    fn end_reason_string(reason: sys::mpv_end_file_reason) -> &'static str {
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
        let wakeup = Arc::new((Mutex::new(false), Condvar::new()));
        let app_handle = app.clone();

        let wakeup_for_loop = wakeup.clone();
        thread::spawn(move || {
            platform::run_event_loop(app_handle, mpv, command_receiver, wakeup_for_loop);
        });

        *backend = Some(PlayerBackend {
            command_sender,
            wakeup,
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

    let send_result = match method {
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
    };

    if send_result.is_ok() {
        backend.signal();
    }

    send_result
}

pub fn stop_and_hide(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<PlayerState>();
    let backend = state.backend.lock().map_err(|e| e.to_string())?;
    let Some(backend) = backend.as_ref() else {
        return Ok(());
    };

    let result = backend
        .command_sender
        .send(PlayerCommand::Command {
            name: "stop".to_string(),
            args: vec![],
        })
        .map_err(|e| e.to_string());

    if result.is_ok() {
        backend.signal();
    }

    result?;
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
    let result = backend
        .command_sender
        .send(PlayerCommand::SetProperty {
            name: "pause".to_string(),
            value: Value::Bool(pause),
        })
        .is_ok();

    if result {
        backend.signal();
    }

    result
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

    // PiP mode suppresses auto-pause when the setting is enabled
    if state.pip_disables_auto_pause.load(Ordering::Relaxed)
        && state.is_pip_mode.load(Ordering::Relaxed)
    {
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

    // Send the unpause command to MPV
    if send_pause(app, false) {
        // Only clear the flag after a successful resume
        state.auto_paused_on_unfocus.store(false, Ordering::Relaxed);
        eprintln!("[StremioLightning] Auto-resumed native player on focus");
    }
}

/// Internal helper to exit PiP mode without requiring the caller to hold any locks.
/// Used by the MPV event loop when playback ends.
/// Restores decorations, removes always-on-top, restores saved window geometry,
/// and notifies the web UI via the hidePictureInPicture event.
fn exit_pip_internal(app: &AppHandle) -> Result<bool, String> {
    let window = app
        .get_window(MAIN_APP_LABEL)
        .ok_or_else(|| "Main window not found".to_string())?;
    let state = app.state::<PlayerState>();

    let _ = window.set_decorations(true);
    let _ = window.set_always_on_top(false);
    state.is_pip_mode.store(false, Ordering::Relaxed);

    // Restore the saved window geometry (size + position)
    if let Ok(mut geo) = state.pre_pip_geometry.lock() {
        if let Some(saved) = geo.take() {
            let _ = window.set_size(PhysicalSize::new(saved.width, saved.height));
            let _ = window.set_position(PhysicalPosition::new(saved.x, saved.y));
            eprintln!(
                "[StremioLightning] Restored window geometry: {}x{} at ({},{})",
                saved.width, saved.height, saved.x, saved.y
            );
        }
    }

    let _ = shell_transport::emit_transport_event(
        app,
        serde_json::json!(["hidePictureInPicture", {}]),
    );

    eprintln!("[StremioLightning] PiP mode disabled");
    Ok(false)
}

/// Toggle Picture-in-Picture mode.
/// - Enter PiP: saves current window geometry, removes decorations, sets always-on-top,
///   resizes to a compact 16:9 PiP window, and notifies the web UI.
/// - Exit PiP: restores decorations, removes always-on-top, restores saved geometry,
///   and notifies the web UI.
/// PiP can only be entered when the native player backend is active (video playing).
pub fn toggle_pip_mode(app: &AppHandle) -> Result<bool, String> {
    let state = app.state::<PlayerState>();
    let currently_pip = state.is_pip_mode.load(Ordering::Relaxed);

    if currently_pip {
        // ── Exit PiP ──
        exit_pip_internal(app)
    } else {
        // ── Enter PiP ──
        let window = app
            .get_window(MAIN_APP_LABEL)
            .ok_or_else(|| "Main window not found".to_string())?;

        // Only allow PiP when the native player backend is initialized (i.e., video is playing)
        {
            let backend = state.backend.lock().map_err(|e| e.to_string())?;
            if backend.is_none() {
                return Err("Picture-in-Picture is only available while the player is active".to_string());
            }
        }

        // Save current window geometry before resizing
        let current_size = window.inner_size().map_err(|e| e.to_string())?;
        let current_pos = window.outer_position().map_err(|e| e.to_string())?;
        if let Ok(mut geo) = state.pre_pip_geometry.lock() {
            *geo = Some(PrePipGeometry {
                width: current_size.width,
                height: current_size.height,
                x: current_pos.x,
                y: current_pos.y,
            });
            eprintln!(
                "[StremioLightning] Saved window geometry: {}x{} at ({},{})",
                current_size.width, current_size.height, current_pos.x, current_pos.y
            );
        }

        // Remove title bar
        let _ = window.set_decorations(false);

        // Resize to compact PiP dimensions (logical 480×270, 16:9 ratio)
        let scale = window.scale_factor().unwrap_or(1.0);
        let pip_w = (480.0 * scale) as u32;
        let pip_h = (270.0 * scale) as u32;
        let _ = window.set_size(PhysicalSize::new(pip_w, pip_h));

        // Position near the bottom-right of the primary monitor
        if let Ok(monitor) = window.primary_monitor() {
            if let Some(monitor) = monitor {
                let screen_size = monitor.size();
                let screen_pos = monitor.position();
                let margin = (16.0 * scale) as i32;
                let pip_x = screen_pos.x + screen_size.width as i32 - pip_w as i32 - margin;
                let pip_y = screen_pos.y + screen_size.height as i32 - pip_h as i32 - margin;
                let _ = window.set_position(PhysicalPosition::new(pip_x, pip_y));
            }
        }

        // Set always-on-top LAST — on Windows, preceding calls to
        // set_decorations / set_size / set_position can reset the TOPMOST flag.
        let _ = window.set_always_on_top(true);

        state.is_pip_mode.store(true, Ordering::Relaxed);

        // Notify the web UI to switch to compact PiP layout
        let _ = shell_transport::emit_transport_event(
            app,
            serde_json::json!(["showPictureInPicture", {}]),
        );

        eprintln!("[StremioLightning] PiP mode enabled ({}x{})", pip_w, pip_h);
        Ok(true)
    }
}

/// Query whether PiP mode is currently active.
pub fn get_pip_mode(app: &AppHandle) -> bool {
    let state = app.state::<PlayerState>();
    state.is_pip_mode.load(Ordering::Relaxed)
}

/// Re-assert the always-on-top flag for the PiP window.
/// Called when the window loses focus while PiP is active, because
/// Windows can demote a TOPMOST window from the z-order when another
/// window is brought to the foreground.
pub fn reinforce_pip_topmost(app: &AppHandle) {
    let state = app.state::<PlayerState>();
    if !state.is_pip_mode.load(Ordering::Relaxed) {
        return;
    }
    drop(state);

    if let Some(window) = app.get_window(MAIN_APP_LABEL) {
        let _ = window.set_always_on_top(true);
    }
}
