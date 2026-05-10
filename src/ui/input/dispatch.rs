use crate::options::{KeyBinding, ViewerAction};
use eframe::egui;
use std::collections::HashMap;

pub(crate) const MOUSE_PRIMARY_BINDING: &str = "MousePrimary";
pub(crate) const MOUSE_SECONDARY_BINDING: &str = "MouseSecondary";
pub(crate) const MOUSE_MIDDLE_BINDING: &str = "MouseMiddle";
pub(crate) const MOUSE_EXTRA1_BINDING: &str = "MouseExtra1";
pub(crate) const MOUSE_EXTRA2_BINDING: &str = "MouseExtra2";
pub(crate) const MOUSE_WHEEL_UP_BINDING: &str = "MouseWheelUp";
pub(crate) const MOUSE_WHEEL_DOWN_BINDING: &str = "MouseWheelDown";

pub(crate) fn collect_triggered_actions(
    ctx: &egui::Context,
    keymap: &HashMap<KeyBinding, ViewerAction>,
) -> Vec<ViewerAction> {
    keymap
        .iter()
        .filter_map(|(binding, action)| binding_pressed(ctx, binding).then(|| action.clone()))
        .collect()
}

fn binding_pressed(ctx: &egui::Context, binding: &KeyBinding) -> bool {
    ctx.input(|i| {
        if let Some(digit_binding) = digit_binding_from_name(&binding.key) {
            if let Some(pressed) = exact_digit_binding_pressed(i, binding, digit_binding) {
                return pressed;
            }
        }
        if binding_mouse_event_pressed(i, binding) {
            return true;
        }
        if !modifiers_match(i.modifiers, binding) {
            return false;
        }
        match key_name_to_egui(&binding.key) {
            Some(key) => i.key_pressed(key),
            None => false,
        }
    })
}

fn modifiers_match(modifiers: egui::Modifiers, binding: &KeyBinding) -> bool {
    modifiers.shift == binding.shift
        && modifiers.ctrl == binding.ctrl
        && modifiers.alt == binding.alt
}

fn binding_mouse_event_pressed(i: &egui::InputState, binding: &KeyBinding) -> bool {
    i.events.iter().any(|event| match event {
        egui::Event::PointerButton {
            button,
            pressed: true,
            modifiers,
            ..
        } => {
            if !modifiers_match(*modifiers, binding) {
                return false;
            }
            matches_pointer_binding(binding.key.trim(), *button)
        }
        egui::Event::MouseWheel {
            delta, modifiers, ..
        } => {
            if !modifiers_match(*modifiers, binding) {
                return false;
            }
            let key = binding.key.trim();
            (key.eq_ignore_ascii_case(MOUSE_WHEEL_UP_BINDING) && delta.y < 0.0)
                || (key.eq_ignore_ascii_case(MOUSE_WHEEL_DOWN_BINDING) && delta.y > 0.0)
        }
        _ => false,
    })
}

fn matches_pointer_binding(name: &str, button: egui::PointerButton) -> bool {
    match button {
        egui::PointerButton::Primary => name.eq_ignore_ascii_case(MOUSE_PRIMARY_BINDING),
        egui::PointerButton::Secondary => name.eq_ignore_ascii_case(MOUSE_SECONDARY_BINDING),
        egui::PointerButton::Middle => name.eq_ignore_ascii_case(MOUSE_MIDDLE_BINDING),
        egui::PointerButton::Extra1 => name.eq_ignore_ascii_case(MOUSE_EXTRA1_BINDING),
        egui::PointerButton::Extra2 => name.eq_ignore_ascii_case(MOUSE_EXTRA2_BINDING),
    }
}

pub(crate) fn pointer_button_binding_name(button: egui::PointerButton) -> &'static str {
    match button {
        egui::PointerButton::Primary => MOUSE_PRIMARY_BINDING,
        egui::PointerButton::Secondary => MOUSE_SECONDARY_BINDING,
        egui::PointerButton::Middle => MOUSE_MIDDLE_BINDING,
        egui::PointerButton::Extra1 => MOUSE_EXTRA1_BINDING,
        egui::PointerButton::Extra2 => MOUSE_EXTRA2_BINDING,
    }
}

pub(crate) fn is_pointer_binding_name(name: &str) -> bool {
    matches!(
        name,
        MOUSE_PRIMARY_BINDING
            | MOUSE_SECONDARY_BINDING
            | MOUSE_MIDDLE_BINDING
            | MOUSE_EXTRA1_BINDING
            | MOUSE_EXTRA2_BINDING
            | MOUSE_WHEEL_UP_BINDING
            | MOUSE_WHEEL_DOWN_BINDING
    )
}

pub(crate) fn key_event_binding_name(key: egui::Key) -> String {
    digit_event_name_from_states(key, platform_digit_key_states)
        .unwrap_or_else(|| key.name().to_string())
}

