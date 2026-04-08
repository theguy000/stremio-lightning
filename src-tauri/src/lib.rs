mod commands;
mod streaming_server;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        // Single instance lock
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            if let Some(url) = argv.iter().find(|arg| arg.starts_with("stremio://")) {
                handle_stremio_url(app, url);
            }
        }))
        // Deep link protocol
        .plugin(tauri_plugin_deep_link::init())
        // Shell plugin (for sidecar process management)
        .plugin(tauri_plugin_shell::init())
        // Manage streaming server state
        .manage(streaming_server::ServerState {
            child: Mutex::new(None),
        })
        // Register commands
        .invoke_handler(tauri::generate_handler![
            commands::toggle_devtools,
            commands::start_streaming_server,
            commands::stop_streaming_server,
            commands::restart_streaming_server,
            commands::get_streaming_server_status,
        ])
        .setup(|app| {
            let bridge_js = include_str!("../scripts/bridge.js");

            let window = tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::External("https://web.stremio.com/".parse().unwrap()),
            )
            .title("Stremio Lightning")
            .inner_size(1500.0, 850.0)
            .center()
            .resizable(true)
            .maximizable(true)
            .initialization_script(bridge_js)
            .build()?;

            // Track window state changes (only emit on actual change)
            let was_maximized = Arc::new(AtomicBool::new(false));
            let was_fullscreen = Arc::new(AtomicBool::new(false));

            let window_clone = window.clone();
            let max_flag = was_maximized.clone();
            let fs_flag = was_fullscreen.clone();
            let app_handle_for_close = app.handle().clone();

            window.on_window_event(move |event| {
                match event {
                    tauri::WindowEvent::Resized(_) => {
                        if let Ok(is_maximized) = window_clone.is_maximized() {
                            let prev = max_flag.swap(is_maximized, Ordering::Relaxed);
                            if is_maximized != prev {
                                let _ = window_clone.emit("window-maximized-changed", is_maximized);
                            }
                        }
                        if let Ok(is_fullscreen) = window_clone.is_fullscreen() {
                            let prev = fs_flag.swap(is_fullscreen, Ordering::Relaxed);
                            if is_fullscreen != prev {
                                let _ = window_clone.emit("window-fullscreen-changed", is_fullscreen);
                            }
                        }
                    }
                    tauri::WindowEvent::CloseRequested { .. } => {
                        // Graceful shutdown: kill the streaming server
                        let _ = streaming_server::stop_server(&app_handle_for_close);
                    }
                    _ => {}
                }
            });

            // Auto-start streaming server
            let app_handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));

                match streaming_server::start_server(&app_handle) {
                    Ok(()) => {
                        // After server starts, tell the Stremio web UI to reconnect
                        let app_for_reload = app_handle.clone();
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                            if let Some(window) = app_for_reload.get_webview_window("main") {
                                let _ = window.eval(
                                    "if (typeof core !== 'undefined' && core.transport) { \
                                        core.transport.dispatch({ action: 'StreamingServer', args: { action: 'Reload' } }); \
                                    }"
                                );
                            }
                        });
                    }
                    Err(e) => {
                        eprintln!("Failed to start streaming server: {}", e);
                    }
                }
            });

            // Handle stremio:// URL from launch args
            // TODO: Replace fixed delay with a page-load-complete event
            let args: Vec<String> = std::env::args().collect();
            if let Some(url) = args.iter().find(|arg| arg.starts_with("stremio://")) {
                let url = url.clone();
                let app_handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    handle_stremio_url(&app_handle, &url);
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn handle_stremio_url(app: &tauri::AppHandle, url: &str) {
    if let Some(window) = app.get_webview_window("main") {
        if url.contains("/manifest.json") {
            let escaped = url.replace('\\', "\\\\").replace('\'', "\\'");
            let nav_js = format!(
                "window.location.href = 'https://web.stremio.com/#/addons?addon=' + encodeURIComponent('{}')",
                escaped
            );
            let _ = window.eval(&nav_js);
        }
    }
}
