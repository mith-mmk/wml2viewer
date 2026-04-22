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

