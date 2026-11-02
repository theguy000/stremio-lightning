//! The globals a plugin sees, and the one place where the JSON a plugin speaks and the JSON the
//! app speaks are turned into each other.
//!
//! `World` is shared by every plugin function of one interpreter: the record of what the plugin
//! asked for, the clock of the evaluation it is inside, and whether the app still wants it at all.
//! Most functions never touch the app. They write requests, and the thread that owns the
//! interpreter replays the requests on the host once the evaluation is over. Only the three
//! settings functions call the host directly, and what they reach is the plugin's own settings
//! file, which cannot touch the window.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rquickjs::function::{IntoArgs, Rest};
use rquickjs::{Ctx, Function, Object, Value};
use serde_json::{Map, Value as Json};

/// The most log lines one evaluation may produce. The interpreter stops the plugin at it.
pub(super) const MAX_LOG_LINES: usize = 200;
/// The most timers one evaluation may arm.
const MAX_TIMERS_PER_STEP: usize = 64;
/// The longest a plugin may ask a timer to wait.
const LONGEST_WAIT_MS: f64 = 3_600_000.0;
/// How deep a value handed to the app may nest before the rest of it is dropped. Reading takes a
/// real stack frame per level, and the stack is shared with everything the interpreter is doing.
const MAX_DEPTH: usize = 64;

/// A function a plugin handed to the app to call later.
pub(super) type Held = std::rc::Rc<rquickjs::Persistent<Function<'static>>>;

/// What a plugin's code asked the app to do during one evaluation.
#[derive(Default, Debug)]
pub(super) struct Requests {
    pub(super) saved: Vec<(String, Json)>,
    pub(super) theme: Option<Vec<Json>>,
    pub(super) logs: Vec<(String, String)>,
    /// Timers armed in this evaluation: id, how long from now, the period of an interval, and
    /// what to call.
    pub(super) timers: Vec<(u32, Duration, Option<Duration>, Held)>,
    /// Timers cleared in this evaluation.
    pub(super) cleared: Vec<u32>,
    /// Callbacks `onSettingsSaved` registered in this evaluation.
    pub(super) saved_callbacks: Vec<Held>,
    /// Callbacks `on(event, callback)` registered in this evaluation.
    pub(super) listeners: Vec<(String, Held)>,
}

/// One plugin's shared state: what it asked for, its clock, and whether it is still wanted.
pub(super) struct World {
    pub(super) requests: Mutex<Requests>,
    deadline: Mutex<Option<Instant>>,
    /// Whether the plugin's time in this evaluation is up. A plugin function that reaches the
    /// deadline sets it; the thread that owns the interpreter reads it after the evaluation, and
    /// the interrupt handler in the runtime does when a plugin runs a loop.
    over: AtomicBool,
    /// Whether the plugin is still wanted. It is the one part of this state the runtime's
    /// interrupt handler can reach: that handler is a closure QuickJS calls while the thread is
    /// already inside the interpreter, so it cannot borrow anything the interpreter thread holds.
    alive: Arc<AtomicBool>,
    /// `getSetting` answers from here while the interpreter is inside the plugin's own
    /// `onSettingsSaved`: the values the page just saved, not the values the plugin started with.
    pub(super) settings: Mutex<Option<Map<String, Json>>>,
    next_timer: AtomicU32,
}

// The state holds no JavaScript value, so it can live for as long as the interpreter does.
// SAFETY: `World` has no `'js` lifetime and no reference into the interpreter.
unsafe impl rquickjs::JsLifetime<'_> for World {
    type Changed<'to> = World;
}

/// The state as the interpreter's own functions reach it.
///
/// A plugin function can only run while the thread that owns the interpreter is inside it, and
/// that thread owns the state, so a plain pointer is enough. It is wrapped because the context's
/// userdata has to name a static type, and it lives on a stack frame of the owning thread for the
/// length of a call: a plugin cannot run outside one.
pub(super) struct Static(Shared);

// SAFETY: `Static` holds no `'js` lifetime and no reference into the interpreter.
unsafe impl<'js> rquickjs::JsLifetime<'js> for Static {
    type Changed<'to> = Static;
}

