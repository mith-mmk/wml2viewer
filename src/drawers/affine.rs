//! RGBA resampling helpers used for zooming inside the viewer.

use super::canvas::Screen;
use super::image::ImageAlign;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterpolationAlgorithm {
    NearestNeighber,
    Bilinear,
    Bicubic,
    BicubicAlpha(Option<u32>),
    Lanzcos3,
    Lanzcos(Option<usize>),
}

pub struct Affine;

impl Affine {
    pub fn resize(
        input_screen: &dyn Screen,
        output_screen: &mut dyn Screen,
        scale: f32,
        algorithm: InterpolationAlgorithm,
        align: ImageAlign,
    ) {
        let input_width = input_screen.width() as usize;
        let input_height = input_screen.height() as usize;
        let output_width = output_screen.width() as usize;
        let output_height = output_screen.height() as usize;
        if input_width == 0 || input_height == 0 || output_width == 0 || output_height == 0 {
            return;
        }

        let scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
        let scaled_width = ((input_width as f32 * scale).round().max(1.0)) as usize;
        let scaled_height = ((input_height as f32 * scale).round().max(1.0)) as usize;
        let (offset_x, offset_y) = aligned_origin(
            output_width,
            output_height,
            scaled_width,
            scaled_height,
            align,
        );

        output_screen.clear_with_color(0x0000_0000);
        let input = input_screen.buffer();
        let output = output_screen.buffer_mut();

        for dy in 0..scaled_height {
            let dest_y = offset_y + dy as isize;
            if dest_y < 0 || dest_y >= output_height as isize {
                continue;
            }

            for dx in 0..scaled_width {
                let dest_x = offset_x + dx as isize;
                if dest_x < 0 || dest_x >= output_width as isize {
                    continue;
                }

                let rgba = if scale < 1.0 {
                    sample_area_mix(input, input_width, input_height, dx, dy, scale)
                } else {
                    let src_x =
                        ((dx as f32 + 0.5) / scale - 0.5).clamp(0.0, input_width as f32 - 1.0);
                    let src_y =
                        ((dy as f32 + 0.5) / scale - 0.5).clamp(0.0, input_height as f32 - 1.0);
                    match algorithm {
                        InterpolationAlgorithm::NearestNeighber => {
                            sample_nearest(input, input_width, input_height, src_x, src_y)
                        }
                        InterpolationAlgorithm::Bilinear => {
                            sample_bilinear(input, input_width, input_height, src_x, src_y)
                        }
                        InterpolationAlgorithm::Bicubic => {
                            sample_bicubic(input, input_width, input_height, src_x, src_y, -0.5)
                        }
                        InterpolationAlgorithm::BicubicAlpha(alpha) => sample_bicubic(
                            input,
                            input_width,
                            input_height,
                            src_x,
                            src_y,
                            alpha.map(|value| -(value as f32 / 100.0)).unwrap_or(-0.5),
                        ),
                        InterpolationAlgorithm::Lanzcos3 => {
                            sample_lanczos(input, input_width, input_height, src_x, src_y, 3)
                        }
                        InterpolationAlgorithm::Lanzcos(radius) => sample_lanczos(
                            input,
                            input_width,
                            input_height,
                            src_x,
                            src_y,
                            radius.unwrap_or(3).clamp(2, 8),
                        ),
                    }
                };
                write_pixel(output, output_width, dest_x as usize, dest_y as usize, rgba);
            }
        }
    }
}

fn aligned_origin(
    output_width: usize,
    output_height: usize,
    scaled_width: usize,
    scaled_height: usize,
    align: ImageAlign,
) -> (isize, isize) {
    let center_x = (output_width as isize - scaled_width as isize) / 2;
    let center_y = (output_height as isize - scaled_height as isize) / 2;
    let right = output_width as isize - scaled_width as isize;
    let bottom = output_height as isize - scaled_height as isize;

    match align {
        ImageAlign::Default | ImageAlign::LeftUp => (0, 0),
        ImageAlign::Center => (center_x, center_y),
        ImageAlign::RightUp => (right, 0),
        ImageAlign::RightBottom => (right, bottom),
        ImageAlign::LeftBottom => (0, bottom),
        ImageAlign::Right => (right, center_y),
        ImageAlign::Left => (0, center_y),
        ImageAlign::Up => (center_x, 0),
        ImageAlign::Bottom => (center_x, bottom),
    }
}

fn sample_nearest(input: &[u8], width: usize, height: usize, src_x: f32, src_y: f32) -> [u8; 4] {
    let x = src_x.round().clamp(0.0, width as f32 - 1.0) as usize;
    let y = src_y.round().clamp(0.0, height as f32 - 1.0) as usize;
    pixel_at(input, width, x, y)
}

fn sample_bilinear(input: &[u8], width: usize, height: usize, src_x: f32, src_y: f32) -> [u8; 4] {
    let x0 = src_x.floor().clamp(0.0, width as f32 - 1.0) as usize;
    let y0 = src_y.floor().clamp(0.0, height as f32 - 1.0) as usize;
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let tx = src_x - x0 as f32;
    let ty = src_y - y0 as f32;

    let p00 = pixel_at(input, width, x0, y0);
    let p10 = pixel_at(input, width, x1, y0);
    let p01 = pixel_at(input, width, x0, y1);
    let p11 = pixel_at(input, width, x1, y1);

    let mut out = [0_u8; 4];
    for i in 0..4 {
        let top = lerp(p00[i] as f32, p10[i] as f32, tx);
        let bottom = lerp(p01[i] as f32, p11[i] as f32, tx);
        out[i] = lerp(top, bottom, ty).round().clamp(0.0, 255.0) as u8;
    }
    out
}

