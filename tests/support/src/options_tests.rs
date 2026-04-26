use super::{
    FileActionOptions, FileActionSlot, InputOptions, KeyBinding, ViewerAction, default_key_mapping,
};
use std::collections::HashMap;

#[test]
fn default_key_mapping_includes_f5_reload() {
    let map = default_key_mapping();

    assert_eq!(map.get(&KeyBinding::new("F5")), Some(&ViewerAction::Reload));
}

#[test]
fn replace_default_keymap_uses_only_custom_bindings() {
    let mut key_mapping = HashMap::new();
    key_mapping.insert(KeyBinding::new("Space"), ViewerAction::PrevImage);
    let options = InputOptions {
        key_mapping,
        replace_default_keymap: true,
    };

    let merged = options.merged_with_defaults();
    assert_eq!(merged.len(), 1);
    assert_eq!(
        merged.get(&KeyBinding::new("Space")),
        Some(&ViewerAction::PrevImage)
    );
    assert!(!merged.contains_key(&KeyBinding::new("ArrowRight")));
}

#[test]
fn viewer_action_from_name_is_case_insensitive() {
    assert_eq!(
        ViewerAction::from_name("togglefiler"),
        Some(ViewerAction::ToggleFiler)
    );
    assert_eq!(
        ViewerAction::from_name("setmovefolder2"),
        Some(ViewerAction::SetMoveFolder2)
    );
    assert!(ViewerAction::from_name("unknown").is_none());
}

#[test]
fn file_action_slots_switch_active_destination() {
    let mut options = FileActionOptions::default();

    options.set_move_folder2();
    options.set_copy_folder2();
    assert_eq!(options.active_move_slot, FileActionSlot::Folder2);
    assert_eq!(options.active_copy_slot, FileActionSlot::Folder2);

    options.set_move_folder1();
    options.set_copy_folder1();
    assert_eq!(options.active_move_slot, FileActionSlot::Folder1);
    assert_eq!(options.active_copy_slot, FileActionSlot::Folder1);
}