impl World {
    pub(super) fn new() -> Shared {
        std::rc::Rc::new(Self {
            requests: Mutex::default(),
            deadline: Mutex::default(),
            over: AtomicBool::default(),
            alive: Arc::new(AtomicBool::new(true)),
            settings: Mutex::default(),
            next_timer: AtomicU32::new(1),
        })
    }

    /// Starts an evaluation with `budget` to spend, and a request list of its own.
    pub(super) fn begin(&self, budget: Duration) {
        *lock(&self.deadline) = Instant::now().checked_add(budget);
        self.over.store(false, Ordering::SeqCst);
        *lock(&self.requests) = Requests::default();
    }

    /// Ends an evaluation: no deadline, and what the functions set is not carried forward.
    pub(super) fn end(&self) {
        *lock(&self.deadline) = None;
        self.over.store(false, Ordering::SeqCst);
    }

    /// Whether this evaluation has used its time, or the app no longer wants the plugin.
    pub(super) fn is_over(&self) -> bool {
        if self.over.load(Ordering::SeqCst) || !self.alive.load(Ordering::SeqCst) {
            return true;
        }
        let deadline = *lock(&self.deadline);
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            self.over.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    /// Everything the evaluation asked for.
    pub(super) fn take(&self) -> Requests {
        std::mem::take(&mut *lock(&self.requests))
    }

    /// How an evaluation ended, from what the interpreter returned and whether the clock ran out.
    /// The context is here for the one thing only it holds: the exception the code threw, which
    /// is gone once the evaluation ends.
    pub(super) fn verdict<R>(
        &self,
        ctx: &rquickjs::Ctx<'_>,
        result: rquickjs::Result<R>,
    ) -> super::Run {
        if self.is_over() {
            return super::Run::RanTooLong;
        }
        match result {
            Ok(_) => super::Run::Clean,
            Err(error) => {
                let exception = matches!(error, rquickjs::Error::Exception)
                    .then(|| {
                        ctx.catch()
                            .into_exception()
                            .map(|thrown| match thrown.message() {
                                Some(message) => message,
                                None => thrown.to_string(),
                            })
                    })
                    .flatten();
                super::Run::Threw(super::describe(error, exception))
            }
        }
    }

    fn log(&self, level: &str, text: String) -> Result<(), Stop> {
        if self.is_over() {
            return Err(Stop::Budget);
        }
        let mut requests = lock(&self.requests);
        if requests.logs.len() >= MAX_LOG_LINES {
            self.over.store(true, Ordering::SeqCst);
            return Err(Stop::Logs);
        }
        requests.logs.push((level.to_owned(), text));
        Ok(())
    }

    /// Arms a timer. Returns zero for code this app cannot call, which is a timer that never fires.
    fn arm<'js>(
        &self,
        ctx: &Ctx<'js>,
        code: &Value<'js>,
        after: f64,
        period: Option<f64>,
    ) -> Result<u32, Stop> {
        if self.is_over() {
            return Err(Stop::Budget);
        }
        let Some(function) = held(ctx, code) else {
            return Ok(0);
        };
        let mut requests = lock(&self.requests);
        if requests.timers.len() >= MAX_TIMERS_PER_STEP {
            return Err(Stop::Timers);
        }
        let id = self.next_timer.fetch_add(1, Ordering::SeqCst);
        requests
            .timers
            .push((id, millis(after), period.map(millis), function));
        Ok(id)
    }

