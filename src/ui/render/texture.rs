use crate::drawers::affine::InterpolationAlgorithm;
use crate::drawers::canvas::Canvas;
use crate::drawers::image::resize_canvas;

pub(crate) fn downscale_for_texture_limit<'a>(
    canvas: &'a Canvas,
    max_texture_side: usize,
    method: InterpolationAlgorithm,
) -> (std::borrow::Cow<'a, Canvas>, f32) {
    let width = canvas.width() as usize;
    let height = canvas.height() as usize;
    let max_side = width.max(height);
    if max_side <= max_texture_side || max_texture_side == 0 {
        return (std::borrow::Cow::Borrowed(canvas), 1.0);
    }

    let scale = max_texture_side as f32 / max_side as f32;
    match resize_canvas(canvas, scale, method) {
        Ok(resized) => (std::borrow::Cow::Owned(resized), scale),
        Err(_) => (std::borrow::Cow::Borrowed(canvas), 1.0),
    }
}
