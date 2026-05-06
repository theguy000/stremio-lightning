#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
fn apply_linux_webkit_backend_workarounds() {
    // WebKitGTK can crash on some Wayland compositors/drivers with:
    //   Gdk-Message: Error 71 (Protocol error) dispatching to Wayland display.
    // Set these before Tauri/GTK initializes so the webview uses the more stable
    // X11/XWayland backend and avoids the DMA-BUF renderer path that commonly
    // triggers Wayland protocol disconnects.
    //
    // Users who explicitly want to test native Wayland can launch with:
    //   STREMIO_LIGHTNING_USE_WAYLAND=1 npm run tauri dev
    if std::env::var_os("STREMIO_LIGHTNING_USE_WAYLAND").is_none()
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
    {
        std::env::set_var("GDK_BACKEND", "x11");
    }

    std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
}

#[cfg(not(target_os = "linux"))]
fn apply_linux_webkit_backend_workarounds() {}

fn main() {
    apply_linux_webkit_backend_workarounds();
    stremio_lightning_lib::run()
}
