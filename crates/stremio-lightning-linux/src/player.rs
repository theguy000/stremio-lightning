use serde_json::Value;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
pub use stremio_lightning_core::player_api::{handle_transport, NativePlayerStatus, PlayerBackend};
use stremio_lightning_core::player_api::{PlayerEnded, PlayerEvent, PlayerPropertyChange};

#[derive(Debug, Clone, PartialEq)]
pub enum MpvBackendCommand {
    ObserveProperty(String),
    SetProperty { name: String, value: Value },
    Command { name: String, args: Vec<String> },
    Stop,
}

#[derive(Debug, Default, Clone)]
pub struct MpvPlayerBackend {
    initialized: Arc<Mutex<bool>>,
    sender: Arc<Mutex<Option<Sender<MpvBackendCommand>>>>,
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

    fn send(&self, command: MpvBackendCommand) -> Result<(), String> {
        let sender = self
            .sender
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or_else(|| {
                "MPV backend is not attached to the native video renderer".to_string()
            })?;
        sender
            .send(command)
            .map_err(|e| format!("Failed to send MPV command to renderer: {e}"))
    }
}

impl PlayerBackend for MpvPlayerBackend {
    fn status(&self) -> NativePlayerStatus {
        NativePlayerStatus {
            enabled: true,
            initialized: self.initialized.lock().map(|guard| *guard).unwrap_or(false),
            backend: "libmpv-opengl".to_string(),
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

pub fn serialize_property_change(name: impl Into<String>, data: Value) -> Value {
    PlayerEvent::PropertyChange(PlayerPropertyChange {
        name: name.into(),
        data,
    })
    .transport_args()
}

pub fn serialize_ended(ended: PlayerEnded) -> Value {
    PlayerEvent::Ended(ended).transport_args()
}

#[cfg(target_os = "linux")]
#[allow(non_camel_case_types)]
pub mod mpv_render_ffi {
    use std::ffi::{c_char, c_int, c_void};

    pub enum mpv_render_context {}

    #[repr(C)]
    pub struct mpv_opengl_init_params {
        pub get_proc_address:
            Option<unsafe extern "C" fn(ctx: *mut c_void, name: *const c_char) -> *mut c_void>,
        pub get_proc_address_ctx: *mut c_void,
        pub extra_exts: *const c_char,
    }

    #[repr(C)]
    pub struct mpv_opengl_fbo {
        pub fbo: c_int,
        pub w: c_int,
        pub h: c_int,
        pub internal_format: c_int,
    }

    #[repr(C)]
    pub struct mpv_render_param {
        pub type_: c_int,
        pub data: *mut c_void,
    }

    pub const MPV_RENDER_API_TYPE_OPENGL: &str = "opengl";
    pub const MPV_RENDER_PARAM_API_TYPE: c_int = 1;
    pub const MPV_RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
    pub const MPV_RENDER_PARAM_OPENGL_FBO: c_int = 3;
    pub const MPV_RENDER_PARAM_FLIP_Y: c_int = 4;
    pub const MPV_RENDER_PARAM_INVALID: c_int = 0;

    extern "C" {
        pub fn mpv_render_context_create(
            res: *mut *mut mpv_render_context,
            mpv: *mut c_void,
            params: *mut mpv_render_param,
        ) -> c_int;
        pub fn mpv_render_context_free(ctx: *mut mpv_render_context);
        pub fn mpv_render_context_render(
            ctx: *mut mpv_render_context,
            params: *mut mpv_render_param,
        );
        pub fn mpv_render_context_set_update_callback(
            ctx: *mut mpv_render_context,
            callback: Option<unsafe extern "C" fn(ctx: *mut c_void)>,
            callback_ctx: *mut c_void,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use stremio_lightning_core::player_api::EndFileCause;

    #[test]
    fn transport_messages_reach_the_attached_renderer_as_backend_commands() {
        let backend = MpvPlayerBackend::default();
        let (sender, receiver) = std::sync::mpsc::channel();
        backend.attach(sender).unwrap();

        handle_transport(&backend, "mpv-observe-prop", Some(json!("pause"))).unwrap();
        handle_transport(&backend, "mpv-set-prop", Some(json!(["pause", true]))).unwrap();
        handle_transport(
            &backend,
            "mpv-command",
            Some(json!(["loadfile", "file:///tmp/sample.mp4", "replace"])),
        )
        .unwrap();
        handle_transport(&backend, "native-player-stop", None).unwrap();

        let received: Vec<_> = receiver.try_iter().collect();
        assert_eq!(
            received,
            vec![
                MpvBackendCommand::ObserveProperty("pause".to_string()),
                MpvBackendCommand::SetProperty {
                    name: "pause".to_string(),
                    value: json!(true),
                },
                MpvBackendCommand::Command {
                    name: "loadfile".to_string(),
                    args: vec!["file:///tmp/sample.mp4".to_string(), "replace".to_string()],
                },
                MpvBackendCommand::Stop,
            ]
        );
    }

    #[test]
    fn serializes_player_events() {
        assert_eq!(
            serialize_property_change("pause", json!(true)),
            json!(["mpv-prop-change", {"name": "pause", "data": true}])
        );
        assert_eq!(
            serialize_ended(PlayerEnded::from_cause(EndFileCause::Eof)),
            json!(["mpv-event-ended", {"reason": "eof"}])
        );
        assert_eq!(
            serialize_ended(PlayerEnded::from_cause(EndFileCause::Error)),
            json!(["mpv-event-ended", {
                "reason": "error",
                "error": { "message": "MPV playback error", "critical": true },
            }])
        );
    }

    #[test]
    fn mpv_backend_forwards_to_attached_renderer() {
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
}
