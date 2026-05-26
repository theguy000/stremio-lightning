use crate::host::{LinuxHost, SHELL_TRANSPORT_EVENT};
use crate::player::{NativePlayerStatus, PlayerAction, PlayerBackend};
use crate::streaming_server::{CommandSpec, ProcessChild, ProcessSpawner, StreamingServer};
use crate::webview_runtime::{linux_host_adapter, InjectionBundle, MOD_UI_NAME};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub fn run_local_e2e() -> Result<(), String> {
    let adapter = linux_host_adapter();
    if !adapter.contains("window.StremioLightningHost") {
        return Err("Linux host adapter did not expose StremioLightningHost".to_string());
    }
    let bundle = InjectionBundle::load()?;
    if !bundle.script_names().contains(&MOD_UI_NAME) {
        return Err("Mod UI bundle was not present in Linux injection scripts".to_string());
    }

    let player = FakePlayerBackend::initialized();
    let spawner = FakeProcessSpawner::default();
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

#[derive(Debug, Default, Clone)]
pub struct FakeProcessSpawner {
    calls: Arc<Mutex<Vec<CommandSpec>>>,
    stopped: Arc<Mutex<Vec<usize>>>,
    next_child_exited: Arc<Mutex<bool>>,
}

impl FakeProcessSpawner {
    pub fn calls(&self) -> Vec<CommandSpec> {
        self.calls.lock().expect("fake spawner poisoned").clone()
    }

    pub fn stopped(&self) -> Vec<usize> {
        self.stopped
            .lock()
            .expect("fake spawner stopped list poisoned")
            .clone()
    }

    pub fn set_next_child_exited(&self, exited: bool) {
        *self
            .next_child_exited
            .lock()
            .expect("fake spawner exit flag poisoned") = exited;
    }
}

#[derive(Debug)]
pub struct FakeProcessChild {
    id: usize,
    stopped: Arc<Mutex<Vec<usize>>>,
    exited: bool,
}

impl ProcessChild for FakeProcessChild {
    fn stop(&mut self) -> Result<(), String> {
        self.stopped
            .lock()
            .map_err(|e| e.to_string())?
            .push(self.id);
        self.exited = true;
        Ok(())
    }

    fn has_exited(&mut self) -> Result<bool, String> {
        Ok(self.exited)
    }
}

impl ProcessSpawner for FakeProcessSpawner {
    type Child = FakeProcessChild;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        let mut calls = self.calls.lock().map_err(|e| e.to_string())?;
        calls.push(spec);
        let exited = *self.next_child_exited.lock().map_err(|e| e.to_string())?;
        Ok(FakeProcessChild {
            id: calls.len(),
            stopped: self.stopped.clone(),
            exited,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct FakePlayerBackend {
    actions: Arc<Mutex<Vec<PlayerAction>>>,
    initialized: bool,
}

impl FakePlayerBackend {
    pub fn initialized() -> Self {
        Self {
            actions: Arc::default(),
            initialized: true,
        }
    }

    pub fn actions(&self) -> Vec<PlayerAction> {
        self.actions.lock().expect("fake player poisoned").clone()
    }
}

impl PlayerBackend for FakePlayerBackend {
    fn status(&self) -> NativePlayerStatus {
        NativePlayerStatus {
            enabled: true,
            initialized: self.initialized,
            backend: "fake".to_string(),
        }
    }

    fn observe_property(&self, name: String) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::ObserveProperty(name));
        Ok(())
    }

    fn set_property(&self, name: String, value: Value) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::SetProperty { name, value });
        Ok(())
    }

    fn command(&self, name: String, args: Vec<String>) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::Command { name, args });
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::Stop);
        Ok(())
    }
}
