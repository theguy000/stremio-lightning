use serde::{Deserialize, Serialize};
use serde_json::Value;

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
