//! One plugin's thread: the interpreter, the call loop, and the rules that turn what a plugin
//! asked for into calls on the host.

use std::collections::BTreeMap;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use rquickjs::Runtime;
use serde_json::{Map, Value as Json};

use super::globals::{self, Held, Settings};
use super::{
    lock, Call, Channel, Host, LogLine, Outcome, Run, Timer, BUDGET, MEMORY_LIMIT, STACK_LIMIT,
};

/// The plugin's own thread's stack. The interpreter's own limit is measured from the frame an
/// evaluation starts in, so the thread needs room for that frame plus [`STACK_LIMIT`] of
/// JavaScript.
const STACK_SIZE: usize = 2 * 1024 * 1024;

/// A plugin that has been started, as the app sees it from outside.
pub struct Running {
    channel: Channel,
    /// Why the plugin is no longer running, empty while it is.
    stopped: String,
    /// Whether the thread's worker holds a timer. Kept here so the app can tell without asking
    /// the thread, which an idle plugin would otherwise be woken for several times a second.
    has_timers: Arc<std::sync::atomic::AtomicBool>,
}

impl Running {
    /// A plugin whose file is still being loaded. Nothing has called into it yet, so it has no
    /// answer to give and no reason to have stopped.
    pub fn starting() -> Self {
        Self {
            channel: Channel {
                calls: None,
                closed: None,
                wound: None,
            },
            stopped: String::new(),
            has_timers: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Starts the plugin on its own thread and loads `source`. `plugin` is the plugin's file name.
    /// The host is what its settings calls reach: a folder in a test, the app in the shell.
    /// Returns the plugin and what the load did: its lines, and whether it stopped.
    pub fn start<H: Host + Send + 'static>(
        source: String,
        plugin: &str,
        host: H,
    ) -> (Self, Outcome) {
        let (calls, commands) = std::sync::mpsc::channel();
        let name = plugin.to_owned();
        let has_timers = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let threads_timers = Arc::clone(&has_timers);
        let spawned = std::thread::Builder::new()
            .name(format!("plugin {}", super::name_of(plugin)))
            .stack_size(STACK_SIZE)
            .spawn(move || worker_main(&name, source, host, &commands, threads_timers));
        let mut running = Self {
            channel: Channel {
                calls: Some(calls),
                closed: None,
                wound: None,
            },
            stopped: String::new(),
            has_timers,
        };
        if let Err(error) = spawned {
            let reason = format!("The plugin's interpreter did not start: {error}");
            running.channel.close(reason.clone());
            running.stopped = reason.clone();
            return (
                running,
                Outcome {
                    run: Run::Threw(reason),
                    ..Outcome::default()
                },
            );
        }
        let outcome = running.channel.ask(Call::Started);
        running.record(&outcome);
        (running, outcome)
    }

    /// Whether the plugin is still able to run: it loaded and has not thrown since.
    pub fn running(&self) -> bool {
        self.stopped.is_empty()
    }

    /// Whether the plugin has a timer that has not fired or been cleared. Asked before waking the
    /// plugin's thread, so an idle plugin costs nothing.
    pub fn holds_timers(&self) -> bool {
        match &self.channel.calls {
            Some(_) => self.has_timers.load(std::sync::atomic::Ordering::Relaxed),
            None => false,
        }
    }

    /// Why the plugin stopped, empty while it runs.
    pub fn stopped(&self) -> &str {
        &self.stopped
    }

    /// Tells the plugin its settings were saved. `values` is the plugin's settings file as the
    /// Plugin settings page left it, the schema included.
    pub fn settings_saved(&mut self, values: Json) -> Outcome {
        let outcome = self.channel.ask(|reply| Call::SettingsSaved(values, reply));
        self.record(&outcome);
        outcome
    }

    /// Runs the timers that have come due since the last call and returns what they logged and
    /// asked for. A plugin's timer code only runs inside a call into it, so the app's pump is what
    /// makes a `setTimeout` fire: nothing else calls in when the app has nothing to report.
    pub fn advance(&mut self) -> Outcome {
        let outcome = self.channel.ask(Call::Tick);
        self.record(&outcome);
        outcome
    }

    /// Tells every listener for `event` that it happened.
    pub fn deliver(&mut self, event: super::Event) -> Outcome {
        let (name, value) = event.named();
        let outcome = self
            .channel
            .ask(|reply| Call::Event(name.to_owned(), value, reply));
        self.record(&outcome);
        outcome
    }

    /// Keeps the plugin's state up to date with what one call did. A call that went through
    /// leaves it running, whatever a call before it said: the reason a call stopped is about that
    /// call, and a plugin that answered this one is not stopped. A failure replaces the reason
    /// with its own.
    fn record(&mut self, outcome: &Outcome) {
        self.stopped = if outcome.run.failed() {
            outcome.run.said()
        } else {
            String::new()
        };
    }
}

fn worker_main<H: Host + 'static>(
    plugin: &str,
    source: String,
    host: H,
    calls: &Receiver<Call>,
    has_timers: Arc<std::sync::atomic::AtomicBool>,
) {
    let mut worker = match Worker::new(plugin, host, has_timers) {
        Ok(worker) => worker,
        Err(error) => {
            // Nothing is listening for this answer: the start is still setting its channel up.
            // Fail the start instead, so it says why no plugin is running.
            while let Ok(call) = calls.recv() {
                call.answer(error.clone());
            }
            return;
        }
    };
    // A panic here is a bug in the app rather than in the plugin, and this thread is where it
    // lands: it must not take the window with it. The panic is answered as the call's own
    // outcome, and the line that says so is reported like any other stop.
    while let Ok(call) = calls.recv() {
        let reply = call.reply().clone();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            call.run(&mut worker, &source)
        }));
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(_) => {
                let reason = super::PANICKED.to_owned();
                // The start's outcome is the one the caller keeps, so the reason goes back on the
                // channel with the answer the file's own lines never made.
                lock(&worker.host).log(plugin, LogLine::new("error", reason.clone()), true);
                Outcome {
                    run: Run::Threw(reason),
                    ..Outcome::default()
                }
            }
        };
        let closed = outcome.run.failed();
        if closed {
            let reason = outcome.run.said();
            lock(&worker.host).log(plugin, LogLine::new("error", reason), true);
        }
        // The reply goes back before the thread ends, so the call that panicked hears about it
        // rather than timing out. Dropping the channel afterwards is what tells every later call
        // the plugin is gone.
        let _ = reply.send(outcome);
        drop(reply);
        if closed {
            return;
        }
    }
}

