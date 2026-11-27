//! Background gamepad listener (`gilrs`) emitting plain `GamepadAction` events.
//!
//! Spawns its OS input thread only while enabled and tears it down cleanly when disabled.
//! Zero Slint imports.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, Event, EventType, Gilrs};

const DEADZONE: f32 = 0.28;
const REPEAT_INITIAL: Duration = Duration::from_millis(250);
const REPEAT_MIN: Duration = Duration::from_millis(100);
const WAIT_SLICE: Duration = Duration::from_millis(50);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControllerFamily {
    PlayStation,
    Xbox,
    Generic,
}

impl ControllerFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PlayStation => "playstation",
            Self::Xbox => "xbox",
            Self::Generic => "generic",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamepadAction {
    Connected(ControllerFamily),
    Disconnected,
    Confirm,
    Back,
    ActionX,
    Fullscreen,
    PrevTab,
    NextTab,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    SeekBack,
    SeekForward,
    VolumeUp,
    VolumeDown,
}

impl GamepadAction {
    pub fn control_id(self) -> &'static str {
        match self {
            Self::Confirm => "a",
            Self::Back => "b",
            Self::ActionX => "x",
            Self::Fullscreen => "y",
            Self::PrevTab => "lb",
            Self::NextTab => "rb",
            Self::MoveUp | Self::MoveDown | Self::MoveLeft | Self::MoveRight => "lstick",
            Self::SeekBack | Self::SeekForward | Self::VolumeUp | Self::VolumeDown => "rstick",
            Self::Connected(_) | Self::Disconnected => "",
        }
    }
}

/// Detects PlayStation vs Xbox vs generic controller family (matching `GamepadProvider.tsx`).
pub fn detect_family(name: &str) -> ControllerFamily {
    let l = name.to_ascii_lowercase();
    if ["sony", "playstation", "dualsense", "dualshock", "054c"]
        .iter()
        .any(|k| l.contains(k))
    {
        ControllerFamily::PlayStation
    } else if ["xbox", "microsoft", "xinput", "045e"]
        .iter()
        .any(|k| l.contains(k))
    {
        ControllerFamily::Xbox
    } else {
        ControllerFamily::Generic
    }
}

pub fn map_button(button: Button) -> Option<GamepadAction> {
    match button {
        Button::South => Some(GamepadAction::Confirm),
        Button::East => Some(GamepadAction::Back),
        Button::West => Some(GamepadAction::ActionX),
        Button::North => Some(GamepadAction::Fullscreen),
        Button::LeftTrigger => Some(GamepadAction::PrevTab),
        Button::RightTrigger => Some(GamepadAction::NextTab),
        Button::DPadUp => Some(GamepadAction::MoveUp),
        Button::DPadDown => Some(GamepadAction::MoveDown),
        Button::DPadLeft => Some(GamepadAction::MoveLeft),
        Button::DPadRight => Some(GamepadAction::MoveRight),
        _ => None,
    }
}

/// Maps stick axis + value (gilrs uses +Y = up on standard gamepads) to an action.
pub fn map_axis(axis: Axis, value: f32) -> Option<GamepadAction> {
    if value.abs() < DEADZONE {
        return None;
    }
    match axis {
        Axis::LeftStickX | Axis::DPadX => Some(if value < 0.0 {
            GamepadAction::MoveLeft
        } else {
            GamepadAction::MoveRight
        }),
        Axis::LeftStickY | Axis::DPadY => Some(if value > 0.0 {
            GamepadAction::MoveUp
        } else {
            GamepadAction::MoveDown
        }),
        Axis::RightStickX => Some(if value < 0.0 {
            GamepadAction::SeekBack
        } else {
            GamepadAction::SeekForward
        }),
        Axis::RightStickY => Some(if value > 0.0 {
            GamepadAction::VolumeUp
        } else {
            GamepadAction::VolumeDown
        }),
        _ => None,
    }
}

pub fn repeat_interval(magnitude: f32) -> Duration {
    let mag = magnitude.abs().clamp(DEADZONE, 1.0);
    let span_ms = (1.0 - mag) * 400.0;
    REPEAT_MIN + Duration::from_millis(span_ms as u64)
}

struct RunningWorker {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Drop for RunningWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub struct GamepadService {
    on_action: Arc<dyn Fn(GamepadAction) + Send + Sync>,
    worker: Mutex<Option<RunningWorker>>,
}

impl GamepadService {
    pub fn new(on_action: impl Fn(GamepadAction) + Send + Sync + 'static) -> Self {
        Self {
            on_action: Arc::new(on_action),
            worker: Mutex::new(None),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut slot = self.worker.lock().unwrap_or_else(|e| e.into_inner());
        match (enabled, slot.is_some()) {
            (true, false) => {
                let stop = Arc::new(AtomicBool::new(false));
                let stop_run = Arc::clone(&stop);
                let cb = Arc::clone(&self.on_action);
                if let Ok(join) = std::thread::Builder::new()
                    .name("stremio-gamepad".into())
                    .spawn(move || run_worker(stop_run, cb))
                {
                    *slot = Some(RunningWorker {
                        stop,
                        join: Some(join),
                    });
                }
            }
            (false, true) => {
                slot.take();
            }
            _ => {}
        }
    }
}

fn run_worker(stop: Arc<AtomicBool>, cb: Arc<dyn Fn(GamepadAction) + Send + Sync>) {
    let Ok(mut gilrs) = Gilrs::new() else {
        return;
    };
    let mut held_left: Option<(GamepadAction, f32, Instant)> = None;
    let mut held_right: Option<(GamepadAction, f32, Instant)> = None;

    while !stop.load(Ordering::Relaxed) {
        let mut pending = gilrs.next_event_blocking(Some(WAIT_SLICE));
        while let Some(Event { id, event, .. }) = pending {
            match event {
                EventType::Connected => {
                    let family = detect_family(gilrs.gamepad(id).name());
                    cb(GamepadAction::Connected(family));
                }
                EventType::Disconnected => {
                    held_left = None;
                    held_right = None;
                    cb(GamepadAction::Disconnected);
                }
                EventType::ButtonPressed(btn, _) => {
                    if let Some(act) = map_button(btn) {
                        cb(act);
                    }
                }
                EventType::AxisChanged(axis, val, _) => {
                    let slot = match axis {
                        Axis::LeftStickX | Axis::LeftStickY | Axis::DPadX | Axis::DPadY => {
                            &mut held_left
                        }
                        Axis::RightStickX | Axis::RightStickY => &mut held_right,
                        _ => {
                            pending = gilrs.next_event();
                            continue;
                        }
                    };
                    match map_axis(axis, val) {
                        Some(act) => {
                            let is_new = slot.map(|(a, _, _)| a) != Some(act);
                            if is_new {
                                cb(act);
                                *slot = Some((act, val.abs(), Instant::now() + REPEAT_INITIAL));
                            } else if let Some((_, mag, _)) = slot.as_mut() {
                                *mag = val.abs();
                            }
                        }
                        None => {
                            *slot = None;
                        }
                    }
                }
                _ => {}
            }
            pending = gilrs.next_event();
        }
        let now = Instant::now();
        for slot in [&mut held_left, &mut held_right] {
            if let Some((act, mag, due)) = slot.as_mut() {
                if now >= *due {
                    cb(*act);
                    *due = now + repeat_interval(*mag);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "gamepad_tests.rs"]
mod tests;
