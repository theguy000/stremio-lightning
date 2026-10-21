//! What each screen shows, asked of the app's own model.
//!
//! This is the app's main test doorway: build a model from a profile, call a screen's data
//! function, and check the plain result. No window, no display, no network.

use std::sync::{Arc, Once};

use stremio_lightning_native::{app, screens};

/// The app's own runtime, built exactly as `run()` builds it, over a throwaway data folder so
/// the real profile is never touched.
fn fresh() -> (Arc<app::AppRuntime>, screens::settings::LocalExtras) {
    // `AppEnv::init` may only run once per process, so every test in this binary shares it.
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("stremio-lightning-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp folder");
        let tokio = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a tokio runtime");
        stremio_lightning_native_env::AppEnv::init(dir, tokio.handle().clone())
            .expect("an environment");
    });

    let (runtime, _events) = app::build::runtime(Default::default());
    (Arc::new(runtime), Default::default())
}

#[test]
fn a_new_install_shows_every_screen_without_waiting_for_anything() {
    let (runtime, extras) = fresh();
    let model = runtime.model().expect("the model is readable");

    // The board has nothing to show yet, but it says so rather than panicking.
    let (status, rows) = screens::board::data(&model);
    assert!(rows.is_empty(), "no catalogs have loaded yet");
    assert!(status.contains("0 catalogs ready"), "{status}");

    // Discover has its type menu from the core's own defaults, but no cards until it loads.
    let discover = screens::discover::data(&model);
    assert!(discover.cards.is_empty());
    assert!(screens::discover::preview_images(&discover, 0).is_empty());

    // Details has no item, so it has no panel to show and offers no episode picker, because
    // nothing has answered with a title that could have Seasons.
    let details = screens::details::data(&model, None, None);
    assert!(details.preview.is_none());
    assert!(details.streams.is_empty());
    assert!(details.seasons.is_empty());
    assert!(!details.show_picker);
    assert!(details.status.contains("0 streams"), "{}", details.status);

    // Addons knows its two menus before it has loaded anything into them.
    let addons = screens::addons::data(&model);
    assert_eq!(addons.inputs.len(), 2, "the catalog and type menus");
    assert!(addons.cards.is_empty());

    // Settings is fully populated from defaults, which is what the screen opens on.
    let settings = screens::settings::data(&model, &extras);
    assert!(!settings.logged_in);
    assert_eq!(settings.user_email, "Anonymous user");
    assert_eq!(settings.ui_language.title, "English");
    assert!(settings.player_config.hardware_decoding);
}

#[test]
fn a_setting_the_profile_does_not_carry_falls_back_to_what_the_app_defaults_to() {
    let (runtime, mut extras) = fresh();

    // The core's default profile leaves the caching size unset for this shell, so the screen
    // shows what the app itself would use.
    {
        let model = runtime.model().expect("the model is readable");
        let settings = screens::settings::data(&model, &extras);
        assert_eq!(settings.caching.title, "2 GB");
    }

    // Choosing another size is what the shell stores beside the profile, and the screen follows.
    extras
        .overrides
        .insert("caching".to_owned(), serde_json::json!("10737418240"));
    let model = runtime.model().expect("the model is readable");
    let settings = screens::settings::data(&model, &extras);
    assert_eq!(settings.caching.title, "10 GB");
}

#[test]
fn leaving_the_sign_in_screen_while_a_browser_attempt_runs_makes_the_attempt_stop() {
    // The screen's own guard is what the poll task consults, so this is the whole rule.
    let generation = stremio_lightning_native::auth::Generation::default();
    let attempt = generation.begin();
    generation.cancel();
    assert!(!generation.is_current(attempt));
}