fn sample_bicubic(
    input: &[u8],
    width: usize,
    height: usize,
    src_x: f32,
    src_y: f32,
    alpha: f32,
) -> [u8; 4] {
    sample_kernel(input, width, height, src_x, src_y, 2, |distance| {
        cubic_kernel(distance, alpha)
    })
}

fn sample_lanczos(
    input: &[u8],
    width: usize,
    height: usize,
    src_x: f32,
    src_y: f32,
    lobes: usize,
) -> [u8; 4] {
    sample_kernel(
        input,
        width,
        height,
        src_x,
        src_y,
        lobes as isize,
        |distance| lanczos_kernel(distance, lobes as f32),
    )
}

fn sample_kernel<F>(
    input: &[u8],
    width: usize,
    height: usize,
    src_x: f32,
    src_y: f32,
    radius: isize,
    kernel: F,
) -> [u8; 4]
where
    F: Fn(f32) -> f32,
{
    let base_x = src_x.floor() as isize;
    let base_y = src_y.floor() as isize;
    let mut sums = [0.0_f32; 4];
    let mut total = 0.0_f32;

    for y in (base_y - radius + 1)..=(base_y + radius) {
        let clamped_y = y.clamp(0, height as isize - 1) as usize;
        let wy = kernel(src_y - y as f32);
        if wy.abs() <= f32::EPSILON {
            continue;
        }
        for x in (base_x - radius + 1)..=(base_x + radius) {
            let clamped_x = x.clamp(0, width as isize - 1) as usize;
            let wx = kernel(src_x - x as f32);
            let weight = wx * wy;
            if weight.abs() <= f32::EPSILON {
                continue;
            }
            let pixel = pixel_at(input, width, clamped_x, clamped_y);
            for channel in 0..4 {
                sums[channel] += pixel[channel] as f32 * weight;
            }
            total += weight;
        }
    }

    if total.abs() <= f32::EPSILON {
        return sample_bilinear(input, width, height, src_x, src_y);
    }

    let mut out = [0_u8; 4];
    for channel in 0..4 {
        out[channel] = (sums[channel] / total).round().clamp(0.0, 255.0) as u8;
    }
    out
}

fn sample_area_mix(
    input: &[u8],
    width: usize,
    height: usize,
    dest_x: usize,
    dest_y: usize,
    scale: f32,
) -> [u8; 4] {
    let src_x0 = dest_x as f32 / scale;
    let src_x1 = (dest_x as f32 + 1.0) / scale;
    let src_y0 = dest_y as f32 / scale;
    let src_y1 = (dest_y as f32 + 1.0) / scale;

    let x_start = src_x0.floor().clamp(0.0, width as f32 - 1.0) as usize;
    let x_end = src_x1.ceil().clamp(1.0, width as f32) as usize;
    let y_start = src_y0.floor().clamp(0.0, height as f32 - 1.0) as usize;
    let y_end = src_y1.ceil().clamp(1.0, height as f32) as usize;

    let mut sums = [0.0_f32; 4];
    let mut total = 0.0_f32;

    for sy in y_start..y_end {
        let y0 = sy as f32;
        let y1 = y0 + 1.0;
        let overlap_y = (src_y1.min(y1) - src_y0.max(y0)).max(0.0);
        if overlap_y <= f32::EPSILON {
            continue;
        }

        for sx in x_start..x_end {
            let x0 = sx as f32;
            let x1 = x0 + 1.0;
            let overlap_x = (src_x1.min(x1) - src_x0.max(x0)).max(0.0);
            let weight = overlap_x * overlap_y;
            if weight <= f32::EPSILON {
                continue;
            }

            let pixel = pixel_at(input, width, sx, sy);
            for channel in 0..4 {
                sums[channel] += pixel[channel] as f32 * weight;
            }
            total += weight;
        }
    }

    if total <= f32::EPSILON {
        return sample_nearest(input, width, height, src_x0, src_y0);
    }

    let mut out = [0_u8; 4];
    for channel in 0..4 {
        out[channel] = (sums[channel] / total).round().clamp(0.0, 255.0) as u8;
    }
    out
}

fn cubic_kernel(distance: f32, alpha: f32) -> f32 {
    let x = distance.abs();
    if x <= 1.0 {
        (alpha + 2.0) * x.powi(3) - (alpha + 3.0) * x.powi(2) + 1.0
    } else if x < 2.0 {
        alpha * x.powi(3) - 5.0 * alpha * x.powi(2) + 8.0 * alpha * x - 4.0 * alpha
    } else {
        0.0
    }
}

fn lanczos_kernel(distance: f32, lobes: f32) -> f32 {
    let x = distance.abs();
    if x <= f32::EPSILON {
        return 1.0;
    }
    if x >= lobes {
        return 0.0;
    }
    sinc(x) * sinc(x / lobes)
}

fn sinc(x: f32) -> f32 {
    let px = std::f32::consts::PI * x;
    px.sin() / px
}

fn pixel_at(input: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * width + x) * 4;
    [
        input[offset],
        input[offset + 1],
        input[offset + 2],
        input[offset + 3],
    ]
}

fn write_pixel(output: &mut [u8], width: usize, x: usize, y: usize, rgba: [u8; 4]) {
    let offset = (y * width + x) * 4;
    output[offset] = rgba[0];
    output[offset + 1] = rgba[1];
    output[offset + 2] = rgba[2];
    output[offset + 3] = rgba[3];
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
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
}
