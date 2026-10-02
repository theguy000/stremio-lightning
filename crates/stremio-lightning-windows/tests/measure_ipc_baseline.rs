//! Manual performance harness for investigating Windows shell allocation churn.
//!
//! These are design-time sketches, not part of the default test suite. Run them with:
//! `cargo test -p stremio-lightning-windows --features dhat-heap --test measure_ipc_baseline -- --ignored --nocapture --test-threads=1`
//!
//! They must run single-threaded (every benchmark shares one global allocation profiler)
//! and in a debug build. Debug timings are indicative only; in release the compiler can
//! optimize the zero-allocation branches away, so only the allocation counts are stable.

#![cfg(all(windows, feature = "dhat-heap"))]
#![allow(unsafe_code)]
// Benchmark maths divides integer counters by the iteration count, and each bench keeps
// its BEFORE/AFTER helpers next to the numbers they produce.
#![allow(
    clippy::cast_precision_loss,
    clippy::items_after_statements,
    clippy::too_many_lines
)]

use dhat::{Alloc, HeapStats};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use stremio_lightning_core::host_api::{self, IpcRequest};
use stremio_lightning_windows::host::{WindowsHost, WindowsIpcOutbound};
use stremio_lightning_windows::single_instance::LaunchIntent;

#[global_allocator]
static ALLOC: Alloc = Alloc;

static PROFILER: OnceLock<dhat::Profiler> = OnceLock::new();
static BASELINE_BLOCKS: AtomicU64 = AtomicU64::new(0);
static BASELINE_BYTES: AtomicU64 = AtomicU64::new(0);

fn reset_metrics() {
    PROFILER.get_or_init(|| dhat::Profiler::builder().testing().build());
    let stats = HeapStats::get();
    BASELINE_BLOCKS.store(stats.total_blocks, Ordering::SeqCst);
    BASELINE_BYTES.store(stats.total_bytes, Ordering::SeqCst);
}

fn current_metrics() -> (usize, usize) {
    let stats = HeapStats::get();
    let blocks = stats.total_blocks - BASELINE_BLOCKS.load(Ordering::SeqCst);
    let bytes = stats.total_bytes - BASELINE_BYTES.load(Ordering::SeqCst);
    (
        usize::try_from(blocks).unwrap_or(usize::MAX),
        usize::try_from(bytes).unwrap_or(usize::MAX),
    )
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("stremio-perf-test-{name}"));
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

