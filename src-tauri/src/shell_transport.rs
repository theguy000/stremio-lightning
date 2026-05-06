use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Webview, Window};
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

struct TransportQueue {
    ready: bool,
    pending: VecDeque<String>,
}

pub struct ShellTransportState {
    bridge_ready: Mutex<bool>,
    bridge_ready_condvar: Condvar,
    queue: Mutex<TransportQueue>,
}

impl Default for ShellTransportState {
    fn default() -> Self {
        Self {
            bridge_ready: Mutex::new(false),
            bridge_ready_condvar: Condvar::new(),
            queue: Mutex::new(TransportQueue {
                ready: false,
                pending: VecDeque::new(),
            }),
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
    let result = if let Ok(ready_guard) = state.bridge_ready.lock() {
        if let Ok((guard, _)) =
            state
                .bridge_ready_condvar
                .wait_timeout_while(ready_guard, timeout, |ready| !*ready)
        {
            *guard
        } else {
            false
        }
    } else {
        false
    };
    result
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
            "win-focus" | "win-set-focus" | "app-focus" => {
                with_main_window(app, Window::set_focus, "focus")
            }
            "win-minimize" => with_main_window(app, Window::minimize, "minimize"),
            "win-maximize" => with_main_window(app, Window::maximize, "maximize"),
            "win-unmaximize" => with_main_window(app, Window::unmaximize, "unmaximize"),
            "win-close" | "app-quit" | "quit" => with_main_window(app, Window::close, "close"),
            "win-show" | "win-restore" => with_main_window(app, Window::show, "show"),
            "win-hide" => with_main_window(app, Window::hide, "hide"),
            "win-dev-tools" => toggle_devtools(app),
            "win-center" => with_main_window(app, Window::center, "center"),
            "win-toggle-fullscreen" => with_main_window(
                app,
                |w| w.set_fullscreen(!w.is_fullscreen()?),
                "toggle fullscreen",
            ),
            "native-player-stop" => player::stop_and_hide(app),
            "win-toggle-pip" => {
                player::toggle_pip_mode(app)?;
                Ok(())
            }
            "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" => {
                if !player::native_player_enabled() {
                    // The web app may send native-player commands as soon as it
                    // detects a desktop shell. On platforms where libmpv is not
                    // implemented yet (currently Linux/macOS), keep the shell
                    // transport alive for streaming-server integration but drop
                    // MPV commands instead of returning noisy errors.
                    return Ok(());
                }
                player::handle_transport(app, &method, data)
            }
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
    let state = if minimized {
        WIN_STATE_MINIMIZED
    } else {
        WIN_STATE_NORMAL
    };

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
            return Err(format!(
                "Unsupported shell transport request type: {request_type}"
            ));
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
    let mut queue = state.queue.lock().map_err(|e| e.to_string())?;
    queue.ready = true;
    Ok(())
}

fn flush_pending_messages(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<ShellTransportState>();
    let mut queue = state.queue.lock().map_err(|e| e.to_string())?;
    let messages: Vec<String> = queue.pending.drain(..).collect();
    drop(queue);

    for message in messages {
        emit_message_now(app, message)?;
    }

    Ok(())
}

fn emit_or_queue_message(app: &AppHandle, message: String) -> Result<(), String> {
    let state = app.state::<ShellTransportState>();
    let mut queue = state.queue.lock().map_err(|e| e.to_string())?;
    if queue.ready {
        drop(queue);
        emit_message_now(app, message)
    } else {
        if queue.pending.len() >= MAX_PENDING_MESSAGES {
            queue.pending.pop_front();
        }
        queue.pending.push_back(message);
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
    let allowed = [
        "http://", "https://", "rtp://", "rtsp://", "ftp://", "ipfs://",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix));

    if !allowed {
        return Err("Rejected non-whitelisted open-external URL".into());
    }

    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Failed to open URL: {e}"))
}

fn main_window(app: &AppHandle) -> Result<Window, String> {
    app.get_window(player::MAIN_APP_LABEL)
        .ok_or_else(|| "Main window not found".to_string())
}

fn with_main_window<F>(app: &AppHandle, f: F, action: &str) -> Result<(), String>
where
    F: FnOnce(&Window) -> tauri::Result<()>,
{
    let window = main_window(app)?;
    f(&window).map_err(|e| format!("Failed to {} main window: {}", action, e))
}

fn main_webview(app: &AppHandle) -> Result<Webview, String> {
    app.get_webview(player::MAIN_APP_LABEL)
        .ok_or_else(|| "Main webview not found".to_string())
}

fn toggle_devtools(app: &AppHandle) -> Result<(), String> {
    let webview = main_webview(app)?;
    if webview.is_devtools_open() {
        webview.close_devtools();
    } else {
        webview.open_devtools();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        handshake_response, parse_request, response_message, ParsedRequest, RPC_TYPE_INIT,
        RPC_TYPE_SIGNAL,
    };
    use serde_json::{json, Value};

    #[test]
    fn parses_handshake_request() {
        let request = parse_request(r#"{"id":0,"type":3}"#).unwrap();
        assert_eq!(request, ParsedRequest::Handshake);
    }

    #[test]
    fn parses_command_request() {
        let request =
            parse_request(r#"{"id":7,"type":6,"args":["mpv-command",["stop"]]}"#).unwrap();
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
        let payload: Value =
            serde_json::from_str(&response_message(json!(["open-media", "stremio://foo"])))
                .unwrap();
        assert_eq!(payload["object"], "transport");
        assert_eq!(payload["type"], RPC_TYPE_SIGNAL);
        assert_eq!(payload["args"][0], "open-media");
    }
}
