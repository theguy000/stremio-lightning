use std::sync::OnceLock;
use serde::Deserialize;
use serde_json::{json, Value};

use super::types::{
    ParsedRequest, RpcRequest, RpcResponse, RpcResponseData, RpcResponseDataTransport,
    RPC_TYPE_INIT, RPC_TYPE_INVOKE_METHOD, RPC_TYPE_SIGNAL, TRANSPORT_OBJECT,
};

pub fn parse_request(message: &str) -> Result<ParsedRequest, String> {
    let request: RpcRequest = serde_json::from_str(message)
        .map_err(|e| format!("Failed to parse shell transport message: {e}"))?;

    if request.request_type == Some(RPC_TYPE_INIT)
        || (request.id == 0 && request.request_type.is_none() && request.args.is_none())
    {
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

#[must_use]
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

#[must_use]
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

#[must_use]
pub fn stremio_deep_link_transport_args(url: &str) -> Value {
    let lower = url.trim().to_ascii_lowercase();
    let event = if lower.starts_with("stremio:///detail/") || lower.starts_with("stremio://detail/")
    {
        "open-media"
    } else {
        "addon-install"
    };
    json!([event, url])
}

#[must_use]
pub fn serialize_window_visibility(visible: bool, is_fullscreen: bool) -> Value {
    serde_json::json!([
        "win-visibility-changed",
        {
            "visible": visible,
            "visibility": u8::from(is_fullscreen),
            "isFullscreen": is_fullscreen
        }
    ])
}

#[must_use]
pub fn safe_native_player_status(status: &Value) -> String {
    if status.is_null() {
        return "unavailable".to_string();
    }
    for key in ["initialized", "available", "running"] {
        if let Some(value) = status.get(key).and_then(Value::as_bool) {
            return format!("{key}={value}");
        }
    }
    "available".to_string()
}

pub fn parse_payload<T>(command: &str, payload: Option<Value>) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(payload.unwrap_or(Value::Null))
        .map_err(|e| format!("Invalid {command} payload: {e}"))
}

#[must_use]
pub fn parse_optional_bool(payload: Option<Value>) -> Option<bool> {
    let value = payload?;
    value
        .as_bool()
        .or_else(|| value.get("enabled").and_then(Value::as_bool))
        .or_else(|| value.get("value").and_then(Value::as_bool))
}

pub fn async_runtime() -> &'static tokio::runtime::Runtime {
    static TOKIO_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    TOKIO_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create async runtime")
    })
}

pub fn get_async_runtime() -> &'static tokio::runtime::Runtime {
    async_runtime()
}
