//! Helpers for decoding `wml2` images into the viewer-side canvas model.

use std::io;
use std::path::Path;

use crate::dependent::plugins::{
    decode_image_from_bytes_with_plugins, decode_image_from_file_with_plugins,
};
use wml2::color::RGBA;
use wml2::draw::{
    AnimationLayer as WmlAnimationLayer, ImageBuffer, NextBlend, NextDispose, image_from,
    image_from_file,
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

    pub fn all() -> [SaveFormat; 5] {
        [
            SaveFormat::Png,
            SaveFormat::Jpeg,
            SaveFormat::Bmp,
            SaveFormat::Gif,
            SaveFormat::Webp,
        ]
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
    let image = image_from_file(path.to_string_lossy().into_owned())?;
    convert_image(image, Some(path))
}

pub(crate) fn load_canvas_from_bytes_internal(data: &[u8]) -> Result<LoadedImage> {
    let image = image_from(data)?;
    convert_image(image, None)
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
    let mut buffer = image_to_buffer(image);
    let encoded = wml2::draw::image_to(&mut buffer, save_format_to_image_format(format), None)?;
    std::fs::write(path, encoded)?;
    Ok(())
}

fn image_to_buffer(image: &LoadedImage) -> ImageBuffer {
    let mut buffer = ImageBuffer::from_buffer(
        image.canvas.width() as usize,
        image.canvas.height() as usize,
        image.canvas.buffer().to_vec(),
    );
    if !image.animation.is_empty() {
        buffer.set_animation(true);
        buffer.loop_count = image.loop_count;
        for frame in &image.animation {
            buffer.animation.as_mut().unwrap().push(WmlAnimationLayer {
                width: frame.canvas.width() as usize,
                height: frame.canvas.height() as usize,
                start_x: 0,
                start_y: 0,
                buffer: frame.canvas.buffer().to_vec(),
                control: wml2::draw::NextOptions::wait(frame.delay_ms),
            });
        }
    }
    buffer
}

fn save_format_to_image_format(format: SaveFormat) -> wml2::util::ImageFormat {
    match format {
        SaveFormat::Png => wml2::util::ImageFormat::Png,
        SaveFormat::Jpeg => wml2::util::ImageFormat::Jpeg,
        SaveFormat::Bmp => wml2::util::ImageFormat::Bmp,
        SaveFormat::Gif => wml2::util::ImageFormat::Gif,
        SaveFormat::Webp => wml2::util::ImageFormat::Webp,
    }
}

fn normalized_scale(scale: f32) -> f32 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

fn convert_image(image: ImageBuffer, path: Option<&Path>) -> Result<LoadedImage> {
    if image.width == 0 || image.height == 0 {
        let label = path
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<memory>".to_string());
        return Err(Box::new(io::Error::other(format!(
            "decoded image has invalid size: {label}"
        ))));
    }

    let rgba = image.buffer.as_ref().ok_or_else(|| {
        let label = path
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<memory>".to_string());
        Box::new(io::Error::other(format!(
            "decoded image buffer is missing: {label}"
        ))) as Box<dyn std::error::Error>
    })?;

    let base = Canvas::from_rgba(image.width as u32, image.height as u32, rgba.clone())?;
    let animation = compose_animation_frames(
        &base,
        image.animation.as_deref().unwrap_or(&[]),
        image.background_color.as_ref(),
    )?;
    let canvas = animation
        .first()
        .map(|frame| frame.canvas.clone())
        .unwrap_or_else(|| base.clone());

    Ok(LoadedImage {
        canvas,
        animation,
        loop_count: image.loop_count,
    })
}

fn compose_animation_frames(
    base: &Canvas,
    layers: &[WmlAnimationLayer],
    background: Option<&RGBA>,
) -> Result<Vec<AnimationFrame>> {
    if layers.is_empty() {
        return Ok(Vec::new());
    }

    let background = background_rgba(background);
    let mut frames = Vec::with_capacity(layers.len());
    let mut composited = Canvas::new(base.width(), base.height());
    for pixel in composited.buffer_mut().chunks_exact_mut(4) {
        pixel.copy_from_slice(&background);
    }

    for layer in layers {
        let previous = composited.clone();
        let mut frame_canvas = composited.clone();
        apply_layer(&mut frame_canvas, layer);

        frames.push(AnimationFrame {
            canvas: frame_canvas.clone(),
            delay_ms: layer.control.await_time,
        });

        composited = frame_canvas;
        match layer.control.dispose_option {
            Some(NextDispose::Background) => {
                clear_rect(&mut composited, layer, background);
            }
            Some(NextDispose::Previous) => {
                composited = previous;
            }
            _ => {}
        }
    }

    Ok(frames)
}

fn apply_layer(canvas: &mut Canvas, layer: &WmlAnimationLayer) {
    let dest_width = canvas.width() as usize;
    let dest_height = canvas.height() as usize;
    let frame_width = layer.width;
    let frame_height = layer.height;
    let dest = canvas.buffer_mut();
    let source = &layer.buffer;
    let alpha_blend = matches!(layer.control.blend, Some(NextBlend::Source));

    for y in 0..frame_height {
        let dest_y = layer.start_y + y as i32;
        if dest_y < 0 || dest_y >= dest_height as i32 {
            continue;
        }

        for x in 0..frame_width {
            let dest_x = layer.start_x + x as i32;
            if dest_x < 0 || dest_x >= dest_width as i32 {
                continue;
            }

            let src_offset = (y * frame_width + x) * 4;
            let dst_offset = ((dest_y as usize * dest_width) + dest_x as usize) * 4;
            let src = [
                source[src_offset],
                source[src_offset + 1],
                source[src_offset + 2],
                source[src_offset + 3],
            ];

            if alpha_blend {
                let dst = [
                    dest[dst_offset],
                    dest[dst_offset + 1],
                    dest[dst_offset + 2],
                    dest[dst_offset + 3],
                ];
                let out = blend_rgba(src, dst);
                dest[dst_offset] = out[0];
                dest[dst_offset + 1] = out[1];
                dest[dst_offset + 2] = out[2];
                dest[dst_offset + 3] = out[3];
            } else {
                dest[dst_offset] = src[0];
                dest[dst_offset + 1] = src[1];
                dest[dst_offset + 2] = src[2];
                dest[dst_offset + 3] = src[3];
            }
        }
    }
}

fn clear_rect(canvas: &mut Canvas, layer: &WmlAnimationLayer, background: [u8; 4]) {
    let dest_width = canvas.width() as usize;
    let dest_height = canvas.height() as usize;
    let dest = canvas.buffer_mut();

    for y in 0..layer.height {
        let dest_y = layer.start_y + y as i32;
        if dest_y < 0 || dest_y >= dest_height as i32 {
            continue;
        }

        for x in 0..layer.width {
            let dest_x = layer.start_x + x as i32;
            if dest_x < 0 || dest_x >= dest_width as i32 {
                continue;
            }

            let dst_offset = ((dest_y as usize * dest_width) + dest_x as usize) * 4;
            dest[dst_offset] = background[0];
            dest[dst_offset + 1] = background[1];
            dest[dst_offset + 2] = background[2];
            dest[dst_offset + 3] = background[3];
        }
    }
}

fn background_rgba(background: Option<&RGBA>) -> [u8; 4] {
    if let Some(background) = background {
        [
            background.red,
            background.green,
            background.blue,
            background.alpha,
        ]
    } else {
        [0, 0, 0, 0]
    }
}

fn blend_rgba(src: [u8; 4], dst: [u8; 4]) -> [u8; 4] {
    let src_alpha = src[3] as f32 / 255.0;
    let dst_alpha = dst[3] as f32 / 255.0;
    let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);
    if out_alpha <= f32::EPSILON {
        return [0, 0, 0, 0];
    }

    let mut out = [0_u8; 4];
    for channel in 0..3 {
        let src_value = src[channel] as f32 / 255.0;
        let dst_value = dst[channel] as f32 / 255.0;
        let blended =
            (src_value * src_alpha + dst_value * dst_alpha * (1.0 - src_alpha)) / out_alpha;
        out[channel] = (blended * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    out[3] = (out_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    out
}

#[cfg(test)]
mod tests {
    use super::{load_canvas_from_file, load_canvas_from_file_internal};
    use crate::dependent::plugins::{
        PluginConfig, PluginProviderConfig, discover_plugin_modules, set_runtime_plugin_config,
    };
    use std::path::PathBuf;
    use wml2::draw::image_from_file;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn bundled_test_image_path(name: &str) -> PathBuf {
        repo_root()
            .join("test")
            .join("images")
            .join("bundled")
            .join(name)
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn plugin_decode_is_visible_from_viewer_load_path() {
        let config = PluginConfig {
            ffmpeg: PluginProviderConfig {
                enable: true,
                priority: 100,
                search_path: vec![repo_root().join("test").join("plugins").join("ffmpeg")],
                modules: Vec::new(),
            },
            ..PluginConfig::default()
        };
        if discover_plugin_modules("ffmpeg", &config.ffmpeg).is_empty() {
            return;
        }
        set_runtime_plugin_config(config);

        let decoded = load_canvas_from_file(&repo_root().join("samples").join("WML2Viewer.avif"));
        assert!(decoded.is_ok());
    }

    #[test]
    fn loads_bundled_webp_sample() {
        let decoded = load_canvas_from_file(&repo_root().join("samples").join("WML2Viewer.webp"));
        assert!(decoded.is_ok());
    }

    #[test]
    fn bundled_webp_sample_matches_raw_decoder_canvas() {
        let path = repo_root().join("samples").join("WML2Viewer.webp");
        let raw = image_from_file(path.to_string_lossy().into_owned()).unwrap();
        let loaded = load_canvas_from_file_internal(&path).unwrap();

        assert_eq!(loaded.canvas.width() as usize, raw.width);
        assert_eq!(loaded.canvas.height() as usize, raw.height);
        assert_eq!(loaded.canvas.buffer(), raw.buffer.as_ref().unwrap());
    }

    #[test]
    fn bundled_error_webp_sample_matches_raw_decoder_canvas() {
        let path = bundled_test_image_path("WML2Viewer_error.webp");
        let raw = image_from_file(path.to_string_lossy().into_owned()).unwrap();
        let loaded = load_canvas_from_file_internal(&path).unwrap();

        assert_eq!(loaded.canvas.width() as usize, raw.width);
        assert_eq!(loaded.canvas.height() as usize, raw.height);
        assert_eq!(loaded.canvas.buffer(), raw.buffer.as_ref().unwrap());
    }
}