pub(crate) fn canonical_key_binding_name(key: &str) -> String {
    match digit_binding_from_name(key) {
        Some(DigitBinding {
            digit,
            source: DigitSource::Numpad,
        }) => format!("Numpad{digit}"),
        Some(DigitBinding {
            digit,
            source: DigitSource::TopRow,
        }) => digit.to_string(),
        None => key.trim().to_string(),
    }
}

fn key_name_to_egui(key: &str) -> Option<egui::Key> {
    egui::Key::from_name(normalize_key_name(key))
}

fn normalize_key_name(key: &str) -> &str {
    match key.trim() {
        "Num0" => "Numpad0",
        "Num1" => "Numpad1",
        "Num2" => "Numpad2",
        "Num3" => "Numpad3",
        "Num4" => "Numpad4",
        "Num5" => "Numpad5",
        "Num6" => "Numpad6",
        "Num7" => "Numpad7",
        "Num8" => "Numpad8",
        "Num9" => "Numpad9",
        value => value,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DigitSource {
    TopRow,
    Numpad,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DigitBinding {
    digit: u8,
    source: DigitSource,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DigitKeyStates {
    top_row_down: bool,
    numpad_down: bool,
}

fn exact_digit_binding_pressed(
    input: &egui::InputState,
    binding: &KeyBinding,
    digit_binding: DigitBinding,
) -> Option<bool> {
    let states = platform_digit_key_states(digit_binding.digit)?;
    Some(input.events.iter().any(|event| match event {
        egui::Event::Key {
            key,
            pressed: true,
            repeat: false,
            modifiers,
            ..
        } => {
            digit_from_egui_key(*key) == Some(digit_binding.digit)
                && modifiers_match(*modifiers, binding)
                && digit_source_down(states, digit_binding.source)
        }
        _ => false,
    }))
}

fn digit_event_name_from_states(
    key: egui::Key,
    states_for_digit: impl Fn(u8) -> Option<DigitKeyStates>,
) -> Option<String> {
    let digit = digit_from_egui_key(key)?;
    let states = states_for_digit(digit)?;
    if states.numpad_down {
        Some(format!("Numpad{digit}"))
    } else if states.top_row_down {
        Some(digit.to_string())
    } else {
        None
    }
}

fn digit_binding_from_name(key: &str) -> Option<DigitBinding> {
    let trimmed = key.trim();
    if let Some(digit) = trimmed.strip_prefix("Numpad").and_then(parse_single_digit) {
        return Some(DigitBinding {
            digit,
            source: DigitSource::Numpad,
        });
    }
    if let Some(digit) = trimmed.strip_prefix("Digit").and_then(parse_single_digit) {
        return Some(DigitBinding {
            digit,
            source: DigitSource::TopRow,
        });
    }
    if let Some(digit) = trimmed.strip_prefix("Num").and_then(parse_single_digit) {
        return Some(DigitBinding {
            digit,
            source: DigitSource::Numpad,
        });
    }
    parse_single_digit(trimmed).map(|digit| DigitBinding {
        digit,
        source: DigitSource::TopRow,
    })
}

fn parse_single_digit(value: &str) -> Option<u8> {
    let mut chars = value.chars();
    let digit = chars.next()?.to_digit(10)?;
    chars.next().is_none().then_some(digit as u8)
}

fn digit_from_egui_key(key: egui::Key) -> Option<u8> {
    match key {
        egui::Key::Num0 => Some(0),
        egui::Key::Num1 => Some(1),
        egui::Key::Num2 => Some(2),
        egui::Key::Num3 => Some(3),
        egui::Key::Num4 => Some(4),
        egui::Key::Num5 => Some(5),
        egui::Key::Num6 => Some(6),
        egui::Key::Num7 => Some(7),
        egui::Key::Num8 => Some(8),
        egui::Key::Num9 => Some(9),
        _ => None,
    }
}

fn digit_source_down(states: DigitKeyStates, source: DigitSource) -> bool {
    match source {
        DigitSource::TopRow => states.top_row_down,
        DigitSource::Numpad => states.numpad_down,
    }
}

#[cfg(windows)]
fn platform_digit_key_states(digit: u8) -> Option<DigitKeyStates> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_0, VK_NUMPAD0};

    (digit <= 9).then(|| unsafe {
        DigitKeyStates {
            top_row_down: GetAsyncKeyState((VK_0 + digit as u16) as i32) < 0,
            numpad_down: GetAsyncKeyState((VK_NUMPAD0 + digit as u16) as i32) < 0,
        }
    })
}

#[cfg(not(windows))]
fn platform_digit_key_states(_digit: u8) -> Option<DigitKeyStates> {
    None
}

#[cfg(test)]
#[path = "../../../tests/support/src/ui/input/dispatch_tests.rs"]
mod tests;
