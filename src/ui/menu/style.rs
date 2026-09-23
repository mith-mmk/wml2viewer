use crate::ui::theme::DesignTokens;
use eframe::egui;

pub(crate) const CONTROL_HEIGHT: f32 = 32.0;
pub(crate) const MENU_ROW_HEIGHT: f32 = 32.0;
pub(crate) const MENU_FRAME_MARGIN: i8 = 4;

#[derive(Clone, Copy)]
pub(crate) enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

fn tokens(ui: &egui::Ui) -> DesignTokens {
    DesignTokens::from_visuals(ui.visuals())
}

pub(crate) fn menu_frame(ctx: &egui::Context) -> egui::Frame {
    let tokens = DesignTokens::from_visuals(&ctx.style().visuals);
    egui::Frame::popup(&ctx.style())
        .fill(tokens.overlay)
        .stroke(egui::Stroke::new(1.0, tokens.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(MENU_FRAME_MARGIN))
}

pub(crate) fn settings_frame(ctx: &egui::Context) -> egui::Frame {
    let tokens = DesignTokens::from_visuals(&ctx.style().visuals);
    egui::Frame::window(&ctx.style())
        .fill(tokens.overlay)
        .stroke(egui::Stroke::new(1.0, tokens.border))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
}

pub(crate) fn dialog_frame(ctx: &egui::Context) -> egui::Frame {
    settings_frame(ctx).inner_margin(egui::Margin::same(16))
}

pub(crate) fn panel_frame(ui: &egui::Ui) -> egui::Frame {
    let tokens = tokens(ui);
    egui::Frame::NONE
        .fill(tokens.surface)
        .stroke(egui::Stroke::new(1.0, tokens.border.gamma_multiply(0.65)))
        .inner_margin(egui::Margin::same(8))
}

pub(crate) fn button(
    ui: &mut egui::Ui,
    kind: ButtonKind,
    label: impl Into<String>,
) -> egui::Response {
    let tokens = tokens(ui);
    let (fill, stroke, text) = match kind {
        ButtonKind::Primary => (
            tokens.accent,
            tokens.accent,
            ui.visuals().selection.stroke.color,
        ),
        ButtonKind::Secondary => (tokens.surface, tokens.border, tokens.text),
        ButtonKind::Ghost => (
            egui::Color32::TRANSPARENT,
            egui::Color32::TRANSPARENT,
            tokens.text,
        ),
        ButtonKind::Danger => (tokens.danger, tokens.danger, egui::Color32::WHITE),
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label.into()).color(text))
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke))
            .min_size(egui::vec2(0.0, CONTROL_HEIGHT)),
    )
}

pub(crate) fn menu_row(
    ui: &mut egui::Ui,
    label: &str,
    trailing: Option<&str>,
    selected: bool,
) -> egui::Response {
    ui.spacing_mut().item_spacing.y = 0.0;
    let tokens = tokens(ui);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), MENU_ROW_HEIGHT),
        egui::Sense::click(),
    );
    let fill = if selected {
        tokens.surface_selected
    } else if response.hovered() {
        tokens.surface_hovered
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 5.0, fill);
    ui.painter().text(
        egui::pos2(rect.left() + 8.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::TextStyle::Body.resolve(ui.style()),
        tokens.text,
    );
    if let Some(trailing) = trailing {
        ui.painter().text(
            egui::pos2(rect.right() - 8.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            trailing,
            egui::TextStyle::Small.resolve(ui.style()),
            tokens.weak_text,
        );
    }
    response
}

pub(crate) fn sidebar_tab(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), egui::Sense::click());
    let tokens = tokens(ui);
    let fill = if selected {
        tokens.surface_selected
    } else if response.hovered() {
        tokens.surface_hovered
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 8.0, fill);
    if selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                rect.left_top(),
                egui::pos2(rect.left() + 3.0, rect.bottom()),
            ),
            2.0,
            tokens.accent,
        );
    }
    ui.painter().text(
        egui::pos2(rect.left() + 14.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.0),
        if selected {
            tokens.text
        } else {
            tokens.weak_text
        },
    );
    response
}

pub(crate) fn top_tab(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let tokens = tokens(ui);
    let (fill, stroke, text) = if selected {
        (tokens.surface_selected, tokens.accent, tokens.text)
    } else {
        (
            egui::Color32::TRANSPARENT,
            egui::Color32::TRANSPARENT,
            tokens.weak_text,
        )
    };
    ui.add(
        egui::Button::new(egui::RichText::new(label).color(text))
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, stroke))
            .min_size(egui::vec2(78.0, CONTROL_HEIGHT)),
    )
}

pub(crate) fn settings_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    panel_frame(ui)
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
        .show(ui, add_contents);
    ui.add_space(8.0);
}
