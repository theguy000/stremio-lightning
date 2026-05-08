use crate::player_api::PlayerEvent;
use serde_json::Value;
use std::sync::Mutex;

pub const PIP_WINDOW_WIDTH: i32 = 480;
pub const PIP_WINDOW_HEIGHT: i32 = 270;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PipRestoreSnapshot {
    pub was_fullscreen: bool,
    pub saved_size: Option<(i32, i32)>,
}

#[derive(Debug, Default)]
pub struct PipState {
    inner: Mutex<PipStateInner>,
}

#[derive(Debug, Default)]
struct PipStateInner {
    enabled: bool,
    restore_snapshot: Option<PipRestoreSnapshot>,
}

impl PipState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_enabled(&self) -> Result<bool, String> {
        self.inner
            .lock()
            .map(|inner| inner.enabled)
            .map_err(|e| e.to_string())
    }

    pub fn toggle(&self) -> Result<bool, String> {
        let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
        inner.enabled = !inner.enabled;
        if !inner.enabled {
            inner.restore_snapshot = None;
        }
        Ok(inner.enabled)
    }

    pub fn set_mode(
        &self,
        enabled: bool,
        snapshot: Option<PipRestoreSnapshot>,
    ) -> Result<(), String> {
        if let Some(snapshot) = snapshot.as_ref() {
            log_snapshot_saved(snapshot);
        }

        let mut inner = self.inner.lock().map_err(|e| e.to_string())?;
        inner.enabled = enabled;
        inner.restore_snapshot = enabled.then_some(snapshot).flatten();
        Ok(())
    }

    pub fn snapshot(&self) -> Result<Option<PipRestoreSnapshot>, String> {
        self.inner
            .lock()
            .map(|inner| inner.restore_snapshot.clone())
            .map_err(|e| e.to_string())
    }

    pub fn save_snapshot(&self, snapshot: PipRestoreSnapshot) -> Result<(), String> {
        log_snapshot_saved(&snapshot);
        self.inner
            .lock()
            .map_err(|e| e.to_string())?
            .restore_snapshot = Some(snapshot);
        Ok(())
    }

    pub fn take_snapshot(&self) -> Result<Option<PipRestoreSnapshot>, String> {
        let snapshot = self
            .inner
            .lock()
            .map_err(|e| e.to_string())?
            .restore_snapshot
            .take();
        log_snapshot_restored(snapshot.as_ref());
        Ok(snapshot)
    }
}

fn log_snapshot_saved(snapshot: &PipRestoreSnapshot) {
    if snapshot.was_fullscreen {
        eprintln!("[StremioLightning] Captured PiP restore state: fullscreen");
    } else if let Some((width, height)) = snapshot.saved_size {
        eprintln!("[StremioLightning] Captured PiP restore size: {width}x{height}");
    }
}

fn log_snapshot_restored(snapshot: Option<&PipRestoreSnapshot>) {
    if let Some(snapshot) = snapshot {
        if snapshot.was_fullscreen {
            eprintln!("[StremioLightning] Restoring PiP fullscreen state");
        } else if let Some((width, height)) = snapshot.saved_size {
            eprintln!("[StremioLightning] Restoring PiP size: {width}x{height}");
        }
    }
}

pub fn serialize_picture_in_picture(enabled: bool) -> Value {
    if enabled {
        PlayerEvent::ShowPictureInPicture(Value::Object(Default::default()))
    } else {
        PlayerEvent::HidePictureInPicture(Value::Object(Default::default()))
    }
    .transport_args()
}

pub trait PipWindowController {
    fn enter_pip(&mut self) -> Result<PipRestoreSnapshot, String>;
    fn exit_pip(&mut self, snapshot: PipRestoreSnapshot) -> Result<(), String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pip_state_toggles_and_tracks_snapshot() {
        let state = PipState::new();
        assert_eq!(state.is_enabled().unwrap(), false);
        assert_eq!(state.toggle().unwrap(), true);
        assert_eq!(state.is_enabled().unwrap(), true);

        let snapshot = PipRestoreSnapshot {
            was_fullscreen: true,
            saved_size: Some((1280, 720)),
        };
        state.save_snapshot(snapshot.clone()).unwrap();
        assert_eq!(state.take_snapshot().unwrap(), Some(snapshot));
        assert_eq!(state.take_snapshot().unwrap(), None);

        assert_eq!(state.toggle().unwrap(), false);
    }

    #[test]
    fn serializes_picture_in_picture_events() {
        assert_eq!(
            serialize_picture_in_picture(true),
            json!(["showPictureInPicture", {}])
        );
        assert_eq!(
            serialize_picture_in_picture(false),
            json!(["hidePictureInPicture", {}])
        );
    }
}
