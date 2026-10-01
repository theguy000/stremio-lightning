use super::*;
use stremio_lightning_core::bridge_assets::{bridge_scripts, BRIDGE_NAME, MOD_UI_NAME};

#[test]
fn injects_windows_adapter_before_shared_bridge() {
    let (_tx, rx) = mpsc::channel();
    #[cfg(windows)]
    let shell = WindowsWebView2Shell::new(
        ShellSettings::from_args([] as [&str; 0]),
        rx,
        Arc::new(Mutex::new(None)),
    )
    .unwrap();

    #[cfg(not(windows))]
    let shell = WindowsWebView2Shell::new(ShellSettings::from_args([] as [&str; 0]), rx).unwrap();

    let mut expected = vec![WINDOWS_HOST_ADAPTER_NAME];
    expected.extend(bridge_scripts().iter().map(|script| script.name));
    expected.push(MOD_UI_NAME);
    assert_eq!(shell.document_start_script_names(), expected);
}

#[test]
fn combined_injection_source_concatenates_every_script_in_order() {
    let bundle = InjectionBundle::load().unwrap();
    let combined = bundle.combined_source();

    let expected = bundle
        .scripts()
        .iter()
        .map(|script| script.source.as_str())
        .collect::<Vec<_>>()
        .join("\n;\n");

    assert_eq!(combined, expected);
    assert_eq!(
        combined.matches("\n;\n").count(),
        bundle.scripts().len() - 1
    );
}

#[test]
fn moved_shared_bridge_is_loaded_from_web_folder() {
    let bundle = InjectionBundle::load().unwrap();
    let bridge = bundle
        .scripts()
        .iter()
        .find(|script| script.name == BRIDGE_NAME)
        .unwrap();

    assert!(bridge.source.contains("Native player mode enabled"));
}

#[test]
fn windows_bundle_injects_svelte_mod_ui() {
    let bundle = InjectionBundle::load().unwrap();
    let mod_ui = bundle
        .scripts()
        .iter()
        .find(|script| script.name == MOD_UI_NAME)
        .unwrap();

    assert!(mod_ui.source.contains("Mods UI initialized"));
}

#[test]
fn windows_adapter_resolves_structured_logger_when_an_error_occurs() {
    let adapter = host_adapter();

    assert!(adapter.contains("function logError()"));
    assert!(adapter.contains("nativeWebview.postMessage.bind(nativeWebview)"));
    assert!(adapter.contains("nativePostMessage({"));
    assert!(adapter.contains("var logger = window.StremioLightningLogger"));
    assert!(adapter.contains("bridge.host-adapter.windows"));
}

#[test]
fn webview_navigation_is_limited_to_configured_origin() {
    let app_url = "https://web.stremio.com/#/";

    assert!(navigation::is_allowed_webview_navigation(
        app_url,
        "https://web.stremio.com/#/player"
    ));
    assert!(navigation::is_allowed_webview_navigation(
        app_url,
        "about:blank"
    ));
    assert!(!navigation::is_allowed_webview_navigation(
        app_url,
        "https://example.com/"
    ));
    assert!(!navigation::is_allowed_webview_navigation(
        app_url,
        "file:///C:/test.html"
    ));
    assert!(!navigation::is_allowed_webview_navigation(
        app_url,
        "javascript:alert(1)"
    ));
}

#[test]
fn localhost_webview_origin_includes_port() {
    let app_url = "http://127.0.0.1:5173/";

    assert!(navigation::is_allowed_webview_navigation(
        app_url,
        "http://127.0.0.1:5173/player"
    ));
    assert!(!navigation::is_allowed_webview_navigation(
        app_url,
        "http://127.0.0.1:11470/"
    ));
}

#[test]
fn cleanup_report_records_all_failures_without_short_circuiting() {
    let mut report = types::CleanupReport::default();

    report.record(
        "remove message handler",
        Err("message token failed".to_string()),
    );
    report.record("remove navigation handler", Ok(()));
    report.record("close controller", Err("close failed".to_string()));

    assert_eq!(
        report.failures(),
        [
            "remove message handler: message token failed",
            "close controller: close failed"
        ]
    );
}

#[test]
#[cfg(windows)]
fn webview_failure_descriptors_never_include_raw_uris() {
    assert_eq!(
        platform::safe_webview_resource_descriptor(
            "https://api.example.test/stream/token?secret=hidden"
        ),
        "https resource"
    );
    assert_eq!(
        platform::safe_webview_resource_descriptor("file:///C:/Users/private/video.mkv"),
        "file resource"
    );
}

#[test]
#[cfg(windows)]
fn webview_failure_statuses_are_classified() {
    assert_eq!(platform::web_error_status_name(7), "timeout");
    assert_eq!(platform::process_failed_kind_name(6), "gpu-process-exited");
    assert_eq!(platform::web_error_status_name(99), "unknown");
}

#[test]
#[cfg(windows)]
fn reload_and_print_accelerators_are_blocked() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_F5, VK_P, VK_R};

    assert!(platform::should_block_browser_accelerator(
        u32::from(VK_F5.0),
        false,
    ));
    assert!(platform::should_block_browser_accelerator(
        u32::from(VK_R.0),
        true,
    ));
    assert!(platform::should_block_browser_accelerator(
        u32::from(VK_P.0),
        true,
    ));
}

#[test]
#[cfg(windows)]
fn unrelated_accelerators_are_left_alone() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_R, VK_S};

    assert!(!platform::should_block_browser_accelerator(
        u32::from(VK_R.0),
        false,
    ));
    assert!(!platform::should_block_browser_accelerator(
        u32::from(VK_S.0),
        true,
    ));
}

#[test]
#[cfg(windows)]
fn outbound_scratch_is_reused_and_null_terminated() {
    let mut scratch = Vec::new();
    platform::fill_utf16_scratch(&mut scratch, "ab");
    assert_eq!(scratch, [0x61, 0x62, 0]);
    let capacity = scratch.capacity();

    platform::fill_utf16_scratch(&mut scratch, "é");
    assert_eq!(scratch, [0xE9, 0]);
    assert_eq!(scratch.capacity(), capacity);
}
