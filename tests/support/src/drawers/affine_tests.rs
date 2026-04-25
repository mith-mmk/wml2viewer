use super::{Affine, InterpolationAlgorithm};
use crate::drawers::canvas::Canvas;
use crate::drawers::image::ImageAlign;

#[test]
fn resize_preserves_solid_color() {
    let source = Canvas::from_rgba(1, 1, vec![10, 20, 30, 255]).unwrap();
    let mut output = Canvas::new(4, 4);

    Affine::resize(
        &source,
        &mut output,
        4.0,
        InterpolationAlgorithm::Bilinear,
        ImageAlign::LeftUp,
    );

    assert!(
        output
            .buffer()
            .chunks_exact(4)
            .all(|pixel| pixel == [10, 20, 30, 255])
    );
}

#[test]
fn downscale_uses_pixel_mixing_average() {
    let source = Canvas::from_rgba(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255]).unwrap();
    let mut output = Canvas::new(1, 1);

    Affine::resize(
        &source,
        &mut output,
        0.5,
        InterpolationAlgorithm::NearestNeighber,
        ImageAlign::LeftUp,
    );

    assert_eq!(output.buffer(), &[128, 128, 128, 255]);
}
