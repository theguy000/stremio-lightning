use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::OnceLock;

use super::types::{
    ParsedRequest, RpcRequest, RpcResponse, RpcResponseData, RpcResponseDataTransport,
    RPC_TYPE_INIT, RPC_TYPE_INVOKE_METHOD, RPC_TYPE_SIGNAL, TRANSPORT_OBJECT,
};

/// # Errors
/// Returns an error when the message is not valid JSON or is missing required fields.
pub fn parse_request(message: &str) -> Result<ParsedRequest, String> {
    let mut request: RpcRequest = serde_json::from_str(message)
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

    // Take the args out of the request we own; cloning them would deep-copy the
    // whole payload (stream URLs included) on every transport message.
    let mut args = match request.args.take() {
        Some(Value::Array(args)) => args.into_iter(),
        _ => return Err("Missing shell transport args".to_string()),
    };
    let Some(Value::String(method)) = args.next() else {
        return Err("Missing shell transport method".to_string());
    };
    let data = args.next();

    Ok(ParsedRequest::Command { method, data })
}

/// # Panics
/// Panics if the handshake envelope cannot be serialized, which cannot happen for the fixed shape it builds.
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
                    vec![
                        String::new(),
                        "nativeAssSubtitles".to_string(),
                        String::new(),
                        "true".to_string(),
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

/// # Panics
/// Panics if the response envelope cannot be serialized, which cannot happen for the fixed shape it builds.
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

/// # Errors
/// Returns an error when the payload is absent or is missing its `command`.
pub fn split_invoke_payload(payload: Option<Value>) -> Result<(String, Option<Value>), String> {
    let Some(mut payload) = payload else {
        return Err("Missing invoke payload".to_string());
    };
    // `serde_json::from_value` would deep-clone the inner payload — media URLs
    // included — on every invoke, so take both values out of the object we own.
    let Some(Value::String(command)) = payload
        .as_object_mut()
        .and_then(|object| object.remove("command"))
    else {
        return Err("Missing invoke command".to_string());
    };
    Ok((
        command,
        payload
            .as_object_mut()
            .and_then(|object| object.remove("payload")),
    ))
}

/// # Errors
/// Returns an error when the payload is absent or cannot be deserialized into `T`.
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

#[must_use]
pub fn is_async_command(command: &str) -> bool {
    matches!(
        command,
        "download_mod" | "get_registry" | "check_mod_updates" | "check_app_update"
    )
}

/// # Panics
/// Panics if the shared Tokio runtime cannot be created.
pub fn async_runtime() -> &'static tokio::runtime::Runtime {
    static TOKIO_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    TOKIO_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("Failed to create async runtime")
    })
}
