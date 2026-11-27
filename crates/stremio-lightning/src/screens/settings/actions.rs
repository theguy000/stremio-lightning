//! Pure settings mutation and dispatch helpers (zero Slint or I/O).

use serde_json::{json, Value};
use stremio_core::runtime::msg::{Action, ActionCtx, ActionLoad, ActionStreamingServer};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::profile::Settings as ProfileSettings;

use super::options;
use super::tables::TOGGLES;
use crate::app::AppModelField;
use crate::plumbing::local_settings::LocalExtras;
use crate::AppRuntime;

fn update_profile_and_extras(
    current: &ProfileSettings,
    extras: &mut LocalExtras,
    key: &str,
    new_val: Value,
) -> Option<ProfileSettings> {
    extras.overrides.insert(key.to_owned(), new_val.clone());
    let mut val = serde_json::to_value(current).ok()?;
    let obj = val.as_object_mut()?;
    if obj.contains_key(key) {
        obj.insert(key.to_owned(), new_val);
        serde_json::from_value(val).ok()
    } else {
        None
    }
}

pub(crate) fn apply_select_option(
    current: &ProfileSettings,
    extras: &mut LocalExtras,
    key: &str,
    index: usize,
) -> Option<ProfileSettings> {
    let new_val = options::value_at(key, index)?;
    update_profile_and_extras(current, extras, key, new_val)
}

pub(crate) fn apply_toggle_option(
    current: &ProfileSettings,
    extras: &mut LocalExtras,
    key: &str,
    enabled: bool,
) -> Option<ProfileSettings> {
    if !TOGGLES.contains(&key) {
        return None;
    }
    update_profile_and_extras(current, extras, key, json!(enabled))
}

pub(crate) fn dispatch_settings(runtime: &AppRuntime, settings: ProfileSettings) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::UpdateSettings(settings)),
    });
}

pub(crate) fn logout(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::Logout),
    });
}

pub(crate) fn logout_trakt(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::LogoutTrakt),
    });
}

pub(crate) fn start_data_export(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: Some(AppModelField::DataExport),
        action: Action::Load(ActionLoad::DataExport),
    });
}

pub(crate) fn reload_server(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::StreamingServer(ActionStreamingServer::Reload),
    });
}

pub(crate) fn server_status(state: &crate::plumbing::stream_server::State) -> String {
    use crate::plumbing::stream_server::State;
    match state {
        State::NotStarted(error) => format!("Stream server: not started ({error})"),
        State::Ready => "Stream server: ready".to_owned(),
        State::PortTaken => format!(
            "Stream server: using the one already running on {}",
            crate::plumbing::stream_server::ADDRESS
        ),
        State::NotAnswering => "Stream server: started but not answering".to_owned(),
    }
}
