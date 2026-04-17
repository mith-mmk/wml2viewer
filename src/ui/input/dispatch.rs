use crate::options::{KeyBinding, ViewerAction};
use eframe::egui;
use std::collections::HashMap;

const SUPPORTED_KEY_MAPPINGS: &[(&str, egui::Key)] = &[
    ("Plus", egui::Key::Plus),
    ("Minus", egui::Key::Minus),
    ("Num0", egui::Key::Num0),
    ("Enter", egui::Key::Enter),
    ("F5", egui::Key::F5),
    ("R", egui::Key::R),
    ("Space", egui::Key::Space),
    ("ArrowRight", egui::Key::ArrowRight),
    ("ArrowLeft", egui::Key::ArrowLeft),
    ("Home", egui::Key::Home),
    ("End", egui::Key::End),
    ("G", egui::Key::G),
    ("C", egui::Key::C),
    ("V", egui::Key::V),
    ("F", egui::Key::F),
    ("P", egui::Key::P),
];

pub(crate) fn supported_key_names() -> &'static [&'static str] {
    &[
        "Plus",
        "Minus",
        "Num0",
        "Enter",
        "F5",
        "R",
        "Space",
        "ArrowRight",
        "ArrowLeft",
        "Home",
        "End",
        "G",
        "C",
        "V",
        "F",
        "P",
    ]
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
    SUPPORTED_KEY_MAPPINGS
        .iter()
        .find_map(|(name, mapped)| (*name == key).then_some(*mapped))
}

#[cfg(test)]
mod tests {
    use super::key_name_to_egui;
    use eframe::egui;

    #[test]
    fn key_name_to_egui_supports_f5() {
        assert_eq!(key_name_to_egui("F5"), Some(egui::Key::F5));
    }
}