    fn clear(&self, id: u32) {
        let mut requests = lock(&self.requests);
        requests.cleared.push(id);
        requests.timers.retain(|(other, ..)| *other != id);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic inside the plugin cannot leave half a request here: every field is replaced whole,
    // so the recoverable lock is the right one.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How an evaluation ended, as a plugin function reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stop {
    /// The evaluation returned.
    Finished,
    /// The plugin used its whole budget for this evaluation.
    Budget,
    /// The plugin logged more lines than one evaluation may.
    Logs,
    /// The plugin armed more timers in one evaluation than it may.
    Timers,
}

impl Stop {
    /// The text a stop is thrown with. A plugin that catches it has still been told; one that
    /// does not is reported to the screen with a sentence built from this.
    fn message(self) -> String {
        match self {
            Self::Finished => String::new(),
            Self::Budget => format!(
                "Stopped: a plugin step runs for {} milliseconds. Work that needs longer belongs in setTimeout.",
                super::BUDGET.as_millis()
            ),
            Self::Logs => {
                format!("Stopped: a plugin step may log at most {MAX_LOG_LINES} lines.")
            }
            Self::Timers => {
                format!(
                    "Stopped: a plugin step may arm at most {MAX_TIMERS_PER_STEP} timers."
                )
            }
        }
    }
}

/// The state a plugin function reaches through its context. One thread owns the interpreter and
/// every copy of this, so a plain counted reference is enough.
pub(super) type Shared = std::rc::Rc<World>;

/// The state as the context's userdata holds it, which has to name a static type.
pub(super) fn stash(world: &Shared) -> Static {
    Static(Shared::clone(world))
}

/// How a plugin reaches its settings.
pub(super) trait Settings {
    fn get(&mut self, key: &str) -> Json;
    fn save(&mut self, key: &str, value: &Json) -> Result<(), String>;
    fn register(&mut self, schema: &Json) -> Result<(), String>;
}

fn world<'js>(ctx: &Ctx<'js>) -> Call<Shared> {
    // The thread that owns the interpreter puts the state in once, before any plugin code runs,
    // and leaves it there. A plugin that cannot see it is a bug in this file.
    ctx.userdata::<Static>()
        .map(|world| Shared::clone(&world.0))
        .ok_or_else(|| stopped(Stop::Finished))
}

thread_local! {
    /// The settings the plugin's own functions reach while the interpreter is inside one call.
    /// The thread owns the interpreter and only enters it for a call, so this is the current
    /// call's settings. It is empty outside a call, and a settings function that finds it empty
    /// fails rather than answering a made-up value.
    static SESSION: std::cell::RefCell<Option<SettingsHandle>> = const { std::cell::RefCell::new(None) };
}

/// One call's settings, as the thread that owns the interpreter handed them over.
pub(super) struct SettingsHandle(Mutex<Arc<dyn Settings>>);

impl SettingsHandle {
    pub(super) fn new(settings: impl Settings + 'static) -> Self {
        Self(Mutex::new(Arc::new(settings)))
    }

    /// The settings of one call. The handle is only in the thread's slot while a call runs, so
    /// the lock is never held across anything the plugin does.
    fn with<R>(&self, f: impl FnOnce(&mut dyn Settings) -> R) -> R {
        let mut settings = lock(&self.0);
        f(&mut *Arc::get_mut(&mut settings).expect("the session is the only holder"))
    }
}

/// Runs `f` with `settings` reachable from the plugin's globals, then puts back whatever was
/// reachable before.
pub(super) fn inside_call<R>(settings: SettingsHandle, f: impl FnOnce() -> R) -> R {
    let before = SESSION.with(|cell| cell.borrow_mut().replace(settings));
    let result = f();
    SESSION.with(|cell| *cell.borrow_mut() = before);
    result
}

/// The settings one call reaches, taken out of the thread while the call uses them.
fn take_session() -> Option<SettingsHandle> {
    SESSION.with(|cell| cell.borrow_mut().take())
}

/// Puts a call's settings back, for the thread's next call.
fn put_session(settings: Option<SettingsHandle>) {
    SESSION.with(|cell| *cell.borrow_mut() = settings);
}

/// The settings reachable from inside one plugin call, and the state that shares the call. The
/// settings are taken out of the thread for the length of the call and put back after it.
fn in_call<R>(ctx: &Ctx<'_>, f: impl FnOnce(&World, &mut dyn Settings) -> Call<R>) -> Call<R> {
    let world = world(ctx)?;
    let Some(session) = take_session() else {
        return Err(stopped(Stop::Finished));
    };
    let result = session.with(|settings| guarded(&world, || f(&world, settings)));
    put_session(Some(session));
    result
}

/// Marks a function that asked the app to stop: the stop's own sentence is what the plugin reads.
fn stopped(stop: Stop) -> (String, Stop) {
    (stop.message(), stop)
}

/// What the state says about one evaluation: the value it produced, or the sentence it throws.
type Call<T> = Result<T, (String, Stop)>;

/// Runs `f` unless the plugin must stop first, then checks the clock again after it, so a function
/// that took the whole budget is reported as the budget rather than as the value it returned.
fn guarded<T>(world: &World, f: impl FnOnce() -> Call<T>) -> Call<T> {
    if world.is_over() {
        return Err(stopped(Stop::Budget));
    }
    let value = f()?;
    if world.is_over() {
        return Err(stopped(Stop::Budget));
    }
    Ok(value)
}

