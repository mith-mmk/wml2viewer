use super::{
    DigitBinding, DigitKeyStates, DigitSource, MOUSE_PRIMARY_BINDING, canonical_key_binding_name,
    digit_binding_from_name, digit_event_name_from_states, key_name_to_egui,
    pointer_button_binding_name,
};
use eframe::egui;

#[test]
fn key_name_to_egui_supports_f5() {
    assert_eq!(key_name_to_egui("F5"), Some(egui::Key::F5));
}

#[test]
fn key_name_to_egui_supports_existing_num_aliases() {
    assert_eq!(key_name_to_egui("Num0"), Some(egui::Key::Num0));
    assert_eq!(key_name_to_egui("Num9"), Some(egui::Key::Num9));
    assert_eq!(key_name_to_egui("Numpad0"), Some(egui::Key::Num0));
    assert_eq!(key_name_to_egui("Digit0"), Some(egui::Key::Num0));
}

#[test]
fn digit_binding_names_distinguish_top_row_and_numpad() {
    assert_eq!(
        digit_binding_from_name("0"),
        Some(DigitBinding {
            digit: 0,
            source: DigitSource::TopRow
        })
    );
    assert_eq!(
        digit_binding_from_name("Digit9"),
        Some(DigitBinding {
            digit: 9,
            source: DigitSource::TopRow
        })
    );
    assert_eq!(
        digit_binding_from_name("Numpad0"),
        Some(DigitBinding {
            digit: 0,
            source: DigitSource::Numpad
        })
    );
    assert_eq!(
        digit_binding_from_name("Num9"),
        Some(DigitBinding {
            digit: 9,
            source: DigitSource::Numpad
        })
    );
}

#[test]
fn canonical_key_binding_names_use_numpad_not_num_aliases() {
    assert_eq!(canonical_key_binding_name("Num0"), "Numpad0");
    assert_eq!(canonical_key_binding_name("Numpad9"), "Numpad9");
    assert_eq!(canonical_key_binding_name("Digit1"), "1");
    assert_eq!(canonical_key_binding_name("2"), "2");
}

#[test]
fn digit_capture_names_prefer_numpad_when_platform_reports_it() {
    let name = digit_event_name_from_states(egui::Key::Num0, |digit| {
        assert_eq!(digit, 0);
        Some(DigitKeyStates {
            top_row_down: false,
            numpad_down: true,
        })
    });

    assert_eq!(name.as_deref(), Some("Numpad0"));
}

#[test]
fn digit_capture_names_top_row_when_platform_reports_it() {
    let name = digit_event_name_from_states(egui::Key::Num9, |digit| {
        assert_eq!(digit, 9);
        Some(DigitKeyStates {
            top_row_down: true,
            numpad_down: false,
        })
    });

    assert_eq!(name.as_deref(), Some("9"));
}

#[test]
fn supports_more_than_101_keyboard_bindings() {
    assert!(
        egui::Key::ALL.len() >= 101,
        "supported key count = {}",
        egui::Key::ALL.len()
    );
}

#[test]
fn pointer_binding_name_primary() {
    assert_eq!(
        pointer_button_binding_name(egui::PointerButton::Primary),
        MOUSE_PRIMARY_BINDING
    );
}
