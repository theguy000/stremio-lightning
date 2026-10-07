use super::types::WindowsIpcOutbound;
use super::url_policy::validate_external_url;
use super::WindowsHost;
use crate::single_instance::LaunchIntent;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use stremio_lightning_core::mods;

static TEMP_ID: AtomicUsize = AtomicUsize::new(0);

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "stremio-lightning-windows-host-test-{}-{}-{}",
        std::process::id(),
        name,
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn host_with_app_data(app_data_dir: PathBuf) -> WindowsHost {
    WindowsHost::with_app_data_dir_and_server_disabled(
        stremio_lightning_core::SHELL_VERSION,
        app_data_dir,
        true,
    )
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future)
}

fn expected_init_contract() -> Value {
    json!({
        "platform": "windows",
        "shell": "webview2",
        "shellVersion": stremio_lightning_core::SHELL_VERSION,
        "nativePlayer": {
            "enabled": cfg!(windows),
            "initialized": false,
            "backend": "webview2-libmpv",
        },
        "streamingServerRunning": false,
        "diagnostics": {
            "persistent": false,
            "nativeHttpCapture": false,
            "nativeNetworkFailureCapture": true,
            "webviewEngine": "WebView2",
            "webviewVersion": null,
        },
    })
}

#[test]
fn exposes_webview2_init_contract() {
    assert_eq!(
        WindowsHost::default().invoke("init", None).unwrap(),
        expected_init_contract()
    );
}