/// A settings file that would not take the change, as the sentence the plugin reads.
fn failed(error: String) -> (String, Stop) {
    (error, Stop::Finished)
}

/// Hands a value back to JavaScript without asking the app for anything.
fn plain<T>(value: T) -> Call<T> {
    Ok(value)
}

/// The error a plugin sees for a stop: an `Error` thrown with the sentence, which ends the
/// evaluation unless the plugin catches it.
fn thrown<'js>(ctx: Ctx<'js>, (message, _): (String, Stop)) -> rquickjs::Error {
    // An `Error` rather than a bare string: a plugin that catches it reads `error.message`, which
    // is what a plugin author expects, and what the screen shows when it is not caught.
    match rquickjs::Exception::from_message(ctx.clone(), &message) {
        Ok(error) => ctx.throw(error.into_value()),
        Err(error) => error,
    }
}

/// A JSON value as JavaScript sees it.
pub(super) fn to_js<'js>(ctx: &Ctx<'js>, value: &Json) -> rquickjs::Result<Value<'js>> {
    Ok(match value {
        Json::Null => Value::new_null(ctx.clone()),
        Json::Bool(flag) => Value::new_bool(ctx.clone(), *flag),
        Json::Number(number) => {
            if let Some(whole) = number.as_i64().and_then(|whole| i32::try_from(whole).ok()) {
                Value::new_int(ctx.clone(), whole)
            } else {
                Value::new_float(ctx.clone(), number.as_f64().unwrap_or_default())
            }
        }
        Json::String(text) => rquickjs::String::from_str(ctx.clone(), text)?.into_value(),
        Json::Array(items) => {
            let array = rquickjs::Array::new(ctx.clone())?;
            for (index, item) in items.iter().enumerate() {
                array.set(index, to_js(ctx, item)?)?;
            }
            array.into_value()
        }
        Json::Object(object) => {
            let result = Object::new(ctx.clone())?;
            for (key, item) in object {
                result.set(key.as_str(), to_js(ctx, item)?)?;
            }
            result.into_value()
        }
    })
}

/// A JavaScript value as JSON sees it. Strings stay strings, lists and objects are walked into
/// plain values, and anything JSON has no shape for (a function, a symbol, a value that refers to
/// itself) becomes nothing.
pub(super) fn from_js(value: &Value<'_>) -> Json {
    walk(value, 0)
}

/// `from_js` at one level of nesting. Past [`MAX_DEPTH`] a container becomes `null`: the value is
/// still stored, it just stops at that level rather than reading a deeper stack than the plugin
/// was given.
fn walk(value: &Value<'_>, depth: usize) -> Json {
    if let Some(text) = value.as_string().and_then(|text| text.to_string().ok()) {
        return Json::String(text);
    }
    if let Some(flag) = value.as_bool() {
        return Json::Bool(flag);
    }
    if let Some(number) = value.as_int() {
        return Json::from(number);
    }
    if let Some(number) = value.as_float() {
        return serde_json::Number::from_f64(number).map_or(Json::Null, Json::Number);
    }
    if value.is_undefined() || value.is_null() || value.is_function() || value.is_symbol() {
        return Json::Null;
    }
    let Some(object) = value.as_object() else {
        return Json::Null;
    };
    if depth >= MAX_DEPTH {
        return Json::Null;
    }
    let inner = depth + 1;
    if let Some(array) = object.as_array() {
        let items = (0..array.len())
            .filter_map(|index| array.get::<Value>(index).ok())
            .map(|item| walk(&item, inner))
            .collect();
        return Json::Array(items);
    }
    let mut map = Map::new();
    for entry in object.props::<String, Value>() {
        let Ok((key, item)) = entry else { continue };
        if item.is_function() || item.is_symbol() || item.is_undefined() {
            continue;
        }
        map.insert(key, walk(&item, inner));
    }
    Json::Object(map)
}

/// A function a plugin handed over, saved so it can be called after the evaluation that handed it
/// over has ended. `None` for a value that is not a function.
fn held<'js>(ctx: &Ctx<'js>, value: &Value<'js>) -> Option<Held> {
    let function = value.as_function()?;
    Some(std::rc::Rc::new(rquickjs::Persistent::save(
        ctx,
        function.clone(),
    )))
}