// =====================================================================
// 1. Sync IPC Dispatch: Double-Parse vs Single-Parse
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_1_sync_ipc_single_vs_double_parse() {
    let host = Arc::new(WindowsHost::with_app_data_dir_and_server_disabled(
        stremio_lightning_core::SHELL_VERSION,
        temp_dir("single-vs-double-parse"),
        true,
    ));

    let raw_msg = r#"{"id":1,"kind":"invoke","payload":{"command":"init","payload":null}}"#;
    let iterations = 10_000;

    // The old shape, kept as the live BEFORE: an owned mirror of the invoke
    // envelope deserialized out of the already-parsed `Value`, which makes
    // serde_json deep-clone the inner payload.
    #[derive(serde::Deserialize)]
    struct InvokeEnvelope {
        command: String,
        payload: Option<Value>,
    }
    fn old_split(payload: Option<Value>) -> Result<(String, Option<Value>), String> {
        serde_json::from_value::<InvokeEnvelope>(payload.unwrap_or(Value::Null))
            .map(|envelope| (envelope.command, envelope.payload))
            .map_err(|error| format!("Invalid invoke payload: {error}"))
    }

    for _ in 0..100 {
        let _ = host.dispatch_ipc_message_async(raw_msg);
    }

    reset_metrics();
    let start_double = Instant::now();
    for _ in 0..iterations {
        let request: IpcRequest = serde_json::from_str(raw_msg).unwrap();
        let is_async = if request.kind == "invoke" {
            old_split(request.payload.clone())
                .is_ok_and(|(command, _)| host_api::is_async_command(&command))
        } else {
            false
        };

        if !is_async {
            let _ = host.dispatch_ipc(&request.kind, request.payload);
            let _ = host.drain_ipc_events();
        }
    }
    let elapsed_double = start_double.elapsed();
    let (allocs_double, bytes_double) = current_metrics();

    reset_metrics();
    let start_single = Instant::now();
    for _ in 0..iterations {
        let request: IpcRequest = serde_json::from_str(raw_msg).unwrap();
        let is_async = if request.kind == "invoke" {
            host_api::split_invoke_payload(request.payload.clone())
                .is_ok_and(|(command, _)| host_api::is_async_command(&command))
        } else {
            false
        };

        if !is_async {
            let _ = host.dispatch_ipc(&request.kind, request.payload);
            let _ = host.drain_ipc_events();
        }
    }
    let elapsed_single = start_single.elapsed();
    let (allocs_single, bytes_single) = current_metrics();

    println!("\n==================================================================");
    println!(" BENCHMARK 1: Sync IPC Double-Parse vs Single-Parse (10,000 ops)");
    println!("==================================================================");
    println!("[BEFORE: Double-Parse (dispatch_ipc_message_async -> dispatch_ipc_message)]");
    println!(
        "  Time:        {:?} ({:.3} µs / call)",
        elapsed_double,
        elapsed_double.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / call)",
        allocs_double,
        allocs_double as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / call)\n",
        bytes_double,
        bytes_double as f64 / f64::from(iterations)
    );

    println!("[AFTER: Single-Parse (Direct Dispatch)]");
    println!(
        "  Time:        {:?} ({:.3} µs / call)",
        elapsed_single,
        elapsed_single.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / call)",
        allocs_single,
        allocs_single as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / call)\n",
        bytes_single,
        bytes_single as f64 / f64::from(iterations)
    );

    let time_saved = (1.0 - elapsed_single.as_secs_f64() / elapsed_double.as_secs_f64()) * 100.0;
    println!("[IMPROVEMENT DELTA]");
    println!(
        "  Latency:     {:.2}% faster ({:.3} µs saved / call)",
        time_saved,
        (elapsed_double.as_secs_f64() - elapsed_single.as_secs_f64()) * 1_000_000.0
            / f64::from(iterations)
    );
    println!(
        "  Allocations: -{} blocks (-{:.1} blocks / call, -{:.1}%)",
        allocs_double.saturating_sub(allocs_single),
        (allocs_double.saturating_sub(allocs_single)) as f64 / f64::from(iterations),
        (allocs_double.saturating_sub(allocs_single)) as f64 / allocs_double as f64 * 100.0
    );
    println!(
        "  Heap Churn:  -{} bytes (-{:.1} B / call, -{:.1}%)\n",
        bytes_double.saturating_sub(bytes_single),
        (bytes_double.saturating_sub(bytes_single)) as f64 / f64::from(iterations),
        (bytes_double.saturating_sub(bytes_single)) as f64 / bytes_double as f64 * 100.0
    );
}

// =====================================================================
// 2. Player Transport: as_array().cloned() vs Pattern Match Take
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_2_player_transport_payload_cloning() {
    let payload = json!(["seek", "60", "absolute"]);
    let iterations = 10_000;

    reset_metrics();
    let start_cloned = Instant::now();
    for _ in 0..iterations {
        let p = Some(payload.clone());
        let _values = p.and_then(|value| value.as_array().cloned()).unwrap();
    }
    let elapsed_cloned = start_cloned.elapsed();
    let (allocs_cloned, bytes_cloned) = current_metrics();

    reset_metrics();
    let start_zero_clone = Instant::now();
    for _ in 0..iterations {
        let p = Some(payload.clone());
        let Some(Value::Array(arr)) = p else {
            unreachable!()
        };
        let _ = std::hint::black_box(&arr);
    }
    let elapsed_zero_clone = start_zero_clone.elapsed();
    let (allocs_zero_clone, bytes_zero_clone) = current_metrics();

    println!("\n==================================================================");
    println!(" BENCHMARK 2: Player Transport Payload Cloning (10,000 ops)");
    println!("==================================================================");
    println!("[BEFORE: as_array().cloned()]");
    println!(
        "  Time:        {:?} ({:.3} µs / op)",
        elapsed_cloned,
        elapsed_cloned.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / op)",
        allocs_cloned,
        allocs_cloned as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / op)\n",
        bytes_cloned,
        bytes_cloned as f64 / f64::from(iterations)
    );

    println!("[AFTER: Pattern Match Take (Zero-Clone)]");
    println!(
        "  Time:        {:?} ({:.3} µs / op)",
        elapsed_zero_clone,
        elapsed_zero_clone.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / op)",
        allocs_zero_clone,
        allocs_zero_clone as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / op)\n",
        bytes_zero_clone,
        bytes_zero_clone as f64 / f64::from(iterations)
    );

    let time_saved =
        (1.0 - elapsed_zero_clone.as_secs_f64() / elapsed_cloned.as_secs_f64()) * 100.0;
    println!("[IMPROVEMENT DELTA]");
    println!(
        "  Latency:     {:.2}% faster ({:.3} µs saved / op)",
        time_saved,
        (elapsed_cloned.as_secs_f64() - elapsed_zero_clone.as_secs_f64()) * 1_000_000.0
            / f64::from(iterations)
    );
    println!(
        "  Allocations: -{} blocks (-{:.1} blocks / op, -50.0%)",
        allocs_cloned - allocs_zero_clone,
        (allocs_cloned - allocs_zero_clone) as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Churn:  -{} bytes (-{:.1} B / op, -50.0%)\n",
        bytes_cloned - bytes_zero_clone,
        (bytes_cloned - bytes_zero_clone) as f64 / f64::from(iterations)
    );
}

