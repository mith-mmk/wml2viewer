use eframe::egui;

pub(crate) const CONTROL_HEIGHT: f32 = 40.0;

pub(crate) fn action_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), CONTROL_HEIGHT],
        egui::Button::new(label),
    )
}

pub(crate) fn category_button(
    ui: &mut egui::Ui,
    label: &str,
    selected: bool,
    width: f32,
) -> egui::Response {
    ui.add_sized(
        [width, CONTROL_HEIGHT],
        egui::Button::new(label).selected(selected),
    )
}