/// Calls a function the plugin handed over earlier. The function is restored from the same
/// interpreter it was saved from, which is the one running this call.
pub(super) fn handed<'js>(
    ctx: &Ctx<'js>,
    callback: &Held,
    args: impl IntoArgs<'js>,
) -> rquickjs::Result<()> {
    match rquickjs::Persistent::clone(&**callback).restore(ctx) {
        Ok(function) => function.call::<_, Value>(args).map(|_| ()),
        Err(error) => Err(error),
    }
}

/// Every argument of a log call, one after another, the way `console.log` shows them.
fn logged(args: &[Value<'_>]) -> String {
    args.iter()
        .map(
            |value| match value.as_string().and_then(|text| text.to_string().ok()) {
                Some(text) => text,
                None => from_js(value).to_string(),
            },
        )
        .collect::<Vec<_>>()
        .join(" ")
}

/// The name a plugin gave as a key: the string itself, or the text of whatever else it was.
fn name_of(value: &Value<'_>) -> String {
    match value.as_string().and_then(|text| text.to_string().ok()) {
        Some(text) => text,
        None => from_js(value).to_string(),
    }
}

/// The milliseconds a timer was given. A value that is not a positive number waits no time at all,
/// as a browser's timer does, and nothing waits longer than an hour.
fn millis(value: f64) -> Duration {
    let value = if value.is_finite() && value > 0.0 {
        value.min(LONGEST_WAIT_MS)
    } else {
        0.0
    };
    Duration::from_millis(value.round() as u64)
}

/// Defines every global a plugin may use. Nothing beyond what JavaScript itself has is defined:
/// no `fetch`, no `XMLHttpRequest`, no `require`, no `window`.
///
/// Each global is a named function with the call's lifetime written out. A closure passed straight
/// to `Function::new` cannot say that its `Ctx` and its `Value` are the same lifetime as the return
/// type, so it fails to compile; naming them is the whole reason these are separate functions.
pub(super) fn install(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    let globals = ctx.globals();

    // console.log and its company: one line each, every argument shown.
    let console = Object::new(ctx.clone())?;
    for (name, level) in [
        ("log", "info"),
        ("info", "info"),
        ("debug", "debug"),
        ("warn", "warn"),
        ("error", "error"),
    ] {
        console.set(name, logging_function(ctx, level)?.with_name(name)?)?;
    }
    globals.set("console", console)?;

    // logger: the same lines, with one text.
    let logger = Object::new(ctx.clone())?;
    for (name, level) in [
        ("debug", "debug"),
        ("info", "info"),
        ("warn", "warn"),
        ("error", "error"),
    ] {
        logger.set(name, logging_function(ctx, level)?.with_name(name)?)?;
    }
    globals.set("logger", logger)?;

    // The three settings functions, which reach the plugin's own settings file.
    globals.set(
        "getSetting",
        Function::new(ctx.clone(), getting)?.with_name("getSetting")?,
    )?;
    globals.set(
        "saveSetting",
        Function::new(ctx.clone(), saving)?.with_name("saveSetting")?,
    )?;
    globals.set(
        "registerSettings",
        Function::new(ctx.clone(), registering)?.with_name("registerSettings")?,
    )?;

    // The three functions that hand the thread something to call later.
    globals.set(
        "onSettingsSaved",
        Function::new(ctx.clone(), keeping_saved)?.with_name("onSettingsSaved")?,
    )?;
    globals.set(
        "on",
        Function::new(ctx.clone(), keeping_event)?.with_name("on")?,
    )?;
    globals.set(
        "applyTheme",
        Function::new(ctx.clone(), theming)?.with_name("applyTheme")?,
    )?;

    // setTimeout and setInterval: the thread fires them while the plugin is loaded.
    globals.set(
        "setTimeout",
        Function::new(ctx.clone(), armed_once)?.with_name("setTimeout")?,
    )?;
    globals.set(
        "setInterval",
        Function::new(ctx.clone(), armed_again)?.with_name("setInterval")?,
    )?;

    // clearTimeout and clearInterval: a timer that has not fired does not fire.
    for name in ["clearTimeout", "clearInterval"] {
        globals.set(name, Function::new(ctx.clone(), clearing)?.with_name(name)?)?;
    }
    Ok(())
}

/// The number a plugin passed, whether it came as a whole number or a decimal.
fn number(value: &Value<'_>) -> Option<f64> {
    value.as_float().or_else(|| value.as_int().map(f64::from))
}

/// `getSetting(key)`: the value the settings file holds for the key, or `null`.
fn getting<'js>(ctx: Ctx<'js>, key: Value<'js>) -> rquickjs::Result<Value<'js>> {
    let key = name_of(&key);
    in_call(&ctx, |_, session| {
        let value = session.get(&key);
        to_js(&ctx, &value).map_err(|error| failed(error.to_string()))
    })
    .map_err(|trouble| thrown(ctx, trouble))
}

/// `saveSetting(key, value)`: the value goes to the plugin's own settings file.
fn saving<'js>(ctx: Ctx<'js>, key: Value<'js>, value: Value<'js>) -> rquickjs::Result<Value<'js>> {
    let key = name_of(&key);
    let value = from_js(&value);
    in_call(&ctx, |_, session| {
        session.save(&key, &value).map_err(failed)?;
        plain(Value::new_undefined(ctx.clone()))
    })
    .map_err(|trouble| thrown(ctx, trouble))
}

