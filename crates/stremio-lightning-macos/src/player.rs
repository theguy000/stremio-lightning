use serde_json::{json, Value};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
pub use stremio_lightning_core::player_api::{
    handle_transport, NativePlayerStatus, PlayerAction, PlayerBackend as TransportPlayerBackend,
};
use stremio_lightning_core::player_api::{
    EndFileCause, PlayerEnded, PlayerEndedError, PlayerEvent, PlayerPropertyChange,
};

/// A native player that also queues the events it raises for the host to drain.
/// Transport requests go through the core [`TransportPlayerBackend`] operations.
pub trait PlayerBackend: TransportPlayerBackend + Clone {
    fn drain_events(&self) -> Result<Vec<PlayerEvent>, String>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum MpvBackendCommand {
    ObserveProperty(String),
    SetProperty { name: String, value: Value },
    Command { name: String, args: Vec<String> },
    Stop,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpvOption {
    pub name: &'static str,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MpvVideoLayerHandle {
    raw_view: usize,
}

impl MpvVideoLayerHandle {
    pub fn new(raw_view: usize) -> Result<Self, String> {
        if raw_view == 0 {
            Err("macOS MPV video layer handle cannot be null".to_string())
        } else {
            Ok(Self { raw_view })
        }
    }

    pub fn raw_view(self) -> usize {
        self.raw_view
    }
}

pub fn default_mpv_options(app_name: &str, debug: bool) -> Vec<MpvOption> {
    vec![
        MpvOption {
            name: "title",
            value: app_name.to_string(),
        },
        MpvOption {
            name: "audio-client-name",
            value: app_name.to_string(),
        },
        MpvOption {
            name: "terminal",
            value: "yes".to_string(),
        },
        MpvOption {
            name: "msg-level",
            value: if debug {
                "all=no,cplayer=debug"
            } else {
                "all=no"
            }
            .to_string(),
        },
        MpvOption {
            name: "quiet",
            value: "yes".to_string(),
        },
        MpvOption {
            name: "hwdec",
            value: "yes".to_string(),
        },
        MpvOption {
            name: "audio-fallback-to-null",
            value: "yes".to_string(),
        },
        MpvOption {
            name: "cache",
            value: "yes".to_string(),
        },
    ]
}

#[derive(Debug, Default, Clone)]
pub struct FakePlayerBackend {
    actions: Arc<Mutex<Vec<PlayerAction>>>,
    events: Arc<Mutex<Vec<PlayerEvent>>>,
    initialized: bool,
}

impl FakePlayerBackend {
    pub fn initialized() -> Self {
        Self {
            actions: Arc::default(),
            events: Arc::default(),
            initialized: true,
        }
    }

    pub fn stopped(&self) -> bool {
        self.actions()
            .iter()
            .any(|action| matches!(action, PlayerAction::Stop))
    }

    pub fn actions(&self) -> Vec<PlayerAction> {
        self.actions
            .lock()
            .expect("fake macOS player actions poisoned")
            .clone()
    }

    pub fn push_event(&self, event: PlayerEvent) -> Result<(), String> {
        self.events.lock().map_err(|e| e.to_string())?.push(event);
        Ok(())
    }
}

impl TransportPlayerBackend for FakePlayerBackend {
    fn status(&self) -> NativePlayerStatus {
        NativePlayerStatus {
            enabled: true,
            initialized: self.initialized,
            backend: "fake".to_string(),
        }
    }

    fn observe_property(&self, name: String) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::ObserveProperty(name));
        Ok(())
    }

    fn set_property(&self, name: String, value: Value) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::SetProperty { name, value });
        Ok(())
    }

    fn command(&self, name: String, args: Vec<String>) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::Command { name, args });
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::Stop);
        Ok(())
    }
}

impl PlayerBackend for FakePlayerBackend {
    fn drain_events(&self) -> Result<Vec<PlayerEvent>, String> {
        Ok(std::mem::take(
            &mut *self.events.lock().map_err(|e| e.to_string())?,
        ))
    }
}

