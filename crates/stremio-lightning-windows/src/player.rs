use serde::Serialize;
#[cfg(windows)]
use serde_json::json;
use serde_json::Value;
#[cfg(windows)]
use std::ffi::CString;
#[cfg(windows)]
use std::sync::mpsc::{Receiver, Sender};
#[cfg(windows)]
use stremio_lightning_core::player_api::PlayerEndedError;
use stremio_lightning_core::player_api::{
    PlayerCommand, PlayerEnded, PlayerEvent, PlayerPropertyChange,
};

const PRIMARY_SUBTITLE_PROPERTY: &str = "sid";
const SECONDARY_SUBTITLE_PROPERTY: &str = "secondary-sid";
const SUB_ADD_COMMAND: &str = "sub-add";
const MAX_MPV_LOG_MESSAGE_LENGTH: usize = 2_048;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NativePlayerStatus {
    pub enabled: bool,
    pub initialized: bool,
    pub backend: &'static str,
}

impl Default for NativePlayerStatus {
    fn default() -> Self {
        Self {
            enabled: cfg!(windows),
            initialized: false,
            backend: "webview2-libmpv",
        }
    }
}

#[derive(Debug, Default)]
pub struct WindowsPlayer {
    commands: Vec<PlayerCommand>,
    events: Vec<PlayerEvent>,
    backend: platform::PlayerBackend,
}

impl WindowsPlayer {
    #[cfg(windows)]
    pub fn initialize(
        &mut self,
        hwnd: windows::Win32::Foundation::HWND,
        notifier: crate::window::UiThreadNotifier,
    ) -> Result<(), String> {
        self.backend.initialize(hwnd, notifier)
    }

    pub fn status(&self) -> NativePlayerStatus {
        self.backend.status()
    }

    pub fn handle_transport(&mut self, method: &str, payload: Option<Value>) -> Result<(), String> {
        let command = match method {
            "mpv-observe-prop" => PlayerCommand::ObserveProperty(
                payload
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
                    .ok_or_else(|| "Missing mpv-observe-prop payload".to_string())?,
            ),
            "mpv-set-prop" => {
                let values = payload
                    .and_then(|value| value.as_array().cloned())
                    .ok_or_else(|| "Invalid mpv-set-prop payload".to_string())?;
                let name = values
                    .first()
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing mpv-set-prop name".to_string())?
                    .to_string();
                let value = values
                    .get(1)
                    .cloned()
                    .ok_or_else(|| "Missing mpv-set-prop value".to_string())?;
                PlayerCommand::SetProperty(name, value)
            }
            "mpv-command" => PlayerCommand::Command(
                payload
                    .and_then(|value| value.as_array().cloned())
                    .ok_or_else(|| "Invalid mpv-command payload".to_string())?,
            ),
            "native-player-stop" => PlayerCommand::Stop,
            other => return Err(format!("Unsupported Windows player command: {other}")),
        };

        log_player_command(&command);

        if matches!(&command, PlayerCommand::SetProperty(name, _) if name == PRIMARY_SUBTITLE_PROPERTY)
        {
            self.disable_secondary_subtitle()?;
        }
        let is_sub_add = matches!(&command, PlayerCommand::Command(values) if values.first().and_then(Value::as_str) == Some(SUB_ADD_COMMAND));
        self.handle_command(command)?;
        if is_sub_add {
            self.disable_secondary_subtitle()?;
        }
        Ok(())
    }

    fn disable_secondary_subtitle(&mut self) -> Result<(), String> {
        self.handle_command(PlayerCommand::SetProperty(
            SECONDARY_SUBTITLE_PROPERTY.to_string(),
            Value::from("no"),
        ))
    }

    fn handle_command(&mut self, command: PlayerCommand) -> Result<(), String> {
        self.backend.handle_command(command.clone())?;
        self.commands.push(command);
        Ok(())
    }

    pub fn emit_property_change(&mut self, name: impl Into<String>, data: Value) {
        self.events
            .push(PlayerEvent::PropertyChange(PlayerPropertyChange {
                name: name.into(),
                data,
            }));
    }

    pub fn emit_ended(&mut self, reason: impl Into<String>) {
        self.events.push(PlayerEvent::Ended(PlayerEnded {
            reason: reason.into(),
            error: None,
        }));
    }

    pub fn commands(&self) -> &[PlayerCommand] {
        &self.commands
    }

    pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
        let mut events = std::mem::take(&mut self.events);
        events.extend(self.backend.drain_events());
        events
    }

    pub fn shutdown(&mut self) {
        self.backend.shutdown();
    }
}

