use crate::ui::viewer::options::WindowUiTheme;
use eframe::egui;

pub(crate) fn apply_window_theme(
    ctx: &egui::Context,
    theme: WindowUiTheme,
    system_visuals: &egui::Visuals,
) {
    let visuals = match theme {
        WindowUiTheme::System => system_visuals.clone(),
        WindowUiTheme::Light => egui::Visuals::light(),
        WindowUiTheme::Dark => egui::Visuals::dark(),
    };
    ctx.set_visuals(visuals);
}

#[derive(Clone, Copy)]
pub(crate) struct DesignTokens {
    pub surface: egui::Color32,
    pub surface_hovered: egui::Color32,
    pub surface_selected: egui::Color32,
    pub overlay: egui::Color32,
    pub border: egui::Color32,
    pub accent: egui::Color32,
    pub danger: egui::Color32,
    pub text: egui::Color32,
    pub weak_text: egui::Color32,
}

impl DesignTokens {
    pub(crate) fn from_visuals(visuals: &egui::Visuals) -> Self {
        let accent = visuals.selection.bg_fill;
        let danger = if visuals.dark_mode {
            egui::Color32::from_rgb(238, 103, 103)
        } else {
            egui::Color32::from_rgb(190, 49, 49)
        };
        let surface = visuals.panel_fill;
        let surface_hovered = visuals.widgets.hovered.bg_fill;
        let surface_selected = visuals.selection.bg_fill.gamma_multiply(0.55);
        let overlay = if visuals.dark_mode {
            visuals.window_fill.gamma_multiply(1.08)
        } else {
            visuals.window_fill.gamma_multiply(0.98)
        };
        Self {
            surface,
            surface_hovered,
            surface_selected,
            overlay,
            border: visuals.window_stroke.color,
            accent,
            danger,
            text: visuals.text_color(),
            weak_text: visuals.weak_text_color(),
        }
    }
}