#[derive(Debug, Default, Clone)]
pub struct MpvPlayerBackend {
    initialized: Arc<Mutex<bool>>,
    sender: Arc<Mutex<Option<Sender<MpvBackendCommand>>>>,
    events: Arc<Mutex<Vec<PlayerEvent>>>,
}

impl MpvPlayerBackend {
    pub fn attach(&self, sender: Sender<MpvBackendCommand>) -> Result<(), String> {
        *self.sender.lock().map_err(|e| e.to_string())? = Some(sender);
        self.mark_initialized()
    }

    pub fn mark_initialized(&self) -> Result<(), String> {
        *self.initialized.lock().map_err(|e| e.to_string())? = true;
        Ok(())
    }

    pub fn push_event(&self, event: PlayerEvent) -> Result<(), String> {
        self.events.lock().map_err(|e| e.to_string())?.push(event);
        Ok(())
    }

    #[cfg(target_os = "macos")]
    pub fn attach_to_video_layer(
        &self,
        handle: MpvVideoLayerHandle,
        app_name: &str,
    ) -> Result<MacosMpvRenderer, String> {
        let (command_sender, command_receiver) = std::sync::mpsc::channel();
        let renderer = macos_mpv::spawn_renderer(
            handle,
            app_name.to_string(),
            command_receiver,
            self.events.clone(),
        )?;
        self.attach(command_sender)?;
        Ok(renderer)
    }

    fn send(&self, command: MpvBackendCommand) -> Result<(), String> {
        let sender = self
            .sender
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or_else(|| {
                "macOS MPV backend is not attached to a native video layer".to_string()
            })?;
        sender
            .send(command)
            .map_err(|e| format!("Failed to send command to macOS MPV backend: {e}"))
    }
}

#[cfg(target_os = "macos")]
pub struct MacosMpvRenderer {
    sender: Sender<MpvBackendCommand>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(target_os = "macos")]
