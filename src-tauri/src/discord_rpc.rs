use discord_rich_presence::{
    activity::{self, ActivityType, Assets, Button, Timestamps},
    DiscordIpc, DiscordIpcClient,
};
use serde::Deserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::Manager;

const CLIENT_ID: &str = "1492155780839768216";
const RECONNECT_INTERVAL: Duration = Duration::from_secs(10);
const DEFAULT_IMAGE: &str = "stremio";

pub struct DiscordRpcState {
    client: Mutex<Option<DiscordIpcClient>>,
    enabled: AtomicBool,
    reconnecting: AtomicBool,
}

impl Default for DiscordRpcState {
    fn default() -> Self {
        Self {
            client: Mutex::new(None),
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

pub fn start(app: &tauri::AppHandle, state: &DiscordRpcState) -> Result<(), String> {
    let mut client_guard = state.client.lock().map_err(|e| e.to_string())?;

    // If already connected, just ensure enabled flag is set
    if client_guard.is_some() {
        eprintln!("[DiscordRPC] Already have client, setting enabled=true");
        state.enabled.store(true, Ordering::SeqCst);
        return Ok(());
    }

    eprintln!("[DiscordRPC] Creating new IPC client with ID: {CLIENT_ID}");
    let mut client = DiscordIpcClient::new(CLIENT_ID);

    match client.connect() {
        Ok(()) => {
            eprintln!("[DiscordRPC] Connected to Discord successfully");
            *client_guard = Some(client);
            state.enabled.store(true, Ordering::SeqCst);
            Ok(())
        }
        Err(e) => {
            eprintln!("[DiscordRPC] Failed to connect to Discord: {e}");
            // Store the client even if connection failed — we'll reconnect
            *client_guard = Some(client);
            state.enabled.store(true, Ordering::SeqCst);
            // Spawn reconnect thread
            drop(client_guard);
            spawn_reconnect(app.clone());
            Ok(())
        }
    }
}

pub fn stop(state: &DiscordRpcState) -> Result<(), String> {
    state.enabled.store(false, Ordering::SeqCst);
    let mut client_guard = state.client.lock().map_err(|e| e.to_string())?;

    if let Some(ref mut client) = *client_guard {
        let _ = client.clear_activity();
        let _ = client.close();
        eprintln!("[DiscordRPC] Disconnected from Discord");
    }

    *client_guard = None;
    Ok(())
}

pub fn update_activity(
    app: &tauri::AppHandle,
    state: &DiscordRpcState,
    payload: ActivityPayload,
) -> Result<(), String> {
    if !state.enabled.load(Ordering::SeqCst) {
        eprintln!("[DiscordRPC] update_activity called but RPC is disabled");
        return Ok(());
    }

    let mut client_guard = state.client.lock().map_err(|e| e.to_string())?;

    let client = match client_guard.as_mut() {
        Some(c) => c,
        None => {
            eprintln!("[DiscordRPC] update_activity called but no client exists");
            return Ok(());
        }
    };

    eprintln!(
        "[DiscordRPC] Setting activity: details={:?}, state={:?}, type={:?}",
        payload.details, payload.state, payload.activity_type
    );

    let mut act = activity::Activity::new();

    if let Some(ref details) = payload.details {
        act = act.details(details.as_str());
    }

    if let Some(ref s) = payload.state {
        act = act.state(s.as_str());
    }

    // Build assets
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
    let has_start = payload.start_timestamp.is_some();
    let has_end = payload.end_timestamp.is_some();
    if has_start || has_end {
        let mut ts = Timestamps::new();
        if let Some(start) = payload.start_timestamp {
            ts = ts.start(start);
        }
        if let Some(end) = payload.end_timestamp {
            ts = ts.end(end);
        }
        act = act.timestamps(ts);
    }

    // Activity type (3 = Watching)
    if let Some(at) = payload.activity_type {
        let activity_type = match at {
            3 => ActivityType::Watching,
            2 => ActivityType::Listening,
            5 => ActivityType::Competing,
            _ => ActivityType::Playing,
        };
        act = act.activity_type(activity_type);
    }

    // Buttons
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

    match client.set_activity(act) {
        Ok(()) => {
            eprintln!("[DiscordRPC] Activity set successfully");
            Ok(())
        }
        Err(e) => {
            eprintln!("[DiscordRPC] Failed to set activity: {e}");
            // Connection may be dead — try to reconnect
            drop(client_guard);
            spawn_reconnect(app.clone());
            Err(format!("Failed to set Discord activity: {e}"))
        }
    }
}

fn spawn_reconnect(app: tauri::AppHandle) {
    let state = app.state::<DiscordRpcState>();

    // Only allow one reconnect thread at a time
    if state
        .reconnecting
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    std::thread::spawn(move || {
        let state = app.state::<DiscordRpcState>();

        loop {
            // Staggered sleep: check the enabled flag every second instead of
            // sleeping the full reconnect interval at once.  This lets the
            // thread exit quickly when the user toggles RPC off and also
            // clears the `reconnecting` flag promptly so a subsequent
            // start → spawn_reconnect cycle isn't silently blocked.
            for _ in 0..RECONNECT_INTERVAL.as_secs() {
                std::thread::sleep(Duration::from_secs(1));
                if !state.enabled.load(Ordering::SeqCst) {
                    eprintln!("[DiscordRPC] Reconnect cancelled (disabled)");
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

            if let Some(ref mut client) = *client_guard {
                match client.reconnect() {
                    Ok(()) => {
                        eprintln!("[DiscordRPC] Reconnected to Discord");
                        state.reconnecting.store(false, Ordering::SeqCst);
                        return;
                    }
                    Err(e) => {
                        eprintln!("[DiscordRPC] Reconnect attempt failed: {e}");
                    }
                }
            } else {
                // Client was dropped (stop was called)
                state.reconnecting.store(false, Ordering::SeqCst);
                return;
            }
        }
    });
}
