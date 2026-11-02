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
}

impl Running {
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
        let spawned = std::thread::Builder::new()
            .name(format!("plugin {}", super::name_of(plugin)))
            .stack_size(STACK_SIZE)
            .spawn(move || worker_main(&name, source, host, &commands));
        let mut running = Self {
            channel: Channel {
                calls: Some(calls),
                closed: None,
            },
            stopped: String::new(),
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

    pub fn deliver(&mut self, event: super::Event) -> Outcome {
        let (name, value) = event.named();
        let outcome = self
            .channel
            .ask(|reply| Call::Event(name.to_owned(), value, reply));
        self.record(&outcome);
        outcome
    }

    fn record(&mut self, outcome: &Outcome) {
        if outcome.run.failed() {
            self.stopped = outcome.run.said();
        }
    }
}

fn worker_main<H: Host + 'static>(plugin: &str, source: String, host: H, calls: &Receiver<Call>) {
    let mut worker = match Worker::new(plugin, host) {
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
    while let Ok(call) = calls.recv() {
        let (outcome, reply) = match call {
            // The start's own call: the plugin's file runs for the first time.
            Call::Started(reply) => (worker.load(&source), reply),
            Call::SettingsSaved(values, reply) => (worker.settings_saved(values), reply),
            Call::Event(name, value, reply) => (worker.deliver(&name, value), reply),
        };
        let closed = outcome.run.failed();
        if closed {
            let reason = outcome.run.said();
            lock(&worker.host).log(plugin, LogLine::new("error", reason), true);
        }
        let _ = reply.send(outcome);
        if closed {
            return;
        }
    }
}

/// The plugin's thread: the interpreter, and the state that outlives one call.
struct Worker<H: Host> {
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
    fn new(plugin: &str, host: H) -> Result<Self, String> {
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
            context,
        })
    }

    /// Runs the plugin's file for the first time, then every timer that came due while it loaded.
    fn load(&mut self, source: &str) -> Outcome {
        let settled = self.settle(|_, ctx| {
            let _: rquickjs::Value = ctx.eval(source)?;
            Ok(())
        });
        self.finish(settled)
    }

    /// Calls the plugin's `onSettingsSaved` callbacks with the saved values. While they run,
    /// `getSetting` answers from the file's new values.
    fn settings_saved(&mut self, values: Json) -> Outcome {
        let fresh: Map<String, Json> = values.as_object().cloned().unwrap_or_default();
        *lock(&self.world.settings) = Some(fresh);
        let callbacks = std::mem::take(&mut self.on_settings_saved);
        let settled = self.settle(|this, ctx| this.called(ctx, &callbacks, &values));
        *lock(&self.world.settings) = None;
        let stopped = settled.outcome.run.failed();
        let outcome = self.finish(settled);
        if !stopped {
            self.on_settings_saved = callbacks;
        }
        outcome
    }

    /// Calls the plugin's callbacks for one event, with the event's value.
    fn deliver(&mut self, event: &str, value: Json) -> Outcome {
        let callbacks = self.listeners.remove(event).unwrap_or_default();
        let settled = self.settle(|this, ctx| this.called(ctx, &callbacks, &value));
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
        for schema in &requests.schemas {
            lock(&self.host).register_settings(&name, schema);
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
                let mut outcome = settled.outcome;
                outcome.run = Run::Threw(format!(
                    "It left more than {} timers waiting at once, so it was stopped.",
                    super::MAX_TIMERS
                ));
                return outcome;
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
                    outcome.run = Run::Threw(format!(
                        "It left more than {} timers waiting at once, so it was stopped.",
                        super::MAX_TIMERS
                    ));
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

    fn register(&mut self, schema: &Json) -> Result<(), String> {
        lock(&self.host).register_settings(&self.plugin, schema);
        Ok(())
    }
}
