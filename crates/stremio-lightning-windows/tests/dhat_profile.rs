//! Whole-workload DHAT heap profile: writes `dhat-heap.json` with a backtrace per
//! allocation site, so the viewer ranks the real call sites instead of aggregate counts.
//!
//! Run it alone (a second `Profiler` in the same process panics) with symbols retained:
//! `cargo test -p stremio-lightning-windows --features dhat-heap --profile profiling --test dhat_profile -- --ignored --nocapture`
//! then open `dhat-heap.json` in the dhat viewer: <https://nnethercote.github.io/dh_view/dhat-view.html>
//!
//! The workload mirrors production ratios: continuous mpv property ticks (outbound),
//! IPC command replies (outbound), and page-originated transport messages (inbound).
//! WebView2/COM cannot run headless, so the serialization steps platform.rs performs
//! before `PostWebMessageAsJson` are replayed verbatim here.

#![cfg(all(windows, feature = "dhat-heap"))]
#![allow(unsafe_code, clippy::cast_precision_loss)]

use dhat::Alloc;
use serde_json::json;
use std::sync::Arc;
use stremio_lightning_windows::host::{WindowsHost, WindowsIpcOutbound};

#[global_allocator]
static ALLOC: Alloc = Alloc;

const TICKS: usize = 20_000;
const ROUND_TRIPS: usize = 4_000;

#[ignore = "manual dhat profile: run with --profile profiling -- --ignored --nocapture"]
#[test]
fn profile_production_workload() {
    let profiler = dhat::Profiler::builder()
        .file_name("dhat-heap.json")
        .build();

    let dir = std::env::temp_dir().join("stremio-dhat-profile");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    let host = Arc::new(WindowsHost::with_app_data_dir_and_server_disabled(
        stremio_lightning_core::SHELL_VERSION,
        dir,
        true,
    ));
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 1, "event": "shell-transport-message" })),
    )
    .unwrap();
    host.base.mark_bridge_ready().unwrap();
    host.base.mark_transport_ready().unwrap();

    let media_url = format!(
        "https://cdn.example.test/media/{}?token=abcdef0123456789",
        "segment-path/".repeat(200)
    );
    let inbound = json!({
        "id": 7,
        "kind": "invoke",
        "payload": {
            "command": "shell_transport_send",
            "payload": {
                "message": json!({
                    "id": 42,
                    "type": 6,
                    "args": ["open-url", media_url],
                })
                .to_string(),
            },
        },
    })
    .to_string();
    let status_reply = json!({
        "id": 0,
        "kind": "invoke",
        "payload": { "command": "get_native_player_status", "payload": null },
    })
    .to_string();

    let mut utf8 = Vec::with_capacity(4096);
    let mut utf16 = Vec::with_capacity(4096);
    // Same buffer the WebView2 message handler keeps for the app's lifetime.
    let mut outbound = Vec::with_capacity(16);

    // Outbound: one mpv property tick per iteration, exactly as backend.rs emits them.
    for tick in 0..TICKS {
        host.player()
            .lock()
            .unwrap()
            .emit_property_change("time-pos", json!(tick as f64 / 10.0));
        outbound.clear();
        host.drain_ipc_events_into(&mut outbound);
        post_like_platform(&mut utf8, &mut utf16, &outbound);
    }

    // Outbound: command replies, the other continuous webview message source.
    for _ in 0..ROUND_TRIPS {
        outbound.clear();
        host.dispatch_ipc_message_async_into(&status_reply, &mut outbound);
        post_like_platform(&mut utf8, &mut utf16, &outbound);
    }

    // Inbound: the page forwarding transport messages through the shell.
    for _ in 0..ROUND_TRIPS {
        outbound.clear();
        host.dispatch_ipc_message_async_into(&inbound, &mut outbound);
        post_like_platform(&mut utf8, &mut utf16, &outbound);
    }

    drop(host);
    drop(profiler);
}

/// The serialization steps platform.rs runs before `PostWebMessageAsJson`.
fn post_like_platform(utf8: &mut Vec<u8>, utf16: &mut Vec<u16>, outbound: &[WindowsIpcOutbound]) {
    for message in outbound {
        utf8.clear();
        serde_json::to_writer(&mut *utf8, message).unwrap();
        // SAFETY: serde_json writes valid UTF-8 into the scratch buffer.
        let serialized = unsafe { std::str::from_utf8_unchecked(utf8) };
        utf16.clear();
        utf16.extend(serialized.encode_utf16());
        utf16.push(0);
        assert!(!std::hint::black_box(utf16.as_ptr()).is_null());
    }
}
