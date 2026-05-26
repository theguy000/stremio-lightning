use crate::player::{
    handle_transport, serialize_ended, serialize_property_change, NativePlayerStatus, PlayerBackend,
};
use crate::streaming_server::{ProcessSpawner, StreamingServer};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use stremio_lightning_core::host_api::{self, BaseHost, HostEvent, HostEventRecord, PlatformBridge};
pub use stremio_lightning_core::host_api::SHELL_TRANSPORT_EVENT;
use stremio_lightning_core::pip::{
    serialize_picture_in_picture, PipRestoreSnapshot, PipState, PipWindowController,
};

pub struct LinuxBridge<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub player: B,
    pub streaming_server: StreamingServer<P>,
    pub pip_state: PipState,
}

impl<B, P> PlatformBridge for LinuxBridge<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    fn platform_name(&self) -> &'static str {
        "linux"
    }

    fn shell_name(&self) -> &'static str {
        ""
    }

    fn native_player_status(&self) -> Value {
        serde_json::to_value(self.player.status()).unwrap_or(Value::Null)
    }

    fn is_streaming_server_running(&self) -> bool {
        self.streaming_server.is_running()
    }

    fn toggle_picture_in_picture(&self) -> Result<bool, String> {
        let enabled = !self.pip_state.is_enabled()?;
        self.pip_state.set_mode(enabled, None)?;
        Ok(enabled)
    }

    fn is_pip_enabled(&self) -> Result<bool, String> {
        self.pip_state.is_enabled()
    }

    fn open_external_url(&self, url: &str) -> Result<(), String> {
        validate_external_url(url)?;
        Ok(())
    }

    fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.start()
    }

    fn stop_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.stop()
    }

    fn restart_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.restart()
    }

    fn handle_custom_transport(&self, method: &str, data: Option<Value>) -> Result<(), String> {
        match method {
            "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                handle_transport(&self.player, method, data)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

pub struct LinuxHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub base: BaseHost<LinuxBridge<B, P>>,
}

pub type Host<B, P> = LinuxHost<B, P>;

impl<B, P> LinuxHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub fn new(player: B, streaming_server: StreamingServer<P>) -> Self {
        Self::with_app_data_dir(player, streaming_server, default_app_data_dir())
    }

    pub fn with_app_data_dir(
        player: B,
        streaming_server: StreamingServer<P>,
        app_data_dir: impl Into<PathBuf>,
    ) -> Self {
        let bridge = LinuxBridge {
            player,
            streaming_server,
            pip_state: PipState::new(),
        };
        Self {
            base: BaseHost::new(bridge, app_data_dir.into(), env!("CARGO_PKG_VERSION")),
        }
    }

    pub fn player(&self) -> &B {
        &self.base.bridge.player
    }

    pub fn streaming_server(&self) -> &StreamingServer<P> {
        &self.base.bridge.streaming_server
    }

    pub fn pip_state(&self) -> &PipState {
        &self.base.bridge.pip_state
    }

    pub fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server().start()?;
        self.emit_server_started()?;
        Ok(())
    }

    pub fn shutdown(&self) -> Result<(), String> {
        self.player().stop().ok();
        self.streaming_server().stop()
    }

    pub fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        self.base.dispatch_ipc(kind, payload)
    }

    pub fn dispatch_linux_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        self.dispatch_ipc(kind, payload)
    }

    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        self.base.invoke(command, payload)
    }

    pub fn listen(&self, event: impl Into<String>) -> Result<u64, String> {
        Ok(self.base.lock_listeners()?.listen(event))
    }

    pub fn listen_with_id(&self, id: u64, event: impl Into<String>) -> Result<(), String> {
        self.base.listen_with_id(id, event)
    }

    pub fn unlisten(&self, id: u64) -> Result<(), String> {
        self.base.unlisten(id)
    }

    pub fn emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        Ok(self.base.lock_listeners()?.emitted.clone())
    }

    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        self.base.drain_emitted_events()
    }

    pub fn native_player_status(&self) -> NativePlayerStatus {
        self.player().status()
    }

    pub fn emit_transport_event(&self, args: Value) -> Result<(), String> {
        self.base.queue_transport_message(host_api::response_message(args))
    }

    pub fn emit_native_player_property_changed(
        &self,
        name: impl Into<String>,
        data: Value,
    ) -> Result<(), String> {
        self.emit_transport_event(serialize_property_change(name, data))
    }

    pub fn emit_native_player_ended(&self, reason: impl Into<String>) -> Result<(), String> {
        self.emit_transport_event(serialize_ended(reason))
    }

    pub fn set_picture_in_picture(
        &self,
        enabled: bool,
        snapshot: Option<PipRestoreSnapshot>,
    ) -> Result<(), String> {
        self.pip_state().set_mode(enabled, snapshot)?;
        self.emit_transport_event(serialize_picture_in_picture(enabled))
    }

    pub fn toggle_picture_in_picture(
        &self,
        controller: &mut impl PipWindowController,
    ) -> Result<bool, String> {
        let enabled = self.pip_state().toggle_window_pip(controller)?;
        self.emit_transport_event(serialize_picture_in_picture(enabled))?;
        Ok(enabled)
    }

    pub fn exit_picture_in_picture(
        &self,
        controller: &mut impl PipWindowController,
    ) -> Result<bool, String> {
        let changed = self.pip_state().exit_window_pip(controller)?;
        self.emit_picture_in_picture_exit(changed)
    }

    pub fn exit_picture_in_picture_for_player_end(
        &self,
        controller: &mut impl PipWindowController,
    ) -> Result<bool, String> {
        self.exit_picture_in_picture(controller)
    }

    fn emit_picture_in_picture_exit(&self, changed: bool) -> Result<bool, String> {
        if changed {
            self.emit_transport_event(serialize_picture_in_picture(false))?;
        }
        Ok(changed)
    }

    pub fn emit_window_maximized_changed(&self, maximized: bool) -> Result<(), String> {
        self.base.emit_host_event(HostEvent::WindowMaximizedChanged, json!(maximized))
    }

    pub fn emit_window_fullscreen_changed(&self, fullscreen: bool) -> Result<(), String> {
        self.base.emit_host_event(HostEvent::WindowFullscreenChanged, json!(fullscreen))?;
        self.base.emit_transport_message(host_api::response_message(host_api::serialize_window_visibility(true, fullscreen)))
    }

    pub fn emit_server_started(&self) -> Result<(), String> {
        self.base.emit_host_event(HostEvent::ServerStarted, Value::Null)
    }

    pub fn emit_server_stopped(&self) -> Result<(), String> {
        self.base.emit_host_event(HostEvent::ServerStopped, Value::Null)
    }
}

