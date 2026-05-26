use serde_json::json;
use std::path::PathBuf;
use stremio_lightning_linux::host::{LinuxHost, SHELL_TRANSPORT_EVENT};
use stremio_lightning_linux::player::MpvPlayerBackend;
use stremio_lightning_linux::streaming_server::{RealProcessSpawner, StreamingServer};
use stremio_lightning_linux::webview_runtime::{linux_host_adapter, InjectionBundle, MOD_UI_NAME};

#[test]
#[ignore = "requires STREMIO_LIGHTNING_LINUX_E2E=1"]
fn linux_shell_e2e() {
    let enabled = std::env::var("STREMIO_LIGHTNING_LINUX_E2E")
        .or_else(|_| std::env::var("STREMIO_LIGHTNING_LINUX_SMOKE"))
        .ok()
        .map(|v| v == "1")
        .unwrap_or(false);

    if !enabled {
        eprintln!("Set STREMIO_LIGHTNING_LINUX_E2E=1 or STREMIO_LIGHTNING_LINUX_SMOKE=1 to run the Linux shell E2E integration test");
        return;
    }

    run_local_e2e().unwrap();
}

fn run_local_e2e() -> Result<(), String> {
    let adapter = linux_host_adapter();
    if !adapter.contains("window.StremioLightningHost") {
        return Err("Linux host adapter did not expose StremioLightningHost".to_string());
    }
    let bundle = InjectionBundle::load()?;
    if !bundle.script_names().contains(&MOD_UI_NAME) {
        return Err("Mod UI bundle was not present in Linux injection scripts".to_string());
    }

    let player = MpvPlayerBackend::default();
    let spawner = RealProcessSpawner;
    let app_data_dir = std::env::temp_dir().join(format!(
        "stremio-lightning-linux-e2e-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&app_data_dir);
    std::fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create e2e app data dir: {e}"))?;

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