fn log_player_command(command: &PlayerCommand) {
    match command {
        PlayerCommand::Command(values) => {
            let Some(name) = values.first().and_then(Value::as_str) else {
                return;
            };
            let message =
                format!("[StremioLightning] MPV command received: {name} (arguments redacted)");
            if name == "loadfile" {
                stremio_lightning_core::logging::info("native.player", message);
            } else {
                stremio_lightning_core::logging::debug("native.player", message);
            }
        }
        PlayerCommand::Stop => stremio_lightning_core::logging::info(
            "native.player",
            "[StremioLightning] MPV stop requested",
        ),
        PlayerCommand::ObserveProperty(_) | PlayerCommand::SetProperty(_, _) => {}
    }
}

fn sanitize_mpv_log_message(message: &str) -> String {
    let mut sanitized = stremio_lightning_core::logging::sanitize_text(message.trim());

    if sanitized.len() > MAX_MPV_LOG_MESSAGE_LENGTH {
        let mut boundary = MAX_MPV_LOG_MESSAGE_LENGTH;
        while !sanitized.is_char_boundary(boundary) {
            boundary -= 1;
        }
        sanitized.truncate(boundary);
        sanitized.push_str("... [truncated]");
    }
    sanitized
}

#[cfg(any(windows, test))]
fn command_name_and_args(values: &[Value]) -> Result<(String, Vec<String>), String> {
    let name = values
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing mpv-command name".to_string())?
        .to_string();
    let args = values
        .iter()
        .skip(1)
        .map(|value| match value {
            Value::String(value) => value.clone(),
            other => other.to_string(),
        })
        .collect();
    Ok((name, args))
}

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::window::UiThreadNotifier;
    use libmpv2::{
        events::{Event, PropertyData},
        mpv_end_file_reason, Format, Mpv, SetData,
    };
    use std::sync::mpsc::TryRecvError;
    use std::thread::{self, JoinHandle};
    use windows::Win32::Foundation::HWND;

    enum BackendCommand {
        Player(PlayerCommand),
        Shutdown,
    }

    #[derive(Default)]
    pub struct PlayerBackend {
        sender: Option<Sender<BackendCommand>>,
        receiver: Option<Receiver<PlayerEvent>>,
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
        pub fn initialize(&mut self, hwnd: HWND, notifier: UiThreadNotifier) -> Result<(), String> {
            if self.initialized {
                return Ok(());
            }

            let mpv = create_mpv(hwnd)?;
            let (command_sender, command_receiver) = std::sync::mpsc::channel();
            let (event_sender, event_receiver) = std::sync::mpsc::channel();

            self.thread = Some(spawn_player_thread(
                mpv,
                command_receiver,
                event_sender,
                notifier,
            ));
            self.sender = Some(command_sender);
            self.receiver = Some(event_receiver);
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

        pub fn handle_command(&self, command: PlayerCommand) -> Result<(), String> {
            self.sender
                .as_ref()
                .ok_or_else(|| "Windows MPV backend is not initialized".to_string())?
                .send(BackendCommand::Player(command))
                .map_err(|error| format!("Failed to send command to Windows MPV backend: {error}"))
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

    fn create_mpv(hwnd: HWND) -> Result<Mpv, String> {
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
        .map_err(|error| format!("Failed to initialize Windows MPV backend: {error}"))?;
        if let Err(error) = request_mpv_log_messages(&mpv) {
            stremio_lightning_core::logging::warn("native.player", error);
        }
        Ok(mpv)
    }

    fn request_mpv_log_messages(mpv: &Mpv) -> Result<(), String> {
        let level = CString::new("warn").expect("static MPV log level must not contain NUL");
        let result =
            unsafe { libmpv2_sys::mpv_request_log_messages(mpv.ctx.as_ptr(), level.as_ptr()) };
        if result < 0 {
            return Err(format!(
                "Failed to enable Windows MPV diagnostics (error code {result})"
            ));
        }
        Ok(())
    }

    fn spawn_player_thread(
        mut mpv: Mpv,
        command_receiver: Receiver<BackendCommand>,
        event_sender: Sender<PlayerEvent>,
        notifier: UiThreadNotifier,
    ) -> JoinHandle<()> {
        thread::spawn(move || {
            let _ = mpv.disable_deprecated_events();

            loop {
                if should_shutdown(&mpv, &command_receiver) {
                    return;
                }

                if let Some(event) = mpv.wait_event(0.05) {
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

    enum MpvEventAction {
        Emit(PlayerEvent),
        Continue,
        Shutdown,
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
        let text = sanitize_mpv_log_message(text);
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

    fn handle_mpv_command(mpv: &Mpv, command: PlayerCommand) -> Result<(), String> {
        match command {
            PlayerCommand::ObserveProperty(name) => {
                let format = observe_format(&name);
                mpv.observe_property(&name, format, 0)
                    .map_err(|error| format!("Failed to observe MPV property '{name}': {error}"))?;
                mpv.wake_up();
                Ok(())
            }
            PlayerCommand::SetProperty(name, value) => set_property(mpv, &name, value),
            PlayerCommand::Command(values) => {
                let (name, args) = command_name_and_args(&values)?;
                let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
                mpv.command(&name, &refs)
                    .map_err(|error| format!("Failed to execute MPV command '{name}': {error}"))
            }
            PlayerCommand::Stop => mpv
                .command("stop", &[])
                .map_err(|error| format!("Failed to stop MPV playback: {error}")),
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
                    Err(format!("Invalid numeric MPV property value for '{name}'"))
                }
            }
            Value::String(value) => set_property_value(mpv, name, value),
            other => set_property_value(mpv, name, other.to_string()),
        }
    }

    fn set_property_value<T: SetData>(mpv: &Mpv, name: &str, value: T) -> Result<(), String> {
        mpv.set_property(name, value)
            .map_err(|error| format!("Failed to set MPV property '{name}': {error}"))
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

    trait MpvWakeUp {
        fn wake_up(&self);
    }

    impl MpvWakeUp for Mpv {
        fn wake_up(&self) {
            unsafe { libmpv2_sys::mpv_wakeup(self.ctx.as_ptr()) }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    #[derive(Debug, Default)]
    pub struct PlayerBackend;

    impl PlayerBackend {
        pub fn status(&self) -> NativePlayerStatus {
            NativePlayerStatus::default()
        }

        pub fn handle_command(&self, _command: PlayerCommand) -> Result<(), String> {
            Ok(())
        }

        pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
            Vec::new()
        }

        pub fn shutdown(&mut self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_transport_commands_to_shared_player_commands() {
        let mut player = WindowsPlayer::default();
        player
            .handle_transport("mpv-observe-prop", Some(json!("pause")))
            .unwrap();
        player
            .handle_transport("mpv-set-prop", Some(json!(["pause", true])))
            .unwrap();
        player
            .handle_transport(
                "mpv-command",
                Some(json!(["loadfile", "file:///video.mp4"])),
            )
            .unwrap();

        assert_eq!(
            player.commands(),
            &[
                PlayerCommand::ObserveProperty("pause".to_string()),
                PlayerCommand::SetProperty("pause".to_string(), json!(true)),
                PlayerCommand::Command(vec![json!("loadfile"), json!("file:///video.mp4")]),
            ]
        );
    }

    #[test]
    fn clears_secondary_subtitle_before_setting_sid() {
        let mut player = WindowsPlayer::default();
        player
            .handle_transport("mpv-set-prop", Some(json!(["sid", 3])))
            .unwrap();

        assert_eq!(
            player.commands(),
            &[
                PlayerCommand::SetProperty("secondary-sid".to_string(), json!("no")),
                PlayerCommand::SetProperty("sid".to_string(), json!(3)),
            ]
        );
    }

    #[test]
    fn clears_secondary_subtitle_after_sub_add() {
        let mut player = WindowsPlayer::default();
        player
            .handle_transport(
                "mpv-command",
                Some(json!(["sub-add", "file:///tmp/sub.srt", "select"])),
            )
            .unwrap();

        assert_eq!(
            player.commands(),
            &[
                PlayerCommand::Command(vec![
                    json!("sub-add"),
                    json!("file:///tmp/sub.srt"),
                    json!("select"),
                ]),
                PlayerCommand::SetProperty("secondary-sid".to_string(), json!("no")),
            ]
        );
    }

    #[test]
    fn extracts_mpv_command_name_and_string_args() {
        assert_eq!(
            command_name_and_args(&[json!("loadfile"), json!("file:///video.mp4"), json!(true)])
                .unwrap(),
            (
                "loadfile".to_string(),
                vec!["file:///video.mp4".to_string(), "true".to_string()]
            )
        );
    }

    #[test]
    fn preserves_urls_redacts_secrets_and_bounds_mpv_diagnostics() {
        let sanitized = sanitize_mpv_log_message(
            "Failed HTTPS://media.example/video?token=secret and magnet:?xt=secret",
        );
        assert_eq!(
            sanitized,
            "Failed HTTPS://media.example/video?token=[redacted] and magnet:?xt=secret"
        );

        let long = sanitize_mpv_log_message(&"x".repeat(MAX_MPV_LOG_MESSAGE_LENGTH + 1));
        assert!(long.starts_with(&"x".repeat(MAX_MPV_LOG_MESSAGE_LENGTH)));
        assert!(long.ends_with("... [truncated]"));
    }
}
