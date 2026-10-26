//! Drives the real window the way a person does, and writes down what it expected against what
//! the window then showed.
//!
//! Nothing here reaches for the core directly: a step either presses something a person could
//! press -- found by the name it says on screen -- or reads back a property the window itself
//! draws from. That way an install is only "done" when the card says Uninstall, and a catalog is
//! only "there" when its cards are on the screen.

#![allow(dead_code)]

use std::time::{Duration, Instant};

use i_slint_backend_testing::{ElementHandle, ElementRoot};
use i_slint_core::items::PointerEventButton;
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, Timer, TimerMode};

use stremio_lightning::MainWindow;

/// What an expectation came back with: a line for the report, or why it did not hold.
pub type Check = Box<dyn Fn(&MainWindow) -> Result<String, String>>;
/// Works out the name a press should look for, at the moment the press is made.
pub type LabelOf = Box<dyn Fn(&MainWindow) -> Result<String, String>>;
/// Works out the point a press should land on, at the moment the press is made.
pub type SpotOf = Box<dyn Fn(&MainWindow) -> Result<(f32, f32), String>>;

pub enum Step {
    /// Press the element that says `label`.
    Click(String),
    /// Press a point, for what lives in a popup: a popup is a window of its own, not a child of
    /// the one element handles are searched in.
    ClickAt(f32, f32),
    /// Press the element whose name this window works out when the step is taken -- a menu's name
    /// is the option it is showing, and that is not known until the model behind it has arrived.
    ClickLabel(&'static str, LabelOf),
    /// Press a point this window works out when the step is taken, for a row in a popup: where its
    /// rows sit depends on what the menu had loaded by the moment it was opened.
    ClickWhere(&'static str, SpotOf),
    /// Type into whatever took the focus.
    Type(String),
    /// Something that was not judged, with the reason it was not.
    Note(&'static str, String),
    /// Let the window settle for this long, without judging anything.
    Pause(u64),
    /// Press Backspace `n` times.
    Erase(usize),
    /// Check straight away.
    Expect(&'static str, Check),
    /// Check until it holds or `budget` runs out.
    Wait {
        what: &'static str,
        budget: Duration,
        check: Check,
    },
}

#[derive(Clone)]
pub struct Outcome {
    pub what: String,
    /// `None` when the step could not be judged -- an addon whose own manifest does not answer is
    /// no mark against the app -- and the reason then lives in `detail`.
    pub ok: Option<bool>,
    pub detail: String,
}

impl Outcome {
    pub fn line(&self) -> String {
        let verdict = match self.ok {
            Some(true) => "PASS",
            Some(false) => "FAIL",
            None => "SKIP",
        };
        format!("{verdict}  {:<46} {}", self.what, self.detail)
    }
}

fn tick() -> Duration {
    Duration::from_millis(60)
}

/// One step, taken now. Steps that wait only move on when they hold, or when their budget is gone.
fn step_once(
    window: &MainWindow,
    steps: &[Step],
    at: &mut usize,
    deadline: &mut Option<Instant>,
    outcomes: &mut Vec<Outcome>,
) {
    let Some(step) = steps.get(*at) else {
        return;
    };
    let mut record = |what: String, ok: Option<bool>, detail: String| {
        let outcome = Outcome { what, ok, detail };
        // Printed as it happens: a run that waits on the network should show its work.
        println!("{}", outcome.line());
        outcomes.push(outcome);
    };
    let advance = |at: &mut usize, deadline: &mut Option<Instant>| {
        *at += 1;
        *deadline = None;
    };
    match step {
        Step::Click(label) => {
            let result = click(window, label);
            let detail = match &result {
                Ok(()) => "pressed".to_owned(),
                Err(error) => error.clone(),
            };
            record(format!("press \u{201c}{label}\u{201d}"), Some(result.is_ok()), detail);
            advance(at, deadline);
        }
        Step::ClickAt(x, y) => {
            press_at(window, *x, *y);
            record(format!("press at ({x:.0}, {y:.0})"), Some(true), "pressed".to_owned());
            advance(at, deadline);
        }
        Step::ClickLabel(what, label_of) => {
            match label_of(window) {
                Ok(label) => {
                    let result = click(window, &label);
                    let detail = match &result {
                        Ok(()) => format!("pressed \u{201c}{label}\u{201d}"),
                        Err(error) => error.clone(),
                    };
                    record((*what).to_owned(), Some(result.is_ok()), detail);
                }
                Err(error) => record((*what).to_owned(), Some(false), error.clone()),
            }
            advance(at, deadline);
        }
        Step::ClickWhere(what, spot_of) => {
            match spot_of(window) {
                Ok((x, y)) => {
                    press_at(window, x, y);
                    record((*what).to_owned(), Some(true), format!("pressed at ({x:.0}, {y:.0})"));
                }
                Err(error) => record((*what).to_owned(), Some(false), error.clone()),
            }
            advance(at, deadline);
        }
        Step::Type(text) => {
            type_text(window, text);
            record(format!("type \u{201c}{text}\u{201d}"), Some(true), "typed".to_owned());
            advance(at, deadline);
        }
        Step::Erase(count) => {
            for _ in 0..*count {
                press_key(window, Key::Backspace);
            }
            advance(at, deadline);
        }
        Step::Pause(millis) => {
            std::thread::sleep(Duration::from_millis(*millis));
            advance(at, deadline);
        }
        Step::Note(what, reason) => {
            record((*what).to_owned(), None, reason.clone());
            advance(at, deadline);
        }
        Step::Expect(what, check) => {
            let (ok, detail) = match check(window) {
                Ok(detail) => (true, detail),
                Err(detail) => (false, detail),
            };
            record((*what).to_owned(), Some(ok), detail);
            advance(at, deadline);
        }
        Step::Wait { what, budget, check } => {
            let until = match *deadline {
                Some(until) => until,
                None => {
                    let until = Instant::now() + *budget;
                    *deadline = Some(until);
                    until
                }
            };
            match check(window) {
                Ok(detail) => {
                    record((*what).to_owned(), Some(true), detail);
                    advance(at, deadline);
                }
                Err(detail) => {
                    if Instant::now() >= until {
                        record((*what).to_owned(), Some(false), format!("gave up: {detail}"));
                        advance(at, deadline);
                    }
                }
            }
        }
    }
}

/// Runs the script against `window`, a step at a time, until the steps run out.
///
/// The app posts its news from other threads, so the window only catches up while its event loop
/// turns; that loop is run in short slices between steps. Pressing something *inside* a timer
/// callback would be a mistake: a press makes Slint look at its timers again, and Slint refuses to
/// do that while one is being called. Slices keep the presses outside.
pub fn run(window: &MainWindow, steps: Vec<Step>) -> Vec<Outcome> {
    let mut outcomes: Vec<Outcome> = Vec::new();
    let mut at = 0usize;
    let mut deadline: Option<Instant> = None;
    while at < steps.len() {
        pump(Duration::from_millis(25));
        step_once(window, &steps, &mut at, &mut deadline, &mut outcomes);
    }
    pump(Duration::from_millis(80));
    outcomes
}

/// Lets the event loop turn for `slice`, which is when queued updates from other threads land.
fn pump(slice: Duration) {
    let timer = Timer::default();
    timer.start(TimerMode::SingleShot, slice, || {
        slint::quit_event_loop().ok();
    });
    slint::run_event_loop().expect("the event loop runs");
}

/// Every label on screen, for when a press misses: a failure should say what was there instead.
pub fn labels(window: &MainWindow) -> Vec<String> {
    let mut names: Vec<String> = window
        .root_element()
        .query_descendants()
        .find_all()
        .iter()
        .filter_map(|element| element.accessible_label())
        .map(|label| label.to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

fn click(window: &MainWindow, label: &str) -> Result<(), String> {
    let found = ElementHandle::find_by_accessible_label(window, label).next();
    let Some(handle) = found else {
        let names = labels(window);
        let near: Vec<&String> = names.iter().filter(|name| name.contains(label)).take(4).collect();
        return Err(format!(
            "nothing says this; on screen: {:?}{}",
            names.iter().take(12).collect::<Vec<_>>(),
            if near.is_empty() { String::new() } else { format!(" (near: {near:?})") }
        ));
    };
    let position = handle.absolute_position();
    let size = handle.size();
    println!("  press {label:?} at {position:?} size {size:?}");
    let centre = slint::LogicalPosition::new(
        position.x + size.width / 2.0,
        position.y + size.height / 2.0,
    );
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position: centre });
    pump(Duration::from_millis(40));
    window
        .window()
        .dispatch_event(WindowEvent::PointerPressed { position: centre, button: PointerEventButton::Left });
    // The pointer has to rest on the control long enough for its hover to show, the way a hand
    // does: a press that arrives in the same instant as the move is not what the screen sees.
    pump(Duration::from_millis(40));
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased { position: centre, button: PointerEventButton::Left });
    pump(Duration::from_millis(40));
    Ok(())
}

fn press_at(window: &MainWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
    // No pause between the two: a popup's row is pressed and let go the way `send_mouse_click`
    // does it, and holding the button down here was enough to lose the press.
    window
        .window()
        .dispatch_event(WindowEvent::PointerPressed { position, button: PointerEventButton::Left });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased { position, button: PointerEventButton::Left });
    pump(Duration::from_millis(60));
}

fn type_text(window: &MainWindow, text: &str) {
    for character in text.chars() {
        press_key(window, character);
    }
}

fn press_key(window: &MainWindow, key: impl Into<slint::SharedString>) {
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
}

/// The story the report needs: every outcome, then a verdict line.
pub fn report(outcomes: &[Outcome]) -> bool {
    for outcome in outcomes {
        println!("{}", outcome.line());
    }
    let judged = outcomes.iter().filter(|outcome| outcome.ok.is_some()).count();
    let failed = outcomes.iter().filter(|outcome| outcome.ok == Some(false)).count();
    let skipped = outcomes.len() - judged;
    println!(
        "\n{} of {} expectations held{}{}",
        judged - failed,
        judged,
        if skipped == 0 { String::new() } else { format!(", {skipped} not judged") },
        if failed == 0 { String::new() } else { format!(", {failed} did not") }
    );
    failed == 0
}