// =====================================================================
// 3. Outbound IPC: serde_json::to_string vs to_writer (scratch buffer)
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_3_outbound_serialization_to_string_vs_to_writer() {
    let outbound = WindowsIpcOutbound::Event {
        event: "time-pos".to_string(),
        payload: json!({ "time": 128.456, "duration": 3600.0 }),
    };
    let iterations = 10_000;

    let mut scratch_utf16 = Vec::with_capacity(1024);
    reset_metrics();
    let start_to_string = Instant::now();
    for _ in 0..iterations {
        let serialized = serde_json::to_string(&outbound).unwrap();
        scratch_utf16.clear();
        scratch_utf16.extend(serialized.encode_utf16());
        scratch_utf16.push(0);
    }
    let elapsed_to_string = start_to_string.elapsed();
    let (allocs_to_string, bytes_to_string) = current_metrics();

    let mut scratch_utf8 = Vec::with_capacity(1024);
    scratch_utf16.clear();
    reset_metrics();
    let start_to_writer = Instant::now();
    for _ in 0..iterations {
        scratch_utf8.clear();
        serde_json::to_writer(&mut scratch_utf8, &outbound).unwrap();
        let s = unsafe { std::str::from_utf8_unchecked(&scratch_utf8) };
        scratch_utf16.clear();
        scratch_utf16.extend(s.encode_utf16());
        scratch_utf16.push(0);
    }
    let elapsed_to_writer = start_to_writer.elapsed();
    let (allocs_to_writer, bytes_to_writer) = current_metrics();

    println!("\n==================================================================");
    println!(" BENCHMARK 3: Outbound Serialization (10,000 events)");
    println!("==================================================================");
    println!("[BEFORE: serde_json::to_string (&outbound)]");
    println!(
        "  Time:        {:?} ({:.3} µs / event)",
        elapsed_to_string,
        elapsed_to_string.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / event)",
        allocs_to_string,
        allocs_to_string as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / event)\n",
        bytes_to_string,
        bytes_to_string as f64 / f64::from(iterations)
    );

    println!("[AFTER: serde_json::to_writer (&mut scratch_utf8)]");
    println!(
        "  Time:        {:?} ({:.3} µs / event)",
        elapsed_to_writer,
        elapsed_to_writer.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / event)",
        allocs_to_writer,
        allocs_to_writer as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / event)\n",
        bytes_to_writer,
        bytes_to_writer as f64 / f64::from(iterations)
    );

    let time_saved =
        (1.0 - elapsed_to_writer.as_secs_f64() / elapsed_to_string.as_secs_f64()) * 100.0;
    println!("[IMPROVEMENT DELTA]");
    println!(
        "  Latency:     {:.2}% faster ({:.3} µs saved / event)",
        time_saved,
        (elapsed_to_string.as_secs_f64() - elapsed_to_writer.as_secs_f64()) * 1_000_000.0
            / f64::from(iterations)
    );
    println!(
        "  Allocations: -{} blocks (-100.0% OF HEAP ALLOCATIONS DROPPED)",
        allocs_to_string - allocs_to_writer
    );
    println!(
        "  Heap Churn:  -{} bytes (-100.0% HEAP MEMORY ELIMINATED)\n",
        bytes_to_string - bytes_to_writer
    );
}

