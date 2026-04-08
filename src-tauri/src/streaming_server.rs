use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;

use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

/// Holds the streaming server child process handle.
/// Managed as Tauri app state so commands can access it.
pub struct ServerState {
    pub child: Mutex<Option<CommandChild>>,
}

/// Start the streaming server sidecar process.
pub fn start_server(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<ServerState>();
    let mut child_lock = state.child.lock().map_err(|e| e.to_string())?;

    if child_lock.is_some() {
        return Err("Server is already running".into());
    }

    // Resolve resource paths
    let resource_dir = app
        .path()
        .resolve("", BaseDirectory::Resource)
        .map_err(|e| format!("Failed to resolve resource dir: {}", e))?;

    let server_js = resource_dir.join("server.js");
    let ffmpeg_path = resource_dir.join("ffmpeg.exe");
    let ffprobe_path = resource_dir.join("ffprobe.exe");

    if !server_js.exists() {
        return Err(format!("server.js not found at {:?}", server_js));
    }

    // Build sidecar command
    let sidecar = app
        .shell()
        .sidecar("stremio-runtime")
        .map_err(|e| format!("Failed to create sidecar command: {}", e))?
        .args([server_js.to_string_lossy().as_ref()])
        .env("NO_CORS", "1")
        .env("FFMPEG_BIN", ffmpeg_path.to_string_lossy().as_ref())
        .env("FFPROBE_BIN", ffprobe_path.to_string_lossy().as_ref());

    // Spawn the process
    let (mut rx, child) = sidecar
        .spawn()
        .map_err(|e| format!("Failed to spawn streaming server: {}", e))?;

    *child_lock = Some(child);
    drop(child_lock);

    // Set up log file
    let log_path = app
        .path()
        .app_data_dir()
        .map(|p| p.join("stremio-server.log"))
        .unwrap_or_else(|_| std::path::PathBuf::from("stremio-server.log"));

    let app_handle = app.clone();

    // Monitor process output and lifecycle in background
    tauri::async_runtime::spawn(async move {
        let mut log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();

        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line) => {
                    if let Some(ref mut f) = log_file {
                        let _ = f.write_all(&line);
                        let _ = f.write_all(b"\n");
                    }
                }
                CommandEvent::Stderr(line) => {
                    if let Some(ref mut f) = log_file {
                        let _ = f.write_all(b"[stderr] ");
                        let _ = f.write_all(&line);
                        let _ = f.write_all(b"\n");
                    }
                }
                CommandEvent::Terminated(payload) => {
                    let _ = app_handle.emit("server-stopped", &payload.code);
                    if let Some(state) = app_handle.try_state::<ServerState>() {
                        if let Ok(mut child) = state.child.lock() {
                            *child = None;
                        }
                    }
                    if let Some(ref mut f) = log_file {
                        let msg = format!("[server] Process exited with code: {:?}\n", payload.code);
                        let _ = f.write_all(msg.as_bytes());
                    }
                    break;
                }
                _ => {}
            }
        }
    });

    let _ = app.emit("server-started", ());
    Ok(())
}

/// Stop the streaming server process.
pub fn stop_server(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<ServerState>();
    let mut child_lock = state.child.lock().map_err(|e| e.to_string())?;

    if let Some(child) = child_lock.take() {
        child.kill().map_err(|e| format!("Failed to kill server: {}", e))?;
        let _ = app.emit("server-stopped", Option::<i32>::None);
        Ok(())
    } else {
        Err("Server is not running".into())
    }
}

/// Check if the streaming server is currently running.
pub fn is_server_running(app: &AppHandle) -> bool {
    if let Some(state) = app.try_state::<ServerState>() {
        if let Ok(child) = state.child.lock() {
            return child.is_some();
        }
    }
    false
}
