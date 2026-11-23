use super::*;
use crate::screens::mods::catalogue;

#[test]
fn builtin_cards_switches_fields_and_submitter_behave_purely() {
    let mut extras = LocalExtras::default();
    let c = cards(&extras);
    assert_eq!(
        (c.len(), c[0].name.as_str(), c[0].builtin, c[0].has_settings),
        (1, "AniSkip", true, true)
    );
    assert_eq!(
        catalogue::byline(&c[0]),
        format!("v{}", env!("CARGO_PKG_VERSION"))
    );
    assert!(set_enabled(&mut extras, "builtin:aniskip", false) && !extras.skip_intros);
    assert!(!set_enabled(&mut extras, "builtin:aniskip", false));
    assert!(!set_enabled(&mut extras, "unknown", true));

    let f = fields("builtin:aniskip")
        .iter()
        .find(|f| f.key == "submit_button")
        .unwrap();
    set_value(
        &mut extras,
        "builtin:aniskip",
        "submit_button",
        json!("yes"),
    );
    assert_eq!(value(&extras, "builtin:aniskip", f), json!(true));
    set_value(
        &mut extras,
        "builtin:aniskip",
        "submit_button",
        json!(false),
    );
    assert_eq!(value(&extras, "builtin:aniskip", f), json!(false));

    let id = "11111111-2222-4333-8444-555555555555";
    assert!(ensure_submitter(&mut extras, id) && submitter(&extras).as_deref() == Some(id));
    assert!(!ensure_submitter(&mut extras, id));
    set_value(
        &mut extras,
        "builtin:aniskip",
        "submitter_id",
        json!("nope"),
    );
    assert!(submitter(&extras).is_none());

    let t = theme_cards(&extras);
    assert_eq!(
        (t.len(), t[0].id.as_str(), t[0].enabled),
        (1, "builtin:amoled", false)
    );
    extras.enabled_themes = vec!["builtin:amoled".into()];
    assert!(theme_cards(&extras)[0].enabled);
}