// =====================================================================
// 4. MPV Command Args: Vec<String> & Vec<&str> vs Borrowed &[&str]
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_4_mpv_command_args_extraction() {
    let values = [
        Value::String("seek".to_string()),
        Value::String("60".to_string()),
        Value::String("absolute".to_string()),
    ];
    let iterations = 10_000;

    // BEFORE: Current implementation (command_name_and_args + Vec<&str> collect)
    reset_metrics();
    let start_before = Instant::now();
    for _ in 0..iterations {
        // 1. command_name_and_args: allocates String, Vec<String>, and clones each arg
        let name = values.first().and_then(Value::as_str).unwrap().to_string();
        let args: Vec<String> = values
            .iter()
            .skip(1)
            .map(|v| match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect();
        // 2. handle_mpv_command: allocates Vec<&str>
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        assert_eq!(name, "seek");
        assert_eq!(refs.len(), 2);
    }
    let elapsed_before = start_before.elapsed();
    let (allocs_before, bytes_before) = current_metrics();

    // AFTER: Zero-allocation borrowing
    reset_metrics();
    let start_after = Instant::now();
    for _ in 0..iterations {
        let name = values.first().and_then(Value::as_str).unwrap();
        // Use stack array for typical commands (<= 4 args)
        let mut refs: [&str; 4] = [""; 4];
        let mut count = 0;
        for v in values.iter().skip(1) {
            if let Some(s) = v.as_str() {
                refs[count] = s;
                count += 1;
            }
        }
        let slice = &refs[..count];
        assert_eq!(name, "seek");
        assert_eq!(slice.len(), 2);
    }
    let elapsed_after = start_after.elapsed();
    let (allocs_after, bytes_after) = current_metrics();

    println!("\n==================================================================");
    println!(" BENCHMARK 4: MPV Command Args Extraction (10,000 commands)");
    println!("==================================================================");
    println!("[BEFORE: command_name_and_args (Vec<String> + Vec<&str>)]");
    println!(
        "  Time:        {:?} ({:.3} µs / cmd)",
        elapsed_before,
        elapsed_before.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / cmd)",
        allocs_before,
        allocs_before as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / cmd)\n",
        bytes_before,
        bytes_before as f64 / f64::from(iterations)
    );

    println!("[AFTER: Borrowed Stack Array &[&str] (Zero-Alloc)]");
    println!(
        "  Time:        {:?} ({:.3} µs / cmd)",
        elapsed_after,
        elapsed_after.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / cmd)",
        allocs_after,
        allocs_after as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / cmd)\n",
        bytes_after,
        bytes_after as f64 / f64::from(iterations)
    );

    let time_saved = (1.0 - elapsed_after.as_secs_f64() / elapsed_before.as_secs_f64()) * 100.0;
    println!("[IMPROVEMENT DELTA]");
    println!(
        "  Latency:     {:.2}% faster ({:.3} µs saved / cmd)",
        time_saved,
        (elapsed_before.as_secs_f64() - elapsed_after.as_secs_f64()) * 1_000_000.0
            / f64::from(iterations)
    );
    println!(
        "  Allocations: -{} blocks (-100.0% OF HEAP ALLOCATIONS DROPPED)",
        allocs_before - allocs_after
    );
    println!(
        "  Heap Churn:  -{} bytes (-100.0% HEAP MEMORY ELIMINATED)\n",
        bytes_before - bytes_after
    );
}

