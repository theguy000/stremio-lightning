//! Discord Rich Presence service matching Stremio Desktop (`stremio-shell-ng`).
//!
//! Uses Stremio's Discord Application ID (`1452620752263319665`), `ActivityType::Watching` (`3`),
//! 5-second timestamp drift debouncing, and a 15-second reconnect backoff when enabled and
//! disconnected. Blocks cleanly on `mpsc::Receiver` with zero active polling when idle or disabled.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use super::discord_ipc::IpcConnection;

pub const APP_ID: &str = "1452620752263319665";
const ACTIVITY_TYPE_WATCHING: u8 = 3;
const TIMESTAMP_DRIFT_SECS: i64 = 5;
const RECONNECT_RETRY: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq)]
pub struct PresenceInput {
    pub title: String,
    pub image: Option<String>,
    pub paused: bool,
    pub time_secs: Option<f64>,
    pub duration_secs: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivityPayload {
    pub details: String,
    pub state: String,
    pub large_image: String,
    pub large_text: &'static str,
    pub start_timestamp: Option<i64>,
    pub end_timestamp: Option<i64>,
}

impl ActivityPayload {
    pub fn to_ipc_json(&self) -> Value {
        let mut obj = serde_json::Map::new();
        obj.insert("type".to_owned(), json!(ACTIVITY_TYPE_WATCHING));
        obj.insert("details".to_owned(), json!(self.details));
        obj.insert("state".to_owned(), json!(self.state));
        obj.insert(
            "assets".to_owned(),
            json!({
                "large_image": self.large_image,
                "large_text": self.large_text,
            }),
        );
        if self.start_timestamp.is_some() || self.end_timestamp.is_some() {
            let mut ts = serde_json::Map::new();
            if let Some(s) = self.start_timestamp {
                ts.insert("start".to_owned(), json!(s));
            }
            if let Some(e) = self.end_timestamp {
                ts.insert("end".to_owned(), json!(e));
            }
            obj.insert("timestamps".to_owned(), Value::Object(ts));
        }
        Value::Object(obj)
    }
}

type SpySink = Box<dyn Fn(Option<&ActivityPayload>) + Send + Sync>;
static SPY: Mutex<Option<SpySink>> = Mutex::new(None);

/// Intercepts Discord presence updates in tests without touching a live Discord socket.
pub fn watch(sink: Option<SpySink>) {
    *SPY.lock().unwrap_or_else(|e| e.into_inner()) = sink;
}

/// Formats seconds as `MM:SS`, or `HH:MM:SS` when at least one hour.
pub fn format_time(secs: f64) -> String {
    let total = secs.max(0.0).floor() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Builds the Discord activity payload, preserving prior timestamps when within 5s drift.
pub fn build_activity(
    input: &PresenceInput,
    now_epoch_secs: i64,
    prev: Option<&ActivityPayload>,
) -> Option<ActivityPayload> {
    let title = input.title.trim();
    if title.is_empty() {
        return None;
    }
    let state = if !input.paused {
        "Watching".to_owned()
    } else {
        match (
            input.time_secs.filter(|v| v.is_finite() && *v >= 0.0),
            input.duration_secs.filter(|v| v.is_finite() && *v > 0.0),
        ) {
            (Some(t), Some(d)) => format!("Paused at {} / {}", format_time(t), format_time(d)),
            _ => "Paused".to_owned(),
        }
    };
    let large_image = input
        .image
        .as_deref()
        .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
        .unwrap_or("stremio_logo")
        .to_owned();

    let (start_timestamp, end_timestamp) = if input.paused {
        (None, None)
    } else if let Some(t) = input.time_secs.filter(|v| v.is_finite() && *v >= 0.0) {
        let elapsed = t.floor() as i64;
        let new_start = now_epoch_secs - elapsed;
        let new_end = input
            .duration_secs
            .filter(|d| d.is_finite() && *d > 0.0)
            .map(|d| now_epoch_secs + ((d.floor() as i64) - elapsed).max(0));
        let keep_prev = prev
            .filter(|p| p.details == title && p.state == state)
            .and_then(|p| {
                let s_ok = p
                    .start_timestamp
                    .is_some_and(|ps| (new_start - ps).abs() <= TIMESTAMP_DRIFT_SECS);
                let e_ok = match (p.end_timestamp, new_end) {
                    (Some(pe), Some(ne)) => (ne - pe).abs() <= TIMESTAMP_DRIFT_SECS,
                    (None, None) => true,
                    _ => false,
                };
                (s_ok && e_ok).then_some((p.start_timestamp, p.end_timestamp))
            });
        keep_prev.unwrap_or((Some(new_start), new_end))
    } else {
        (None, None)
    };

    Some(ActivityPayload {
        details: title.to_owned(),
        state,
        large_image,
        large_text: "Stremio",
        start_timestamp,
        end_timestamp,
    })
}

enum Cmd {
    SetEnabled(bool),
    Update(PresenceInput),
    Clear,
    Shutdown,
}

pub struct DiscordService {
    tx: Sender<Cmd>,
}

impl DiscordService {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("stremio-discord-rpc".into())
            .spawn(move || worker_loop(rx));
        Self { tx }
    }

    pub fn set_enabled(&self, enabled: bool) {
        let _ = self.tx.send(Cmd::SetEnabled(enabled));
    }

    pub fn update(&self, input: PresenceInput) {
        let _ = self.tx.send(Cmd::Update(input));
    }

    pub fn clear(&self) {
        let _ = self.tx.send(Cmd::Clear);
    }
}

impl Drop for DiscordService {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
    }
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn worker_loop(rx: mpsc::Receiver<Cmd>) {
    let mut enabled = false;
    let mut conn: Option<IpcConnection> = None;
    let mut desired: Option<ActivityPayload> = None;
    let mut last_sent: Option<Option<ActivityPayload>> = None;

    loop {
        let cmd =
            if enabled && conn.is_none() && SPY.lock().unwrap_or_else(|e| e.into_inner()).is_none()
            {
                match rx.recv_timeout(RECONNECT_RETRY) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match rx.recv() {
                    Ok(c) => Some(c),
                    Err(_) => break,
                }
            };

        if let Some(cmd) = cmd {
            match cmd {
                Cmd::Shutdown => break,
                Cmd::SetEnabled(next) => {
                    enabled = next;
                    if !enabled {
                        if let Some(c) = conn.take() {
                            c.close();
                        }
                        if last_sent.as_ref().is_some_and(Option::is_some) {
                            notify_spy(None);
                        }
                        last_sent = None;
                        continue;
                    }
                }
                Cmd::Update(input) => {
                    desired = build_activity(&input, now_secs(), desired.as_ref());
                }
                Cmd::Clear => {
                    desired = None;
                }
            }
        }

        if !enabled {
            continue;
        }
        if last_sent.as_ref() == Some(&desired) && (conn.is_some() || has_spy()) {
            continue;
        }
        if has_spy() {
            notify_spy(desired.as_ref());
            last_sent = Some(desired.clone());
            continue;
        }
        if conn.is_none() {
            conn = IpcConnection::connect(APP_ID).ok();
            last_sent = None;
        }
        if let Some(c) = conn.as_mut() {
            let json = desired.as_ref().map(ActivityPayload::to_ipc_json);
            if c.set_activity(json.as_ref()).is_ok() {
                last_sent = Some(desired.clone());
            } else {
                conn = None;
                last_sent = None;
            }
        }
    }
    if let Some(c) = conn.take() {
        c.close();
    }
}

fn has_spy() -> bool {
    SPY.lock().unwrap_or_else(|e| e.into_inner()).is_some()
}

fn notify_spy(payload: Option<&ActivityPayload>) {
    if let Some(spy) = SPY.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        spy(payload);
    }
}

#[cfg(test)]
#[path = "discord_tests.rs"]
mod tests;
