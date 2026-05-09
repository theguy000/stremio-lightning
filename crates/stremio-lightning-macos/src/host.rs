use crate::player::{NativePlayerStatus, PlayerBackend};
use crate::streaming_server::{ProcessSpawner, StreamingServer};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;

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
    fn listen(&mut self, event: impl Into<String>) -> u64 {
        self.next_id += 1;
        self.listeners.insert(self.next_id, event.into());
        self.next_id
    }

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

pub struct MacosHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    player: B,
    streaming_server: StreamingServer<P>,
    listeners: Mutex<ListenerRegistry>,
}

pub type Host<B, P> = MacosHost<B, P>;

impl<B, P> MacosHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub fn new(player: B, streaming_server: StreamingServer<P>) -> Self {
        Self {
            player,
            streaming_server,
            listeners: Mutex::default(),
        }
    }

    pub fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.start()
    }

    pub fn shutdown(&self) -> Result<(), String> {
        self.player.stop().ok();
        self.streaming_server.stop()
    }

    pub fn native_player_status(&self) -> NativePlayerStatus {
        self.player.status()
    }

    pub fn invoke(&self, command: &str, _payload: Option<Value>) -> Result<Value, String> {
        match command {
            "init" => Ok(json!({
                "platform": "macos",
                "shellVersion": env!("CARGO_PKG_VERSION"),
                "nativePlayer": self.native_player_status(),
                "streamingServerRunning": self.streaming_server.is_running(),
            })),
            "shell_bridge_ready" => Ok(Value::Null),
            other => Err(format!("Unsupported macOS host command: {other}")),
        }
    }

    pub fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
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
            other => Err(format!("Unsupported macOS IPC kind: {other}")),
        }
    }

    pub fn listen(&self, event: impl Into<String>) -> Result<u64, String> {
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .listen(event))
    }

    pub fn listen_with_id(&self, id: u64, event: impl Into<String>) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .listen_with_id(id, event);
        Ok(())
    }

    pub fn unlisten(&self, id: u64) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .unlisten(id);
        Ok(())
    }

    pub fn emit_window_fullscreen_changed(&self, fullscreen: bool) -> Result<(), String> {
        self.emit_event(
            "window-fullscreen-changed",
            json!({ "fullscreen": fullscreen }),
        )
    }

    pub fn emit_native_player_property_changed(
        &self,
        name: impl Into<String>,
        data: Value,
    ) -> Result<(), String> {
        self.emit_event(
            SHELL_TRANSPORT_EVENT,
            json!({
                "type": "mpv-prop-change",
                "name": name.into(),
                "data": data,
            }),
        )
    }

    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .drain_emitted())
    }

    fn emit_event(&self, event: impl Into<String>, payload: Value) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .emit(event, payload);
        Ok(())
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

fn parse_payload<T>(label: &str, payload: Option<Value>) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(payload.unwrap_or(Value::Null))
        .map_err(|e| format!("Invalid macOS IPC payload for {label}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::FakePlayerBackend;
    use crate::streaming_server::{FakeProcessSpawner, StreamingServer};

    fn test_host() -> Host<FakePlayerBackend, FakeProcessSpawner> {
        Host::new(
            FakePlayerBackend::initialized(),
            StreamingServer::new(FakeProcessSpawner::default()),
        )
    }

    #[test]
    fn dispatch_ipc_routes_invoke_to_host() {
        let host = test_host();
        let value = host
            .dispatch_ipc("invoke", Some(json!({"command": "init"})))
            .unwrap();
        assert_eq!(value["platform"], "macos");
        assert_eq!(value["nativePlayer"]["backend"], "fake");
    }

    #[test]
    fn dispatch_ipc_validates_payload_shape() {
        let host = test_host();
        let error = host
            .dispatch_ipc("listen", Some(json!({"id": 1})))
            .unwrap_err();
        assert!(error.contains("Invalid macOS IPC payload for listen"));
    }

    #[test]
    fn unsupported_commands_return_errors() {
        let host = test_host();
        assert_eq!(
            host.dispatch_ipc("invoke", Some(json!({"command": "missing"})))
                .unwrap_err(),
            "Unsupported macOS host command: missing"
        );
        assert_eq!(
            host.dispatch_ipc("unknown.kind", None).unwrap_err(),
            "Unsupported macOS IPC kind: unknown.kind"
        );
    }

    #[test]
    fn listeners_gate_drained_events() {
        let host = test_host();
        host.emit_native_player_property_changed("pause", json!(true))
            .unwrap();
        assert!(host.drain_emitted_events().unwrap().is_empty());

        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 7, "event": SHELL_TRANSPORT_EVENT})),
        )
        .unwrap();
        host.emit_native_player_property_changed("pause", json!(false))
            .unwrap();
        assert_eq!(host.drain_emitted_events().unwrap().len(), 1);

        host.dispatch_ipc("unlisten", Some(json!({"id": 7})))
            .unwrap();
        host.emit_native_player_property_changed("pause", json!(true))
            .unwrap();
        assert!(host.drain_emitted_events().unwrap().is_empty());
    }
}