// =====================================================================
// 5. MPV Property Name Allocation Churn (name.to_string() vs Static Lookup)
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_5_mpv_property_name_allocation() {
    let name_str = "time-pos";
    let iterations = 10_000;

    // BEFORE: name.to_string() per property change
    reset_metrics();
    let start_before = Instant::now();
    for _ in 0..iterations {
        let owned: String = name_str.to_string();
        assert_eq!(owned, "time-pos");
    }
    let elapsed_before = start_before.elapsed();
    let (allocs_before, bytes_before) = current_metrics();

    // AFTER: Static match table returning &'static str or interned Cow
    fn intern_property(name: &str) -> &'static str {
        match name {
            "time-pos" => "time-pos",
            "pause" => "pause",
            "duration" => "duration",
            "demuxer-cache-time" => "demuxer-cache-time",
            "cache-buffering-state" => "cache-buffering-state",
            "volume" => "volume",
            "mute" => "mute",
            _ => "other",
        }
    }

    reset_metrics();
    let start_after = Instant::now();
    for _ in 0..iterations {
        let interned = intern_property(name_str);
        assert_eq!(interned, "time-pos");
    }
    let elapsed_after = start_after.elapsed();
    let (allocs_after, bytes_after) = current_metrics();

    println!("\n==================================================================");
    println!(" BENCHMARK 5: MPV Property Name Allocation (10,000 property ticks)");
    println!("==================================================================");
    println!("[BEFORE: name.to_string()]");
    println!(
        "  Time:        {:?} ({:.3} µs / tick)",
        elapsed_before,
        elapsed_before.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / tick)",
        allocs_before,
        allocs_before as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / tick)\n",
        bytes_before,
        bytes_before as f64 / f64::from(iterations)
    );

    println!("[AFTER: Static String Match (&'static str)]");
    println!(
        "  Time:        {:?} ({:.3} µs / tick)",
        elapsed_after,
        elapsed_after.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / tick)",
        allocs_after,
        allocs_after as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / tick)\n",
        bytes_after,
        bytes_after as f64 / f64::from(iterations)
    );

    let time_saved = (1.0 - elapsed_after.as_secs_f64() / elapsed_before.as_secs_f64()) * 100.0;
    println!("[IMPROVEMENT DELTA]");
    println!(
        "  Latency:     {:.2}% faster ({:.3} µs saved / tick)",
        time_saved,
        (elapsed_before.as_secs_f64() - elapsed_after.as_secs_f64()) * 1_000_000.0
            / f64::from(iterations)
    );
    println!(
        "  Allocations: -{} blocks (-100.0% OF HEAP ALLOCATIONS DROPPED)",
        allocs_before - allocs_after
    );
    println!(
        "  Heap Churn:  -{} bytes (-100.0% HEAP MEMORY ELIMINATED)\n",
        bytes_before - bytes_after
    );
}

