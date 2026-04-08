mod commands;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::Emitter;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        // Single instance lock: focus existing window if second instance launches
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            // Handle deep link from second instance
            if let Some(url) = argv.iter().find(|arg| arg.starts_with("stremio://")) {
                handle_stremio_url(app, url);
            }
        }))
        // Deep link protocol: stremio://
        .plugin(tauri_plugin_deep_link::init())
        // Register custom commands
        .invoke_handler(tauri::generate_handler![
            commands::toggle_devtools,
        ])
        .setup(|app| {
            // Create main window programmatically (allows initialization_script)
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

            // Track previous window state to only emit on actual changes
            let was_maximized = Arc::new(AtomicBool::new(false));
            let was_fullscreen = Arc::new(AtomicBool::new(false));

            let window_clone = window.clone();
            let max_flag = was_maximized.clone();
            let fs_flag = was_fullscreen.clone();

            window.on_window_event(move |event| {
                if let tauri::WindowEvent::Resized(_) = event {
                    // Emit maximized change only when state actually changes
                    if let Ok(is_maximized) = window_clone.is_maximized() {
                        let prev = max_flag.swap(is_maximized, Ordering::Relaxed);
                        if is_maximized != prev {
                            let _ = window_clone.emit("window-maximized-changed", is_maximized);
                        }
                    }
                    // Emit fullscreen change only when state actually changes
                    if let Ok(is_fullscreen) = window_clone.is_fullscreen() {
                        let prev = fs_flag.swap(is_fullscreen, Ordering::Relaxed);
                        if is_fullscreen != prev {
                            let _ = window_clone.emit("window-fullscreen-changed", is_fullscreen);
                        }
                    }
                }
            });

            // Handle stremio:// URL from launch args (Windows/Linux)
            // TODO: Replace fixed delay with a page-load-complete event for robustness
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

/// Navigate the webview to install an addon from a stremio:// deep link URL.
/// Only handles addon manifest URLs for now (Phase 1 scope).
fn handle_stremio_url(app: &tauri::AppHandle, url: &str) {
    if let Some(window) = app.get_webview_window("main") {
        if url.contains("/manifest.json") {
            // Escape backslashes first, then single quotes, to prevent JS injection
            let escaped = url.replace('\\', "\\\\").replace('\'', "\\'");
            let nav_js = format!(
                "window.location.href = 'https://web.stremio.com/#/addons?addon=' + encodeURIComponent('{}')",
                escaped
            );
            let _ = window.eval(&nav_js);
        }
    }
}