fn default_app_data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| Path::new(&home).join(".local").join("share"))
        })
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn validate_external_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    let allowed = [
        "http://", "https://", "rtp://", "rtsp://", "ftp://", "ipfs://",
    ]
    .iter()
    .any(|prefix| {
        trimmed
            .get(..prefix.len())
            .map_or(false, |s| s.eq_ignore_ascii_case(prefix))
    });

    if allowed {
        Ok(())
    } else {
        Err("Rejected non-whitelisted open_external_url URL".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::FakePlayerBackend;
    use crate::streaming_server::FakeProcessSpawner;
    use stremio_lightning_core::mods;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn host() -> (
        LinuxHost<FakePlayerBackend, FakeProcessSpawner>,
        FakePlayerBackend,
        FakeProcessSpawner,
    ) {
        host_with_app_data(temp_dir("default"))
    }

    fn host_with_app_data(
        app_data_dir: PathBuf,
    ) -> (
        LinuxHost<FakePlayerBackend, FakeProcessSpawner>,
        FakePlayerBackend,
        FakeProcessSpawner,
    ) {
        let player = FakePlayerBackend::initialized();
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_project_root(spawner.clone(), PathBuf::from("/repo"));
        (
            LinuxHost::with_app_data_dir(player.clone(), server, app_data_dir),
            player,
            spawner,
        )
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stremio-lightning-linux-host-test-{}-{}-{}",
            std::process::id(),
            name,
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn dispatches_phase_three_host_commands() {
        let (host, _player, spawner) = host();
        host.listen("server-started").unwrap();
        host.listen("server-stopped").unwrap();

        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap(),
            json!(false)
        );
        host.invoke("start_streaming_server", None).unwrap();
        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap(),
            json!(true)
        );
        host.invoke("stop_streaming_server", None).unwrap();
        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap(),
            json!(false)
        );
        assert_eq!(spawner.calls().len(), 1);

        let init = host.invoke("init", None).unwrap();
        assert_eq!(init["platform"], "linux");
        assert_eq!(init["nativePlayer"]["enabled"], true);
        assert_eq!(init["streamingServerRunning"], false);

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
    fn lists_reads_and_deletes_plugin_and_theme_mods() {
        let root = temp_dir("mods-contract");
        let (host, _player, _spawner) = host_with_app_data(root.clone());

        assert_eq!(host.invoke("get_plugins", None).unwrap(), json!([]));
        assert_eq!(host.invoke("get_themes", None).unwrap(), json!([]));

        mods::write_mod_content(
            &root,
            "sample.plugin.js",
            mods::ModType::Plugin,
            br#"/**
 * @name Sample Plugin
 * @description Demo plugin
 * @author Tester
 * @version 1.0.0
 */
console.log("sample");"#,
        )
        .unwrap();
        mods::write_mod_content(
            &root,
            "sample.theme.css",
            mods::ModType::Theme,
            br#"/**
 * @name Sample Theme
 * @description Demo theme
 * @author Tester
 * @version 1.0.0
 */
:root { --sl-test-color: red; }"#,
        )
        .unwrap();
        host.invoke(
            "save_setting",
            Some(json!({"pluginName": "sample", "key": "enabled", "value": "true"})),
        )
        .unwrap();

        let plugins = host.invoke("get_plugins", None).unwrap();
        assert_eq!(plugins[0]["filename"], "sample.plugin.js");
        assert_eq!(plugins[0]["mod_type"], "plugin");
        assert_eq!(plugins[0]["metadata"]["name"], "Sample Plugin");

        let themes = host.invoke("get_themes", None).unwrap();
        assert_eq!(themes[0]["filename"], "sample.theme.css");
        assert_eq!(themes[0]["mod_type"], "theme");

        let content = host
            .base
            .invoke(
                "get_mod_content",
                Some(json!({"filename": "sample.plugin.js", "modType": "plugin"})),
            )
            .unwrap();
        assert!(content.as_str().unwrap().contains("console.log"));

        host.invoke(
            "delete_mod",
            Some(json!({"filename": "sample.plugin.js", "modType": "plugin"})),
        )
        .unwrap();
        assert_eq!(host.base.invoke("get_plugins", None).unwrap(), json!([]));
        assert!(!mods::mods_dir(&root, mods::ModType::Plugin)
            .join("sample.plugin.json")
            .exists());
    }

    #[test]
    fn rejects_invalid_mod_payloads() {
        let (host, _player, _spawner) = host();
        let traversal = host
            .base
            .invoke(
                "get_mod_content",
                Some(json!({"filename": "../evil.plugin.js", "modType": "plugin"})),
            )
            .unwrap_err();
        assert!(traversal.contains("Invalid filename"));

        let invalid_type = host
            .invoke(
                "delete_mod",
                Some(json!({"filename": "sample.plugin.js", "modType": "script"})),
            )
            .unwrap_err();
        assert!(invalid_type.contains("Unknown mod type"));

        let download_error = block_on(host.base.invoke_async(
            "download_mod",
            Some(json!({"url": "https://example.test/evil.theme.css", "modType": "plugin"})),
        ))
        .unwrap_err();
        assert!(download_error.contains("Invalid plugin filename extension"));
    }

    #[test]
    fn plugin_settings_round_trip_and_validate() {
        let root = temp_dir("settings-contract");
        let (host, _player, _spawner) = host_with_app_data(root.clone());

        host.invoke(
            "register_settings",
            Some(json!({
                "pluginName": "sample",
                "schema": r#"[{"key":"enabled","type":"toggle"}]"#
            })),
        )
        .unwrap();
        assert_eq!(
            host.invoke("get_registered_settings", None).unwrap(),
            json!({"sample": [{"key": "enabled", "type": "toggle"}]})
        );

        host.invoke(
            "save_setting",
            Some(json!({"pluginName": "sample", "key": "enabled", "value": "true"})),
        )
        .unwrap();
        assert_eq!(
            host.invoke(
                "get_setting",
                Some(json!({"pluginName": "sample", "key": "enabled"}))
            )
            .unwrap(),
            json!(true)
        );

        host.base.invoke(
            "save_setting",
            Some(json!({"pluginName": "sample", "key": "mode", "value": "plain text"})),
        )
        .unwrap();
        assert_eq!(
            host.invoke(
                "get_setting",
                Some(json!({"pluginName": "sample", "key": "mode"}))
            )
            .unwrap(),
            json!("plain text")
        );

        let invalid_schema = host
            .invoke(
                "register_settings",
                Some(json!({"pluginName": "sample", "schema": "{"})),
            )
            .unwrap_err();
        assert!(invalid_schema.contains("Failed to parse settings schema"));
    }
}