#[test]
fn handles_shell_transport_handshake() {
    let host = WindowsHost::new("0.1.4");
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 7, "event": "shell-transport-message" })),
    )
    .unwrap();
    host.invoke(
        "shell_transport_send",
        Some(json!({ "message": r#"{"id":0,"type":3}"# })),
    )
    .unwrap();
    let response = host.drain_emitted_events().unwrap().remove(0).payload;
    let payload: Value = serde_json::from_str(response.as_str().unwrap()).unwrap();
    assert_eq!(payload["type"], json!(3));
    let properties = payload["data"]["transport"]["properties"].as_array().unwrap();
    assert!(properties.iter().any(|prop| {
        prop.get(1).and_then(Value::as_str) == Some("streamingServerUrl")
            && prop.get(3).and_then(Value::as_str) == Some("http://127.0.0.1:11470")
    }));
    assert!(properties.iter().any(|prop| {
        prop.get(1).and_then(Value::as_str) == Some("nativeInterfaceScale")
            && prop.get(3).and_then(Value::as_str) == Some("true")
    }));
}

fn send_shell_transport_command(host: &WindowsHost, message: &str) -> Result<Value, String> {
    host.invoke("shell_transport_send", Some(json!({ "message": message })))
}

fn transport_event_payload(host: &WindowsHost) -> Value {
    let event = host.drain_emitted_events().unwrap().remove(0);
    serde_json::from_str(event.payload.as_str().unwrap()).unwrap()
}

#[test]
fn handles_win_set_visibility_transport() {
    let host = WindowsHost::default();
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 7, "event": "shell-transport-message" })),
    )
    .unwrap();
    host.dispatch_windows_ipc("invoke", Some(json!({ "command": "shell_bridge_ready" })))
        .unwrap();
    host.invoke(
        "shell_transport_send",
        Some(json!({ "message": r#"{"id":0,"type":3}"# })),
    )
    .unwrap();
    host.drain_emitted_events().unwrap();

    send_shell_transport_command(
        &host,
        r#"{"id":1,"type":6,"args":["win-set-visibility",{"fullscreen":true}]}"#,
    )
    .unwrap();
    assert!(host.is_window_fullscreen().unwrap());
    assert_eq!(
        transport_event_payload(&host)["args"],
        json!(["win-visibility-changed", {
            "visible": true,
            "visibility": 1,
            "isFullscreen": true
        }])
    );

    send_shell_transport_command(
        &host,
        r#"{"id":2,"type":6,"args":["win-set-visibility",{"fullscreen":false}]}"#,
    )
    .unwrap();
    assert!(!host.is_window_fullscreen().unwrap());
    assert_eq!(
        transport_event_payload(&host)["args"],
        json!(["win-visibility-changed", {
            "visible": true,
            "visibility": 0,
            "isFullscreen": false
        }])
    );
}

#[test]
fn win_set_visibility_rejects_invalid_payload_and_emits_repeated_state() {
    let host = WindowsHost::default();
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 7, "event": "shell-transport-message" })),
    )
    .unwrap();
    host.dispatch_windows_ipc("invoke", Some(json!({ "command": "shell_bridge_ready" })))
        .unwrap();
    host.invoke(
        "shell_transport_send",
        Some(json!({ "message": r#"{"id":0,"type":3}"# })),
    )
    .unwrap();
    host.drain_emitted_events().unwrap();

    let error = send_shell_transport_command(
        &host,
        r#"{"id":1,"type":6,"args":["win-set-visibility",{"fullscreen":"yes"}]}"#,
    )
    .unwrap_err();
    assert!(error.contains("Invalid win-set-visibility payload"));

    send_shell_transport_command(
        &host,
        r#"{"id":2,"type":6,"args":["win-set-visibility",{"fullscreen":false}]}"#,
    )
    .unwrap();
    assert_eq!(
        transport_event_payload(&host)["args"],
        json!(["win-visibility-changed", {
            "visible": true,
            "visibility": 0,
            "isFullscreen": false
        }])
    );
}

#[test]
fn dispatches_request_response_ipc() {
    let host = WindowsHost::default();
    let outbound = host.dispatch_ipc_message(
        r#"{"id":42,"kind":"invoke","payload":{"command":"init","payload":null}}"#,
    );

    assert_eq!(
        outbound[0],
        WindowsIpcOutbound::Response {
            id: 42,
            ok: true,
            value: expected_init_contract(),
        }
    );
}

#[test]
fn returns_structured_error_for_invalid_command() {
    let host = WindowsHost::default();
    let outbound = host.dispatch_ipc_message(
        r#"{"id":9,"kind":"invoke","payload":{"command":"missing","payload":null}}"#,
    );

    assert_eq!(
        outbound[0],
        WindowsIpcOutbound::Response {
            id: 9,
            ok: false,
            value: json!({ "message": "Unsupported Windows host command: missing" }),
        }
    );
}

#[test]
fn listener_registration_controls_events() {
    let host = WindowsHost::default();
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 2, "event": "window-fullscreen-changed" })),
    )
    .unwrap();

    let outbound = host.dispatch_ipc_message(
        r#"{"id":3,"kind":"window.setFullscreen","payload":{"fullscreen":true}}"#,
    );
    assert_eq!(outbound.len(), 2);
    assert_eq!(
        outbound[1],
        WindowsIpcOutbound::Event {
            event: "window-fullscreen-changed".to_string(),
            payload: json!(true),
        }
    );
}

