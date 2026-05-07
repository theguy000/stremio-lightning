use crate::player::WindowsPlayer;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use stremio_lightning_core::host_api::{self, HostEvent, ParsedRequest};

pub const SHELL_TRANSPORT_EVENT: &str = "shell-transport-message";

#[derive(Debug, Clone, PartialEq)]
pub struct HostEventRecord {
    pub event: String,
    pub payload: Value,
}

#[derive(Debug, Default)]
struct ListenerRegistry {
    next_id: u64,
    listeners: HashMap<u64, String>,
    emitted: Vec<HostEventRecord>,
}

impl ListenerRegistry {
    fn listen_with_id(&mut self, id: u64, event: impl Into<String>) {
        self.next_id = self.next_id.max(id);
        self.listeners.insert(id, event.into());
    }

    fn unlisten(&mut self, id: u64) {
        self.listeners.remove(&id);
    }

    fn emit(&mut self, event: impl Into<String>, payload: Value) {
        let event = event.into();
        if self.listeners.values().any(|listener| listener == &event) {
            self.emitted.push(HostEventRecord { event, payload });
        }
    }

    fn drain_emitted(&mut self) -> Vec<HostEventRecord> {
        std::mem::take(&mut self.emitted)
    }
}

#[derive(Debug, Deserialize)]
pub struct WindowsIpcRequest {
    pub id: u64,
    pub kind: String,
    pub payload: Option<Value>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind")]
pub enum WindowsIpcOutbound {
    #[serde(rename = "response")]
    Response { id: u64, ok: bool, value: Value },
    #[serde(rename = "event")]
    Event { event: String, payload: Value },
}

pub struct WindowsHost {
    player: Mutex<WindowsPlayer>,
    listeners: Mutex<ListenerRegistry>,
    package_version: &'static str,
}

impl Default for WindowsHost {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

impl WindowsHost {
    pub fn new(package_version: &'static str) -> Self {
        Self {
            player: Mutex::default(),
            listeners: Mutex::default(),
            package_version,
        }
    }

    pub fn dispatch_ipc_message(&self, raw: &str) -> Vec<WindowsIpcOutbound> {
        let response = serde_json::from_str::<WindowsIpcRequest>(raw)
            .map_err(|error| format!("Invalid Windows WebView2 IPC message: {error}"))
            .and_then(|request| {
                let id = request.id;
                self.dispatch_windows_ipc(&request.kind, request.payload)
                    .map(|value| (id, true, value))
                    .or_else(|error| Ok((id, false, json!({ "message": error }))))
            });

        let mut outbound = match response {
            Ok((id, ok, value)) => vec![WindowsIpcOutbound::Response { id, ok, value }],
            Err(error) => vec![WindowsIpcOutbound::Event {
                event: "windows-ipc-error".to_string(),
                payload: json!({ "message": error }),
            }],
        };

        outbound.extend(
            self.drain_all_emitted_events()
                .unwrap_or_default()
                .into_iter()
                .map(WindowsIpcOutbound::from),
        );
        outbound
    }

    #[cfg(windows)]
    pub fn initialize_native_player(
        &self,
        hwnd: windows::Win32::Foundation::HWND,
        notifier: crate::window::UiThreadNotifier,
    ) -> Result<(), String> {
        self.player
            .lock()
            .map_err(|_| "Windows player lock poisoned".to_string())?
            .initialize(hwnd, notifier)
    }

    pub fn drain_ipc_events(&self) -> Vec<WindowsIpcOutbound> {
        self.drain_all_emitted_events()
            .unwrap_or_default()
            .into_iter()
            .map(WindowsIpcOutbound::from)
            .collect()
    }

