use super::*;

#[test]
fn detects_controller_families_and_maps_standard_inputs() {
    assert_eq!(
        detect_family("DualSense Wireless Controller"),
        ControllerFamily::PlayStation
    );
    assert_eq!(
        detect_family("Xbox Wireless Controller"),
        ControllerFamily::Xbox
    );
    assert_eq!(detect_family("Generic USB Pad"), ControllerFamily::Generic);
    assert_eq!(map_button(Button::South), Some(GamepadAction::Confirm));
    assert_eq!(map_button(Button::East), Some(GamepadAction::Back));
    assert_eq!(map_button(Button::West), Some(GamepadAction::ActionX));
    assert_eq!(map_button(Button::North), Some(GamepadAction::Fullscreen));
    assert_eq!(
        map_button(Button::LeftTrigger),
        Some(GamepadAction::PrevTab)
    );
    assert_eq!(
        map_button(Button::RightTrigger),
        Some(GamepadAction::NextTab)
    );
    assert_eq!(map_axis(Axis::LeftStickX, 0.1), None);
    assert_eq!(
        map_axis(Axis::LeftStickX, -0.8),
        Some(GamepadAction::MoveLeft)
    );
    assert_eq!(
        map_axis(Axis::RightStickX, 0.9),
        Some(GamepadAction::SeekForward)
    );
    assert_eq!(
        map_axis(Axis::RightStickY, 0.9),
        Some(GamepadAction::VolumeUp)
    );
}