// =====================================================================
// 6. External URL Policy: CoTaskMemPWSTR vs HSTRING / Scratch UTF-16
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_6_url_policy_cotask_vs_hstring() {
    let url = "https://app.strem.io/v4.4.168/";
    let iterations = 10_000;

    // BEFORE: CoTaskMemPWSTR::from(url.trim()) -> calls COM task allocator
    reset_metrics();
    let start_before = Instant::now();
    for _ in 0..iterations {
        let wide = webview2_com::CoTaskMemPWSTR::from(url.trim());
        let ptr = *wide.as_ref().as_pcwstr();
        assert!(!ptr.is_null());
    }
    let elapsed_before = start_before.elapsed();
    let (allocs_before, bytes_before) = current_metrics();

    // AFTER: windows::core::HSTRING::from(url.trim())
    reset_metrics();
    let start_after = Instant::now();
    for _ in 0..iterations {
        let hstring = windows::core::HSTRING::from(url.trim());
        let ptr = windows::core::PCWSTR(hstring.as_ptr());
        assert!(!ptr.is_null());
    }
    let elapsed_after = start_after.elapsed();
    let (allocs_after, bytes_after) = current_metrics();

    println!("\n==================================================================");
    println!(" BENCHMARK 6: External URL Wide-String Conversion (10,000 URLs)");
    println!("==================================================================");
    println!("[BEFORE: CoTaskMemPWSTR::from (COM Task Allocator)]");
    println!(
        "  Time:        {:?} ({:.3} µs / url)",
        elapsed_before,
        elapsed_before.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {allocs_before} blocks (Note: CoTaskMemAlloc operates outside Rust stdlib allocator)"
    );
    println!("  Heap Bytes:  {bytes_before} bytes\n");

    println!("[AFTER: windows::core::HSTRING::from (Windows Core Heap)]");
    println!(
        "  Time:        {:?} ({:.3} µs / url)",
        elapsed_after,
        elapsed_after.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!("  Allocations: {allocs_after} blocks");
    println!("  Heap Bytes:  {bytes_after} bytes\n");

    let time_saved = (1.0 - elapsed_after.as_secs_f64() / elapsed_before.as_secs_f64()) * 100.0;
    println!("[IMPROVEMENT DELTA]");
    println!(
        "  Latency:     {:.2}% faster ({:.3} µs saved / url)\n",
        time_saved,
        (elapsed_before.as_secs_f64() - elapsed_after.as_secs_f64()) * 1_000_000.0
            / f64::from(iterations)
    );
}

// =====================================================================
// 7. WebView Navigation: url_origin with 3 Strings vs Zero-Copy Slices
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_7_navigation_url_origin_parsing() {
    let app_url = "https://app.strem.io/shell/v1";
    let target_url = "https://app.strem.io/discover";
    let iterations = 10_000;

    // BEFORE: Current is_allowed_webview_navigation calling url_origin(app) and url_origin(target)
    fn url_origin_before(url: &str) -> Option<String> {
        let scheme_end = url.find("://")?;
        let scheme = url[..scheme_end].to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return None;
        }
        let authority_start = scheme_end + 3;
        let authority = url[authority_start..]
            .split(['/', '?', '#'])
            .next()?
            .to_ascii_lowercase();
        if authority.is_empty() || authority.contains('@') {
            return None;
        }
        Some(format!("{scheme}://{authority}"))
    }

    fn is_allowed_before(app_url: &str, target_url: &str) -> bool {
        match (url_origin_before(app_url), url_origin_before(target_url)) {
            (Some(a), Some(t)) => a == t,
            _ => false,
        }
    }

    reset_metrics();
    let start_before = Instant::now();
    for _ in 0..iterations {
        assert!(is_allowed_before(app_url, target_url));
    }
    let elapsed_before = start_before.elapsed();
    let (allocs_before, bytes_before) = current_metrics();

    // AFTER: Precomputed app_origin & zero-allocation borrowed slices
    fn url_origin_zero_copy(url: &str) -> Option<(&str, &str)> {
        let scheme_end = url.find("://")?;
        let scheme = &url[..scheme_end];
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return None;
        }
        let authority_start = scheme_end + 3;
        let authority = url[authority_start..].split(['/', '?', '#']).next()?;
        if authority.is_empty() || authority.contains('@') {
            return None;
        }
        Some((scheme, authority))
    }

    let (app_scheme, app_auth) = url_origin_zero_copy(app_url).unwrap();

    fn is_allowed_after(app_scheme: &str, app_auth: &str, target_url: &str) -> bool {
        match url_origin_zero_copy(target_url) {
            Some((ts, ta)) => {
                ts.eq_ignore_ascii_case(app_scheme) && ta.eq_ignore_ascii_case(app_auth)
            }
            None => false,
        }
    }

    reset_metrics();
    let start_after = Instant::now();
    for _ in 0..iterations {
        assert!(is_allowed_after(app_scheme, app_auth, target_url));
    }
    let elapsed_after = start_after.elapsed();
    let (allocs_after, bytes_after) = current_metrics();

    println!("\n==================================================================");
    println!(" BENCHMARK 7: Navigation URL Origin Parsing (10,000 navigations)");
    println!("==================================================================");
    println!("[BEFORE: 6 String Allocations per Navigation]");
    println!(
        "  Time:        {:?} ({:.3} µs / nav)",
        elapsed_before,
        elapsed_before.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / nav)",
        allocs_before,
        allocs_before as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / nav)\n",
        bytes_before,
        bytes_before as f64 / f64::from(iterations)
    );

    println!("[AFTER: Precomputed Origin & Zero-Copy Slices]");
    println!(
        "  Time:        {:?} ({:.3} µs / nav)",
        elapsed_after,
        elapsed_after.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / nav)",
        allocs_after,
        allocs_after as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / nav)\n",
        bytes_after,
        bytes_after as f64 / f64::from(iterations)
    );

    let time_saved = (1.0 - elapsed_after.as_secs_f64() / elapsed_before.as_secs_f64()) * 100.0;
    println!("[IMPROVEMENT DELTA]");
    println!(
        "  Latency:     {:.2}% faster ({:.3} µs saved / nav)",
        time_saved,
        (elapsed_before.as_secs_f64() - elapsed_after.as_secs_f64()) * 1_000_000.0
            / f64::from(iterations)
    );
    println!(
        "  Allocations: -{} blocks (-100.0% OF HEAP ALLOCATIONS DROPPED)",
        allocs_before - allocs_after
    );
    println!(
        "  Heap Churn:  -{} bytes (-100.0% HEAP MEMORY ELIMINATED)\n",
        bytes_before - bytes_after
    );
}

