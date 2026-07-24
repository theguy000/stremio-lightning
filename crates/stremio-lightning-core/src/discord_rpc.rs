use discord_rich_presence::{
    activity::{self, ActivityType, Assets, Button, Timestamps},
    DiscordIpc, DiscordIpcClient,
};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const CLIENT_ID: &str = "1492155780839768216";
const RECONNECT_DELAYS_SECS: [u64; 5] = [10, 30, 60, 120, 300];
const DEFAULT_IMAGE: &str = "stremio";

pub struct DiscordRpcState {
    client: Mutex<Option<DiscordIpcClient>>,
    latest_activity: Mutex<Option<ActivityPayload>>,
    enabled: AtomicBool,
    reconnecting: AtomicBool,
}

impl Default for DiscordRpcState {
    fn default() -> Self {
        Self {
            client: Mutex::new(None),
            latest_activity: Mutex::new(None),
            enabled: AtomicBool::new(false),
            reconnecting: AtomicBool::new(false),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActivityPayload {
    pub details: Option<String>,
    pub state: Option<String>,
    pub large_image_key: Option<String>,
    pub large_image_text: Option<String>,
    pub small_image_key: Option<String>,
    pub small_image_text: Option<String>,
    pub start_timestamp: Option<i64>,
    pub end_timestamp: Option<i64>,
    pub activity_type: Option<u8>,
    pub buttons: Option<Vec<ButtonPayload>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ButtonPayload {
    pub label: String,
    pub url: String,
}

impl DiscordRpcState {
    pub fn start(self: &Arc<Self>) -> Result<(), String> {
        let mut client_guard = self.client.lock().map_err(|e| e.to_string())?;

        if client_guard.is_some() {
            self.enabled.store(true, Ordering::SeqCst);
            return Ok(());
        }

        let mut client = DiscordIpcClient::new(CLIENT_ID);

        match client.connect() {
            Ok(()) => {
                *client_guard = Some(client);
                self.enabled.store(true, Ordering::SeqCst);
                Ok(())
            }
            Err(e) => {
                crate::logging::info(
                    "native.discord-rpc",
                    format!("[DiscordRPC] Discord unavailable; reconnecting in background: {e}"),
                );
                *client_guard = Some(client);
                self.enabled.store(true, Ordering::SeqCst);
                drop(client_guard);
                self.spawn_reconnect();
                Ok(())
            }
        }
    }

    pub fn stop(&self) -> Result<(), String> {
        self.enabled.store(false, Ordering::SeqCst);
        let mut client_guard = self.client.lock().map_err(|e| e.to_string())?;

        if let Some(ref mut client) = *client_guard {
            let _ = client.clear_activity();
            let _ = client.close();
        }

        *client_guard = None;
        *self.latest_activity.lock().map_err(|e| e.to_string())? = None;
        Ok(())
    }

    pub fn update_activity(self: &Arc<Self>, payload: ActivityPayload) -> Result<(), String> {
        if !self.enabled.load(Ordering::SeqCst) {
            return Ok(());
        }

        // only the newest presence matters while Discord is unavailable.
        *self.latest_activity.lock().map_err(|e| e.to_string())? = Some(payload.clone());

        if self.reconnecting.load(Ordering::SeqCst) {
            return Ok(());
        }

        let mut client_guard = self.client.lock().map_err(|e| e.to_string())?;

        let Some(client) = client_guard.as_mut() else {
            return Ok(());
        };

        let act = self.build_activity(&payload);

        match client.set_activity(act) {
            Ok(()) => Ok(()),
            Err(e) => {
                drop(client_guard);
                if self.spawn_reconnect() {
                    crate::logging::warn(
                        "native.discord-rpc",
                        format!("[DiscordRPC] Connection lost; reconnecting in background: {e}"),
                    );
                }
                Ok(())
            }
        }
    }

    fn build_activity<'a>(&self, payload: &'a ActivityPayload) -> activity::Activity<'a> {
        let mut act = activity::Activity::new();

        if let Some(ref details) = payload.details {
            act = act.details(details.as_str());
        }

        if let Some(ref s) = payload.state {
            act = act.state(s.as_str());
        }

        // Assets
        let large_key = payload.large_image_key.as_deref().unwrap_or(DEFAULT_IMAGE);
        let large_text = payload
            .large_image_text
            .as_deref()
            .unwrap_or("Stremio Lightning");
        let mut assets = Assets::new().large_image(large_key).large_text(large_text);

        if let Some(ref small_key) = payload.small_image_key {
            assets = assets.small_image(small_key.as_str());
        }
        if let Some(ref small_text) = payload.small_image_text {
            assets = assets.small_text(small_text.as_str());
        }

        act = act.assets(assets);

        // Timestamps
        if payload.start_timestamp.is_some() || payload.end_timestamp.is_some() {
            let mut ts = Timestamps::new();
            if let Some(start) = payload.start_timestamp {
                ts = ts.start(start);
            }
            if let Some(end) = payload.end_timestamp {
                ts = ts.end(end);
            }
            act = act.timestamps(ts);
        }

        // Activity type
        if let Some(at) = payload.activity_type {
            let activity_type = match at {
                2 => ActivityType::Listening,
                3 => ActivityType::Watching,
                5 => ActivityType::Competing,
                _ => ActivityType::Playing,
            };
            act = act.activity_type(activity_type);
        }

        // Buttons (Discord allows max 2)
        if let Some(ref buttons) = payload.buttons {
            let btn_vec: Vec<Button> = buttons
                .iter()
                .take(2)
                .map(|b| Button::new(b.label.as_str(), b.url.as_str()))
                .collect();
            if !btn_vec.is_empty() {
                act = act.buttons(btn_vec);
            }
        }

        act
    }

    fn spawn_reconnect(self: &Arc<Self>) -> bool {
        if self
            .reconnecting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }

        let state = self.clone();
        std::thread::spawn(move || {
            let mut attempt = 0;
            loop {
                let delay = reconnect_delay(attempt);
                attempt = attempt.saturating_add(1);
                for _ in 0..delay.as_secs() {
                    std::thread::sleep(Duration::from_secs(1));
                    if !state.enabled.load(Ordering::SeqCst) {
                        state.reconnecting.store(false, Ordering::SeqCst);
                        return;
                    }
                }

                let mut client_guard = match state.client.lock() {
                    Ok(g) => g,
                    Err(_) => {
                        state.reconnecting.store(false, Ordering::SeqCst);
                        return;
                    }
                };

                let Some(ref mut client) = *client_guard else {
                    state.reconnecting.store(false, Ordering::SeqCst);
                    return;
                };

                if client.reconnect().is_err() {
                    continue;
                }

                let latest_activity = match state.latest_activity.lock() {
                    Ok(activity) => activity,
                    Err(_) => {
                        state.reconnecting.store(false, Ordering::SeqCst);
                        return;
                    }
                };
                if let Some(payload) = latest_activity.as_ref() {
                    if client.set_activity(state.build_activity(payload)).is_err() {
                        continue;
                    }
                }

                state.reconnecting.store(false, Ordering::SeqCst);
                drop(latest_activity);
                crate::logging::info("native.discord-rpc", "[DiscordRPC] Connected");
                return;
            }
        });
        true
    }
}

fn reconnect_delay(attempt: usize) -> Duration {
    Duration::from_secs(RECONNECT_DELAYS_SECS[attempt.min(RECONNECT_DELAYS_SECS.len() - 1)])
}
