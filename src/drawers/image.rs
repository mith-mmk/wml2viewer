//! Helpers for decoding `wml2` images into the viewer-side canvas model.

use std::io;
use std::path::Path;

use crate::dependent::plugins::{
    decode_image_from_bytes_with_plugins, decode_image_from_file_with_plugins,
};
use crate::wml2_formats::available_save_formats;
use wml2viewer_core::image::{
    AnimationFrame as CoreAnimationFrame, DecodeRequest, DecodedImage, EncodeFormat, EncodeRequest,
    RgbaImage, decode, encode,
};

use super::affine::{Affine, InterpolationAlgorithm};
use super::canvas::Canvas;
use super::error::Result;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageAlign {
    Default,
    Center,
    RightUp,
    RightBottom,
    LeftUp,
    LeftBottom,
    Right,
    Left,
    Up,
    Bottom,
}

#[derive(Clone, Debug)]
pub struct AnimationFrame {
    pub canvas: Canvas,
    pub delay_ms: u64,
}

#[derive(Clone, Debug)]
pub struct LoadedImage {
    pub canvas: Canvas,
    pub animation: Vec<AnimationFrame>,
    pub loop_count: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveFormat {
    Png,
    Jpeg,
    Bmp,
    Gif,
    Webp,
}

impl LoadedImage {
    pub fn is_animated(&self) -> bool {
        !self.animation.is_empty()
    }

    pub fn frame_count(&self) -> usize {
        self.animation.len().max(1)
    }

    pub fn frame_canvas(&self, index: usize) -> &Canvas {
        if self.animation.is_empty() {
            &self.canvas
        } else {
            &self.animation[index.min(self.animation.len() - 1)].canvas
        }
    }

    pub fn frame_delay_ms(&self, index: usize) -> u64 {
        if self.animation.is_empty() {
            0
        } else {
            self.animation[index.min(self.animation.len() - 1)].delay_ms
        }
    }
}

impl SaveFormat {
    pub fn extension(self) -> &'static str {
        match self {
            SaveFormat::Png => "png",
            SaveFormat::Jpeg => "jpg",
            SaveFormat::Bmp => "bmp",
            SaveFormat::Gif => "gif",
            SaveFormat::Webp => "webp",
        }
    }

    pub fn all_known() -> [SaveFormat; 5] {
        [
            SaveFormat::Png,
            SaveFormat::Jpeg,
            SaveFormat::Bmp,
            SaveFormat::Gif,
            SaveFormat::Webp,
        ]
    }

    pub fn all() -> Vec<SaveFormat> {
        available_save_formats()
    }
}

impl std::fmt::Display for SaveFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            SaveFormat::Png => "PNG",
            SaveFormat::Jpeg => "JPEG",
            SaveFormat::Bmp => "BMP",
            SaveFormat::Gif => "GIF",
            SaveFormat::Webp => "WebP",
        };
        write!(f, "{text}")
    }
}

pub fn load_canvas_from_file(path: &Path) -> Result<LoadedImage> {
    load_canvas_from_file_internal(path).or_else(|_| {
        decode_image_from_file_with_plugins(path)
            .ok_or_else(|| Box::new(io::Error::other("no plugin decoder succeeded")) as _)
    })
}

#[allow(dead_code)]
pub fn load_canvas_from_bytes(data: &[u8]) -> Result<LoadedImage> {
    load_canvas_from_bytes_with_hint(data, None)
}

pub fn load_canvas_from_bytes_with_hint(
    data: &[u8],
    path_hint: Option<&Path>,
) -> Result<LoadedImage> {
    load_canvas_from_bytes_internal(data).or_else(|_| {
        decode_image_from_bytes_with_plugins(data, path_hint)
            .ok_or_else(|| Box::new(io::Error::other("no plugin decoder succeeded")) as _)
    })
}

pub(crate) fn load_canvas_from_file_internal(path: &Path) -> Result<LoadedImage> {
    let data = std::fs::read(path)?;
    let image = decode(DecodeRequest {
        bytes: &data,
        format_hint: path.extension().and_then(|extension| extension.to_str()),
    })?;
    convert_image(image)
}

