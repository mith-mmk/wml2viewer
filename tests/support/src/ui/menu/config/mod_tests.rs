    use super::{
        can_encode_with_default_overlay, capture_pressed_key_name, duplicate_binding_row_indices,
        is_reserved_binding, keymap_from_rows, overlay_keymap_from_effective,
    };
    use crate::options::{KeyBinding, ViewerAction};
    use crate::ui::viewer::KeyMappingRowDraft;
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
    fn capture_pressed_key_name_returns_none_without_events() {
        let ctx = egui::Context::default();
        assert!(capture_pressed_key_name(&ctx).is_none());
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