/// The plugin's thread: the interpreter, and the state that outlives one call.
pub(super) struct Worker<H: Host> {
    plugin: String,
    /// The host behind a lock shared with the settings session. One call at a time uses it, so
    /// the lock is never contended; it is there so the session can own its host.
    host: Arc<Mutex<H>>,
    world: globals::Shared,
    /// The callbacks `onSettingsSaved` registered, kept here rather than in JavaScript so a plugin
    /// cannot lose them by clearing a global.
    on_settings_saved: Vec<Held>,
    /// The callbacks `on` registered, by event name.
    listeners: BTreeMap<String, Vec<Held>>,
    /// The timers that have not fired or been cleared, by id.
    timers: BTreeMap<u32, Timer>,
    /// Whether `timers` is empty, shared with the `Running` the app holds, so the app can tell
    /// whether waking this plugin's thread would run anything at all.
    has_timers: Arc<std::sync::atomic::AtomicBool>,
    /// The interpreter's context. It holds the runtime that made it, so the runtime lives as long
    /// as this does; there is no separate field for it.
    context: rquickjs::Context,
}

/// What one evaluation asked for, after the interpreter is done with it.
struct Settled {
    outcome: Outcome,
    requests: globals::Requests,
}

impl<H: Host> Drop for Worker<H> {
    /// Lets go of everything JavaScript in the order QuickJS requires.
    ///
    /// A `Persistent` saved from a function has to be dropped before the context that function
    /// came from, and the context before the runtime. QuickJS asserts that its object list is
    /// empty when the runtime goes, so an object still held at that point aborts the process
    /// rather than leaking. The struct's own field order would drop the runtime first, which is
    /// why this impl exists at all; the rest of the fields are then dropped in the usual way.
    fn drop(&mut self) {
        let listeners = std::mem::take(&mut self.listeners);
        let on_settings_saved = std::mem::take(&mut self.on_settings_saved);
        let timers = std::mem::take(&mut self.timers);
        drop(listeners);
        drop(on_settings_saved);
        drop(timers);
    }
}