    pub fn dispatch_windows_ipc(
        &self,
        kind: &str,
        payload: Option<Value>,
    ) -> Result<Value, String> {
        match kind {
            "invoke" => {
                let payload: InvokeIpcPayload = parse_payload(kind, payload)?;
                self.invoke(&payload.command, payload.payload)
            }
            "listen" => {
                let payload: ListenIpcPayload = parse_payload(kind, payload)?;
                self.listen_with_id(payload.id, payload.event)?;
                Ok(Value::Null)
            }
            "unlisten" => {
                let payload: UnlistenIpcPayload = parse_payload(kind, payload)?;
                self.unlisten(payload.id)?;
                Ok(Value::Null)
            }
            "window.minimize"
            | "window.toggleMaximize"
            | "window.close"
            | "window.startDragging" => Ok(Value::Null),
            "window.isMaximized" | "window.isFullscreen" => Ok(json!(false)),
            "window.setFullscreen" => {
                let payload: FullscreenIpcPayload = parse_payload(kind, payload)?;
                self.emit_window_fullscreen_changed(payload.fullscreen)?;
                Ok(Value::Null)
            }
            "webview.setZoom" => {
                let payload: ZoomIpcPayload = parse_payload(kind, payload)?;
                if !payload.level.is_finite() || payload.level <= 0.0 {
                    return Err("Invalid webview zoom level".to_string());
                }
                Ok(Value::Null)
            }
            other => Err(format!("Unsupported Windows IPC kind: {other}")),
        }
    }

    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        match command {
            "init" => Ok(json!({ "platform": "windows", "shell": "webview2" })),
            "get_native_player_status" => Ok(serde_json::to_value(
                self.player
                    .lock()
                    .map_err(|_| "Windows player lock poisoned".to_string())?
                    .status(),
            )
            .map_err(|error| error.to_string())?),
            "shell_transport_send" => {
                let message = payload
                    .as_ref()
                    .and_then(|value| value.get("message"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing shell_transport_send message".to_string())?;
                self.handle_shell_transport_message(message)?;
                Ok(Value::Null)
            }
            "shell_bridge_ready" => Ok(Value::Null),
            "open_external_url" => {
                let url = payload
                    .as_ref()
                    .and_then(|value| value.get("url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing open_external_url url".to_string())?;
                validate_external_url(url)?;
                Ok(Value::Null)
            }
            "get_streaming_server_status" => Ok(json!(false)),
            "start_streaming_server" | "stop_streaming_server" | "restart_streaming_server" => {
                Ok(Value::Null)
            }
            "get_plugins" | "get_themes" => Ok(json!([])),
            "get_registry" => Ok(json!({ "plugins": [], "themes": [] })),
            "get_registered_settings" => Ok(Value::Null),
            "get_setting" => Ok(Value::Null),
            "save_setting" | "register_settings" => Ok(Value::Null),
            "download_mod" | "delete_mod" | "get_mod_content" | "check_mod_updates" => Err(
                format!("Windows host command is not implemented before mods storage milestone: {command}"),
            ),
            "toggle_devtools" | "start_discord_rpc" | "stop_discord_rpc" | "update_discord_activity" => {
                Ok(Value::Null)
            }
            "check_app_update" => Ok(Value::Null),
            "set_auto_pause" | "set_pip_disables_auto_pause" => Ok(Value::Null),
            "get_auto_pause" | "get_pip_disables_auto_pause" => Ok(json!(false)),
            "toggle_pip" => Ok(Value::Null),
            "get_pip_mode" => Ok(json!(false)),
            other => Err(format!("Unsupported Windows host command: {other}")),
        }
    }

    fn handle_shell_transport_message(&self, message: &str) -> Result<(), String> {
        match host_api::parse_request(message)? {
            ParsedRequest::Handshake => {
                self.emit_transport_message(host_api::handshake_response(self.package_version))
            }
            ParsedRequest::Command { method, data } => match method.as_str() {
                "app-ready" | "app-error" => Ok(()),
                "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                    self.player
                        .lock()
                        .map_err(|_| "Windows player lock poisoned".to_string())?
                        .handle_transport(&method, data)?;
                    Ok(())
                }
                other => Err(format!("Unsupported shell transport method: {other}")),
            },
        }
    }

    fn listen_with_id(&self, id: u64, event: impl Into<String>) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .listen_with_id(id, event);
        Ok(())
    }

    fn unlisten(&self, id: u64) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .unlisten(id);
        Ok(())
    }

    fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .drain_emitted())
    }

    fn drain_all_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        self.emit_player_events()?;
        self.drain_emitted_events()
    }

    fn emit_player_events(&self) -> Result<(), String> {
        let events = self
            .player
            .lock()
            .map_err(|_| "Windows player lock poisoned".to_string())?
            .drain_events();

        for event in events {
            self.emit_transport_message(host_api::response_message(event.transport_args()))?;
        }
        Ok(())
    }

    fn emit_transport_message(&self, message: String) -> Result<(), String> {
        self.emit_event(SHELL_TRANSPORT_EVENT, json!(message))
    }

    fn emit_window_fullscreen_changed(&self, fullscreen: bool) -> Result<(), String> {
        self.emit_host_event(HostEvent::WindowFullscreenChanged, json!(fullscreen))
    }

    fn emit_host_event(&self, event: HostEvent, payload: Value) -> Result<(), String> {
        let event = serde_json::to_value(event)
            .map_err(|e| format!("Failed to serialize host event: {e}"))?
            .as_str()
            .ok_or_else(|| "Host event did not serialize to a string".to_string())?
            .to_string();
        self.emit_event(event, payload)
    }

    fn emit_event(&self, event: impl Into<String>, payload: Value) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .emit(event, payload);
        Ok(())
    }
}

