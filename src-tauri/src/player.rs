use serde::Serialize;
use serde_json::Value;
use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread;
use tauri::{AppHandle, Manager};

pub const PLAYER_HOST_LABEL: &str = "main";

const FLOAT_PROPERTIES: &[&str] = &[
    "time-pos",
    "duration",
    "volume",
    "speed",
    "sub-pos",
    "sub-scale",
    "sub-delay",
    "cache-buffering-state",
    "mute",
];
const BOOL_PROPERTIES: &[&str] = &[
    "pause",
    "paused-for-cache",
    "seeking",
    "eof-reached",
    "osc",
    "input-default-bindings",
    "input-vo-keyboard",
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
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            backend: Mutex::new(None),
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
    window: tauri::WebviewWindow,
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
        STRING_PROPERTIES,
    };
    use libmpv2::events::{Event, PropertyData};
    use libmpv2::{mpv_end_file_reason, EndFileReason, Format, Mpv, Result as MpvResult};
    use serde::Serialize;
    use serde_json::{json, Value};
    use std::sync::mpsc::Receiver;
    use tauri::AppHandle;

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
                    let payload = PlayerEnded {
                        reason: end_reason_string(reason).to_string(),
                        error: if reason == mpv_end_file_reason::Error {
                            Some(PlayerEndedError {
                                message: "Playback error".to_string(),
                                critical: true,
                            })
                        } else {
                            None
                        },
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
            eprintln!("Failed to execute MPV command {name}: {error}");
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
                    serde_json::from_str::<Value>(&value).ok().or(Some(Value::String(value)))
                } else {
                    Some(Value::String(value))
                }
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

    let state = app.state::<PlayerState>();
    let mut backend = state.backend.lock().map_err(|e| e.to_string())?;
    if backend.is_some() {
        return Ok(());
    }

    let main_window = app
        .get_webview_window("main")
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

    Ok(())
}

pub fn status(app: &AppHandle) -> NativePlayerStatus {
    let enabled = native_player_enabled();
    let (initialized, host_window_label, host_window_visible) = match app.state::<PlayerState>().backend.lock() {
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

pub fn sync_with_main_window(app: &AppHandle) -> Result<(), String> {
    let _ = app;
    Ok(())
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
                eprintln!(
                    "[StremioLightning] MPV loadfile -> {:?}",
                    values
                );
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