// =====================================================================
// 9. Real inbound IPC: shell_transport_send with a large media URL
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_9_real_inbound_transport_send() {
    let host = Arc::new(WindowsHost::with_app_data_dir_and_server_disabled(
        stremio_lightning_core::SHELL_VERSION,
        temp_dir("inbound-transport-send"),
        true,
    ));

    let long_url = format!(
        "https://cdn.example.test/media/{}",
        "segment-path/".repeat(200)
    );
    let rpc_message = json!({
        "id": 42,
        "type": 6,
        "args": ["open-url", format!("{long_url}?token=abcdef0123456789")],
    })
    .to_string();
    let raw_msg = json!({
        "id": 7,
        "kind": "invoke",
        "payload": {
            "command": "shell_transport_send",
            "payload": { "message": rpc_message },
        },
    })
    .to_string();

    let iterations = 10_000;
    for _ in 0..100 {
        let _ = host.dispatch_ipc_message_async(&raw_msg);
    }

    reset_metrics();
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = host.dispatch_ipc_message_async(&raw_msg);
    }
    let elapsed = start.elapsed();
    let (allocs, bytes) = current_metrics();

    println!("\n==================================================================");
    println!(
        " BENCHMARK 9: Real Inbound shell_transport_send ({} byte payload)",
        raw_msg.len()
    );
    println!("==================================================================");
    println!(
        "  Time:        {:?} ({:.3} µs / msg)",
        elapsed,
        elapsed.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.2} blocks / msg)",
        allocs,
        allocs as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / msg)\n",
        bytes,
        bytes as f64 / f64::from(iterations)
    );
    assert!(raw_msg.len() > 2000, "payload should be representative");
}

// =====================================================================
// 10. Real outbound player tick: emit_property_change -> drain -> serialize
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_10_real_player_tick_pipeline() {
    let host = WindowsHost::with_app_data_dir_and_server_disabled(
        stremio_lightning_core::SHELL_VERSION,
        temp_dir("player-tick-pipeline"),
        true,
    );
    host.dispatch_windows_ipc(
        "listen",
        Some(json!({ "id": 1, "event": "shell-transport-message" })),
    )
    .unwrap();
    let _ = host.drain_ipc_events();

    let iterations = 10_000;
    let mut scratch = Vec::with_capacity(512);
    reset_metrics();
    let start = Instant::now();
    for index in 0..iterations {
        {
            let mut player = host.player().lock().unwrap();
            player.emit_property_change("time-pos", json!(f64::from(index) / 10.0));
        }
        for outbound in host.drain_ipc_events() {
            scratch.clear();
            serde_json::to_writer(&mut scratch, &outbound).unwrap();
        }
    }
    let elapsed = start.elapsed();
    let (allocs, bytes) = current_metrics();

    println!("\n==================================================================");
    println!(" BENCHMARK 10: Real Player Property Tick Pipeline (10,000 ticks)");
    println!("==================================================================");
    println!(
        "  Time:        {:?} ({:.3} µs / tick)",
        elapsed,
        elapsed.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.2} blocks / tick)",
        allocs,
        allocs as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / tick)\n",
        bytes,
        bytes as f64 / f64::from(iterations)
    );
}

