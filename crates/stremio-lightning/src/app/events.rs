//! Pure Slint-free `stremio-core` event pump driving `AppStore`.

use std::sync::Arc;

use futures::StreamExt;
use stremio_core::models::ctx::CtxError;
use stremio_core::runtime::msg::Event;
use stremio_core::runtime::RuntimeEvent;

use super::store::AppStore;
use super::{AppModel, AppNotice};
use crate::plumbing::env::AppEnv;
use crate::plumbing::logs::{Level, Source};
use crate::screens::auth;

fn error_text(error: &CtxError) -> String {
    match error {
        CtxError::API(api) => format!("API {}: {}", api.code, api.message),
        CtxError::Env(env) => format!("Environment: {}", env.message()),
        CtxError::Other(other) => format!("Core: {}", other.message()),
    }
}

pub(crate) async fn pump(
    mut events: futures::channel::mpsc::Receiver<RuntimeEvent<AppEnv, AppModel>>,
    store: Arc<AppStore>,
) {
    while let Some(event) = events.next().await {
        match event {
            RuntimeEvent::CoreEvent(Event::Error { error, source }) => {
                store
                    .logs
                    .record(Source::App, "Core", Level::Error, error_text(&error));
                let message = matches!(*source, Event::UserAuthenticated { .. })
                    .then(|| auth::error_message(&error));
                store.notify(AppNotice::AuthFailed(message));
            }
            RuntimeEvent::CoreEvent(Event::UserAuthenticated { .. }) => {
                store.close_sign_in();
            }
            RuntimeEvent::NewState(fields, ..) => {
                store.on_new_state(&fields);
            }
            _ => {}
        }
    }
}