impl From<HostEventRecord> for WindowsIpcOutbound {
    fn from(record: HostEventRecord) -> Self {
        Self::Event {
            event: record.event,
            payload: record.payload,
        }
    }
}

#[derive(Debug, Deserialize)]
struct InvokeIpcPayload {
    command: String,
    payload: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ListenIpcPayload {
    id: u64,
    event: String,
}

#[derive(Debug, Deserialize)]
struct UnlistenIpcPayload {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct FullscreenIpcPayload {
    fullscreen: bool,
}

#[derive(Debug, Deserialize)]
struct ZoomIpcPayload {
    level: f64,
}

fn parse_payload<T>(command: &str, payload: Option<Value>) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(payload.unwrap_or(Value::Null))
        .map_err(|e| format!("Invalid {command} payload: {e}"))
}

fn validate_external_url(url: &str) -> Result<(), String> {
    let lower = url.to_lowercase();
    let allowed = [
        "http://", "https://", "rtp://", "rtsp://", "ftp://", "ipfs://",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix));

    if allowed {
        Ok(())
    } else {
        Err("Rejected non-whitelisted open_external_url URL".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exposes_webview2_init_contract() {
        assert_eq!(
            WindowsHost::default().invoke("init", None).unwrap(),
            json!({ "platform": "windows", "shell": "webview2" })
        );
    }

    #[test]
    fn handles_shell_transport_handshake() {
        let host = WindowsHost::new("0.1.0");
        host.dispatch_windows_ipc(
            "listen",
            Some(json!({ "id": 7, "event": "shell-transport-message" })),
        )
        .unwrap();
        host.invoke(
            "shell_transport_send",
            Some(json!({ "message": r#"{"id":0,"type":3}"# })),
        )
        .unwrap();
        let response = host.drain_emitted_events().unwrap().remove(0).payload;
        assert_eq!(
            serde_json::from_str::<Value>(response.as_str().unwrap()).unwrap()["type"],
            json!(3)
        );
    }

    #[test]
    fn dispatches_request_response_ipc() {
        let host = WindowsHost::default();
        let outbound = host.dispatch_ipc_message(
            r#"{"id":42,"kind":"invoke","payload":{"command":"init","payload":null}}"#,
        );

        assert_eq!(
            outbound[0],
            WindowsIpcOutbound::Response {
                id: 42,
                ok: true,
                value: json!({ "platform": "windows", "shell": "webview2" }),
            }
        );
    }

    #[test]
    fn returns_structured_error_for_invalid_command() {
        let host = WindowsHost::default();
        let outbound = host.dispatch_ipc_message(
            r#"{"id":9,"kind":"invoke","payload":{"command":"missing","payload":null}}"#,
        );

        assert_eq!(
            outbound[0],
            WindowsIpcOutbound::Response {
                id: 9,
                ok: false,
                value: json!({ "message": "Unsupported Windows host command: missing" }),
            }
        );
    }

    #[test]
    fn listener_registration_controls_events() {
        let host = WindowsHost::default();
        host.dispatch_windows_ipc(
            "listen",
            Some(json!({ "id": 2, "event": "window-fullscreen-changed" })),
        )
        .unwrap();

        let outbound = host.dispatch_ipc_message(
            r#"{"id":3,"kind":"window.setFullscreen","payload":{"fullscreen":true}}"#,
        );
        assert_eq!(outbound.len(), 2);
        assert_eq!(
            outbound[1],
            WindowsIpcOutbound::Event {
                event: "window-fullscreen-changed".to_string(),
                payload: json!(true),
            }
        );
    }

    #[test]
    fn matches_json_host_contract_fixture() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/host_contract.json")).unwrap();
        let host = WindowsHost::default();

        let init = host.dispatch_ipc_message(&fixture["invokeInitRequest"].to_string());
        assert_eq!(
            serde_json::to_value(&init[0]).unwrap(),
            fixture["invokeInitResponse"]
        );

        let invalid = host.dispatch_ipc_message(&fixture["invalidCommandRequest"].to_string());
        assert_eq!(
            serde_json::to_value(&invalid[0]).unwrap(),
            fixture["invalidCommandResponse"]
        );
    }
}
