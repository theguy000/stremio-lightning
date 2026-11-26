//! Redaction and UTF-8 truncation shared by log sinks, diagnostics exports, and server capture.

use std::sync::OnceLock;

use regex::Regex;

const MAX_MESSAGE_BYTES: usize = 8 * 1024;
const TRUNCATION_SUFFIX: &str = " [truncated]";

/// Removes control characters, URL credentials, auth/token/cookie secrets, and local home paths.
pub(crate) fn sanitize_text(input: &str) -> String {
    static URL_CREDENTIALS: OnceLock<Regex> = OnceLock::new();
    static SECRETS: OnceLock<Regex> = OnceLock::new();
    static WINDOWS_PATHS: OnceLock<Regex> = OnceLock::new();
    static HOME_PATHS: OnceLock<Regex> = OnceLock::new();

    let mut clean = String::with_capacity(input.len().min(MAX_MESSAGE_BYTES));
    for ch in input.chars() {
        if ch == '\n' || ch == '\t' || !ch.is_control() {
            clean.push(ch);
        } else {
            clean.push(' ');
        }
    }
    clean = URL_CREDENTIALS
        .get_or_init(|| {
            Regex::new(r"(?i)\b((?:https?|ftp|rtsp)://)[^/\s:@]+:[^@\s/]+@")
                .expect("valid URL credential regex")
        })
        .replace_all(&clean, "$1[redacted]@")
        .into_owned();
    clean = SECRETS
        .get_or_init(|| {
            Regex::new(concat!(
                r#"(?i)\b(authorization|proxy-authorization|cookie|set-cookie|"#,
                r#"token|access[_-]?token|refresh[_-]?token|api[_-]?key|"#,
                r#"password|passwd|secret|session[_-]?id)\b"#,
                r#"(\\?["']?\s*[:=]\s*\\?["']?)"#,
                r#"(?:"[^"]*"|'[^']*'|(?:Bearer\s+)?[^\s,;}\]\)\r\n]+)"#,
            ))
            .expect("valid secret redaction regex")
        })
        .replace_all(&clean, "$1$2[redacted]")
        .into_owned();
    clean = WINDOWS_PATHS
        .get_or_init(|| Regex::new(r"(?i)\b[a-z]:\\[^\r\n\t,;\)\]]+").expect("valid path regex"))
        .replace_all(&clean, "[redacted local path]")
        .into_owned();
    clean = HOME_PATHS
        .get_or_init(|| {
            Regex::new(r#"(?i)(?:/home/|/users/)[^\s"'<>]+"#).expect("valid home path regex")
        })
        .replace_all(&clean, "[redacted local path]")
        .into_owned();
    truncate_utf8(&clean, MAX_MESSAGE_BYTES)
}

/// Truncates `value` to at most `max_bytes` UTF-8 bytes, appending ` [truncated]` when space permits.
pub(crate) fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    if max_bytes <= TRUNCATION_SUFFIX.len() {
        let mut end = max_bytes;
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        return value[..end].to_owned();
    }
    let mut end = max_bytes - TRUNCATION_SUFFIX.len();
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = String::with_capacity(end + TRUNCATION_SUFFIX.len());
    out.push_str(&value[..end]);
    out.push_str(TRUNCATION_SUFFIX);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_credentials_secrets_and_local_paths() {
        let raw =
            r#"https://alice:pw@example.com token=secret123 C:\Users\alice\app.log /home/alice/x"#;
        let clean = sanitize_text(raw);
        assert!(clean.contains("https://[redacted]@example.com"));
        assert!(clean.contains("token=[redacted]"));
        assert!(!clean.contains("secret123"));
        assert!(!clean.contains("alice"));
    }

    #[test]
    fn truncates_on_utf8_boundaries() {
        let text = "é".repeat(20);
        let cut = truncate_utf8(&text, 20);
        assert!(cut.len() <= 20);
        assert!(cut.ends_with(" [truncated]"));
    }
}
