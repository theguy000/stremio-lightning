#![cfg_attr(windows, allow(unsafe_code))]

#[cfg(windows)]
pub use platform::PlayerBackend;

#[cfg(not(windows))]
pub use platform::PlayerBackend;

#[cfg(windows)]
mod platform {
    use super::super::types::{BackendCommand, MpvEventAction, NativePlayerStatus, PlayerError};
    use crate::window::UiThreadNotifier;
    use libmpv2::{
        events::{Event, PropertyData},
        mpv_end_file_reason, Format, Mpv, SetData,
    };
    use serde_json::{json, Value};
    use std::ffi::CString;
    use std::ptr;
    use std::sync::atomic::{AtomicPtr, Ordering};
    use std::sync::mpsc::{Receiver, Sender, TryRecvError};
    use std::sync::Arc;
    use std::thread::{self, JoinHandle};
    use stremio_lightning_core::player_api::{
        PlayerCommand, PlayerEnded, PlayerEndedError, PlayerEvent, PlayerPropertyChange,
    };
    use windows::Win32::Foundation::HWND;

    // `AtomicPtr` is `Send + Sync` and `mpv_wakeup` is documented as safe to call from any
    // thread, so the slot needs no lock and no manual thread-safety impls.
    struct MpvWaker {
        ctx: AtomicPtr<libmpv2_sys::mpv_handle>,
    }

    impl MpvWaker {
        fn new(mpv: &Mpv) -> Self {
            Self {
                ctx: AtomicPtr::new(mpv.ctx.as_ptr()),
            }
        }

        fn wake(&self) {
            let ctx = self.ctx.load(Ordering::Acquire);
            if !ctx.is_null() {
                // SAFETY: `mpv_wakeup` is thread-safe and the handle stays valid until
                // `invalidate` clears the slot, which happens before the owning `Mpv` is
                // dropped. The atomic load only keeps the pointer read indivisible.
                unsafe { libmpv2_sys::mpv_wakeup(ctx) };
            }
        }

        fn invalidate(&self) {
            self.ctx.store(ptr::null_mut(), Ordering::Release);
        }
    }

    struct WakerGuard(Arc<MpvWaker>);

    impl Drop for WakerGuard {
        fn drop(&mut self) {
            self.0.invalidate();
        }
    }

    #[derive(Default)]
    pub struct PlayerBackend {
        sender: Option<Sender<BackendCommand>>,
        receiver: Option<Receiver<PlayerEvent>>,
        waker: Option<Arc<MpvWaker>>,
        thread: Option<JoinHandle<()>>,
        initialized: bool,
    }

