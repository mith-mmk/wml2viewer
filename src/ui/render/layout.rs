use crate::drawers::affine::InterpolationAlgorithm;
use crate::drawers::canvas::Canvas;
use crate::drawers::image::ImageAlign;
use crate::ui::viewer::ViewerApp;
use eframe::egui::{self, Color32, ColorImage};

pub(crate) fn canvas_to_color_image(canvas: &Canvas) -> ColorImage {
    ColorImage::from_rgba_unmultiplied(
        [canvas.width() as usize, canvas.height() as usize],
        canvas.buffer(),
    )
}

pub(crate) fn aligned_offset(
    viewport: egui::Vec2,
    draw_size: egui::Vec2,
    align: ImageAlign,
) -> egui::Vec2 {
    let horizontal = match align {
        ImageAlign::Center | ImageAlign::Up | ImageAlign::Bottom => {
            (viewport.x - draw_size.x) * 0.5
        }
        ImageAlign::Right | ImageAlign::RightUp | ImageAlign::RightBottom => {
            viewport.x - draw_size.x
        }
        _ => 0.0,
    };
    let vertical = match align {
        ImageAlign::Center | ImageAlign::Left | ImageAlign::Right => {
            (viewport.y - draw_size.y) * 0.5
        }
        ImageAlign::LeftBottom | ImageAlign::RightBottom | ImageAlign::Bottom => {
            viewport.y - draw_size.y
        }
        _ => 0.0,
    };

    egui::vec2(horizontal, vertical)
}

pub(crate) fn rgba_to_color32([r, g, b, a]: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

pub(crate) fn interpolation_label(method: InterpolationAlgorithm) -> &'static str {
    match method {
        InterpolationAlgorithm::NearestNeighber => "Nearest",
        InterpolationAlgorithm::Bilinear => "Bilinear",
        InterpolationAlgorithm::Bicubic => "Bicubic",
        InterpolationAlgorithm::BicubicAlpha(_) => "Bicubic",
        InterpolationAlgorithm::Lanzcos3 => "Lanczos3",
        InterpolationAlgorithm::Lanzcos(_) => "Lanczos",
    }
}

impl ViewerApp {
    pub(crate) fn paint_background(&self, ui: &mut egui::Ui, rect: egui::Rect) {
        match &self.options.background {
            crate::ui::viewer::options::BackgroundStyle::Solid(color) => {
                ui.painter().rect_filled(rect, 0.0, rgba_to_color32(*color));
            }
            crate::ui::viewer::options::BackgroundStyle::Tile {
                color1,
                color2,
                size,
            } => {
                let size = (*size).max(1) as f32;
                let color1 = rgba_to_color32(*color1);
                let color2 = rgba_to_color32(*color2);
                let mut y = rect.top();
                let mut row = 0_u32;
                while y < rect.bottom() {
                    let mut x = rect.left();
                    let mut col = 0_u32;
                    while x < rect.right() {
                        let color = if (row + col).is_multiple_of(2) {
                            color1
                        } else {
                            color2
                        };
                        let tile = egui::Rect::from_min_size(
                            egui::pos2(x, y),
                            egui::vec2(size.min(rect.right() - x), size.min(rect.bottom() - y)),
                        );
                        ui.painter().rect_filled(tile, 0.0, color);
                        x += size;
                        col += 1;
                    }
                    y += size;
                    row += 1;
                }
            }
        }
    }
}