#[test]
fn queues_open_media_until_shell_transport_is_ready() {
    let host = WindowsHost::default();
    host.emit_launch_intent(&LaunchIntent::Magnet(
        "magnet:?xt=urn:btih:test".to_string(),
    ))
    .unwrap();

    assert!(host.drain_ipc_events().is_empty());

    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 8, "event": "shell-transport-message" })),
    )
    .unwrap();
    assert!(host.drain_ipc_events().is_empty());

    host.dispatch_windows_ipc("invoke", Some(json!({"command": "shell_bridge_ready"})))
        .unwrap();
    assert!(host.drain_ipc_events().is_empty());

    host.invoke(
        "shell_transport_send",
        Some(json!({ "message": r#"{"id":1,"type":6,"args":["app-ready"]}"# })),
    )
    .unwrap();

    let events = host.drain_ipc_events();
    assert_eq!(events.len(), 1);
    let WindowsIpcOutbound::Event { event, payload } = &events[0] else {
        panic!("expected shell transport event");
    };
    assert_eq!(event, "shell-transport-message");
    let transport: Value = serde_json::from_str(payload.as_str().unwrap()).unwrap();
    assert_eq!(
        transport["args"],
        json!(["open-media", "magnet:?xt=urn:btih:test"])
    );
}

#[test]
fn queues_addon_install_for_stremio_manifest_links() {
    let host = WindowsHost::default();
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 8, "event": "shell-transport-message" })),
    )
    .unwrap();
    host.dispatch_windows_ipc("invoke", Some(json!({"command": "shell_bridge_ready"})))
        .unwrap();
    host.invoke(
        "shell_transport_send",
        Some(json!({ "message": r#"{"id":1,"type":6,"args":["app-ready"]}"# })),
    )
    .unwrap();
    host.emit_launch_intent(&LaunchIntent::StremioDeepLink(
        "stremio://addon.example/manifest.json".to_string(),
    ))
    .unwrap();

    let WindowsIpcOutbound::Event { payload, .. } = &host.drain_ipc_events()[0] else {
        panic!("expected shell transport event");
    };
    let transport: Value = serde_json::from_str(payload.as_str().unwrap()).unwrap();
    assert_eq!(
        transport["args"],
        json!(["addon-install", "stremio://addon.example/manifest.json"])
    );
}

#[test]
fn queues_media_keys_through_shell_transport() {
    let host = WindowsHost::default();
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 8, "event": "shell-transport-message" })),
    )
    .unwrap();
    host.dispatch_windows_ipc("invoke", Some(json!({"command": "shell_bridge_ready"})))
        .unwrap();
    host.invoke(
        "shell_transport_send",
        Some(json!({ "message": r#"{"id":1,"type":6,"args":["app-ready"]}"# })),
    )
    .unwrap();

    host.emit_media_key("play-pause").unwrap();

    let events = host.drain_ipc_events();
    let WindowsIpcOutbound::Event { event, payload } = &events[0] else {
        panic!("expected shell transport event");
    };
    assert_eq!(event, "shell-transport-message");
    let transport: Value = serde_json::from_str(payload.as_str().unwrap()).unwrap();
    assert_eq!(transport["args"], json!(["media-key", "play-pause"]));
}

#[test]
fn handles_pip_toggle_state() {
    let host = WindowsHost::default();
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 8, "event": "shell-transport-message" })),
    )
    .unwrap();

    assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(false));
    host.invoke("toggle_pip", None).unwrap();
    assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(true));
    host.invoke("toggle_pip", None).unwrap();
    assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(false));

    let events = host.drain_ipc_events();
    assert_eq!(events.len(), 2);
    let WindowsIpcOutbound::Event { payload, .. } = &events[0] else {
        panic!("expected shell transport event");
    };
    assert!(payload.as_str().unwrap().contains("showPictureInPicture"));
    let WindowsIpcOutbound::Event { payload, .. } = &events[1] else {
        panic!("expected shell transport event");
    };
    assert!(payload.as_str().unwrap().contains("hidePictureInPicture"));
}

#[test]
fn exits_pip_when_native_player_ends() {
    let host = WindowsHost::default();
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 8, "event": "shell-transport-message" })),
    )
    .unwrap();

    host.invoke("toggle_pip", None).unwrap();
    assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(true));
    host.drain_ipc_events();

    host.player().lock().unwrap().emit_ended("eof");

    let events = host.drain_ipc_events();
    assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(false));
    assert_eq!(events.len(), 2);
    let WindowsIpcOutbound::Event { payload, .. } = &events[0] else {
        panic!("expected shell transport event");
    };
    assert!(payload.as_str().unwrap().contains("hidePictureInPicture"));
    let WindowsIpcOutbound::Event { payload, .. } = &events[1] else {
        panic!("expected shell transport event");
    };
    assert!(payload.as_str().unwrap().contains("ended"));
}

#[test]
fn tracks_window_maximized_state() {
    let host = WindowsHost::default();
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 2, "event": "window-maximized-changed" })),
    )
    .unwrap();

    assert_eq!(
        host.dispatch_windows_ipc("window.isMaximized", None)
            .unwrap(),
        json!(false)
    );
    let outbound =
        host.dispatch_ipc_message(r#"{"id":3,"kind":"window.toggleMaximize","payload":null}"#);
    assert_eq!(
        outbound[0],
        WindowsIpcOutbound::Response {
            id: 3,
            ok: true,
            value: Value::Null
        }
    );
    assert_eq!(
        outbound[1],
        WindowsIpcOutbound::Event {
            event: "window-maximized-changed".to_string(),
            payload: json!(true)
        }
    );
    assert_eq!(
        host.dispatch_windows_ipc("window.isMaximized", None)
            .unwrap(),
        json!(true)
    );
}

