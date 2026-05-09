use crate::player::{NativePlayerStatus, PlayerBackend};
use crate::streaming_server::{ProcessSpawner, StreamingServer};
use serde_json::{json, Value};

pub struct MacosHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    player: B,
    streaming_server: StreamingServer<P>,
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
            other => Err(format!("Unsupported macOS host command: {other}")),
        }
    }
}
