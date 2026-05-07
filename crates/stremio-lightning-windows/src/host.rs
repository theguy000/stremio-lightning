use crate::player::WindowsPlayer;
use serde_json::{json, Value};
use std::sync::Mutex;
use stremio_lightning_core::host_api::{self, ParsedRequest};

pub struct WindowsHost {
    player: Mutex<WindowsPlayer>,
    package_version: &'static str,
}

impl Default for WindowsHost {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

impl WindowsHost {
    pub fn new(package_version: &'static str) -> Self {
        Self {
            player: Mutex::default(),
            package_version,
        }
    }

    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        match command {
            "init" => Ok(json!({ "platform": "windows", "shell": "webview2" })),
            "get_native_player_status" => Ok(serde_json::to_value(
                self.player
                    .lock()
                    .map_err(|_| "Windows player lock poisoned".to_string())?
                    .status(),
            )
            .map_err(|error| error.to_string())?),
            "shell_transport_send" => {
                let message = payload
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
                    .ok_or_else(|| "Missing shell transport payload".to_string())?;
                self.handle_shell_transport_message(&message)
                    .map(Value::String)
            }
            other => Err(format!("Unsupported Windows host command: {other}")),
        }
    }

    fn handle_shell_transport_message(&self, message: &str) -> Result<String, String> {
        match host_api::parse_request(message)? {
            ParsedRequest::Handshake => Ok(host_api::handshake_response(self.package_version)),
            ParsedRequest::Command { method, data } => {
                self.player
                    .lock()
                    .map_err(|_| "Windows player lock poisoned".to_string())?
                    .handle_transport(&method, data)?;
                Ok(host_api::response_message(json!(["ok"])))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exposes_webview2_init_contract() {
        assert_eq!(
            WindowsHost::default().invoke("init", None).unwrap(),
            json!({ "platform": "windows", "shell": "webview2" })
        );
    }

    #[test]
    fn handles_shell_transport_handshake() {
        let response = WindowsHost::new("0.1.0")
            .invoke("shell_transport_send", Some(json!(r#"{"id":0,"type":3}"#)))
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(response.as_str().unwrap()).unwrap()["type"],
            json!(3)
        );
    }
}