impl Drop for MacosMpvRenderer {
    fn drop(&mut self) {
        let _ = self.sender.send(MpvBackendCommand::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(target_os = "macos")]
mod macos_mpv {
    use super::*;
    use libmpv2::{
        events::{Event, PropertyData},
        mpv_end_file_reason, Format, Mpv, SetData,
    };
    use std::sync::mpsc::{Receiver, TryRecvError};

    pub fn spawn_renderer(
        handle: MpvVideoLayerHandle,
        app_name: String,
        command_receiver: Receiver<MpvBackendCommand>,
        events: Arc<Mutex<Vec<PlayerEvent>>>,
    ) -> Result<MacosMpvRenderer, String> {
        let mpv = create_mpv(handle, &app_name)?;
        let (shutdown_sender, shutdown_receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            run_renderer_loop(mpv, command_receiver, shutdown_receiver, events);
        });

        Ok(MacosMpvRenderer {
            sender: shutdown_sender,
            thread: Some(thread),
        })
    }

    fn create_mpv(handle: MpvVideoLayerHandle, app_name: &str) -> Result<Mpv, String> {
        Mpv::with_initializer(|initializer| {
            initializer.set_property("wid", handle.raw_view() as i64)?;
            for option in default_mpv_options(app_name, cfg!(debug_assertions)) {
                initializer.set_property(option.name, option.value.as_str())?;
            }
            Ok(())
        })
        .map_err(|error| format!("Failed to initialize macOS MPV backend: {error}"))
    }

    fn run_renderer_loop(
        mut mpv: Mpv,
        command_receiver: Receiver<MpvBackendCommand>,
        shutdown_receiver: Receiver<MpvBackendCommand>,
        events: Arc<Mutex<Vec<PlayerEvent>>>,
    ) {
        let _ = mpv.disable_deprecated_events();

        loop {
            if should_shutdown(&mpv, &command_receiver, &shutdown_receiver) {
                return;
            }

            if let Some(event) = mpv.wait_event(0.05) {
                let event = match event {
                    Ok(event) => event,
                    Err(error) => {
                        push_event(
                            &events,
                            PlayerEvent::Ended(PlayerEnded {
                                reason: "error".to_string(),
                                error: Some(PlayerEndedError {
                                    message: format!("macOS MPV event error: {error:?}"),
                                    critical: true,
                                }),
                            }),
                        );
                        continue;
                    }
                };

                match player_event_from_mpv_event(event) {
                    MpvEventAction::Emit(event) => push_event(&events, event),
                    MpvEventAction::Continue => {}
                    MpvEventAction::Shutdown => return,
                }
            }
        }
    }

    fn should_shutdown(
        mpv: &Mpv,
        command_receiver: &Receiver<MpvBackendCommand>,
        shutdown_receiver: &Receiver<MpvBackendCommand>,
    ) -> bool {
        drain_commands(mpv, shutdown_receiver) || drain_commands(mpv, command_receiver)
    }

    fn drain_commands(mpv: &Mpv, receiver: &Receiver<MpvBackendCommand>) -> bool {
        loop {
            match receiver.try_recv() {
                Ok(MpvBackendCommand::Shutdown) => {
                    let _ = mpv.command("quit", &[]);
                    return true;
                }
                Ok(command) => {
                    if let Err(error) = handle_mpv_command(mpv, command) {
                        stremio_lightning_core::logging::error(
                            "native.player",
                            format!("[StremioLightning] macOS MPV command failed: {error}"),
                        );
                    }
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => return true,
            }
        }
    }

    enum MpvEventAction {
        Emit(PlayerEvent),
        Continue,
        Shutdown,
    }

    fn player_event_from_mpv_event(event: Event<'_>) -> MpvEventAction {
        match event {
            Event::PropertyChange { name, change, .. } => {
                MpvEventAction::Emit(PlayerEvent::PropertyChange(PlayerPropertyChange {
                    name: name.to_string(),
                    data: property_data_to_json(name, change),
                }))
            }
            Event::EndFile(reason) => {
                MpvEventAction::Emit(PlayerEvent::Ended(end_file_reason(reason)))
            }
            Event::Shutdown => MpvEventAction::Shutdown,
            _ => MpvEventAction::Continue,
        }
    }

    fn handle_mpv_command(mpv: &Mpv, command: MpvBackendCommand) -> Result<(), String> {
        match command {
            MpvBackendCommand::ObserveProperty(name) => {
                let format = observe_format(&name);
                mpv.observe_property(&name, format, 0).map_err(|error| {
                    format!("Failed to observe macOS MPV property '{name}': {error}")
                })?;
                mpv.wake_up();
                Ok(())
            }
            MpvBackendCommand::SetProperty { name, value } => set_property(mpv, &name, value),
            MpvBackendCommand::Command { name, args } => {
                let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                mpv.command(&name, &refs).map_err(|error| {
                    format!("Failed to execute macOS MPV command '{name}': {error}")
                })
            }
            MpvBackendCommand::Stop => mpv
                .command("stop", &[])
                .map_err(|error| format!("Failed to stop macOS MPV playback: {error}")),
            MpvBackendCommand::Shutdown => mpv
                .command("quit", &[])
                .map_err(|error| format!("Failed to shut down macOS MPV backend: {error}")),
        }
    }

    fn set_property(mpv: &Mpv, name: &str, value: Value) -> Result<(), String> {
        match value {
            Value::Bool(value) => set_property_value(mpv, name, value),
            Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    set_property_value(mpv, name, value)
                } else if let Some(value) = value.as_f64() {
                    set_property_value(mpv, name, value)
                } else {
                    Err(format!(
                        "Invalid numeric macOS MPV property value for '{name}'"
                    ))
                }
            }
            Value::String(value) => set_property_value(mpv, name, value),
            other => set_property_value(mpv, name, other.to_string()),
        }
    }