/// `registerSettings(schema)`: the schema the Plugin settings page draws.
fn registering<'js>(ctx: Ctx<'js>, schema: Value<'js>) -> rquickjs::Result<Value<'js>> {
    let schema = from_js(&schema);
    in_call(&ctx, |_, session| {
        session.register(&schema).map_err(failed)?;
        plain(Value::new_undefined(ctx.clone()))
    })
    .map_err(|trouble| thrown(ctx, trouble))
}

/// `onSettingsSaved(callback)`: the thread keeps the callback and calls it after the page saves.
fn keeping_saved<'js>(ctx: Ctx<'js>, callback: Value<'js>) -> rquickjs::Result<Value<'js>> {
    match world(&ctx) {
        Ok(this) => guarded(&this, || {
            if let Some(function) = held(&ctx, &callback) {
                lock(&this.requests).saved_callbacks.push(function);
            }
            plain(Value::new_undefined(ctx.clone()))
        }),
        Err(trouble) => Err(trouble),
    }
    .map_err(|trouble| thrown(ctx, trouble))
}

/// `on(event, callback)`: the thread keeps the callback under the event's name, and hands it to
/// the plugin when the app says the event happened.
fn keeping_event<'js>(
    ctx: Ctx<'js>,
    event: Value<'js>,
    callback: Value<'js>,
) -> rquickjs::Result<Value<'js>> {
    let event = name_of(&event);
    match world(&ctx) {
        Ok(this) => guarded(&this, || {
            if let Some(function) = held(&ctx, &callback) {
                lock(&this.requests).listeners.push((event, function));
            }
            plain(Value::new_undefined(ctx.clone()))
        }),
        Err(trouble) => Err(trouble),
    }
    .map_err(|trouble| thrown(ctx, trouble))
}

/// `applyTheme(rules)`: the rules a plugin wants the window drawn with, told to the app after the
/// evaluation, the same way a theme file's are.
fn theming<'js>(ctx: Ctx<'js>, rules: Value<'js>) -> rquickjs::Result<Value<'js>> {
    match world(&ctx) {
        Ok(this) => guarded(&this, || {
            if let Json::Array(rules) = from_js(&rules) {
                lock(&this.requests).theme = Some(rules);
            }
            plain(Value::new_undefined(ctx.clone()))
        }),
        Err(trouble) => Err(trouble),
    }
    .map_err(|trouble| thrown(ctx, trouble))
}

/// `setTimeout(code, after)`: the thread calls `code` once, after `after` milliseconds.
fn armed_once<'js>(
    ctx: Ctx<'js>,
    code: Value<'js>,
    after: Value<'js>,
) -> rquickjs::Result<Value<'js>> {
    arming(ctx, code, after, None)
}

/// `setInterval(code, after)`: the thread calls `code` every `after` milliseconds.
fn armed_again<'js>(
    ctx: Ctx<'js>,
    code: Value<'js>,
    after: Value<'js>,
) -> rquickjs::Result<Value<'js>> {
    arming(ctx, code, after, Some(()))
}

