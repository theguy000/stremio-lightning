use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostCommand {
    ToggleDevtools,
    OpenExternalUrl,
    ShellTransportSend,
    ShellBridgeReady,
    GetNativePlayerStatus,
    StartStreamingServer,
    StopStreamingServer,
    RestartStreamingServer,
    GetStreamingServerStatus,
    ProxyStreamingServerRequest,
    GetPlugins,
    GetThemes,
    DownloadMod,
    DeleteMod,
    GetModContent,
    GetRegistry,
    CheckModUpdates,
    GetSetting,
    SaveSetting,
    RegisterSettings,
    GetRegisteredSettings,
    StartDiscordRpc,
    StopDiscordRpc,
    UpdateDiscordActivity,
    CheckAppUpdate,
    SetAutoPause,
    GetAutoPause,
    SetPipDisablesAutoPause,
    GetPipDisablesAutoPause,
    TogglePip,
    GetPipMode,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum HostEvent {
    #[serde(rename = "window-maximized-changed")]
    WindowMaximizedChanged,
    #[serde(rename = "window-fullscreen-changed")]
    WindowFullscreenChanged,
    #[serde(rename = "server-started")]
    ServerStarted,
    #[serde(rename = "server-stopped")]
    ServerStopped,
    #[serde(rename = "shell-transport-message")]
    ShellTransportMessage,
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct RpcRequest {
    pub id: u64,
    #[serde(rename = "type")]
    pub request_type: Option<u8>,
    pub args: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RpcResponseDataTransport {
    pub properties: Vec<Vec<String>>,
    pub signals: Vec<String>,
    pub methods: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RpcResponseData {
    pub transport: RpcResponseDataTransport,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RpcResponse {
    pub id: u64,
    pub object: String,
    #[serde(rename = "type")]
    pub response_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<RpcResponseData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
}

#[derive(Debug, PartialEq)]
pub enum ParsedRequest {
    Handshake,
    Command { method: String, data: Option<Value> },
}

pub const TRANSPORT_OBJECT: &str = "transport";
pub const RPC_TYPE_INIT: u8 = 3;
pub const RPC_TYPE_SIGNAL: u8 = 1;
pub const RPC_TYPE_INVOKE_METHOD: u8 = 6;

pub fn parse_request(message: &str) -> Result<ParsedRequest, String> {
    let request: RpcRequest = serde_json::from_str(message)
        .map_err(|e| format!("Failed to parse shell transport message: {e}"))?;

    if request.id == 0 || request.request_type == Some(RPC_TYPE_INIT) {
        return Ok(ParsedRequest::Handshake);
    }

    match request.request_type {
        Some(RPC_TYPE_INVOKE_METHOD) | None => {}
        Some(request_type) => {
            return Err(format!(
                "Unsupported shell transport request type: {request_type}"
            ));
        }
    }

    let args = request
        .args
        .and_then(|value| value.as_array().cloned())
        .ok_or_else(|| "Missing shell transport args".to_string())?;
    let method = args
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing shell transport method".to_string())?
        .to_string();
    let data = args.get(1).cloned();

    Ok(ParsedRequest::Command { method, data })
}

pub fn handshake_response(package_version: &str) -> String {
    serde_json::to_string(&RpcResponse {
        id: 0,
        object: TRANSPORT_OBJECT.to_string(),
        response_type: RPC_TYPE_INIT,
        data: Some(RpcResponseData {
            transport: RpcResponseDataTransport {
                properties: vec![
                    vec![],
                    vec![
                        String::new(),
                        "shellVersion".to_string(),
                        String::new(),
                        package_version.to_string(),
                    ],
                ],
                signals: vec![],
                methods: vec![vec!["onEvent".to_string()]],
            },
        }),
        ..Default::default()
    })
    .expect("failed to serialize handshake response")
}

pub fn response_message(args: Value) -> String {
    serde_json::to_string(&RpcResponse {
        id: 1,
        object: TRANSPORT_OBJECT.to_string(),
        response_type: RPC_TYPE_SIGNAL,
        args: Some(args),
        ..Default::default()
    })
    .expect("failed to serialize transport response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn host_command_names_match_frontend() {
        assert_eq!(
            serde_json::to_value(HostCommand::ToggleDevtools).unwrap(),
            json!("toggle_devtools")
        );
        assert_eq!(
            serde_json::to_value(HostCommand::ProxyStreamingServerRequest).unwrap(),
            json!("proxy_streaming_server_request")
        );
        assert_eq!(
            serde_json::to_value(HostCommand::SetPipDisablesAutoPause).unwrap(),
            json!("set_pip_disables_auto_pause")
        );
    }

    #[test]
    fn host_event_names_match_frontend() {
        assert_eq!(
            serde_json::to_value(HostEvent::WindowMaximizedChanged).unwrap(),
            json!("window-maximized-changed")
        );
        assert_eq!(
            serde_json::to_value(HostEvent::ShellTransportMessage).unwrap(),
            json!("shell-transport-message")
        );
    }

    #[test]
    fn parses_handshake_request() {
        assert_eq!(
            parse_request(r#"{"id":0,"type":3}"#).unwrap(),
            ParsedRequest::Handshake
        );
    }

    #[test]
    fn parses_command_request() {
        assert_eq!(
            parse_request(r#"{"id":7,"type":6,"args":["mpv-command",["stop"]]}"#).unwrap(),
            ParsedRequest::Command {
                method: "mpv-command".to_string(),
                data: Some(json!(["stop"])),
            }
        );
    }

    #[test]
    fn serializes_handshake_shape() {
        let payload: Value = serde_json::from_str(&handshake_response("0.1.0")).unwrap();
        assert_eq!(
            payload,
            json!({
                "id": 0,
                "object": "transport",
                "type": 3,
                "data": {
                    "transport": {
                        "properties": [[], ["", "shellVersion", "", "0.1.0"]],
                        "signals": [],
                        "methods": [["onEvent"]]
                    }
                }
            })
        );
    }

    #[test]
    fn serializes_event_shape() {
        let payload: Value =
            serde_json::from_str(&response_message(json!(["open-media", "stremio://foo"])))
                .unwrap();
        assert_eq!(
            payload,
            json!({
                "id": 1,
                "object": "transport",
                "type": 1,
                "args": ["open-media", "stremio://foo"]
            })
        );
    }
}
