use eframe::egui;

pub(crate) const CONTROL_HEIGHT: f32 = 40.0;

pub(crate) fn refine_controls(ui: &mut egui::Ui) {
    let visuals = &mut ui.style_mut().visuals;
    let dark = visuals.dark_mode;
    let (idle, hover, active, border, accent) = if dark {
        (
            egui::Color32::from_rgb(38, 43, 51),
            egui::Color32::from_rgb(50, 59, 70),
            egui::Color32::from_rgb(61, 73, 86),
            egui::Color32::from_rgb(66, 76, 88),
            egui::Color32::from_rgb(42, 112, 150),
        )
    } else {
        (
            egui::Color32::from_rgb(244, 247, 250),
            egui::Color32::from_rgb(229, 239, 246),
            egui::Color32::from_rgb(213, 230, 240),
            egui::Color32::from_rgb(201, 215, 225),
            egui::Color32::from_rgb(37, 111, 153),
        )
    };
    for (widget, fill) in [
        (&mut visuals.widgets.inactive, idle),
        (&mut visuals.widgets.hovered, hover),
        (&mut visuals.widgets.active, active),
    ] {
        widget.weak_bg_fill = fill;
        widget.bg_stroke = egui::Stroke::new(1.0_f32, border);
        widget.corner_radius = egui::CornerRadius::same(8);
    }
    visuals.selection.bg_fill = accent;
    ui.spacing_mut().button_padding = egui::vec2(12.0, 7.0);
    ui.spacing_mut().item_spacing = egui::vec2(7.0, 6.0);
}

pub(crate) fn action_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), CONTROL_HEIGHT],
        egui::Button::new(egui::RichText::new(label).size(14.0)).corner_radius(8),
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
        egui::Button::new(egui::RichText::new(label).size(14.0))
            .selected(selected)
            .corner_radius(8),
    )
}

pub(crate) fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let fill = if ui.visuals().dark_mode {
        egui::Color32::from_rgb(42, 112, 150)
    } else {
        egui::Color32::from_rgb(37, 111, 153)
    };
    ui.add_sized(
        [104.0, CONTROL_HEIGHT],
        egui::Button::new(
            egui::RichText::new(label)
                .size(14.0)
                .color(egui::Color32::WHITE),
        )
        .fill(fill)
        .stroke(egui::Stroke::NONE)
        .corner_radius(8),
    )
}
