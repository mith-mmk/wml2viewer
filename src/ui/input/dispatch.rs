use crate::options::{KeyBinding, ViewerAction};
use eframe::egui;
use std::collections::HashMap;

const SUPPORTED_KEY_NAMES: &[&str] = &[
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "ArrowUp",
    "Escape",
    "Tab",
    "Backspace",
    "Enter",
    "Space",
    "Insert",
    "Delete",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Copy",
    "Cut",
    "Paste",
    "Colon",
    "Comma",
    "Backslash",
    "Slash",
    "Pipe",
    "Questionmark",
    "Exclamationmark",
    "OpenBracket",
    "CloseBracket",
    "OpenCurlyBracket",
    "CloseCurlyBracket",
    "Backtick",
    "Minus",
    "Period",
    "Plus",
    "Equals",
    "Semicolon",
    "Quote",
    "Num0",
    "Num1",
    "Num2",
    "Num3",
    "Num4",
    "Num5",
    "Num6",
    "Num7",
    "Num8",
    "Num9",
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "F1",
    "F2",
    "F3",
    "F4",
    "F5",
    "F6",
    "F7",
    "F8",
    "F9",
    "F10",
    "F11",
    "F12",
    "F13",
    "F14",
    "F15",
    "F16",
    "F17",
    "F18",
    "F19",
    "F20",
    "F21",
    "F22",
    "F23",
    "F24",
    "F25",
    "F26",
    "F27",
    "F28",
    "F29",
    "F30",
    "F31",
    "F32",
    "F33",
    "F34",
    "F35",
    "BrowserBack",
];

pub(crate) fn supported_key_names() -> &'static [&'static str] {
    SUPPORTED_KEY_NAMES
}

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
        let modifiers = i.modifiers;
        if modifiers.shift != binding.shift
            || modifiers.ctrl != binding.ctrl
            || modifiers.alt != binding.alt
        {
            return false;
        }
        match key_name_to_egui(&binding.key) {
            Some(key) => i.key_pressed(key),
            None => false,
        }
    })
}

fn key_name_to_egui(key: &str) -> Option<egui::Key> {
    egui::Key::from_name(key.trim())
}

#[cfg(test)]
mod tests {
    use super::{key_name_to_egui, supported_key_names};
    use eframe::egui;

    #[test]
    fn key_name_to_egui_supports_f5() {
        assert_eq!(key_name_to_egui("F5"), Some(egui::Key::F5));
    }

    #[test]
    fn supports_more_than_101_keyboard_bindings() {
        assert!(
            supported_key_names().len() >= 101,
            "supported key count = {}",
            supported_key_names().len()
        );
    }
}