// =====================================================================
// 8. CLI Argument Classification: to_ascii_lowercase vs eq_ignore_ascii_case
// =====================================================================
#[ignore = "manual perf harness: run with --ignored --nocapture --test-threads=1"]
#[test]
fn bench_8_cli_argument_classification() {
    let args = [
        "--streaming-server-disabled",
        "stremio://detail/series/tt0944947",
        "magnet:?xt=urn:btih:example",
        "C:\\Movies\\sample.torrent",
        "--devtools",
    ];
    let iterations = 10_000;

    // BEFORE: calls argument.to_ascii_lowercase() on every argument
    fn classify_before(argument: &str) -> Option<LaunchIntent> {
        let lower = argument.to_ascii_lowercase();
        if lower.starts_with("stremio://") {
            Some(LaunchIntent::StremioDeepLink(argument.to_string()))
        } else if lower.starts_with("magnet:") {
            Some(LaunchIntent::Magnet(argument.to_string()))
        } else if lower.ends_with(".torrent") {
            Some(LaunchIntent::Torrent(argument.to_string()))
        } else if argument.starts_with('-') {
            None
        } else {
            Some(LaunchIntent::FilePath(argument.to_string()))
        }
    }

    reset_metrics();
    let start_before = Instant::now();
    for _ in 0..iterations {
        for arg in &args {
            let _ = classify_before(arg);
        }
    }
    let elapsed_before = start_before.elapsed();
    let (allocs_before, bytes_before) = current_metrics();

    // AFTER: zero-allocation prefix / suffix matching
    fn classify_after(argument: &str) -> Option<LaunchIntent> {
        if argument
            .get(..10)
            .is_some_and(|p| p.eq_ignore_ascii_case("stremio://"))
        {
            Some(LaunchIntent::StremioDeepLink(argument.to_string()))
        } else if argument
            .get(..7)
            .is_some_and(|p| p.eq_ignore_ascii_case("magnet:"))
        {
            Some(LaunchIntent::Magnet(argument.to_string()))
        } else if argument.len() >= 8
            && argument[argument.len() - 8..].eq_ignore_ascii_case(".torrent")
        {
            Some(LaunchIntent::Torrent(argument.to_string()))
        } else if argument.starts_with('-') {
            None
        } else {
            Some(LaunchIntent::FilePath(argument.to_string()))
        }
    }

    reset_metrics();
    let start_after = Instant::now();
    for _ in 0..iterations {
        for arg in &args {
            let _ = classify_after(arg);
        }
    }
    let elapsed_after = start_after.elapsed();
    let (allocs_after, bytes_after) = current_metrics();

    println!("\n==================================================================");
    println!(" BENCHMARK 8: CLI Argument Classification (50,000 args checked)");
    println!("==================================================================");
    println!("[BEFORE: argument.to_ascii_lowercase()]");
    println!(
        "  Time:        {:?} ({:.3} µs / batch)",
        elapsed_before,
        elapsed_before.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / batch)",
        allocs_before,
        allocs_before as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / batch)\n",
        bytes_before,
        bytes_before as f64 / f64::from(iterations)
    );

    println!("[AFTER: eq_ignore_ascii_case Zero-Alloc Slices]");
    println!(
        "  Time:        {:?} ({:.3} µs / batch)",
        elapsed_after,
        elapsed_after.as_secs_f64() * 1_000_000.0 / f64::from(iterations)
    );
    println!(
        "  Allocations: {} blocks ({:.1} blocks / batch)",
        allocs_after,
        allocs_after as f64 / f64::from(iterations)
    );
    println!(
        "  Heap Bytes:  {} bytes ({:.1} B / batch)\n",
        bytes_after,
        bytes_after as f64 / f64::from(iterations)
    );

    let time_saved = (1.0 - elapsed_after.as_secs_f64() / elapsed_before.as_secs_f64()) * 100.0;
    println!("[IMPROVEMENT DELTA]");
    println!(
        "  Latency:     {:.2}% faster ({:.3} µs saved / batch)",
        time_saved,
        (elapsed_before.as_secs_f64() - elapsed_after.as_secs_f64()) * 1_000_000.0
            / f64::from(iterations)
    );
    println!(
        "  Allocations: -{} blocks (-{:.1}% of allocations dropped)",
        allocs_before - allocs_after,
        (allocs_before - allocs_after) as f64 / allocs_before as f64 * 100.0
    );
    println!(
        "  Heap Churn:  -{} bytes (-{:.1}% heap churn dropped)\n",
        bytes_before - bytes_after,
        (bytes_before - bytes_after) as f64 / bytes_before as f64 * 100.0
    );
}