#[test]
fn external_url_policy_rejects_unsafe_schemes() {
    assert!(validate_external_url("https://web.stremio.com/").is_ok());
    assert!(validate_external_url("http://127.0.0.1:11470/").is_ok());
    assert!(validate_external_url("mailto:support@example.com").is_ok());
    assert!(validate_external_url("file:///C:/Windows/notepad.exe").is_err());
    assert!(validate_external_url("javascript:alert(1)").is_err());
    assert!(validate_external_url("ms-settings:privacy").is_err());
    assert!(validate_external_url("https://example.com/\ncalc").is_err());
}

#[test]
fn external_url_policy_accepts_the_shared_stream_schemes() {
    for url in [
        "rtsp://example.com:5544/stream",
        "rtp://example.com:5544",
        "ftp://example.com/file.mkv",
        "ipfs://bafybeigdyrzt",
    ] {
        assert!(
            validate_external_url(url).is_ok(),
            "expected {url} to be allowed on both shells"
        );
    }
}

#[test]
fn matches_json_host_contract_fixture() {
    let mut fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/host_contract.json")).unwrap();
    fixture["invokeInitResponse"]["value"]["nativePlayer"]["enabled"] = json!(cfg!(windows));
    fixture["invokeInitResponse"]["value"]["shellVersion"] =
        json!(stremio_lightning_core::SHELL_VERSION);
    let host = WindowsHost::default();

    let init = host.dispatch_ipc_message(&fixture["invokeInitRequest"].to_string());
    assert_eq!(
        serde_json::to_value(&init[0]).unwrap(),
        fixture["invokeInitResponse"]
    );

    let invalid = host.dispatch_ipc_message(&fixture["invalidCommandRequest"].to_string());
    assert_eq!(
        serde_json::to_value(&invalid[0]).unwrap(),
        fixture["invalidCommandResponse"]
    );
}

#[test]
fn lists_reads_and_deletes_plugin_and_theme_mods() {
    let root = temp_dir("mods-contract");
    let host = host_with_app_data(root.clone());

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
        br"/**
 * @name Sample Theme
 * @description Demo theme
 * @author Tester
 * @version 1.0.0
 */
:root { --sl-test-color: red; }",
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
    let host = WindowsHost::default();
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
    let host = host_with_app_data(root.clone());

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

    host.base
        .invoke(
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

#[test]
#[cfg(windows)]
fn async_dispatch_returns_before_network_completes() {
    let host = Arc::new(host_with_app_data(temp_dir("async-dispatch")));
    let immediate = host.dispatch_ipc_message_async(
        r#"{"id":21,"kind":"invoke","payload":{"command":"download_mod","payload":{"url":"https://example.test/evil.theme.css","modType":"plugin"}}}"#,
    );
    assert!(
        !immediate
            .iter()
            .any(|outbound| matches!(outbound, WindowsIpcOutbound::Response { .. })),
        "network command must not block the caller: {immediate:?}"
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        for outbound in host.drain_pending_responses() {
            if let WindowsIpcOutbound::Response {
                id: 21,
                ok: false,
                value,
            } = outbound
            {
                assert!(value["message"]
                    .as_str()
                    .unwrap()
                    .contains("Invalid plugin filename extension"));
                return;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "async response did not arrive before the deadline"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
#[cfg(windows)]
fn async_dispatch_falls_back_for_sync_commands() {
    let host = Arc::new(host_with_app_data(temp_dir("async-fallback")));
    let outbound = host.dispatch_ipc_message_async(
        r#"{"id":7,"kind":"invoke","payload":{"command":"init","payload":null}}"#,
    );

    assert_eq!(
        outbound[0],
        WindowsIpcOutbound::Response {
            id: 7,
            ok: true,
            value: expected_init_contract(),
        }
    );
    assert!(host.drain_pending_responses().is_empty());
}
