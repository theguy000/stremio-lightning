use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativePlayerStatus {
    pub enabled: bool,
    pub initialized: bool,
    pub backend: String,
}

pub trait PlayerBackend: Clone + Send + Sync + 'static {
    fn status(&self) -> NativePlayerStatus;
    fn stop(&self) -> Result<(), String>;
}

#[derive(Debug, Default, Clone)]
pub struct FakePlayerBackend {
    stopped: Arc<Mutex<bool>>,
    initialized: bool,
}

impl FakePlayerBackend {
    pub fn initialized() -> Self {
        Self {
            stopped: Arc::default(),
            initialized: true,
        }
    }

    pub fn stopped(&self) -> bool {
        self.stopped.lock().map(|stopped| *stopped).unwrap_or(false)
    }
}

impl PlayerBackend for FakePlayerBackend {
    fn status(&self) -> NativePlayerStatus {
        NativePlayerStatus {
            enabled: true,
            initialized: self.initialized,
            backend: "fake".to_string(),
        }
    }

    fn stop(&self) -> Result<(), String> {
        *self.stopped.lock().map_err(|e| e.to_string())? = true;
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct MpvPlayerBackend;

impl PlayerBackend for MpvPlayerBackend {
    fn status(&self) -> NativePlayerStatus {
        NativePlayerStatus {
            enabled: true,
            initialized: false,
            backend: "libmpv-macos".to_string(),
        }
    }

    fn stop(&self) -> Result<(), String> {
        Ok(())
    }
}

pub fn handle_transport<B: PlayerBackend>(
    _backend: &B,
    method: &str,
    _data: Option<Value>,
) -> Result<(), String> {
    Err(format!(
        "Unsupported macOS player transport method: {method}"
    ))
}
