use std::path::PathBuf;
use std::sync::Mutex;
use serde_json::{json, Value};

use super::bridge::BaseHost;
use super::handlers::{
    handshake_response, is_async_command, parse_request, response_message,
    serialize_window_visibility, stremio_deep_link_transport_args,
};
use super::types::{
    HostCommand, HostEvent, ParsedRequest, PlatformBridge, SHELL_TRANSPORT_EVENT,
};
use crate::logging;

#[derive(Default)]
struct TestBridge {
    pip_enabled: bool,
    fail_custom_transport: bool,
    fullscreen: Mutex<bool>,
}

impl PlatformBridge for TestBridge {
    fn platform_name(&self) -> &'static str {
        "test"
    }

    fn shell_name(&self) -> &'static str {
        "test-shell"
    }

    fn native_player_status(&self) -> Value {
        Value::Null
    }

    fn is_streaming_server_running(&self) -> bool {
        false
    }

    fn is_window_fullscreen(&self) -> Result<bool, String> {
        Ok(*self.fullscreen.lock().unwrap())
    }

    fn set_window_fullscreen(&self, fullscreen: bool) -> Result<(), String> {
        *self.fullscreen.lock().unwrap() = fullscreen;
        Ok(())
    }

    fn toggle_picture_in_picture(&self) -> Result<bool, String> {
        Ok(self.pip_enabled)
    }

    fn is_pip_enabled(&self) -> Result<bool, String> {
        Ok(self.pip_enabled)
    }

    fn handle_custom_transport(
        &self,
        method: &str,
        _data: Option<Value>,
    ) -> Result<(), String> {
        if self.fail_custom_transport {
            Err(format!("transport failed for {method}"))
        } else {
            Ok(())
        }
    }
}

fn test_host(bridge: TestBridge) -> BaseHost<TestBridge> {
    BaseHost::new(bridge, PathBuf::new(), "0.0.0")
}

#[test]
fn classifies_network_backed_commands_as_async() {
    for command in [
        "download_mod",
        "get_registry",
        "check_mod_updates",
        "check_app_update",
    ] {
        assert!(is_async_command(command), "{command} should be async");
    }

    for command in ["init", "get_plugins", "shell_transport_send"] {
        assert!(!is_async_command(command), "{command} should be sync");
    }
}

#[test]
fn host_command_names_match_frontend() {
    assert_eq!(
        serde_json::to_value(HostCommand::ToggleDevtools).unwrap(),
        json!("toggle_devtools")
    );
    assert_eq!(
        serde_json::to_value(HostCommand::SetPipDisablesAutoPause).unwrap(),
        json!("set_pip_disables_auto_pause")
    );
    assert_eq!(
        serde_json::to_value(HostCommand::GetLogs).unwrap(),
        json!("get_logs")
    );
    assert_eq!(
        serde_json::to_value(HostCommand::GetDiagnosticReport).unwrap(),
        json!("get_diagnostic_report")
    );
}

#[test]
fn host_event_names_match_frontend() {
    assert_eq!(
        serde_json::to_value(HostEvent::WindowMaximizedChanged).unwrap(),
        json!("window-maximized-changed")
    );
    assert_eq!(
        serde_json::to_value(HostEvent::ShellTransportMessage).unwrap(),
        json!("shell-transport-message")
    );
}

#[test]
fn parses_handshake_request() {
    assert_eq!(
        parse_request(r#"{"id":0,"type":3}"#).unwrap(),
        ParsedRequest::Handshake
    );
    assert_eq!(
        parse_request(r#"{"id":0}"#).unwrap(),
        ParsedRequest::Handshake
    );
}

#[test]
fn parses_command_request() {
    assert_eq!(
        parse_request(r#"{"id":7,"type":6,"args":["mpv-command",["stop"]]}"#).unwrap(),
        ParsedRequest::Command {
            method: "mpv-command".to_string(),
            data: Some(json!(["stop"])),
        }
    );
}

#[test]
fn parses_current_stremio_command_with_zero_id() {
    assert_eq!(
        parse_request(
            r#"{"id":0,"type":6,"args":["mpv-command",["loadfile","https://example.test/video"]]}"#
        )
        .unwrap(),
        ParsedRequest::Command {
            method: "mpv-command".to_string(),
            data: Some(json!(["loadfile", "https://example.test/video"])),
        }
    );
}

#[test]
fn serializes_handshake_shape() {
    let payload: Value = serde_json::from_str(&handshake_response("0.1.4")).unwrap();
    assert_eq!(
        payload,
        json!({
            "id": 0,
            "object": "transport",
            "type": 3,
            "data": {
                "transport": {
                    "properties": [[], ["", "shellVersion", "", "0.1.4"]],
                    "signals": [],
                    "methods": [["onEvent"]]
                }
            }
        })
    );
}

#[test]
fn serializes_event_shape() {
    let payload: Value =
        serde_json::from_str(&response_message(json!(["open-media", "stremio://foo"])))
            .unwrap();
    assert_eq!(
        payload,
        json!({
            "id": 1,
            "object": "transport",
            "type": 1,
            "args": ["open-media", "stremio://foo"]
        })
    );
}

#[test]
fn classifies_stremio_deep_link_transport_events() {
    assert_eq!(
        stremio_deep_link_transport_args("stremio://addon.example/manifest.json"),
        json!(["addon-install", "stremio://addon.example/manifest.json"])
    );
    assert_eq!(
        stremio_deep_link_transport_args("stremio:///detail/movie/tt123"),
        json!(["open-media", "stremio:///detail/movie/tt123"])
    );
}

#[test]
fn serializes_window_visibility_event() {
    assert_eq!(
        serialize_window_visibility(true, true),
        json!(["win-visibility-changed", {
            "visible": true,
            "visibility": 1,
            "isFullscreen": true
        }])
    );
    assert_eq!(
        serialize_window_visibility(true, false),
        json!(["win-visibility-changed", {
            "visible": true,
            "visibility": 0,
            "isFullscreen": false
        }])
    );
}

#[test]
fn handles_win_set_visibility_and_emits_resulting_state_every_time() {
    let host = test_host(TestBridge::default());
    host.listen_with_id(7, SHELL_TRANSPORT_EVENT).unwrap();

    for fullscreen in [true, true, false] {
        host.handle_shell_transport_message(
            &json!({
                "id": 1,
                "type": 6,
                "args": ["win-set-visibility", { "fullscreen": fullscreen }]
            })
            .to_string(),
        )
        .unwrap();

        assert_eq!(*host.bridge.fullscreen.lock().unwrap(), fullscreen);
        let event = host.drain_emitted_events().unwrap().remove(0);
        let response: Value = serde_json::from_str(event.payload.as_str().unwrap()).unwrap();
        assert_eq!(
            response["args"],
            serialize_window_visibility(true, fullscreen)
        );
    }
}

#[test]
fn rejects_invalid_win_set_visibility_payload() {
    let host = test_host(TestBridge::default());
    let error = host
        .handle_shell_transport_message(
            r#"{"id":1,"type":6,"args":["win-set-visibility",{"fullscreen":"yes"}]}"#,
        )
        .unwrap_err();

    assert!(error.contains("Invalid win-set-visibility payload"));
}

#[test]
fn poisoned_shell_preferences_return_command_error() {
    let host = test_host(TestBridge::default());

    let poison_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = host.shell_preferences.lock().unwrap();
        panic!("poison shell preferences");
    }));
    assert!(poison_result.is_err());

    let error = host.invoke_sync("get_auto_pause", None).unwrap_err();
    assert!(error.contains("Shell preferences lock poisoned"));
}

