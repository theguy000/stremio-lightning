use crate::player::{handle_transport, NativePlayerStatus, PlayerBackend};
use crate::streaming_server::{ProcessSpawner, StreamingServer};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use stremio_lightning_core::host_api::{self, ParsedRequest};

pub const SHELL_TRANSPORT_EVENT: &str = "shell-transport-message";
const MAX_PENDING_MESSAGES: usize = 512;

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

    fn unlisten(&mut self, id: u64) {
        self.listeners.remove(&id);
    }

    fn emit(&mut self, event: impl Into<String>, payload: Value) {
        let event = event.into();
        if self.listeners.values().any(|listener| listener == &event) {
            self.emitted.push(HostEventRecord { event, payload });
        }
    }
}

#[derive(Debug)]
struct TransportQueue {
    bridge_ready: bool,
    transport_ready: bool,
    pending: VecDeque<String>,
}

impl Default for TransportQueue {
    fn default() -> Self {
        Self {
            bridge_ready: false,
            transport_ready: false,
            pending: VecDeque::new(),
        }
    }
}

pub struct LinuxHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    player: B,
    streaming_server: StreamingServer<P>,
    listeners: Mutex<ListenerRegistry>,
    transport: Mutex<TransportQueue>,
}

impl<B, P> LinuxHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub fn new(player: B, streaming_server: StreamingServer<P>) -> Self {
        Self {
            player,
            streaming_server,
            listeners: Mutex::default(),
            transport: Mutex::default(),
        }
    }

    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        match command {
            "open_external_url" => {
                let url = payload
                    .as_ref()
                    .and_then(|value| value.get("url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing open_external_url url".to_string())?;
                validate_external_url(url)?;
                Ok(Value::Null)
            }
            "start_streaming_server" => {
                self.streaming_server.start()?;
                self.emit_event("server-started", Value::Null)?;
                Ok(Value::Null)
            }
            "get_streaming_server_status" => Ok(json!(self.streaming_server.is_running())),
            "shell_transport_send" => {
                let message = payload
                    .as_ref()
                    .and_then(|value| value.get("message"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing shell_transport_send message".to_string())?;
                self.handle_shell_transport_message(message)?;
                Ok(Value::Null)
            }
            "shell_bridge_ready" => {
                self.mark_bridge_ready()?;
                Ok(Value::Null)
            }
            "get_native_player_status" => Ok(serde_json::to_value(self.native_player_status())
                .map_err(|e| format!("Failed to serialize player status: {e}"))?),
            other => Err(format!("Unsupported Linux host command: {other}")),
        }
    }

    pub fn listen(&self, event: impl Into<String>) -> Result<u64, String> {
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .listen(event))
    }

    pub fn unlisten(&self, id: u64) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .unlisten(id);
        Ok(())
    }

    pub fn emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .emitted
            .clone())
    }

    pub fn native_player_status(&self) -> NativePlayerStatus {
        self.player.status()
    }

    pub fn emit_transport_event(&self, args: Value) -> Result<(), String> {
        self.emit_or_queue_transport_message(host_api::response_message(args))
    }

    fn handle_shell_transport_message(&self, message: &str) -> Result<(), String> {
        match host_api::parse_request(message)? {
            ParsedRequest::Handshake => self.emit_transport_message_now(
                host_api::handshake_response(env!("CARGO_PKG_VERSION")),
            ),
            ParsedRequest::Command { method, data } => match method.as_str() {
                "app-ready" => {
                    self.mark_transport_ready()?;
                    self.flush_pending_transport_messages()
                }
                "app-error" => {
                    self.mark_transport_ready()?;
                    Ok(())
                }
                "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                    handle_transport(&self.player, &method, data)
                }
                other => Err(format!("Unsupported shell transport method: {other}")),
            },
        }
    }

    fn mark_bridge_ready(&self) -> Result<(), String> {
        self.transport
            .lock()
            .map_err(|e| e.to_string())?
            .bridge_ready = true;
        Ok(())
    }

    fn mark_transport_ready(&self) -> Result<(), String> {
        self.transport
            .lock()
            .map_err(|e| e.to_string())?
            .transport_ready = true;
        Ok(())
    }

    fn emit_or_queue_transport_message(&self, message: String) -> Result<(), String> {
        let mut transport = self.transport.lock().map_err(|e| e.to_string())?;
        if transport.bridge_ready && transport.transport_ready {
            drop(transport);
            self.emit_transport_message_now(message)
        } else {
            if transport.pending.len() >= MAX_PENDING_MESSAGES {
                transport.pending.pop_front();
            }
            transport.pending.push_back(message);
            Ok(())
        }
    }

    fn flush_pending_transport_messages(&self) -> Result<(), String> {
        let mut transport = self.transport.lock().map_err(|e| e.to_string())?;
        if !transport.bridge_ready || !transport.transport_ready {
            return Ok(());
        }
        let messages: Vec<String> = transport.pending.drain(..).collect();
        drop(transport);

        for message in messages {
            self.emit_transport_message_now(message)?;
        }
        Ok(())
    }

    fn emit_transport_message_now(&self, message: String) -> Result<(), String> {
        self.emit_event(SHELL_TRANSPORT_EVENT, json!(message))
    }

    fn emit_event(&self, event: impl Into<String>, payload: Value) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .emit(event, payload);
        Ok(())
    }
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
    use crate::player::{FakePlayerBackend, PlayerAction};
    use crate::streaming_server::FakeProcessSpawner;
    use serde_json::json;
    use std::path::PathBuf;

    fn host() -> (
        LinuxHost<FakePlayerBackend, FakeProcessSpawner>,
        FakePlayerBackend,
        FakeProcessSpawner,
    ) {
        let player = FakePlayerBackend::initialized();
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_project_root(spawner.clone(), PathBuf::from("/repo"));
        (LinuxHost::new(player.clone(), server), player, spawner)
    }

    #[test]
    fn dispatches_phase_three_host_commands() {
        let (host, _player, spawner) = host();
        host.listen("server-started").unwrap();

        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap(),
            json!(false)
        );
        host.invoke("start_streaming_server", None).unwrap();
        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap(),
            json!(true)
        );
        assert_eq!(spawner.calls().len(), 1);

        let status = host.invoke("get_native_player_status", None).unwrap();
        assert_eq!(status["enabled"], true);
        assert_eq!(status["initialized"], true);

        host.invoke(
            "open_external_url",
            Some(json!({"url": "https://web.stremio.com/"})),
        )
        .unwrap();
    }

    #[test]
    fn rejects_unsupported_command() {
        let (host, _player, _spawner) = host();
        let error = host.invoke("get_plugins", None).unwrap_err();
        assert!(error.contains("Unsupported Linux host command"));
    }

    #[test]
    fn queues_shell_transport_events_until_bridge_and_app_ready() {
        let (host, _player, _spawner) = host();
        host.listen(SHELL_TRANSPORT_EVENT).unwrap();

        host.emit_transport_event(json!(["mpv-prop-change", {"name": "pause", "data": true}]))
            .unwrap();
        assert!(host.emitted_events().unwrap().is_empty());

        host.invoke("shell_bridge_ready", None).unwrap();
        assert!(host.emitted_events().unwrap().is_empty());

        host.invoke(
            "shell_transport_send",
            Some(json!({"message": r#"{"id":1,"type":6,"args":["app-ready"]}"#})),
        )
        .unwrap();

        let events = host.emitted_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, SHELL_TRANSPORT_EVENT);
        assert!(events[0]
            .payload
            .as_str()
            .unwrap()
            .contains("mpv-prop-change"));
    }

    #[test]
    fn shell_transport_maps_player_commands() {
        let (host, player, _spawner) = host();
        host.invoke(
            "shell_transport_send",
            Some(json!({"message": r#"{"id":7,"type":6,"args":["mpv-command",["loadfile","file:///tmp/a.mp4","replace"]]}"#})),
        )
        .unwrap();

        assert_eq!(
            player.actions(),
            vec![PlayerAction::Command {
                name: "loadfile".to_string(),
                args: vec!["file:///tmp/a.mp4".to_string(), "replace".to_string()],
            }]
        );
    }

    #[test]
    fn shell_transport_reports_unsupported_methods() {
        let (host, _player, _spawner) = host();
        let error = host
            .invoke(
                "shell_transport_send",
                Some(json!({"message": r#"{"id":7,"type":6,"args":["unknown-method"]}"#})),
            )
            .unwrap_err();
        assert!(error.contains("Unsupported shell transport method"));
    }
}
