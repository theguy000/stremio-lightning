# Flatpak Sandbox Extension Requirements for Servo Runtime

To package and run Stremio Lightning with the Servo Web Engine inside a Flatpak sandbox on Linux, several manifest configuration changes and runtime permissions are required. This document outlines the sandbox constraints and module definitions required to compile and run Servo hermetically.

---

## 1. Sandbox Permissions (`[Context]` / metadata)

Servo's rendering pipeline (wgpu + WebRender) and script/network layers require direct hardware access and socket communication.

Add or verify the following inside the Flatpak metadata context:

```ini
[Context]
# Graphics & Windowing
sockets=x11;wayland;pulseaudio;
devices=dri;

# Network & IPC (for the streaming server sidecar stremio-service)
shared=ipc;network;
```

* **`devices=dri;`**: Mandatory for WebRender to initialize OpenGL/Vulkan contexts on the GPU. Without this, Servo will fail to initialize `wgpu`.
* **`sockets=x11;wayland;`**: Ensures compatibility with both legacy X11 display servers and modern Wayland compositors under `winit`.

---

## 2. Flatpak Runtime and SDK

Servo is built on modern Rust and requires up-to-date graphics drivers.

* **Minimum SDK/Runtime**: `org.freedesktop.Sdk` and `org.freedesktop.Platform` version `23.08` (or newer).
* **Rust Extension**: The `org.freedesktop.Sdk.Extension.rust-stable` SDK extension is required to compile the Rust source during the flatpak build process.

---

## 3. Bundling Servo as a Module

Since Servo is not part of the standard Freedesktop runtime, its engine library (`libservo.so` or the statically linked Rust library) must be compiled and placed in the `/app/lib` directory.

### Build Manifest Example (`flatpak-builder` JSON)

```json
{
  "name": "servo",
  "buildsystem": "simple",
  "build-commands": [
    "cargo build --release -p servo",
    "install -Dm755 target/release/libservo.so /app/lib/libservo.so"
  ],
  "sources": [
    {
      "type": "git",
      "url": "https://github.com/servo/servo.git",
      "branch": "main"
    }
  ]
}
```

* **LD_LIBRARY_PATH**: The wrapper script at `/app/bin/stremio-lightning` must prepend `/app/lib` to `LD_LIBRARY_PATH` to ensure the dynamically loaded Servo library can be resolved:
  ```bash
  export LD_LIBRARY_PATH="/app/lib:${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  ```

---

## 4. GStreamer Sandbox Support

Servo delegates HTML5 video/audio playback to **GStreamer**. To ensure media playback operates correctly inside the sandbox:

1. The Flatpak runtime must include GStreamer plugins (`gst-plugins-base`, `gst-plugins-good`, `gst-plugins-bad`, `gst-plugins-ugly`).
2. GStreamer must have access to the hardware decoding APIs via standard Flatpak runtime extensions.