#[test]
fn get_logs_uses_camel_case_cursor_without_generating_records() {
    let host = test_host(TestBridge::default());

    let result = host
        .invoke_sync("get_logs", Some(json!({ "afterId": u64::MAX })))
        .unwrap();

    assert_eq!(result, json!([]));
}

#[test]
fn diagnostics_commands_validate_payloads_and_report_capabilities() {
    let host = test_host(TestBridge::default());
    let init = host.invoke_sync("init", None).unwrap();
    assert_eq!(init["diagnostics"]["nativeHttpCapture"], false);
    assert_eq!(init["diagnostics"]["nativeNetworkFailureCapture"], false);
    assert_eq!(init["diagnostics"]["webviewEngine"], "test-shell");

    host.invoke_sync("set_extended_diagnostics", Some(json!({ "enabled": true })))
        .unwrap();
    assert!(logging::is_extended());
    host.invoke_sync(
        "submit_diagnostic_logs",
        Some(json!({
            "entries": [{
                "level": "info",
                "source": "bridge.test",
                "message": "safe"
            }]
        })),
    )
    .unwrap();
    assert!(host
        .invoke_sync(
            "submit_diagnostic_logs",
            Some(json!({ "entries": [{ "level": "invalid", "source": "x", "message": "x" }] })),
        )
        .is_err());
    assert!(host
        .invoke_sync(
            "submit_diagnostic_logs",
            Some(json!({
                "entries": [{
                    "level": "error",
                    "source": "bridge.test",
                    "message": "x".repeat(logging::MAX_EXTERNAL_BATCH_BYTES)
                }]
            })),
        )
        .is_err());
    let report = host.invoke_sync("get_diagnostic_report", None).unwrap();
    assert!(report
        .as_str()
        .is_some_and(|report| report.contains("Stremio Lightning diagnostic report")));
    logging::set_extended(false);
}

#[test]
fn update_window_focus_returns_auto_pause_transport_error() {
    let host = test_host(TestBridge {
        fail_custom_transport: true,
        ..Default::default()
    });

    {
        let mut prefs = host.shell_preferences.lock().unwrap();
        prefs.player_active = true;
        prefs.player_paused = false;
    }

    let error = host.update_window_focus(false).unwrap_err();
    assert_eq!(error, "transport failed for mpv-set-prop");
    assert!(!host.shell_preferences.lock().unwrap().auto_paused);
}

#[test]
fn auto_pause_ignores_stale_unpaused_state_without_active_playback() {
    let host = test_host(TestBridge {
        fail_custom_transport: true,
        ..Default::default()
    });
    host.shell_preferences.lock().unwrap().player_paused = false;

    host.update_window_focus(false).unwrap();

    assert!(!host.shell_preferences.lock().unwrap().auto_paused);
}

#[test]
fn player_stop_disables_auto_pause_before_late_property_events() {
    let host = test_host(TestBridge::default());
    host.handle_shell_transport_message(
        r#"{"id":0,"type":6,"args":["mpv-command",["loadfile","https://example.test/video"]]}"#,
    )
    .unwrap();
    host.queue_transport_message(response_message(json!([
        "mpv-prop-change",
        {"name": "pause", "data": false}
    ])))
    .unwrap();

    host.update_window_focus(false).unwrap();
    assert!(host.shell_preferences.lock().unwrap().auto_paused);
    host.update_window_focus(true).unwrap();

    host.handle_shell_transport_message(r#"{"id":0,"type":6,"args":["mpv-command",["stop"]]}"#)
        .unwrap();
    host.queue_transport_message(response_message(json!([
        "mpv-prop-change",
        {"name": "pause", "data": false}
    ])))
    .unwrap();
    host.update_window_focus(false).unwrap();

    let prefs = host.shell_preferences.lock().unwrap();
    assert!(!prefs.player_active);
    assert!(!prefs.auto_paused);
}
