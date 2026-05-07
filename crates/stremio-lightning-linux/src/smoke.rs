use crate::cef::{linux_host_adapter, native_flags};
use crate::host::{LinuxHost, SHELL_TRANSPORT_EVENT};
use crate::player::FakePlayerBackend;
use crate::streaming_server::{FakeProcessSpawner, StreamingServer};
use serde_json::json;
use std::path::PathBuf;

pub fn run_local_smoke() -> Result<(), String> {
    let adapter = linux_host_adapter();
    if !adapter.contains("window.StremioLightningHost") {
        return Err("Linux host adapter did not expose StremioLightningHost".to_string());
    }
    if !native_flags().contains("__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__ = true") {
        return Err("Native player flag was not enabled".to_string());
    }

    let player = FakePlayerBackend::initialized();
    let spawner = FakeProcessSpawner::default();
    let host = LinuxHost::new(
        player,
        StreamingServer::with_project_root(spawner, PathBuf::from("/repo")),
    );
    host.listen(SHELL_TRANSPORT_EVENT)?;
    host.invoke("shell_bridge_ready", None)?;
    host.emit_transport_event(json!(["mpv-prop-change", {"name": "pause", "data": false}]))?;
    host.invoke(
        "shell_transport_send",
        Some(json!({"message": r#"{"id":1,"type":6,"args":["app-ready"]}"#})),
    )?;

    if host.emitted_events()?.is_empty() {
        return Err("Queued shell transport event was not flushed".to_string());
    }

    Ok(())
}
