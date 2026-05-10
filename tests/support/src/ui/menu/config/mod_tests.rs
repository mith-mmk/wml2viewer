use super::{
    INPUT_ACTION_FIELD_WIDTH, INPUT_KEY_FIELD_WIDTH, SETTINGS_INPUT_DEFAULT_WIDTH,
    SETTINGS_INPUT_MIN_WIDTH, SETTINGS_MIN_WIDTH, can_encode_with_default_overlay,
    capture_pressed_key_name, centered_cell_rect, duplicate_binding_row_indices,
    input_bindings_table_width, input_settings_content_width, is_reserved_binding,
    keymap_from_rows, overlay_keymap_from_effective, settings_dialog_default_size,
    settings_dialog_min_width,
};
use crate::options::{KeyBinding, ViewerAction};
use crate::ui::viewer::{KeyMappingRowDraft, SettingsTab};
use eframe::egui;
use std::collections::HashMap;

#[test]
fn builds_keymap_from_rows_with_modifiers() {
    let rows = vec![
        KeyMappingRowDraft {
            binding: KeyBinding::new("Space"),
            action: ViewerAction::NextImage,
        },
        KeyMappingRowDraft {
            binding: KeyBinding::new("Space").with_shift(),
            action: ViewerAction::PrevImage,
        },
        KeyMappingRowDraft {
            binding: KeyBinding {
                key: "F".to_string(),
                shift: false,
                ctrl: true,
                alt: false,
            },
            action: ViewerAction::ToggleFiler,
        },
    ];
    let duplicates = duplicate_binding_row_indices(&rows);
    let (parsed, warning) = keymap_from_rows(&rows, &duplicates);
    assert!(warning.is_none());

    assert_eq!(
        parsed.get(&KeyBinding::new("Space")),
        Some(&ViewerAction::NextImage)
    );
    assert_eq!(
        parsed.get(&KeyBinding::new("Space").with_shift()),
        Some(&ViewerAction::PrevImage)
    );
    assert_eq!(
        parsed.get(&KeyBinding {
            key: "F".to_string(),
            shift: false,
            ctrl: true,
            alt: false,
        }),
        Some(&ViewerAction::ToggleFiler)
    );
}

#[test]
fn duplicate_rows_emit_warning_and_last_wins() {
    let rows = vec![
        KeyMappingRowDraft {
            binding: KeyBinding::new("Space"),
            action: ViewerAction::PrevImage,
        },
        KeyMappingRowDraft {
            binding: KeyBinding::new("Space"),
            action: ViewerAction::NextImage,
        },
    ];
    let duplicates = duplicate_binding_row_indices(&rows);
    let (parsed, warning) = keymap_from_rows(&rows, &duplicates);

    assert_eq!(
        parsed.get(&KeyBinding::new("Space")),
        Some(&ViewerAction::NextImage)
    );
    assert!(warning.is_some());
    assert!(duplicates.contains(&0));
    assert!(duplicates.contains(&1));
}

#[test]
fn duplicate_rows_treat_num_aliases_as_numpad() {
    let rows = vec![
        KeyMappingRowDraft {
            binding: KeyBinding::new("Num0").with_shift(),
            action: ViewerAction::ZoomReset,
        },
        KeyMappingRowDraft {
            binding: KeyBinding::new("Numpad0").with_shift(),
            action: ViewerAction::ZoomReset,
        },
    ];
    let duplicates = duplicate_binding_row_indices(&rows);
    let (parsed, warning) = keymap_from_rows(&rows, &duplicates);

    assert!(duplicates.contains(&0));
    assert!(duplicates.contains(&1));
    assert!(warning.is_some());
    assert!(parsed.contains_key(&KeyBinding::new("Numpad0").with_shift()));
    assert!(!parsed.contains_key(&KeyBinding::new("Num0").with_shift()));
}

#[test]
fn capture_pressed_key_name_returns_none_without_events() {
    let ctx = egui::Context::default();
    assert!(capture_pressed_key_name(&ctx).is_none());
}

