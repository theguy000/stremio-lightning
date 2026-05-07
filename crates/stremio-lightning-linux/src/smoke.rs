use crate::cef::{linux_host_adapter, native_flags, InjectionBundle, MOD_UI_NAME};
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
    let bundle = InjectionBundle::load()?;
    if !bundle.script_names().contains(&MOD_UI_NAME) {
        return Err("Mod UI bundle was not present in Linux injection scripts".to_string());
    }

    let player = FakePlayerBackend::initialized();
    let spawner = FakeProcessSpawner::default();
    let app_data_dir = std::env::temp_dir().join(format!(
        "stremio-lightning-linux-smoke-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&app_data_dir);
    std::fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create smoke app data dir: {e}"))?;

    let host = LinuxHost::with_app_data_dir(
        player,
        StreamingServer::with_project_root(spawner, PathBuf::from("/repo")),
        app_data_dir.clone(),
    );
    host.invoke(
        "register_settings",
        Some(json!({"pluginName": "sample", "schema": r#"[{"key":"enabled","type":"toggle"}]"#})),
    )?;
    host.invoke(
        "save_setting",
        Some(json!({"pluginName": "sample", "key": "enabled", "value": "true"})),
    )?;
    if host.invoke(
        "get_setting",
        Some(json!({"pluginName": "sample", "key": "enabled"})),
    )? != json!(true)
    {
        return Err("Plugin settings did not roundtrip through Linux host".to_string());
    }

    host.listen(SHELL_TRANSPORT_EVENT)?;
    host.invoke("shell_bridge_ready", None)?;
    host.emit_native_player_property_changed("pause", json!(false))?;
    host.invoke(
        "shell_transport_send",
        Some(json!({"message": r#"{"id":1,"type":6,"args":["app-ready"]}"#})),
    )?;

    if host.emitted_events()?.is_empty() {
        return Err("Queued shell transport event was not flushed".to_string());
    }

    let _ = std::fs::remove_dir_all(app_data_dir);
    Ok(())
}
