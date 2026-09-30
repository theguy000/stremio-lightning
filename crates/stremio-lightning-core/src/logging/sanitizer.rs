use std::sync::OnceLock;
use regex::Regex;
use super::types::{truncate_utf8, MAX_MESSAGE_LENGTH, MAX_SOURCE_LENGTH};

#[must_use]
pub fn sanitize_message(message: &str) -> (String, bool) {
    static URL_CREDENTIALS: OnceLock<Regex> = OnceLock::new();
    static SECRETS: OnceLock<Regex> = OnceLock::new();
    static WINDOWS_PATHS: OnceLock<Regex> = OnceLock::new();
    static HOME_PATHS: OnceLock<Regex> = OnceLock::new();

    let mut clean = String::with_capacity(message.len().min(MAX_MESSAGE_LENGTH));
    for character in message.chars() {
        if character == '\n' || character == '\t' || !character.is_control() {
            clean.push(character);
        } else {
            clean.push(' ');
        }
    }
    clean = URL_CREDENTIALS
        .get_or_init(|| {
            Regex::new(r#"(?i)\b((?:https?|ftp|rtsp)://)[^/\s:@]+:[^@\s/]+@"#)
                .expect("valid URL credential redaction regex")
        })
        .replace_all(&clean, "$1[redacted]@")
        .into_owned();
    clean = SECRETS
        .get_or_init(|| {
            Regex::new(concat!(
                r#"(?i)\b(authorization|proxy-authorization|cookie|set-cookie|"#,
                r#"token|access[_-]?token|refresh[_-]?token|api[_-]?key|"#,
                r#"password|passwd|secret|session[_-]?id)\b"#,
                r#"(\\?[\"']?\s*[:=]\s*\\?[\"']?)"#,
                r#"(?:\"[^\"]*\"|'[^']*'|(?:Bearer\s+)?[^\s,;}\]\)\r\n]+)"#,
            ))
            .expect("valid secret redaction regex")
        })
        .replace_all(&clean, "$1$2[redacted]")
        .into_owned();
    clean = WINDOWS_PATHS
        .get_or_init(|| {
            Regex::new(r#"(?i)\b[a-z]:\\[^\r\n\t,;\)\]]+"#).expect("valid path regex")
        })
        .replace_all(&clean, "[redacted local path]")
        .into_owned();
    clean = HOME_PATHS
        .get_or_init(|| {
            Regex::new(r#"(?i)(?:/home/|/users/)[^\s\"'<>]+"#).expect("valid home path regex")
        })
        .replace_all(&clean, "[redacted local path]")
        .into_owned();
    let truncated = clean.len() > MAX_MESSAGE_LENGTH;
    (truncate_utf8(&clean, MAX_MESSAGE_LENGTH), truncated)
}

#[must_use]
pub fn sanitize_identifier(value: &str, limit: usize) -> String {
    let clean = value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    truncate_utf8(&sanitize_message(&clean).0, limit)
}

#[must_use]
pub fn sanitize_source(source: &str) -> (String, bool) {
    let mut output = String::with_capacity(source.len().min(MAX_SOURCE_LENGTH));
    let mut truncated = false;
    for character in source.chars() {
        let character = if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        {
            character
        } else {
            '_'
        };
        if output.len() + character.len_utf8() > MAX_SOURCE_LENGTH {
            truncated = true;
            break;
        }
        output.push(character);
    }
    if output.is_empty() {
        output.push_str("unknown");
    }
    let changed_length = output.len() < source.len();
    (output, truncated || changed_length)
}

#[must_use]
pub fn looks_like_url(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.contains("://")
        || value.starts_with("magnet:")
        || value.starts_with("data:")
        || value.starts_with("file:")
}
