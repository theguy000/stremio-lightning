mod commands;
mod discord_rpc;
mod mod_manager;
mod player;
mod shell_transport;
mod streaming_server;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tauri::webview::WebviewBuilder;
use tauri::window::WindowBuilder;
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        // Single instance lock
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(window) = app.get_window(player::MAIN_APP_LABEL) {
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
        // Opener plugin (for opening URLs in system browser)
        .plugin(tauri_plugin_opener::init())
        // Manage streaming server state
        .manage(streaming_server::ServerState {
            child: Mutex::new(None),
        })
        .manage(player::PlayerState::default())
        .manage(shell_transport::ShellTransportState::default())
        // Manage Discord RPC state
        .manage(discord_rpc::DiscordRpcState::default())
        // Manage mod manager state
        .manage(mod_manager::ModManagerState {
            registered_schemas: Mutex::new(HashMap::new()),
        })
        // Register commands
        .invoke_handler(tauri::generate_handler![
            commands::toggle_devtools,
            commands::open_external_url,
            commands::shell_transport_send,
            commands::shell_bridge_ready,
            commands::get_native_player_status,
            commands::start_streaming_server,
            commands::stop_streaming_server,
            commands::restart_streaming_server,
            commands::get_streaming_server_status,
            commands::get_plugins,
            commands::get_themes,
            commands::download_mod,
            commands::delete_mod,
            commands::get_mod_content,
            commands::get_registry,
            commands::check_mod_updates,
            commands::get_setting,
            commands::save_setting,
            commands::register_settings,
            commands::get_registered_settings,
            commands::start_discord_rpc,
            commands::stop_discord_rpc,
            commands::update_discord_activity,
        ])
        .setup(|app| {
            // Ensure mod directories exist
            if let Err(e) = mod_manager::ensure_dirs(app.handle()) {
                eprintln!("Failed to create mod directories: {}", e);
            }

            let native_player_flag_js = format!(
                "window.__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__ = {};",
                if player::native_player_enabled() { "true" } else { "false" }
            );
            let bridge_js = include_str!("../scripts/bridge.js");
            let mod_ui_js = include_str!("../scripts/mod-ui.js");

            let window = WindowBuilder::new(app, player::MAIN_APP_LABEL)
            .title("Stremio Lightning")
            .inner_size(1500.0, 850.0)
            .center()
            .resizable(true)
            .maximizable(true)
            .visible(false)
            .background_color(tauri::webview::Color(0, 0, 0, 255))
            .build()?;

            let webview = window.add_child(
                WebviewBuilder::new(
                    player::MAIN_APP_LABEL,
                    tauri::WebviewUrl::External("https://web.stremio.com/".parse().unwrap()),
                )
                .transparent(true)
                .auto_resize()
                .initialization_script(&native_player_flag_js)
                .initialization_script(bridge_js)
                .initialization_script(mod_ui_js),
                tauri::LogicalPosition::new(0.0, 0.0),
                window.inner_size()?,
            )?;
            webview.set_background_color(Some(tauri::webview::Color(0, 0, 0, 0)))?;
            window.show()?;

            if let Err(error) = player::initialize(app.handle()) {
                eprintln!("Failed to initialize native player: {error}");
                return Err(error.into());
            }

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
                                let _ = shell_transport::emit_window_state_change(&app_handle_for_close);
                            }
                        }
                        if let Ok(is_fullscreen) = window_clone.is_fullscreen() {
                            let prev = fs_flag.swap(is_fullscreen, Ordering::Relaxed);
                            if is_fullscreen != prev {
                                let _ = window_clone.emit("window-fullscreen-changed", is_fullscreen);
                                let _ = shell_transport::emit_window_visibility_change(&app_handle_for_close);
                            }
                        }
                    }
                    tauri::WindowEvent::CloseRequested { .. } => {
                        // Graceful shutdown: kill the streaming server and Discord RPC
                        let _ = streaming_server::stop_server(&app_handle_for_close);
                        let _ = player::stop_and_hide(&app_handle_for_close);
                        let discord_state = app_handle_for_close.state::<discord_rpc::DiscordRpcState>();
                        let _ = discord_rpc::stop(&discord_state);
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
                        // Wait for the bridge so reload is no longer tied to a blind sleep.
                        let app_for_reload = app_handle.clone();
                        std::thread::spawn(move || {
                            let _ = shell_transport::wait_until_bridge_ready(
                                &app_for_reload,
                                std::time::Duration::from_secs(15),
                            );
                            if let Some(webview) = app_for_reload.get_webview(player::MAIN_APP_LABEL) {
                                let _ = webview.eval(
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
                    let _ = shell_transport::wait_until_bridge_ready(
                        &app_handle,
                        std::time::Duration::from_secs(15),
                    );
                    handle_stremio_url(&app_handle, &url);
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn handle_stremio_url(app: &tauri::AppHandle, url: &str) {
    if let Some(webview) = app.get_webview(player::MAIN_APP_LABEL) {
        if url.contains("/manifest.json") {
            let escaped = url.replace('\\', "\\\\").replace('\'', "\\'");
            let nav_js = format!(
                "window.location.href = 'https://web.stremio.com/#/addons?addon=' + encodeURIComponent('{}')",
                escaped
            );
            let _ = webview.eval(&nav_js);
        } else {
            let _ = shell_transport::enqueue_open_media(app, url.to_string());
        }
    }
}
