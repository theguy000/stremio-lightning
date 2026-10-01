pub mod bridge;
pub mod handlers;
pub mod types;

#[cfg(test)]
mod tests;

pub use bridge::BaseHost;
pub use handlers::{
    async_runtime, handshake_response, is_async_command, parse_optional_bool, parse_payload,
    parse_request, response_message, safe_native_player_status, serialize_window_visibility,
    stremio_deep_link_transport_args,
};
pub use types::{
    DownloadModPayload, FocusChangedPayload, FullscreenIpcPayload, GetLogsPayload, HostApiError,
    HostCommand, HostEvent, HostEventRecord, InvokeIpcPayload, IpcRequest, ListenIpcPayload,
    ListenerRegistry, ModFilePayload, ModTypePayload, ParsedRequest, PlatformBridge,
    RegisterSettingsPayload, RpcRequest, RpcResponse, RpcResponseData, RpcResponseDataTransport,
    SaveSettingPayload, SetExtendedDiagnosticsPayload, SettingKeyPayload, ShellPreferenceState,
    SubmitDiagnosticLogsPayload, UnlistenIpcPayload, ZoomIpcPayload, RPC_TYPE_INIT,
    RPC_TYPE_INVOKE_METHOD, RPC_TYPE_SIGNAL, SHELL_TRANSPORT_EVENT, TRANSPORT_OBJECT,
};