    fn set_property_value<T: SetData>(mpv: &Mpv, name: &str, value: T) -> Result<(), String> {
        mpv.set_property(name, value)
            .map_err(|error| format!("Failed to set macOS MPV property '{name}': {error}"))
    }

    fn observe_format(name: &str) -> Format {
        match name {
            "pause" | "paused-for-cache" | "seeking" | "eof-reached" | "keepaspect" => Format::Flag,
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
        PlayerEnded::from_cause(match reason {
            mpv_end_file_reason::Eof => EndFileCause::Eof,
            mpv_end_file_reason::Stop => EndFileCause::Stop,
            mpv_end_file_reason::Redirect => EndFileCause::Redirect,
            mpv_end_file_reason::Error => EndFileCause::Error,
            mpv_end_file_reason::Quit => EndFileCause::Quit,
            _ => EndFileCause::Other,
        })
    }

    fn push_event(events: &Arc<Mutex<Vec<PlayerEvent>>>, event: PlayerEvent) {
        if let Ok(mut events) = events.lock() {
            events.push(event);
        }
    }

    trait MpvWakeUp {
        fn wake_up(&self);
    }

    impl MpvWakeUp for Mpv {
        fn wake_up(&self) {
            unsafe { libmpv2_sys::mpv_wakeup(self.ctx.as_ptr()) }
        }
    }
}

impl TransportPlayerBackend for MpvPlayerBackend {
    fn status(&self) -> NativePlayerStatus {
        NativePlayerStatus {
            enabled: true,
            initialized: self.initialized.lock().map(|guard| *guard).unwrap_or(false),
            backend: "libmpv-macos".to_string(),
        }
    }

    fn observe_property(&self, name: String) -> Result<(), String> {
        self.send(MpvBackendCommand::ObserveProperty(name))
    }

    fn set_property(&self, name: String, value: Value) -> Result<(), String> {
        self.send(MpvBackendCommand::SetProperty { name, value })
    }

    fn command(&self, name: String, args: Vec<String>) -> Result<(), String> {
        self.send(MpvBackendCommand::Command { name, args })
    }