fn arming<'js>(
    ctx: Ctx<'js>,
    code: Value<'js>,
    after: Value<'js>,
    repeat: Option<()>,
) -> rquickjs::Result<Value<'js>> {
    let after = number(&after).unwrap_or(0.0);
    match world(&ctx) {
        Ok(this) => guarded(&this, || {
            let period = repeat.map(|()| after);
            let id = this.arm(&ctx, &code, after, period).map_err(stopped)?;
            plain(Value::new_int(ctx.clone(), id as i32))
        }),
        Err(trouble) => Err(trouble),
    }
    .map_err(|trouble| thrown(ctx, trouble))
}

/// `clearTimeout(id)` and `clearInterval(id)`: a timer that has not fired does not fire.
fn clearing<'js>(ctx: Ctx<'js>, id: Value<'js>) -> rquickjs::Result<Value<'js>> {
    let Some(id) = number(&id) else {
        return Ok(Value::new_undefined(ctx));
    };
    match world(&ctx) {
        Ok(this) => guarded(&this, || {
            this.clear(id as u32);
            plain(Value::new_undefined(ctx.clone()))
        }),
        Err(trouble) => Err(trouble),
    }
    .map_err(|trouble| thrown(ctx, trouble))
}

/// One line from `console.log` and its company: the arguments joined, then recorded.
fn logging<'js>(ctx: Ctx<'js>, level: &str, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
    let text = logged(args);
    match world(&ctx) {
        Ok(this) => guarded(&this, || {
            this.log(level, text).map_err(stopped)?;
            plain(Value::new_undefined(ctx.clone()))
        }),
        Err(trouble) => Err(trouble),
    }
    .map_err(|trouble| thrown(ctx, trouble))
}

/// A `console` or `logger` method. Each level is its own named function, for the same reason as
/// the globals: the lifetime of the arguments and of the answer is the call's.
fn logging_function<'js>(ctx: &Ctx<'js>, level: &str) -> rquickjs::Result<Function<'js>> {
    Ok(match level {
        "debug" => Function::new(ctx.clone(), log_debug)?,
        "warn" => Function::new(ctx.clone(), log_warn)?,
        "error" => Function::new(ctx.clone(), log_error)?,
        _ => Function::new(ctx.clone(), log_info)?,
    })
}

fn log_debug<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<Value<'js>> {
    logging(ctx, "debug", &args.0)
}

fn log_info<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<Value<'js>> {
    logging(ctx, "info", &args.0)
}

fn log_warn<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<Value<'js>> {
    logging(ctx, "warn", &args.0)
}

fn log_error<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<Value<'js>> {
    logging(ctx, "error", &args.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an object nested `levels` deep inside the interpreter and walks it there, the way a
    /// plugin passing it to `saveSetting` does. Returns the walk and how many levels it kept.
    fn walk_nested(levels: usize) -> (Json, usize) {
        let runtime = rquickjs::Runtime::new().expect("an interpreter");
        let context = rquickjs::Context::full(&runtime).expect("a context");
        let walked = context.with(|ctx| {
            let start = rquickjs::Object::new(ctx.clone()).expect("an object");
            let mut here = start.clone();
            for _ in 0..levels {
                let next = rquickjs::Object::new(ctx.clone()).expect("a level");
                here.set("down", next).expect("a child");
                here = here.get::<_, rquickjs::Object>("down").expect("the child");
            }
            here.set("bottom", "reached").expect("the bottom");

            let mut json = from_js(&start.into_value());
            let mut depth = 0;
            while json.is_object() {
                depth += 1;
                match json.get("down") {
                    Some(next) => json = next.clone(),
                    None => break,
                }
            }
            (json, depth)
        });
        walked
    }

    #[test]
    fn a_value_nested_past_the_depth_limit_is_read_up_to_the_limit() {
        // Twice the limit: the walk has to stop by itself, and the level it stops at is dropped.
        let (deepest, depth) = walk_nested(MAX_DEPTH * 2);
        assert_eq!(depth, MAX_DEPTH, "the walk stopped at the limit");
        assert_eq!(deepest, Json::Null, "the level past the limit is not read");

        // A value inside the limit is read whole, down to its last string. The bottom object is
        // one level of its own.
        let (shallow, depth) = walk_nested(MAX_DEPTH - 1);
        assert_eq!(depth, MAX_DEPTH);
        assert_eq!(shallow["bottom"], Json::from("reached"));
    }
}
