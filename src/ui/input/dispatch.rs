use crate::options::{KeyBinding, ViewerAction};
use eframe::egui;
use std::collections::HashMap;

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
    match key {
        "Plus" => Some(egui::Key::Plus),
        "Minus" => Some(egui::Key::Minus),
        "Num0" => Some(egui::Key::Num0),
        "Enter" => Some(egui::Key::Enter),
        "R" => Some(egui::Key::R),
        "Space" => Some(egui::Key::Space),
        "ArrowRight" => Some(egui::Key::ArrowRight),
        "ArrowLeft" => Some(egui::Key::ArrowLeft),
        "Home" => Some(egui::Key::Home),
        "End" => Some(egui::Key::End),
        "G" => Some(egui::Key::G),
        "C" => Some(egui::Key::C),
        "V" => Some(egui::Key::V),
        "F" => Some(egui::Key::F),
        "P" => Some(egui::Key::P),
        _ => None,
    }
}
