use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

use crate::player;

pub const SHELL_TRANSPORT_EVENT: &str = "shell-transport-message";
const TRANSPORT_OBJECT: &str = "transport";
const RPC_TYPE_INIT: u8 = 3;
const RPC_TYPE_SIGNAL: u8 = 1;
const RPC_TYPE_INVOKE_METHOD: u8 = 6;
const WIN_STATE_NORMAL: u32 = 8;
const WIN_STATE_MINIMIZED: u32 = 9;
const MAX_PENDING_MESSAGES: usize = 512;

pub struct ShellTransportState {
    bridge_ready: Mutex<bool>,
    bridge_ready_condvar: Condvar,
    transport_ready: Mutex<bool>,
    pending_messages: Mutex<VecDeque<String>>,
}

impl Default for ShellTransportState {
    fn default() -> Self {
        Self {
            bridge_ready: Mutex::new(false),
            bridge_ready_condvar: Condvar::new(),
            transport_ready: Mutex::new(false),
            pending_messages: Mutex::new(VecDeque::new()),
        }
    }
}

#[derive(Deserialize, Debug)]
struct RpcRequest {
    id: u64,
    #[serde(rename = "type")]
    request_type: Option<u8>,
    args: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct RpcResponseDataTransport {
    properties: Vec<Vec<String>>,
    signals: Vec<String>,
    methods: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct RpcResponseData {
    transport: RpcResponseDataTransport,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
struct RpcResponse {
    id: u64,
    object: String,
    #[serde(rename = "type")]
    response_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<RpcResponseData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<Value>,
}

#[derive(Debug, PartialEq)]
enum ParsedRequest {
    Handshake,
    Command { method: String, data: Option<Value> },
}

pub fn notify_bridge_ready(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<ShellTransportState>();
    let mut ready = state.bridge_ready.lock().map_err(|e| e.to_string())?;
    *ready = true;
    state.bridge_ready_condvar.notify_all();
    Ok(())
}

pub fn wait_until_bridge_ready(app: &AppHandle, timeout: Duration) -> bool {
    let state = app.state::<ShellTransportState>();
    let mut ready = match state.bridge_ready.lock() {
        Ok(ready) => ready,
        Err(_) => return false,
    };

    if *ready {
        return true;
    }

    let deadline = Instant::now() + timeout;
    loop {
        let now = Instant::now();
        if now >= deadline {
            return *ready;
        }

        let remaining = deadline.saturating_duration_since(now);
        let result = state.bridge_ready_condvar.wait_timeout(ready, remaining);
        let (next_ready, _) = match result {
            Ok(result) => result,
            Err(_) => return false,
        };
        ready = next_ready;

        if *ready {
            return true;
        }
    }
}

pub fn handle_message(app: &AppHandle, message: &str) -> Result<(), String> {
    match parse_request(message)? {
        ParsedRequest::Handshake => emit_message_now(app, handshake_response()),
        ParsedRequest::Command { method, data } => match method.as_str() {
            "app-ready" => {
                mark_transport_ready(app)?;
                emit_window_visibility_change(app)?;
                emit_window_state_change(app)?;
                flush_pending_messages(app)
            }
            "app-error" => {
                mark_transport_ready(app)?;
                if let Some(payload) = data {
                    eprintln!("Web app error: {payload}");
                }
                Ok(())
            }
            "win-set-visibility" => apply_window_visibility(app, data.as_ref()),
            "open-external" => {
                if let Some(Value::String(url)) = data {
                    open_external_if_allowed(app, &url)
                } else {
                    Err("Invalid open-external payload".into())
                }
            }
            "quit" => close_main_window(app),
            "mpv-command" | "mpv-observe-prop" | "mpv-set-prop" => {
                player::handle_transport(app, &method, data)
            }
            "win-focus" | "win-set-focus" | "app-focus" => focus_main_window(app),
            "win-minimize" => minimize_main_window(app),
            "win-maximize" => maximize_main_window(app),
            "win-unmaximize" => unmaximize_main_window(app),
            "win-close" | "app-quit" => close_main_window(app),
            "win-show" | "win-restore" => show_main_window(app),
            "win-hide" => hide_main_window(app),
            "win-dev-tools" => toggle_devtools(app),
            "win-center" => center_main_window(app),
            "win-toggle-fullscreen" => toggle_main_fullscreen(app),
            "native-player-stop" => player::stop_and_hide(app),
            other => {
                eprintln!("Unsupported shell transport method: {other}");
                Ok(())
            }
        },
    }
}

pub fn emit_transport_event(app: &AppHandle, args: Value) -> Result<(), String> {
    emit_or_queue_message(app, response_message(args))
}

pub fn emit_window_visibility_change(app: &AppHandle) -> Result<(), String> {
    let window = main_window(app)?;
    let visible = window.is_visible().map_err(|e| e.to_string())?;
    let fullscreen = window.is_fullscreen().map_err(|e| e.to_string())?;

    emit_or_queue_message(
        app,
        response_message(json!([
            "win-visibility-changed",
            {
                "visible": visible,
                "visibility": u32::from(visible),
                "isFullscreen": fullscreen,
            }
        ])),
    )
}

pub fn emit_window_state_change(app: &AppHandle) -> Result<(), String> {
    let window = main_window(app)?;
    let minimized = window.is_minimized().map_err(|e| e.to_string())?;
    let state = if minimized { WIN_STATE_MINIMIZED } else { WIN_STATE_NORMAL };

    emit_or_queue_message(
        app,
        response_message(json!([
            "win-state-changed",
            {
                "state": state,
            }
        ])),
    )
}

pub fn enqueue_open_media(app: &AppHandle, url: String) -> Result<(), String> {
    emit_or_queue_message(app, response_message(json!(["open-media", url])))
}

fn parse_request(message: &str) -> Result<ParsedRequest, String> {
    let request: RpcRequest = serde_json::from_str(message)
        .map_err(|e| format!("Failed to parse shell transport message: {e}"))?;

    if request.id == 0 || request.request_type == Some(RPC_TYPE_INIT) {
        return Ok(ParsedRequest::Handshake);
    }

    if let Some(request_type) = request.request_type {
        if request_type != RPC_TYPE_INVOKE_METHOD {
            return Err(format!("Unsupported shell transport request type: {request_type}"));
        }
    }

    let args = request
        .args
        .and_then(|value| value.as_array().cloned())
        .ok_or_else(|| "Missing shell transport args".to_string())?;
    let method = args
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing shell transport method".to_string())?
        .to_string();
    let data = args.get(1).cloned();

    Ok(ParsedRequest::Command { method, data })
}

fn handshake_response() -> String {
    serde_json::to_string(&RpcResponse {
        id: 0,
        object: TRANSPORT_OBJECT.to_string(),
        response_type: RPC_TYPE_INIT,
        data: Some(RpcResponseData {
            transport: RpcResponseDataTransport {
                properties: vec![
                    vec![],
                    vec![
                        String::new(),
                        "shellVersion".to_string(),
                        String::new(),
                        env!("CARGO_PKG_VERSION").to_string(),
                    ],
                ],
                signals: vec![],
                methods: vec![vec!["onEvent".to_string()]],
            },
        }),
        ..Default::default()
    })
    .expect("failed to serialize handshake response")
}

fn response_message(args: Value) -> String {
    serde_json::to_string(&RpcResponse {
        id: 1,
        object: TRANSPORT_OBJECT.to_string(),
        response_type: RPC_TYPE_SIGNAL,
        args: Some(args),
        ..Default::default()
    })
    .expect("failed to serialize transport response")
}

fn mark_transport_ready(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<ShellTransportState>();
    let mut ready = state.transport_ready.lock().map_err(|e| e.to_string())?;
    *ready = true;
    Ok(())
}

fn flush_pending_messages(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<ShellTransportState>();
    let mut pending = state.pending_messages.lock().map_err(|e| e.to_string())?;
    let messages: Vec<String> = pending.drain(..).collect();
    drop(pending);

    for message in messages {
        emit_message_now(app, message)?;
    }

    Ok(())
}

fn emit_or_queue_message(app: &AppHandle, message: String) -> Result<(), String> {
    let state = app.state::<ShellTransportState>();
    let ready = *state.transport_ready.lock().map_err(|e| e.to_string())?;
    if ready {
        emit_message_now(app, message)
    } else {
        let mut pending = state.pending_messages.lock().map_err(|e| e.to_string())?;
        if pending.len() >= MAX_PENDING_MESSAGES {
            pending.pop_front();
        }
        pending.push_back(message);
        Ok(())
    }
}

fn emit_message_now(app: &AppHandle, message: String) -> Result<(), String> {
    app.emit(SHELL_TRANSPORT_EVENT, message)
        .map_err(|e| format!("Failed to emit shell transport message: {e}"))
}

fn apply_window_visibility(app: &AppHandle, data: Option<&Value>) -> Result<(), String> {
    let window = main_window(app)?;

    if let Some(fullscreen) = data
        .and_then(|value| value.get("fullscreen"))
        .and_then(Value::as_bool)
    {
        window
            .set_fullscreen(fullscreen)
            .map_err(|e| format!("Failed to set fullscreen: {e}"))?;
    }

    Ok(())
}

fn open_external_if_allowed(app: &AppHandle, url: &str) -> Result<(), String> {
    let lower = url.to_lowercase();
    let allowed = ["http://", "https://", "rtp://", "rtsp://", "ftp://", "ipfs://"]
        .iter()
        .any(|prefix| lower.starts_with(prefix));

    if !allowed {
        return Err("Rejected non-whitelisted open-external URL".into());
    }

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Failed to open URL: {e}"))
}

fn main_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    app.get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())
}

fn close_main_window(app: &AppHandle) -> Result<(), String> {
    main_window(app)?.close().map_err(|e| format!("Failed to close main window: {e}"))
}

fn focus_main_window(app: &AppHandle) -> Result<(), String> {
    main_window(app)?.set_focus().map_err(|e| format!("Failed to focus main window: {e}"))
}

fn minimize_main_window(app: &AppHandle) -> Result<(), String> {
    main_window(app)?.minimize().map_err(|e| format!("Failed to minimize main window: {e}"))
}

fn maximize_main_window(app: &AppHandle) -> Result<(), String> {
    main_window(app)?.maximize().map_err(|e| format!("Failed to maximize main window: {e}"))
}

fn unmaximize_main_window(app: &AppHandle) -> Result<(), String> {
    main_window(app)?.unmaximize().map_err(|e| format!("Failed to unmaximize main window: {e}"))
}

fn show_main_window(app: &AppHandle) -> Result<(), String> {
    main_window(app)?.show().map_err(|e| format!("Failed to show main window: {e}"))
}

fn hide_main_window(app: &AppHandle) -> Result<(), String> {
    main_window(app)?.hide().map_err(|e| format!("Failed to hide main window: {e}"))
}

fn center_main_window(app: &AppHandle) -> Result<(), String> {
    main_window(app)?.center().map_err(|e| format!("Failed to center main window: {e}"))
}

fn toggle_main_fullscreen(app: &AppHandle) -> Result<(), String> {
    let window = main_window(app)?;
    let fullscreen = window.is_fullscreen().map_err(|e| e.to_string())?;
    window
        .set_fullscreen(!fullscreen)
        .map_err(|e| format!("Failed to toggle fullscreen: {e}"))
}

fn toggle_devtools(app: &AppHandle) -> Result<(), String> {
    let window = main_window(app)?;
    if window.is_devtools_open() {
        window.close_devtools();
    } else {
        window.open_devtools();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{handshake_response, parse_request, response_message, ParsedRequest, RPC_TYPE_INIT, RPC_TYPE_SIGNAL};
    use serde_json::{json, Value};

    #[test]
    fn parses_handshake_request() {
        let request = parse_request(r#"{"id":0,"type":3}"#).unwrap();
        assert_eq!(request, ParsedRequest::Handshake);
    }

    #[test]
    fn parses_command_request() {
        let request = parse_request(r#"{"id":7,"type":6,"args":["mpv-command",["stop"]]}"#).unwrap();
        assert_eq!(
            request,
            ParsedRequest::Command {
                method: "mpv-command".to_string(),
                data: Some(json!(["stop"])),
            }
        );
    }

    #[test]
    fn serializes_handshake_shape() {
        let payload: Value = serde_json::from_str(&handshake_response()).unwrap();
        assert_eq!(payload["object"], "transport");
        assert_eq!(payload["type"], RPC_TYPE_INIT);
        assert_eq!(payload["data"]["transport"]["methods"][0][0], "onEvent");
    }

    #[test]
    fn serializes_event_shape() {
        let payload: Value = serde_json::from_str(&response_message(json!(["open-media", "stremio://foo"]))).unwrap();
        assert_eq!(payload["object"], "transport");
        assert_eq!(payload["type"], RPC_TYPE_SIGNAL);
        assert_eq!(payload["args"][0], "open-media");
    }
}