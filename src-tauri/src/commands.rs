use tauri::WebviewWindow;

use crate::streaming_server;

#[tauri::command]
pub fn toggle_devtools(window: WebviewWindow) {
    if window.is_devtools_open() {
        window.close_devtools();
    } else {
        window.open_devtools();
    }
}

#[tauri::command]
pub async fn start_streaming_server(app: tauri::AppHandle) -> Result<(), String> {
    streaming_server::start_server(&app)
}

#[tauri::command]
pub async fn stop_streaming_server(app: tauri::AppHandle) -> Result<(), String> {
    streaming_server::stop_server(&app)
}

#[tauri::command]
pub async fn restart_streaming_server(app: tauri::AppHandle) -> Result<(), String> {
    let _ = streaming_server::stop_server(&app);
    std::thread::sleep(std::time::Duration::from_millis(500));
    streaming_server::start_server(&app)
}

#[tauri::command]
pub async fn get_streaming_server_status(app: tauri::AppHandle) -> bool {
    streaming_server::is_server_running(&app)
}