pub(crate) fn load_canvas_from_bytes_internal(data: &[u8]) -> Result<LoadedImage> {
    let image = decode(DecodeRequest {
        bytes: data,
        format_hint: None,
    })?;
    convert_image(image)
}

pub(crate) fn load_canvas_from_path_or_bytes_internal(
    data: &[u8],
    path_hint: Option<&Path>,
) -> Result<LoadedImage> {
    if let Some(path) = path_hint {
        return load_canvas_from_file_internal(path);
    }
    load_canvas_from_bytes_internal(data)
}

pub fn resize_canvas(
    source: &Canvas,
    scale: f32,
    algorithm: InterpolationAlgorithm,
) -> Result<Canvas> {
    let scale = normalized_scale(scale);
    let output_width = ((source.width() as f32 * scale).round().max(1.0)) as u32;
    let output_height = ((source.height() as f32 * scale).round().max(1.0)) as u32;
    let mut output = Canvas::new(output_width, output_height);
    Affine::resize(source, &mut output, scale, algorithm, ImageAlign::LeftUp);
    Ok(output)
}

pub fn resize_loaded_image(
    source: &LoadedImage,
    scale: f32,
    algorithm: InterpolationAlgorithm,
) -> Result<LoadedImage> {
    let canvas = resize_canvas(&source.canvas, scale, algorithm)?;
    let mut animation = Vec::with_capacity(source.animation.len());
    for frame in &source.animation {
        animation.push(AnimationFrame {
            canvas: resize_canvas(&frame.canvas, scale, algorithm)?,
            delay_ms: frame.delay_ms,
        });
    }

    Ok(LoadedImage {
        canvas,
        animation,
        loop_count: source.loop_count,
    })
}

pub fn save_loaded_image(path: &Path, image: &LoadedImage, format: SaveFormat) -> Result<()> {
    let image = image_to_core(image)?;
    let encoded = encode(EncodeRequest {
        image: &image,
        format: save_format_to_core_format(format),
    })?;
    std::fs::write(path, encoded)?;
    Ok(())
}

fn save_format_to_core_format(format: SaveFormat) -> EncodeFormat {
    match format {
        SaveFormat::Png => EncodeFormat::Png,
        SaveFormat::Jpeg => EncodeFormat::Jpeg,
        SaveFormat::Bmp => EncodeFormat::Bmp,
        SaveFormat::Gif => EncodeFormat::Gif,
        SaveFormat::Webp => EncodeFormat::Webp,
    }
}

fn normalized_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn convert_image(image: DecodedImage) -> Result<LoadedImage> {
    let canvas = Canvas::from_rgba(
        image.poster.width(),
        image.poster.height(),
        image.poster.pixels().to_vec(),
    )?;
    let animation = image
        .animation
        .into_iter()
        .map(|frame| {
            Ok(AnimationFrame {
                canvas: Canvas::from_rgba(
                    frame.image.width(),
                    frame.image.height(),
                    frame.image.into_pixels(),
                )?,
                delay_ms: frame.duration_ms,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(LoadedImage {
        canvas,
        animation,
        loop_count: image.loop_count,
    })
}

fn image_to_core(image: &LoadedImage) -> Result<DecodedImage> {
    let poster = RgbaImage::new(
        image.canvas.width(),
        image.canvas.height(),
        image.canvas.buffer().to_vec(),
    )?;
    let animation = image
        .animation
        .iter()
        .map(|frame| {
            Ok(CoreAnimationFrame {
                image: RgbaImage::new(
                    frame.canvas.width(),
                    frame.canvas.height(),
                    frame.canvas.buffer().to_vec(),
                )?,
                duration_ms: frame.delay_ms,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(DecodedImage {
        poster,
        animation,
        loop_count: image.loop_count,
    })
}

#[cfg(test)]
#[path = "../../tests/support/src/drawers/image_tests.rs"]
mod tests;