#[test]
fn input_settings_dialog_starts_wider_than_regular_tabs() {
    let content = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1280.0, 720.0));

    assert_eq!(
        settings_dialog_min_width(content, SettingsTab::Input),
        SETTINGS_INPUT_MIN_WIDTH
    );
    assert_eq!(
        settings_dialog_min_width(content, SettingsTab::Viewer),
        SETTINGS_MIN_WIDTH
    );
    assert_eq!(
        settings_dialog_default_size(content, SettingsTab::Input).x,
        SETTINGS_INPUT_DEFAULT_WIDTH
    );
    assert!(
        settings_dialog_default_size(content, SettingsTab::Input).x
            > settings_dialog_default_size(content, SettingsTab::Viewer).x
    );
    assert!(INPUT_KEY_FIELD_WIDTH >= 180.0);
    assert!(input_bindings_table_width() >= INPUT_ACTION_FIELD_WIDTH + INPUT_KEY_FIELD_WIDTH);
    assert!(input_settings_content_width() >= input_bindings_table_width());
}

#[test]
fn modifier_checkbox_rect_is_centered_in_column_cell() {
    let cell = egui::Rect::from_min_size(egui::pos2(520.0, 64.0), egui::vec2(56.0, 20.0));
    let checkbox = centered_cell_rect(cell, egui::vec2(18.0, 18.0));

    assert_eq!(checkbox.center(), cell.center());
    assert!(checkbox.left() > cell.left());
    assert!(checkbox.right() < cell.right());
}

#[test]
fn reserved_binding_rejects_alt_f4() {
    let binding = KeyBinding {
        key: "F4".to_string(),
        shift: false,
        ctrl: false,
        alt: true,
    };
    assert!(is_reserved_binding(&binding));
}

#[test]
fn keymap_from_rows_ignores_reserved_bindings() {
    let rows = vec![
        KeyMappingRowDraft {
            binding: KeyBinding::new("F1"),
            action: ViewerAction::NextImage,
        },
        KeyMappingRowDraft {
            binding: KeyBinding {
                key: "F4".to_string(),
                shift: false,
                ctrl: false,
                alt: true,
            },
            action: ViewerAction::PrevImage,
        },
        KeyMappingRowDraft {
            binding: KeyBinding::new("Space"),
            action: ViewerAction::NextImage,
        },
    ];
    let duplicates = duplicate_binding_row_indices(&rows);
    let (parsed, _warning) = keymap_from_rows(&rows, &duplicates);

    assert!(!parsed.contains_key(&KeyBinding::new("F1")));
    assert!(!parsed.contains_key(&KeyBinding {
        key: "F4".to_string(),
        shift: false,
        ctrl: false,
        alt: true,
    }));
    assert_eq!(
        parsed.get(&KeyBinding::new("Space")),
        Some(&ViewerAction::NextImage)
    );
}

#[test]
fn overlay_encoding_detects_removed_default_binding() {
    let mut defaults = HashMap::new();
    defaults.insert(KeyBinding::new("Space"), ViewerAction::NextImage);
    defaults.insert(KeyBinding::new("ArrowRight"), ViewerAction::NextImage);

    let mut effective = HashMap::new();
    effective.insert(KeyBinding::new("Space"), ViewerAction::NextImage);

    assert!(!can_encode_with_default_overlay(&effective, &defaults));
}

#[test]
fn overlay_encoding_extracts_only_differences() {
    let mut defaults = HashMap::new();
    defaults.insert(KeyBinding::new("Space"), ViewerAction::NextImage);

    let mut effective = HashMap::new();
    effective.insert(KeyBinding::new("Space"), ViewerAction::PrevImage);
    effective.insert(KeyBinding::new("ArrowRight"), ViewerAction::NextImage);

    assert!(can_encode_with_default_overlay(&effective, &defaults));
    let overlay = overlay_keymap_from_effective(&effective, &defaults);

    assert_eq!(overlay.len(), 2);
    assert_eq!(
        overlay.get(&KeyBinding::new("Space")),
        Some(&ViewerAction::PrevImage)
    );
    assert_eq!(
        overlay.get(&KeyBinding::new("ArrowRight")),
        Some(&ViewerAction::NextImage)
    );
}