    impl std::fmt::Debug for PlayerBackend {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PlayerBackend")
                .field("initialized", &self.initialized)
                .finish_non_exhaustive()
        }
    }

    impl PlayerBackend {
        pub fn initialize(
            &mut self,
            hwnd: HWND,
            notifier: UiThreadNotifier,
        ) -> Result<(), PlayerError> {
            if self.initialized {
                return Ok(());
            }

            let mpv = create_mpv(hwnd)?;
            let waker = Arc::new(MpvWaker::new(&mpv));
            let (command_sender, command_receiver) = std::sync::mpsc::channel();
            let (event_sender, event_receiver) = std::sync::mpsc::channel();

            self.thread = Some(spawn_player_thread(
                mpv,
                command_receiver,
                event_sender,
                notifier,
                Arc::clone(&waker),
            ));
            self.sender = Some(command_sender);
            self.receiver = Some(event_receiver);
            self.waker = Some(waker);
            self.initialized = true;
            stremio_lightning_core::logging::info(
                "native.player",
                "[StremioLightning] Windows MPV backend initialized",
            );
            Ok(())
        }

        pub fn status(&self) -> NativePlayerStatus {
            NativePlayerStatus {
                enabled: true,
                initialized: self.initialized,
                backend: "webview2-libmpv",
            }
        }

        pub fn handle_command(&self, command: PlayerCommand) -> Result<(), PlayerError> {
            let Some(sender) = &self.sender else {
                return Ok(());
            };
            sender
                .send(BackendCommand::Player(command))
                .map_err(|error| PlayerError::SendCommand(error.to_string()))?;
            if let Some(waker) = &self.waker {
                waker.wake();
            }
            Ok(())
        }

        pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
            let Some(receiver) = self.receiver.as_ref() else {
                return Vec::new();
            };
            receiver.try_iter().collect()
        }

        pub fn shutdown(&mut self) {
            if let Some(sender) = self.sender.take() {
                let _ = sender.send(BackendCommand::Shutdown);
            }
            if let Some(waker) = self.waker.take() {
                waker.wake();
            }
            // Do not join thread to prevent circular deadlock during window destruction
            self.thread.take();
            self.initialized = false;
        }
    }

    impl Drop for PlayerBackend {
        fn drop(&mut self) {
            self.shutdown();
        }
    }

    fn create_mpv(hwnd: HWND) -> Result<Mpv, PlayerError> {
        let mpv = Mpv::with_initializer(|initializer| {
            initializer.set_property("wid", hwnd.0 as i64)?;
            initializer.set_property("title", crate::APP_NAME)?;
            initializer.set_property("audio-client-name", crate::APP_NAME)?;
            initializer.set_property("terminal", "yes")?;
            initializer.set_property(
                "msg-level",
                if cfg!(debug_assertions) {
                    "all=no,cplayer=debug"
                } else {
                    "all=no"
                },
            )?;
            initializer.set_property("quiet", "yes")?;
            initializer.set_property("hwdec", "yes")?;
            initializer.set_property("audio-fallback-to-null", "yes")?;
            Ok(())
        })
        .map_err(|error| PlayerError::Initialization(error.to_string()))?;

        if let Err(error) = request_mpv_log_messages(&mpv) {
            stremio_lightning_core::logging::warn("native.player", error.to_string());
        }
        Ok(mpv)
    }

    fn request_mpv_log_messages(mpv: &Mpv) -> Result<(), PlayerError> {
        let level = CString::new("warn").expect("static MPV log level must not contain NUL");
        // SAFETY: `mpv.ctx` points to an initialized MPV instance, and `level` is a static
        // null-terminated C string. `mpv_request_log_messages` safely configures MPV logging.
        let result =
            unsafe { libmpv2_sys::mpv_request_log_messages(mpv.ctx.as_ptr(), level.as_ptr()) };
        if result < 0 {
            return Err(PlayerError::Diagnostics(format!(
                "Failed to enable Windows MPV diagnostics (error code {result})"
            )));
        }
        Ok(())
    }

    fn spawn_player_thread(
        mut mpv: Mpv,
        command_receiver: Receiver<BackendCommand>,
        event_sender: Sender<PlayerEvent>,
        notifier: UiThreadNotifier,
        waker: Arc<MpvWaker>,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let _ = mpv.disable_deprecated_events();
            let _waker_guard = WakerGuard(waker);

            loop {
                if should_shutdown(&mpv, &command_receiver) {
                    break;
                }

                if let Some(event) = mpv.wait_event(-1.0) {
                    let event = match event {
                        Ok(event) => event,
                        Err(error) => {
                            stremio_lightning_core::logging::error(
                                "native.player",
                                format!("[StremioLightning] MPV event error: {error:?}"),
                            );
                            continue;
                        }
                    };

                    match player_event_from_mpv_event(event) {
                        MpvEventAction::Emit(player_event) => {
                            if event_sender.send(player_event).is_ok() {
                                let _ = notifier.notify();
                            }
                        }
                        MpvEventAction::Continue => {}
                        MpvEventAction::Shutdown => break,
                    }
                }
            }
        })
    }

    fn should_shutdown(mpv: &Mpv, command_receiver: &Receiver<BackendCommand>) -> bool {
        loop {
            match command_receiver.try_recv() {
                Ok(BackendCommand::Player(command)) => {
                    if let Err(error) = handle_mpv_command(mpv, command) {
                        stremio_lightning_core::logging::error(
                            "native.player",
                            format!("[StremioLightning] MPV command failed: {error}"),
                        );
                    }
                }
                Ok(BackendCommand::Shutdown) => {
                    let _ = mpv.command("quit", &[]);
                    return true;
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => return true,
            }
        }
    }

    fn player_event_from_mpv_event(event: Event<'_>) -> MpvEventAction {
        match event {
            Event::PropertyChange { name, change, .. } => {
                log_diagnostic_property_change(name, &change);
                MpvEventAction::Emit(PlayerEvent::PropertyChange(PlayerPropertyChange {
                    name: name.to_string(),
                    data: property_data_to_json(name, change),
                }))
            }
            Event::EndFile(reason) => {
                log_end_file(reason);
                MpvEventAction::Emit(PlayerEvent::Ended(end_file_reason(reason)))
            }
            Event::StartFile => {
                stremio_lightning_core::logging::info(
                    "native.player",
                    "[StremioLightning] MPV started opening media",
                );
                MpvEventAction::Continue
            }
            Event::FileLoaded => {
                stremio_lightning_core::logging::info(
                    "native.player",
                    "[StremioLightning] MPV media loaded",
                );
                MpvEventAction::Continue
            }
            Event::PlaybackRestart => {
                stremio_lightning_core::logging::info(
                    "native.player",
                    "[StremioLightning] MPV playback started or resumed",
                );
                MpvEventAction::Continue
            }
            Event::LogMessage {
                prefix,
                level,
                text,
                ..
            } => {
                log_mpv_message(prefix, level, text);
                MpvEventAction::Continue
            }
            Event::QueueOverflow => {
                stremio_lightning_core::logging::warn(
                    "native.player",
                    "[StremioLightning] MPV event queue overflowed",
                );
                MpvEventAction::Continue
            }
            Event::Shutdown => MpvEventAction::Shutdown,
            _ => MpvEventAction::Continue,
        }
    }

    fn log_diagnostic_property_change(name: &str, change: &PropertyData<'_>) {
        let PropertyData::Flag(value) = change else {
            return;
        };
        let message = match name {
            "paused-for-cache" if *value => "[StremioLightning] MPV buffering: waiting for cache",
            "paused-for-cache" => "[StremioLightning] MPV buffering ended",
            "seeking" if *value => "[StremioLightning] MPV seek started",
            "seeking" => "[StremioLightning] MPV seek ended",
            _ => return,
        };
        if name == "paused-for-cache" && *value {
            stremio_lightning_core::logging::warn("native.player", message);
        } else {
            stremio_lightning_core::logging::info("native.player", message);
        }
    }

    fn log_end_file(reason: libmpv2::EndFileReason) {
        let reason_name = match reason {
            mpv_end_file_reason::Eof => "eof",
            mpv_end_file_reason::Stop => "stop",
            mpv_end_file_reason::Quit => "quit",
            mpv_end_file_reason::Error => "error",
            mpv_end_file_reason::Redirect => "redirect",
            _ => "unknown",
        };
        let message = format!("[StremioLightning] MPV playback ended: {reason_name}");
        if reason == mpv_end_file_reason::Error {
            stremio_lightning_core::logging::error("native.player", message);
        } else {
            stremio_lightning_core::logging::info("native.player", message);
        }
    }

    fn log_mpv_message(prefix: &str, level: &str, text: &str) {
        let text = super::super::sanitize_mpv_log_message(text);
        if text.is_empty() {
            return;
        }
        let message = format!("[StremioLightning] MPV {prefix}: {text}");
        match level {
            "fatal" | "error" => stremio_lightning_core::logging::error("native.player", message),
            "warn" => stremio_lightning_core::logging::warn("native.player", message),
            _ => stremio_lightning_core::logging::debug("native.player", message),
        }
    }

    fn handle_mpv_command(mpv: &Mpv, command: PlayerCommand) -> Result<(), PlayerError> {
        match command {
            PlayerCommand::ObserveProperty(name) => {
                let format = observe_format(&name);
                mpv.observe_property(&name, format, 0)
                    .map_err(|error| PlayerError::ObserveProperty(name, error.to_string()))
            }
            PlayerCommand::SetProperty(name, value) => set_property(mpv, &name, value),
            PlayerCommand::Command(values) => {
                let (name, args) = super::super::command_name_and_args(&values)?;
                let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                mpv.command(&name, &refs)
                    .map_err(|error| PlayerError::ExecuteCommand(name, error.to_string()))
            }
            PlayerCommand::Stop => mpv
                .command("stop", &[])
                .map_err(|error| PlayerError::Stop(error.to_string())),
        }
    }

    fn set_property(mpv: &Mpv, name: &str, value: Value) -> Result<(), PlayerError> {
        match value {
            Value::Bool(value) => set_property_value(mpv, name, value),
            Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    set_property_value(mpv, name, value)
                } else if let Some(value) = value.as_f64() {
                    set_property_value(mpv, name, value)
                } else {
                    Err(PlayerError::InvalidNumericPropertyValue(name.to_string()))
                }
            }
            Value::String(value) => set_property_value(mpv, name, value),
            other => set_property_value(mpv, name, other.to_string()),
        }
    }

    fn set_property_value<T: SetData>(
        mpv: &Mpv,
        name: &str,
        value: T,
    ) -> Result<(), PlayerError> {
        mpv.set_property(name, value)
            .map_err(|error| PlayerError::SetProperty(name.to_string(), error.to_string()))
    }

    fn observe_format(name: &str) -> Format {
        match name {
            "pause" | "paused-for-cache" | "seeking" | "eof-reached" | "keepaspect" => {
                Format::Flag
            }
            "aid" | "vid" | "sid" => Format::Int64,
            "time-pos"
            | "mute"
            | "volume"
            | "duration"
            | "sub-delay"
            | "sub-scale"
            | "cache-buffering-state"
            | "demuxer-cache-time"
            | "sub-pos"
            | "speed"
            | "panscan" => Format::Double,
            _ => Format::String,
        }
    }

    fn property_data_to_json(name: &str, data: PropertyData) -> Value {
        match data {
            PropertyData::Flag(value) => Value::Bool(value),
            PropertyData::Int64(value) => json!(value),
            PropertyData::Double(value) => json!(value),
            PropertyData::OsdStr(value) | PropertyData::Str(value) => {
                if matches!(name, "track-list" | "video-params" | "metadata") {
                    serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
                } else {
                    Value::String(value.to_string())
                }
            }
        }
    }

    fn end_file_reason(reason: libmpv2::EndFileReason) -> PlayerEnded {
        let is_error = reason == mpv_end_file_reason::Error;
        PlayerEnded {
            reason: match reason {
                mpv_end_file_reason::Error => "error",
                mpv_end_file_reason::Quit => "quit",
                _ => "other",
            }
            .to_string(),
            error: is_error.then(|| PlayerEndedError {
                message: "MPV playback error".to_string(),
                critical: true,
            }),
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::super::types::{NativePlayerStatus, PlayerError};
    use stremio_lightning_core::player_api::{PlayerCommand, PlayerEvent};

    #[derive(Debug, Default)]
    pub struct PlayerBackend;

    impl PlayerBackend {
        pub fn status(&self) -> NativePlayerStatus {
            NativePlayerStatus::default()
        }

        pub fn handle_command(&self, _command: PlayerCommand) -> Result<(), PlayerError> {
            Ok(())
        }

        pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
            Vec::new()
        }

        pub fn shutdown(&mut self) {}
    }
}
