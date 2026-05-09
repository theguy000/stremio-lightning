use crate::player::{MpvOption, NativePlayerStatus};
use crate::streaming_server::StreamingServerStatus;
use crate::webview_runtime::WebviewLoadState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebviewDiagnostics {
    pub url: String,
    pub loaded: bool,
    pub devtools: bool,
    pub document_start_scripts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcDiagnostics {
    pub handler: String,
    pub recent_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerDiagnostics {
    pub enabled: bool,
    pub initialized: bool,
    pub backend: String,
    pub mpv_options: Vec<(String, String)>,
    pub first_frame_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerDiagnostics {
    pub running: bool,
    pub disabled: bool,
    pub url: String,
    pub stdout_log: PathBuf,
    pub stderr_log: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacosDiagnosticsSnapshot {
    pub webview: WebviewDiagnostics,
    pub ipc: IpcDiagnostics,
    pub player: PlayerDiagnostics,
    pub server: ServerDiagnostics,
}

pub fn diagnostics_snapshot(
    load_state: &WebviewLoadState,
    ipc_errors: Vec<String>,
    player_status: NativePlayerStatus,
    mpv_options: &[MpvOption],
    first_frame_timing: Option<Duration>,
    server_status: StreamingServerStatus,
    stdout_log: PathBuf,
    stderr_log: PathBuf,
) -> MacosDiagnosticsSnapshot {
    MacosDiagnosticsSnapshot {
        webview: WebviewDiagnostics {
            url: load_state.url.clone(),
            loaded: load_state.loaded,
            devtools: load_state.devtools,
            document_start_scripts: load_state
                .document_start_scripts
                .iter()
                .map(|name| (*name).to_string())
                .collect(),
        },
        ipc: IpcDiagnostics {
            handler: crate::native_window::IPC_HANDLER_NAME.to_string(),
            recent_errors: ipc_errors,
        },
        player: PlayerDiagnostics {
            enabled: player_status.enabled,
            initialized: player_status.initialized,
            backend: player_status.backend,
            mpv_options: mpv_options
                .iter()
                .map(|option| (option.name.to_string(), option.value.clone()))
                .collect(),
            first_frame_ms: first_frame_timing.map(|timing| timing.as_millis()),
        },
        server: ServerDiagnostics {
            running: server_status.running,
            disabled: server_status.disabled,
            url: server_status.url,
            stdout_log,
            stderr_log,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::default_mpv_options;
    use crate::streaming_server::DEFAULT_SERVER_URL;
    use crate::webview_runtime::{HOST_ADAPTER_NAME, MOD_UI_NAME};
    use serde_json::json;

    #[test]
    fn serializes_phase_9_diagnostics_snapshot() {
        let snapshot = diagnostics_snapshot(
            &WebviewLoadState {
                url: "http://127.0.0.1:11470/".to_string(),
                devtools: true,
                document_start_scripts: vec![HOST_ADAPTER_NAME, MOD_UI_NAME],
                loaded: true,
            },
            vec!["bad ipc payload".to_string()],
            NativePlayerStatus {
                enabled: true,
                initialized: true,
                backend: "fake".to_string(),
            },
            &default_mpv_options("Stremio Lightning", false),
            Some(Duration::from_millis(42)),
            StreamingServerStatus {
                running: true,
                disabled: false,
                url: DEFAULT_SERVER_URL.to_string(),
            },
            PathBuf::from("/logs/stremio-server.stdout.log"),
            PathBuf::from("/logs/stremio-server.stderr.log"),
        );

        let value = serde_json::to_value(snapshot).unwrap();
        assert_eq!(value["webview"]["loaded"], true);
        assert_eq!(
            value["webview"]["document_start_scripts"],
            json!([HOST_ADAPTER_NAME, MOD_UI_NAME])
        );
        assert_eq!(value["ipc"]["handler"], "ipc");
        assert_eq!(value["ipc"]["recent_errors"], json!(["bad ipc payload"]));
        assert_eq!(value["player"]["first_frame_ms"], 42);
        assert_eq!(
            value["server"]["stdout_log"],
            "/logs/stremio-server.stdout.log"
        );
    }
}