    fn stop(&self) -> Result<(), String> {
        self.send(MpvBackendCommand::Stop)
    }
}

impl PlayerBackend for MpvPlayerBackend {
    fn drain_events(&self) -> Result<Vec<PlayerEvent>, String> {
        Ok(std::mem::take(
            &mut *self.events.lock().map_err(|e| e.to_string())?,
        ))
    }
}

pub fn serialize_property_change(name: impl Into<String>, data: Value) -> Value {
    PlayerEvent::PropertyChange(PlayerPropertyChange {
        name: name.into(),
        data,
    })
    .transport_args()
}

pub fn serialize_ended(reason: impl Into<String>) -> Value {
    PlayerEvent::Ended(PlayerEnded {
        reason: reason.into(),
        error: None,
    })
    .transport_args()
}

pub fn serialize_error(message: impl Into<String>) -> Value {
    PlayerEvent::Ended(PlayerEnded {
        reason: "error".to_string(),
        error: Some(PlayerEndedError {
            message: message.into(),
            critical: true,
        }),
    })
    .transport_args()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VideoVisibilityState {
    visible: bool,
}

impl VideoVisibilityState {
    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn apply_command(&mut self, command: &MpvBackendCommand) {
        match command {
            MpvBackendCommand::Command { name, args } if name == "loadfile" && !args.is_empty() => {
                self.visible = true;
            }
            MpvBackendCommand::Stop | MpvBackendCommand::Shutdown => {
                self.visible = false;
            }
            _ => {}
        }
    }

    pub fn apply_event(&mut self, event: &PlayerEvent) {
        match event {
            PlayerEvent::PropertyChange(change) if change.name == "video-params" => {
                self.visible = !change.data.is_null() && change.data != json!(false);
            }
            PlayerEvent::Ended(_) => {
                self.visible = false;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_player_events() {
        assert_eq!(
            serialize_property_change("pause", json!(true)),
            json!(["mpv-prop-change", {"name": "pause", "data": true}])
        );
        assert_eq!(
            serialize_ended("eof"),
            json!(["mpv-event-ended", {"reason": "eof"}])
        );
        assert_eq!(
            serialize_error("MPV playback error"),
            json!([
                "mpv-event-ended",
                {"reason": "error", "error": {"message": "MPV playback error", "critical": true}}
            ])
        );
    }

    #[test]
    fn mpv_backend_forwards_to_attached_video_layer() {
        let backend = MpvPlayerBackend::default();
        let (sender, receiver) = std::sync::mpsc::channel();
        backend.attach(sender).unwrap();

        backend
            .command(
                "loadfile".to_string(),
                vec!["file:///tmp/a.mp4".to_string()],
            )
            .unwrap();

        assert_eq!(
            receiver.recv().unwrap(),
            MpvBackendCommand::Command {
                name: "loadfile".to_string(),
                args: vec!["file:///tmp/a.mp4".to_string()],
            }
        );
        assert!(backend.status().initialized);
    }

    #[test]
    fn unattached_mpv_backend_rejects_commands() {
        let backend = MpvPlayerBackend::default();
        assert_eq!(
            backend.stop().unwrap_err(),
            "macOS MPV backend is not attached to a native video layer"
        );
    }

    #[test]
    fn video_visibility_tracks_start_video_detection_end_and_error() {
        let mut state = VideoVisibilityState::default();
        assert!(!state.visible());

        state.apply_command(&MpvBackendCommand::Command {
            name: "loadfile".to_string(),
            args: vec!["file:///tmp/sample.mp4".to_string()],
        });
        assert!(state.visible());

        state.apply_event(&PlayerEvent::PropertyChange(PlayerPropertyChange {
            name: "video-params".to_string(),
            data: Value::Null,
        }));
        assert!(!state.visible());

        state.apply_event(&PlayerEvent::PropertyChange(PlayerPropertyChange {
            name: "video-params".to_string(),
            data: json!({"w": 1920, "h": 1080}),
        }));
        assert!(state.visible());

        state.apply_event(&PlayerEvent::Ended(PlayerEnded {
            reason: "eof".to_string(),
            error: None,
        }));
        assert!(!state.visible());

        state.apply_command(&MpvBackendCommand::Command {
            name: "loadfile".to_string(),
            args: vec!["https://example.com/stream.mkv".to_string()],
        });
        state.apply_event(&PlayerEvent::Ended(PlayerEnded {
            reason: "error".to_string(),
            error: Some(PlayerEndedError {
                message: "MPV playback error".to_string(),
                critical: true,
            }),
        }));
        assert!(!state.visible());
    }

    #[test]
    fn default_mpv_options_match_macos_shell_defaults() {
        let options = default_mpv_options("Stremio Lightning", true);
        assert!(options.contains(&MpvOption {
            name: "audio-client-name",
            value: "Stremio Lightning".to_string(),
        }));
        assert!(options.contains(&MpvOption {
            name: "hwdec",
            value: "yes".to_string(),
        }));
        assert!(options.contains(&MpvOption {
            name: "audio-fallback-to-null",
            value: "yes".to_string(),
        }));
        assert!(options.contains(&MpvOption {
            name: "msg-level",
            value: "all=no,cplayer=debug".to_string(),
        }));
    }

    #[test]
    fn mpv_video_layer_handle_rejects_null_views() {
        assert_eq!(
            MpvVideoLayerHandle::new(0).unwrap_err(),
            "macOS MPV video layer handle cannot be null"
        );
        assert_eq!(MpvVideoLayerHandle::new(42).unwrap().raw_view(), 42);
    }
}
