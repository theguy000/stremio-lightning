use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStreamingServerRequest {
    pub method: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStreamingServerResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

pub fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

pub fn should_forward_proxy_header(name: &str) -> bool {
    !is_hop_by_hop_header(name)
}

pub fn validate_proxy_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') || path.starts_with("//") || path.contains("://") {
        return Err("Rejected invalid streaming server proxy path".into());
    }

    if path.contains('\\') || path.contains('\0') {
        return Err("Rejected invalid streaming server proxy path".into());
    }

    Ok(())
}

pub fn normalize_proxy_method(method: &str) -> Result<&'static str, String> {
    match method.trim().to_ascii_uppercase().as_str() {
        "GET" => Ok("GET"),
        "POST" => Ok("POST"),
        "PUT" => Ok("PUT"),
        "PATCH" => Ok("PATCH"),
        "DELETE" => Ok("DELETE"),
        "OPTIONS" => Ok("OPTIONS"),
        "HEAD" => Ok("HEAD"),
        _ => Err("Rejected unsupported streaming server proxy method".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_invalid_streaming_server_proxy_paths() {
        for path in [
            "http://127.0.0.1:11470/status",
            "https://example.com/status",
            "//example.com/status",
            "status",
            "/videos\\movie.mp4",
            "/videos/\0movie.mp4",
        ] {
            assert!(
                validate_proxy_path(path).is_err(),
                "path should be rejected: {path:?}"
            );
        }
    }

    #[test]
    fn accepts_relative_streaming_server_proxy_paths() {
        for path in ["/", "/status", "/stream/movie.mp4?token=abc"] {
            assert!(
                validate_proxy_path(path).is_ok(),
                "path should be accepted: {path:?}"
            );
        }
    }

    #[test]
    fn rejects_hop_by_hop_streaming_server_proxy_headers() {
        for name in [
            "Connection",
            "keep-alive",
            "Proxy-Authenticate",
            "Proxy-Authorization",
            "TE",
            "Trailer",
            "Transfer-Encoding",
            "Upgrade",
            "Host",
            "Content-Length",
        ] {
            assert!(
                !should_forward_proxy_header(name),
                "header should be stripped: {name}"
            );
        }

        assert!(should_forward_proxy_header("Range"));
        assert!(should_forward_proxy_header("Accept"));
    }

    #[test]
    fn parses_supported_streaming_server_proxy_methods() {
        for method in ["GET", "post", " Put ", "PATCH", "DELETE", "OPTIONS", "HEAD"] {
            assert!(
                normalize_proxy_method(method).is_ok(),
                "method should be supported: {method:?}"
            );
        }

        for method in ["CONNECT", "TRACE", "", "GET / HTTP/1.1"] {
            assert!(
                normalize_proxy_method(method).is_err(),
                "method should be rejected: {method:?}"
            );
        }
    }

    #[test]
    fn proxy_payloads_keep_camel_case_shape() {
        let response = ProxyStreamingServerResponse {
            status: 206,
            status_text: "Partial Content".to_string(),
            headers: vec![("content-range".to_string(), "bytes 0-1/2".to_string())],
            body: vec![1, 2],
        };

        assert_eq!(
            serde_json::to_value(response).unwrap(),
            json!({
                "status": 206,
                "statusText": "Partial Content",
                "headers": [["content-range", "bytes 0-1/2"]],
                "body": [1, 2]
            })
        );

        let request = ProxyStreamingServerRequest {
            method: "GET".to_string(),
            path: "/status".to_string(),
            headers: None,
            body: Some(vec![1]),
        };
        assert_eq!(
            serde_json::to_value(request).unwrap(),
            json!({"method": "GET", "path": "/status", "body": [1]})
        );
    }
}
