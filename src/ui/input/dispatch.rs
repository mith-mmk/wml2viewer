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
        if !modifiers_match(i.modifiers, binding) {
            return false;
        }
        if binding_mouse_event_pressed(i, binding) {
            return true;
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

fn key_name_to_egui(key: &str) -> Option<egui::Key> {
    egui::Key::from_name(key.trim())
}

#[cfg(test)]
mod tests {
    use super::{MOUSE_PRIMARY_BINDING, key_name_to_egui, pointer_button_binding_name};
    use eframe::egui;

    #[test]
    fn key_name_to_egui_supports_f5() {
        assert_eq!(key_name_to_egui("F5"), Some(egui::Key::F5));
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
}
