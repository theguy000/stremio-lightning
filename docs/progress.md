# Servo Migration Progress — Linux Only

## Already Complete (from prior branch work)

- [x] YouTube trailer intercept bridge script (`web/bridge/src/youtube-intercept.js`)
- [x] YouTube intercept wired into `InjectionBundle` in `webview_runtime.rs`
- [x] `ytdl` set to `no` on MPV player (`native_window.rs` L956)
- [x] `--engine` CLI parameter parsing in `app.rs` (`WebviewEngine` enum + `parse_args`)
- [x] `servo-engine` cargo feature flag declared in `Cargo.toml` (empty, no dep yet)

---

## Phase 1: Dual-Engine Scaffolding

- [x] 1.1 Create `WebviewShell` trait abstracting webview runtime operations (`load`, `dispatch_ipc`, `shutdown`, `script_source`, `drain_event_dispatch_scripts`, etc.)
- [x] 1.2 Implement `WebviewShell` for existing `LinuxWebviewRuntime` (WebKit backend)
- [x] 1.3 Create stub `ServoWebviewRuntime` behind `#[cfg(feature = "servo-engine")]` implementing `WebviewShell`
- [x] 1.4 Wire `config.engine` into `run()` to branch between WebKit and Servo runtime paths
- [ ] 1.5 Add `servo` crate as optional dependency in `Cargo.toml` behind `servo-engine` feature (deferred to actual Servo compilation step)
- [x] 1.6 Add `servo_runtime` module to `lib.rs` gated by `#[cfg(feature = "servo-engine")]`

## Phase 4: Polyfill & Styling Injection

- [x] 4.1 Create `web/bridge/servo-compat.css` with CSS Grid → Flexbox fallback rules
- [x] 4.2 Create `web/bridge/polyfills.js` with `IntersectionObserver` stub polyfill
- [x] 4.3 Wire `servo-compat.css` and `polyfills.js` into `InjectionBundle` conditionally for Servo engine
- [x] 4.4 Add Servo-specific `User-Agent` header configuration in `ServoWebviewRuntime`

## Phase 2: FFI & IPC Bindings (stubs for now)

- [x] 2.1 Define Servo initialization parameters struct (`ServoConfig`)
- [x] 2.2 Stub background thread initialization for Servo instance
- [x] 2.3 Stub IPC message routing from Servo JS → `dispatch_ipc`

## Phase 3: Unified Wgpu Compositing (stubs for now)

- [x] 3.1 Define `ServoRenderPlan` extending `RenderLoopPlan` with Servo compositing steps
- [x] 3.2 Stub `winit` window loop creation for Servo mode
- [x] 3.3 Document compositing pipeline: `[clear] → [MPV texture] → [Servo WebRender overlay]`

## Phase 5: Linux Packaging

- [x] 5.1 Document Flatpak sandbox extension requirements for Servo runtime (see [flatpak-servo-requirements.md](file:///home/istiak/git/stremio-lightning/docs/flatpak-servo-requirements.md))

## Verification

- [x] V.1 `cargo check --features "servo-engine" -p stremio-lightning-linux` passes
- [x] V.2 `cargo test -p stremio-lightning-linux` passes (no regressions)
- [x] V.3 Unit tests for `WebviewShell` trait dispatch
- [x] V.4 Unit tests for `ServoWebviewRuntime` stub initialization
