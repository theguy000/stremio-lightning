use super::*;

#[test]
fn classifies_navigation_zoom_and_modal_shortcuts() {
    let none = ModifiersState::empty();
    let ctrl = ModifiersState::CONTROL;
    let prim = if cfg!(target_os = "macos") {
        ModifiersState::SUPER
    } else {
        ctrl
    };
    let ch = |s: &str, m, inp, play| classify(&Key::Character(s.into()), true, false, m, inp, play);
    assert_eq!(
        ch("1", none, false, false),
        Some(GlobalShortcut::SwitchSection(Section::Board))
    );
    assert_eq!(
        ch("6", none, false, false),
        Some(GlobalShortcut::SwitchSection(Section::Settings))
    );
    assert_eq!(
        ch("0", none, false, false),
        Some(GlobalShortcut::FocusSearch)
    );
    assert_eq!(
        ch("/", ctrl, false, false),
        Some(GlobalShortcut::ToggleShortcutsModal)
    );
    assert_eq!(
        ch("g", ctrl, false, false),
        Some(GlobalShortcut::ToggleGamepadModal)
    );
    assert_eq!(ch("+", prim, true, true), Some(GlobalShortcut::ZoomIn));
    assert_eq!(ch("1", none, true, false), None);
    assert_eq!(ch("1", none, false, true), None);
    let esc = Key::Named(NamedKey::Escape);
    assert_eq!(
        classify(&esc, true, false, none, true, false),
        Some(GlobalShortcut::EscapeOverlay)
    );
    assert_eq!(
        classify(
            &Key::Character("+".into()),
            true,
            false,
            prim | ModifiersState::SHIFT,
            false,
            false,
        ),
        Some(GlobalShortcut::ZoomIn),
    );
    assert_eq!(
        classify(&esc, true, false, none, false, true),
        Some(GlobalShortcut::EscapeOverlay),
    );
}