impl<H: Host + 'static> Worker<H> {
    fn new(
        plugin: &str,
        host: H,
        has_timers: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self, String> {
        let runtime = Runtime::new()
            .map_err(|error| format!("The JavaScript interpreter did not start: {error}"))?;
        runtime.set_memory_limit(MEMORY_LIMIT);
        // A plugin that recurses without end reaches this limit, not the page after the stack.
        runtime.set_max_stack_size(STACK_LIMIT);
        let world = globals::World::new();
        // The interrupt handler is the one thing that can stop a plugin which never returns to
        // the interpreter: QuickJS calls it every few thousand instructions, and the exception it
        // raises cannot be caught inside the plugin.
        {
            let world = globals::Shared::clone(&world);
            runtime.set_interrupt_handler(Some(Box::new(move || world.is_over())));
        }
        let context = rquickjs::Context::full(&runtime)
            .map_err(|error| format!("The JavaScript interpreter did not start: {error}"))?;
        context
            .with(|ctx| {
                ctx.store_userdata(globals::stash(&world))?;
                globals::install(&ctx)
            })
            .map_err(|error| format!("The plugin's functions could not be set up: {error}"))?;
        Ok(Self {
            plugin: plugin.to_owned(),
            host: Arc::new(Mutex::new(host)),
            world,
            on_settings_saved: Vec::new(),
            listeners: BTreeMap::new(),
            timers: BTreeMap::new(),
            has_timers,
            context,
        })
    }

    /// Runs the plugin's file for the first time, then every timer that came due while it loaded.
    pub(super) fn load(&mut self, source: &str) -> Outcome {
        let settled = self.settle(|_, ctx| {
            let _: rquickjs::Value = ctx.eval(source)?;
            Ok(())
        });
        self.finish(settled)
    }

    /// Calls the plugin's `onSettingsSaved` callbacks with the saved values. While they run,
    /// `getSetting` answers from the file's new values.
    pub(super) fn settings_saved(&mut self, values: Json) -> Outcome {
        let fresh: Map<String, Json> = values.as_object().cloned().unwrap_or_default();
        *lock(&self.world.settings) = Some(fresh);
        let callbacks = std::mem::take(&mut self.on_settings_saved);
        // A panic in the callbacks would leave the saved values standing in for the file's, so
        // they are put back whether the call returned or unwound.
        let settled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.settle(|this, ctx| this.called(ctx, &callbacks, &values))
        }));
        *lock(&self.world.settings) = None;
        let settled = match settled {
            Ok(settled) => settled,
            Err(panic) => std::panic::resume_unwind(panic),
        };
        let stopped = settled.outcome.run.failed();
        let outcome = self.finish(settled);
        if !stopped {
            self.on_settings_saved = callbacks;
        }
        outcome
    }

    /// Calls the plugin's callbacks for one event, with the event's value.
    pub(super) fn deliver(&mut self, event: &str, value: Json) -> Outcome {
        let callbacks = self.listeners.remove(event).unwrap_or_default();
        // A panic leaves the listeners where they were, as if the call had never run.
        let settled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.settle(|this, ctx| this.called(ctx, &callbacks, &value))
        }));
        let settled = match settled {
            Ok(settled) => settled,
            Err(panic) => {
                self.listeners.insert(event.to_owned(), callbacks);
                std::panic::resume_unwind(panic);
            }
        };
        let stopped = settled.outcome.run.failed();
        let outcome = self.finish(settled);
        if !stopped {
            self.listeners.insert(event.to_owned(), callbacks);
        }
        outcome
    }

    /// Runs one evaluation with the budget armed and the host reachable from its settings
    /// functions. `run` gets the interpreter's context, which is needed to read back the
    /// exception a plugin threw before the evaluation ends.
    fn settle(
        &mut self,
        run: impl FnOnce(&mut Self, &rquickjs::Ctx<'_>) -> rquickjs::Result<()>,
    ) -> Settled {
        let session = Session {
            plugin: self.plugin.clone(),
            host: Arc::clone(&self.host),
            world: globals::Shared::clone(&self.world),
        };
        self.world.begin(BUDGET);
        // The context is borrowed for the call, and `run` needs the worker itself, so the call
        // goes through a clone: `Context` is a handle, and both name the same interpreter.
        let context = self.context.clone();
        let result = context.with(|ctx| {
            let result =
                globals::inside_call(globals::SettingsHandle::new(session), || run(self, &ctx));
            // Judged inside the context and before the evaluation is ended: the verdict asks
            // whether the clock ran out, and ending an evaluation clears the deadline that
            // answered it.
            self.world.verdict(&ctx, result)
        });
        let run = result;
        self.world.end();
        let requests = self.world.take();
        let outcome = Outcome {
            run,
            logs: requests
                .logs
                .iter()
                .map(|(level, text)| LogLine::new(level, text.clone()))
                .collect(),
            theme: requests.theme.clone(),
        };
        Settled { outcome, requests }
    }

    /// Puts what one evaluation asked for into effect on the host and on the timer list, then
    /// fires whatever came due because of it.
    fn finish(&mut self, settled: Settled) -> Outcome {
        let name = self.plugin.clone();
        let requests = settled.requests;
        for line in &settled.outcome.logs {
            lock(&self.host).log(&name, line.clone(), false);
        }
        for (key, value) in &requests.saved {
            lock(&self.host).save_setting(&name, key, value);
        }
        if let Some(rules) = &settled.outcome.theme {
            lock(&self.host).apply_theme(&name, rules);
        }
        self.on_settings_saved.extend(requests.saved_callbacks);
        for (event, callback) in requests.listeners {
            self.listeners.entry(event).or_default().push(callback);
        }
        for id in requests.cleared {
            self.timers.remove(&id);
        }
        for (id, after, period, code) in requests.timers {
            if self.timers.len() >= super::MAX_TIMERS {
                return stopped_timers(settled.outcome);
            }
            self.timers.insert(
                id,
                Timer {
                    due: Instant::now() + after,
                    period,
                    code,
                },
            );
        }
        let mut outcome = settled.outcome;
        if !outcome.run.failed() {
            outcome.merge(self.fire_due());
        }
        outcome
    }

    /// Runs every timer that is due, until none is or the plugin stops.
    /// Says whether this worker holds a timer, for the `Running` the app keeps.
    pub(super) fn publish_timers(&self) {
        self.has_timers.store(
            !self.timers.is_empty(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    /// The timers that have come due, as the app's pump asks for them. A plugin with nothing due
    /// has nothing to say, and the pump asks several times a second: that case is answered here
    /// rather than by waking the plugin's thread for it.
    pub(super) fn advance(&mut self) -> Outcome {
        match self.due_now() {
            Some(_) => self.fire_due(),
            None => Outcome::default(),
        }
    }

    fn fire_due(&mut self) -> Outcome {
        let mut outcome = Outcome::default();
        while let Some((id, timer)) = self.due_now() {
            let code = std::rc::Rc::clone(&timer.code);
            let period = timer.period;
            let due = timer.due;
            self.timers.remove(&id);
            let settled = self.settle(|_, ctx| globals::handed(ctx, &code, ()));
            // `finish` runs what this step asked for and every timer it made due, so its answer
            // already holds this step's lines.
            let step = self.finish(settled);
            outcome.merge(step);
            if outcome.run.failed() {
                break;
            }
            // A timer that fires its own successor is a plugin that never stops. The interpreter
            // has already failed the step in that case; this is the check that catches it first.
            if let Some(period) = period {
                if self.timers.len() >= super::MAX_TIMERS {
                    let step = stopped_timers(Outcome::default());
                    outcome.merge(step);
                    break;
                }
                self.timers.insert(
                    id,
                    Timer {
                        due: due + period,
                        period: Some(period),
                        code,
                    },
                );
            }
        }
        outcome
    }

    /// Calls each callback with `value`. The call's settings session is already in place.
    fn called(
        &mut self,
        ctx: &rquickjs::Ctx<'_>,
        callbacks: &[Held],
        value: &Json,
    ) -> rquickjs::Result<()> {
        let session = Session {
            plugin: self.plugin.clone(),
            host: Arc::clone(&self.host),
            world: globals::Shared::clone(&self.world),
        };
        globals::inside_call(globals::SettingsHandle::new(session), || {
            let value = globals::to_js(ctx, value)?;
            for callback in callbacks {
                globals::handed(ctx, callback, (value.clone(),))?;
            }
            Ok::<(), rquickjs::Error>(())
        })
    }

    fn due_now(&self) -> Option<(u32, &Timer)> {
        let now = Instant::now();
        self.timers
            .iter()
            .filter(|(_, timer)| timer.due <= now)
            .min_by_key(|(_, timer)| timer.due)
            .map(|(id, timer)| (*id, timer))
    }
}

/// Why a plugin that left more than [`super::MAX_TIMERS`] timers waiting was stopped, as the
/// outcome that reports it. The sentence is both the stop reason and the line the screens show:
/// a card says "Stopped:" and then the reason, so the reason has to travel with the outcome even
/// when the host listening for it is the one that has just closed.
fn stopped_timers(outcome: Outcome) -> Outcome {
    let reason = format!(
        "It left more than {} timers waiting at once, so it was stopped. A setInterval, or a timer that sets its own successor this often, is how a plugin ends up here.",
        super::MAX_TIMERS
    );
    let mut outcome = outcome;
    outcome.run = Run::Threw(reason.clone());
    outcome.logs.push(LogLine::new("error", reason));
    outcome
}

/// How a plugin reads and writes its settings: the app's host for the settings file, and the
/// values the Plugin settings page has just saved while the plugin's own callbacks run.
///
/// The session has the host to itself for the length of one call. A global that runs inside a
/// plugin can only run during a call, so the globals that write a settings file always have a
/// host to write to.
struct Session<H: Host> {
    plugin: String,
    host: Arc<Mutex<H>>,
    world: globals::Shared,
}

impl<H: Host> Settings for Session<H> {
    fn get(&mut self, key: &str) -> Json {
        if let Some(values) = &*lock(&self.world.settings) {
            if let Some(value) = values.get(key) {
                return value.clone();
            }
        }
        lock(&self.host).get_setting(&self.plugin, key)
    }

    fn save(&mut self, key: &str, value: &Json) -> Result<(), String> {
        lock(&self.host).save_setting(&self.plugin, key, value);
        Ok(())
    }

    /// Records the schema and hands the page's verdict back, so a schema the app cannot read
    /// throws from `registerSettings` rather than being kept as though the page could draw it.
    fn register(&mut self, schema: &Json) -> Result<(), String> {
        lock(&self.host).register_settings(&self.plugin, schema)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    use crate::app::plugins::Asked;
    use crate::plumbing::plugin::{Event, Folder};

    /// A host that fails the way a bug in the app would: the registration it is asked for panics.
    /// Every other call goes through, so the plugin's own code still loads.
    struct Crash;

    impl Host for Crash {
        fn get_setting(&mut self, _plugin: &str, _key: &str) -> Json {
            Json::Null
        }

        fn save_setting(&mut self, _plugin: &str, _key: &str, _value: &Json) {}

        fn register_settings(&mut self, _plugin: &str, _schema: &Json) -> Result<(), String> {
            panic!("the app's own registration failed");
        }

        fn apply_theme(&mut self, _plugin: &str, _rules: &[Json]) {}

        fn log(&mut self, _plugin: &str, _line: LogLine, _stopped: bool) {}
    }

    /// A plugin's own folder: the settings file its callbacks read, and the plugins folder one
    /// test writes to. A folder of its own per call, because tests run side by side.
    fn folder() -> Folder {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let unique = COUNT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "stremio-lightning-worker-{}-{unique}",
            std::process::id()
        ));
        let folder = Folder::new(&dir);
        let plugins = folder.data_dir.join("stremio-lightning").join("plugins");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(plugins);
        folder
    }

    #[test]
    fn a_plugin_that_is_on_its_way_to_running_is_not_started_twice() {
        let dir = folder();
        let path = dir.data_dir.join("stremio-lightning").join("plugins");
        let _ = std::fs::create_dir_all(&path);
        // A tenth of a second of loading: wider than a second press arriving together with the
        // first, and well inside the 200 ms a step is given.
        std::fs::write(
            path.join("slow.plugin.js"),
            "const until = Date.now() + 100; while (Date.now() < until) {} logger.info('loaded once');",
        )
        .expect("the plugin file");
        let runtime = crate::app::plugins::Runtime::new(dir.data_dir.clone());

        // Two threads press the switch at the same time, inside the window the load takes.
        let handlers: Vec<_> = (0..2)
            .map(|_| {
                let runtime = Arc::clone(&runtime);
                std::thread::spawn(move || runtime.start("slow.plugin.js"))
            })
            .collect();
        for handler in handlers {
            let _ = handler.join().expect("the press did not panic");
        }

        // One interpreter, so the file's line is in the plugin's log once.
        let lines: Vec<String> = runtime
            .lines()
            .into_iter()
            .filter(|(file, _)| file == "slow.plugin.js")
            .map(|(_, line)| line.text)
            .collect();
        assert_eq!(lines, vec!["loaded once".to_owned()], "lines: {lines:?}");
    }

    #[test]
    fn a_plugin_that_never_returns_says_why_on_its_card_as_well_as_in_its_log() {
        let dir = folder();
        let path = dir.data_dir.join("stremio-lightning").join("plugins");
        let _ = std::fs::create_dir_all(&path);
        std::fs::write(
            path.join("spin.plugin.js"),
            "on('fullscreen', function () { while (true) {} });",
        )
        .expect("the plugin file");
        let runtime = crate::app::plugins::Runtime::new(dir.data_dir.clone());
        runtime.start("spin.plugin.js");

        // The event is heard inside the plugin, which never comes back. Its thread is the one
        // that would report that, so the reason has to come back with the answer instead.
        let asked = runtime.deliver(Event::Fullscreen { on: true });
        assert!(
            asked
                .iter()
                .any(|ask| matches!(ask, Asked::Changed | Asked::Log(..))),
            "the app was told: {asked:?}"
        );

        let state = runtime.state("spin.plugin.js", true);
        assert!(!state.running);
        assert!(
            state.stopped.contains("past the time"),
            "the card says: {:?}",
            state.stopped
        );
        let logged: Vec<String> = runtime
            .lines()
            .into_iter()
            .filter(|(file, _)| file == "spin.plugin.js")
            .map(|(_, line)| line.text)
            .collect();
        assert!(
            logged.iter().any(|line| line.contains("past the time")),
            "the Logs page has the reason: {logged:?}"
        );
    }

    #[test]
    fn a_line_a_plugin_logs_from_a_timer_reaches_its_card() {
        let dir = folder();
        let path = dir.data_dir.join("stremio-lightning").join("plugins");
        let _ = std::fs::create_dir_all(&path);
        std::fs::write(
            path.join("later.plugin.js"),
            "setTimeout(function () { logger.info('from a timer'); }, 30);",
        )
        .expect("the plugin file");
        let runtime = crate::app::plugins::Runtime::new(dir.data_dir.clone());
        runtime.start("later.plugin.js");
        assert!(runtime.holds_timers(), "the load armed one");

        // The app has nothing to report, so the pump is the only thing that calls in. The line
        // is not there until it does.
        assert!(
            !runtime
                .lines()
                .iter()
                .any(|(_, line)| line.text.contains("from a timer")),
            "the timer has not fired yet"
        );
        let until = Instant::now() + std::time::Duration::from_secs(2);
        while !runtime
            .lines()
            .iter()
            .any(|(_, line)| line.text.contains("from a timer"))
            && Instant::now() < until
        {
            // What `app::events::pump` does on its tick.
            let asked = runtime.advance();
            assert!(!asked.is_empty() || runtime.holds_timers());
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let logged: Vec<String> = runtime
            .lines()
            .into_iter()
            .filter(|(file, _)| file == "later.plugin.js")
            .map(|(_, line)| line.text)
            .collect();
        assert_eq!(logged, vec!["from a timer".to_owned()]);
        assert!(runtime.running("later.plugin.js"));
        assert!(!runtime.holds_timers(), "a timer fires once");
    }

    #[test]
    fn a_panic_under_the_plugin_is_reported_and_later_calls_hear_it_at_once() {
        let (mut running, outcome) =
            Running::start("registerSettings([]);".to_owned(), "crashed", Crash);

        assert!(!running.running(), "the plugin is stopped");
        assert_eq!(outcome.run.said(), super::super::PANICKED);

        // Every later call is answered through the closed channel rather than waiting out the
        // interpreter's timeout, which is what it would do if the thread died mid-call.
        let at = Instant::now();
        let later = running.deliver(Event::PlaybackStopped);
        assert!(later.run.failed());
        assert!(
            at.elapsed() < super::super::ANSWER_WAIT,
            "the later call took {:?}",
            at.elapsed()
        );
        assert_eq!(running.stopped(), later.run.said());
        // And a plugin on a host that does not fail still runs, so one panic did not take the
        // thread machinery with it.
        let (runs, outcome) = Running::start("logger.info('fine');".to_owned(), "fine", folder());
        assert!(runs.running(), "stopped: {}", runs.stopped());
        assert_eq!(outcome.run, Run::Clean);
    }

    /// Item 5 at the registry: a load that failed left its reason in the stopped map, and a
    /// later start that goes through has to clear it. A card that still said why the last run
    /// died, next to a plugin that is running, would be telling the user two things at once.
    #[test]
    fn a_plugin_that_failed_once_and_starts_clean_the_second_time_says_nothing_about_the_first() {
        let dir = folder();
        let path = dir.data_dir.join("stremio-lightning").join("plugins");
        let _ = std::fs::create_dir_all(&path);
        let file = path.join("twice.plugin.js");
        std::fs::write(&file, "throw new Error('not yet');").expect("the first plugin file");
        let runtime = crate::app::plugins::Runtime::new(dir.data_dir.clone());

        runtime.start("twice.plugin.js");
        let first = runtime.state("twice.plugin.js", true);
        assert!(!first.running);
        assert!(
            first.stopped.contains("not yet"),
            "the card says: {:?}",
            first.stopped
        );

        // The user fixes the file and presses the switch again.
        std::fs::write(&file, "logger.info('now it works');").expect("the second plugin file");
        runtime.start("twice.plugin.js");

        let second = runtime.state("twice.plugin.js", true);
        assert!(second.running, "it started: {:?}", second.stopped);
        assert_eq!(second.stopped, "", "the old reason is gone");
        assert_eq!(second.last_error, "");
        let logged: Vec<String> = runtime
            .lines()
            .into_iter()
            .filter(|(name, _)| name == "twice.plugin.js")
            .map(|(_, line)| line.text)
            .collect();
        assert_eq!(logged, vec!["now it works".to_owned()]);
    }
}
