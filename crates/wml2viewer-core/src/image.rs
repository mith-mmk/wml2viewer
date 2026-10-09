//! Byte-oriented image decode and encode contracts.

use crate::{CoreError, CoreResult};
use bin_rs::reader::BytesReader;
use std::panic::{AssertUnwindSafe, catch_unwind};
use wml2::color::RGBA;
use wml2::draw::{
    AnimationLayer as WmlAnimationLayer, CallbackResponse, DecodeOptions as WmlDecodeOptions,
    DrawCallback, DrawOptions, ImageBuffer, InitOptions, NextBlend, NextDispose, NextOptions,
    TerminateOptions, VerboseOptions, image_decoder_with_limits, image_from_with_limits, image_to,
};
use wml2::limits::DecodeLimits as WmlDecodeLimits;
use wml2::metadata::DataMap;

pub type DecodeCancelProbe<'a> = &'a (dyn Fn() -> bool + Send + Sync);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    width: u32,
    height: u32,
    stride: u32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> CoreResult<Self> {
        if width == 0 || height == 0 {
            return Err(CoreError::invalid_input(
                "RGBA image dimensions must be non-zero",
            ));
        }
        let stride = width
            .checked_mul(4)
            .ok_or_else(|| CoreError::invalid_input("RGBA image stride overflow"))?;
        let expected = (stride as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| CoreError::invalid_input("RGBA image buffer length overflow"))?;
        if pixels.len() != expected {
            return Err(CoreError::invalid_input(format!(
                "invalid RGBA buffer length: expected {expected}, got {}",
                pixels.len()
            )));
        }
        Ok(Self {
            width,
            height,
            stride,
            pixels,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }
    pub const fn height(&self) -> u32 {
        self.height
    }
    pub const fn stride(&self) -> u32 {
        self.stride
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    fn try_clone(&self) -> CoreResult<Self> {
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(self.pixels.len())
            .map_err(|error| CoreError::limit(format!("RGBA allocation failed: {error}")))?;
        pixels.extend_from_slice(&self.pixels);
        Ok(Self {
            width: self.width,
            height: self.height,
            stride: self.stride,
            pixels,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimationFrame {
    pub image: RgbaImage,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedImage {
    pub poster: RgbaImage,
    pub animation: Vec<AnimationFrame>,
    pub loop_count: Option<u32>,
}

impl DecodedImage {
    pub fn is_animated(&self) -> bool {
        !self.animation.is_empty()
    }
    pub fn frame_count(&self) -> usize {
        self.animation.len().max(1)
    }

    pub fn display_frame(&self, index: usize) -> &RgbaImage {
        if self.animation.is_empty() {
            &self.poster
        } else {
            &self.animation[index.min(self.animation.len() - 1)].image
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodeFormat {
    Png,
    Jpeg,
    Bmp,
    Gif,
    Webp,
}

pub struct DecodeRequest<'a> {
    pub bytes: &'a [u8],
    pub format_hint: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeLimits {
    pub maximum_frame_pixels: u64,
    pub maximum_frames: usize,
    pub maximum_rgba_bytes: usize,
}

impl DecodeLimits {
    pub const UNLIMITED: Self = Self {
        maximum_frame_pixels: u64::MAX,
        maximum_frames: usize::MAX,
        maximum_rgba_bytes: usize::MAX,
    };
}

pub struct EncodeRequest<'a> {
    pub image: &'a DecodedImage,
    pub format: EncodeFormat,
}

pub trait ImageDecoder: Send + Sync {
    fn decode(&self, request: DecodeRequest<'_>) -> CoreResult<DecodedImage>;
}

pub trait ImageEncoder: Send + Sync {
    fn encode(&self, request: EncodeRequest<'_>) -> CoreResult<Vec<u8>>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Wml2Codec;

impl ImageDecoder for Wml2Codec {
    fn decode(&self, request: DecodeRequest<'_>) -> CoreResult<DecodedImage> {
        let _format_hint = request.format_hint;
        let image = catch_unwind(AssertUnwindSafe(|| {
            image_from_with_limits(request.bytes, WmlDecodeLimits::unlimited())
        }))
        .map_err(|_| CoreError::decode("image decoder panicked"))?
        .map_err(|error| CoreError::decode(error.to_string()))?;
        decoded_from_wml2(image)
    }
}

impl Wml2Codec {
    pub fn decode_with_limits(
        &self,
        request: DecodeRequest<'_>,
        limits: DecodeLimits,
        cancel_probe: Option<DecodeCancelProbe<'_>>,
    ) -> CoreResult<DecodedImage> {
        let _format_hint = request.format_hint;
        check_cancelled(cancel_probe)?;
        let mut target = CancellableImageBuffer::new(cancel_probe);
        let image = catch_unwind(AssertUnwindSafe(|| {
            image_decoder_with_limits(
                &mut BytesReader::new(request.bytes),
                &mut WmlDecodeOptions {
                    debug_flag: 0,
                    drawer: &mut target,
                },
                wml_decode_limits(limits),
            )
        }))
        .map_err(|_| CoreError::decode("image decoder panicked"))?
        .map_err(map_wml2_decode_error)?;
        let _warnings = image;
        let image = target.into_inner();
        check_cancelled(cancel_probe)?;
        decoded_from_wml2_with_limits_and_cancel(image, limits, cancel_probe)
    }
}

impl ImageEncoder for Wml2Codec {
    fn encode(&self, request: EncodeRequest<'_>) -> CoreResult<Vec<u8>> {
        let mut image = decoded_to_wml2(request.image);
        catch_unwind(AssertUnwindSafe(|| {
            image_to(&mut image, encode_format(request.format), None)
        }))
        .map_err(|_| CoreError::encode("image encoder panicked"))?
        .map_err(|error| CoreError::encode(error.to_string()))
    }
}

pub fn decode(request: DecodeRequest<'_>) -> CoreResult<DecodedImage> {
    Wml2Codec.decode(request)
}
pub fn decode_with_limits(
    request: DecodeRequest<'_>,
    limits: DecodeLimits,
    cancel_probe: Option<DecodeCancelProbe<'_>>,
) -> CoreResult<DecodedImage> {
    Wml2Codec.decode_with_limits(request, limits, cancel_probe)
}
pub fn encode(request: EncodeRequest<'_>) -> CoreResult<Vec<u8>> {
    Wml2Codec.encode(request)
}

fn map_wml2_decode_error(error: Box<dyn std::error::Error>) -> CoreError {
    if error.downcast_ref::<DecodeCancelled>().is_some() {
        CoreError::cancelled(error.to_string())
    } else if is_wml2_limit_error(error.as_ref()) {
        CoreError::limit(error.to_string())
    } else {
        CoreError::decode(error.to_string())
    }
}

fn is_wml2_limit_error(error: &(dyn std::error::Error + 'static)) -> bool {
    let message = error.to_string();
    message.contains("exceeds decode limit") || message.contains("exceeded limit")
}

fn wml_decode_limits(limits: DecodeLimits) -> WmlDecodeLimits {
    WmlDecodeLimits {
        input_bytes: usize::MAX,
        pixels: usize::try_from(limits.maximum_frame_pixels).unwrap_or(usize::MAX),
        expanded_bytes: limits.maximum_rgba_bytes,
        metadata_bytes: usize::MAX,
        frames: limits.maximum_frames,
        animation_bytes: limits.maximum_rgba_bytes,
    }
}

#[derive(Debug)]
struct DecodeCancelled;

impl std::fmt::Display for DecodeCancelled {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("image decode cancelled")
    }
}

impl std::error::Error for DecodeCancelled {}

struct CancellableImageBuffer<'a> {
    image: ImageBuffer,
    cancel_probe: Option<DecodeCancelProbe<'a>>,
}

impl<'a> CancellableImageBuffer<'a> {
    fn new(cancel_probe: Option<DecodeCancelProbe<'a>>) -> Self {
        Self {
            image: ImageBuffer::new(),
            cancel_probe,
        }
    }

    fn into_inner(self) -> ImageBuffer {
        self.image
    }

    fn check_cancelled(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.cancel_probe.is_some_and(|probe| probe()) {
            Err(Box::new(DecodeCancelled))
        } else {
            Ok(())
        }
    }
}

impl DrawCallback for CancellableImageBuffer<'_> {
    fn init(
        &mut self,
        width: usize,
        height: usize,
        option: Option<InitOptions>,
    ) -> Result<Option<CallbackResponse>, Box<dyn std::error::Error>> {
        self.check_cancelled()?;
        self.image.init(width, height, option)
    }

    fn draw(
        &mut self,
        start_x: usize,
        start_y: usize,
        width: usize,
        height: usize,
        data: &[u8],
        option: Option<DrawOptions>,
    ) -> Result<Option<CallbackResponse>, Box<dyn std::error::Error>> {
        self.check_cancelled()?;
        self.image
            .draw(start_x, start_y, width, height, data, option)
    }

    fn terminate(
        &mut self,
        option: Option<TerminateOptions>,
    ) -> Result<Option<CallbackResponse>, Box<dyn std::error::Error>> {
        self.check_cancelled()?;
        self.image.terminate(option)
    }

    fn next(
        &mut self,
        option: Option<NextOptions>,
    ) -> Result<Option<CallbackResponse>, Box<dyn std::error::Error>> {
        self.check_cancelled()?;
        self.image.next(option)
    }

    fn verbose(
        &mut self,
        message: &str,
        option: Option<VerboseOptions>,
    ) -> Result<Option<CallbackResponse>, Box<dyn std::error::Error>> {
        self.check_cancelled()?;
        self.image.verbose(message, option)
    }

    fn set_metadata(
        &mut self,
        key: &str,
        value: DataMap,
    ) -> Result<Option<CallbackResponse>, Box<dyn std::error::Error>> {
        self.check_cancelled()?;
        self.image.set_metadata(key, value)
    }
}

fn check_cancelled(cancel_probe: Option<DecodeCancelProbe<'_>>) -> CoreResult<()> {
    if cancel_probe.is_some_and(|probe| probe()) {
        Err(CoreError::cancelled("image decode cancelled"))
    } else {
        Ok(())
    }
}

fn decoded_from_wml2(image: ImageBuffer) -> CoreResult<DecodedImage> {
    decoded_from_wml2_with_cancel(image, None)
}

fn decoded_from_wml2_with_cancel(
    image: ImageBuffer,
    cancel_probe: Option<DecodeCancelProbe<'_>>,
) -> CoreResult<DecodedImage> {
    decoded_from_wml2_with_limits_and_cancel(image, DecodeLimits::UNLIMITED, cancel_probe)
}

fn decoded_from_wml2_with_limits_and_cancel(
    mut image: ImageBuffer,
    limits: DecodeLimits,
    cancel_probe: Option<DecodeCancelProbe<'_>>,
) -> CoreResult<DecodedImage> {
    check_cancelled(cancel_probe)?;
    if image.width == 0 || image.height == 0 {
        return Err(CoreError::decode("decoded image has invalid dimensions"));
    }
    let rgba = image
        .buffer
        .take()
        .ok_or_else(|| CoreError::decode("decoded image buffer is missing"))?;
    let base = RgbaImage::new(image.width as u32, image.height as u32, rgba)
        .map_err(|error| CoreError::decode(error.to_string()))?;
    let layers = image.animation.take().unwrap_or_default();
    check_composited_animation_budget(&base, layers.len(), limits.maximum_rgba_bytes)?;
    let (base, animation) =
        compose_animation_frames(base, layers, image.background_color.as_ref(), cancel_probe)?;
    let poster = if let Some(frame) = animation.first() {
        frame.image.try_clone()?
    } else {
        base
    };
    check_cancelled(cancel_probe)?;
    Ok(DecodedImage {
        poster,
        animation,
        loop_count: image.loop_count,
    })
}

fn check_composited_animation_budget(
    base: &RgbaImage,
    frame_count: usize,
    maximum_rgba_bytes: usize,
) -> CoreResult<()> {
    let retained_canvases = frame_count
        .checked_add(1)
        .ok_or_else(|| CoreError::limit("composited animation frame count overflow"))?;
    let required_rgba_bytes = base
        .pixels()
        .len()
        .checked_mul(retained_canvases)
        .ok_or_else(|| CoreError::limit("composited animation byte count overflow"))?;
    if required_rgba_bytes > maximum_rgba_bytes {
        return Err(CoreError::limit(format!(
            "composited animation requires {required_rgba_bytes} RGBA bytes, limit is {maximum_rgba_bytes}"
        )));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn decoded_from_wml2_for_test(
    image: ImageBuffer,
    cancel_probe: Option<DecodeCancelProbe<'_>>,
) -> CoreResult<DecodedImage> {
    decoded_from_wml2_with_cancel(image, cancel_probe)
}

#[cfg(test)]
pub(crate) fn decoded_from_wml2_with_limits_for_test(
    image: ImageBuffer,
    limits: DecodeLimits,
    cancel_probe: Option<DecodeCancelProbe<'_>>,
) -> CoreResult<DecodedImage> {
    decoded_from_wml2_with_limits_and_cancel(image, limits, cancel_probe)
}

fn decoded_to_wml2(image: &DecodedImage) -> ImageBuffer {
    let mut output = ImageBuffer::from_buffer(
        image.poster.width() as usize,
        image.poster.height() as usize,
        image.poster.pixels().to_vec(),
    );
    if !image.animation.is_empty() {
        output.set_animation(true);
        output.loop_count = image.loop_count;
        for frame in &image.animation {
            output
                .animation
                .as_mut()
                .expect("animation was enabled")
                .push(WmlAnimationLayer {
                    width: frame.image.width() as usize,
                    height: frame.image.height() as usize,
                    start_x: 0,
                    start_y: 0,
                    buffer: frame.image.pixels().to_vec(),
                    control: wml2::draw::NextOptions::wait(frame.duration_ms),
                });
        }
    }
    output
}

fn encode_format(format: EncodeFormat) -> wml2::util::ImageFormat {
    match format {
        EncodeFormat::Png => wml2::util::ImageFormat::Png,
        EncodeFormat::Jpeg => wml2::util::ImageFormat::Jpeg,
        EncodeFormat::Bmp => wml2::util::ImageFormat::Bmp,
        EncodeFormat::Gif => wml2::util::ImageFormat::Gif,
        EncodeFormat::Webp => wml2::util::ImageFormat::Webp,
    }
}

fn compose_animation_frames(
    base: RgbaImage,
    layers: Vec<WmlAnimationLayer>,
    background: Option<&RGBA>,
    cancel_probe: Option<DecodeCancelProbe<'_>>,
) -> CoreResult<(RgbaImage, Vec<AnimationFrame>)> {
    if layers.is_empty() {
        return Ok((base, Vec::new()));
    }
    let background = background_rgba(background);
    let mut frames = Vec::new();
    frames
        .try_reserve_exact(layers.len())
        .map_err(|error| CoreError::limit(format!("animation list allocation failed: {error}")))?;
    let mut composited = base;
    for layer in layers {
        check_cancelled(cancel_probe)?;
        let previous = if matches!(layer.control.dispose_option, Some(NextDispose::Previous)) {
            Some(composited.try_clone()?)
        } else {
            None
        };
        apply_layer(&mut composited, &layer, cancel_probe)?;
        let frame = composited.try_clone()?;
        frames.push(AnimationFrame {
            image: frame,
            duration_ms: layer.control.await_time,
        });
        match layer.control.dispose_option {
            Some(NextDispose::Background) => {
                clear_rect(&mut composited, &layer, background, cancel_probe)?
            }
            Some(NextDispose::Previous) => {
                composited = previous
                    .ok_or_else(|| CoreError::decode("animation previous canvas is missing"))?;
            }
            _ => {}
        }
        check_cancelled(cancel_probe)?;
    }
    Ok((composited, frames))
}

fn apply_layer(
    image: &mut RgbaImage,
    layer: &WmlAnimationLayer,
    cancel_probe: Option<DecodeCancelProbe<'_>>,
) -> CoreResult<()> {
    let dest_width = image.width as usize;
    let dest_height = image.height as usize;
    let alpha_blend = matches!(layer.control.blend, Some(NextBlend::Source));
    for y in 0..layer.height {
        if y % 64 == 0 {
            check_cancelled(cancel_probe)?;
        }
        let dest_y = layer.start_y + y as i32;
        if dest_y < 0 || dest_y >= dest_height as i32 {
            continue;
        }
        for x in 0..layer.width {
            let dest_x = layer.start_x + x as i32;
            if dest_x < 0 || dest_x >= dest_width as i32 {
                continue;
            }
            let source_offset = (y * layer.width + x) * 4;
            if source_offset + 4 > layer.buffer.len() {
                continue;
            }
            let destination_offset = ((dest_y as usize * dest_width) + dest_x as usize) * 4;
            let source = [
                layer.buffer[source_offset],
                layer.buffer[source_offset + 1],
                layer.buffer[source_offset + 2],
                layer.buffer[source_offset + 3],
            ];
            if alpha_blend {
                let destination = [
                    image.pixels[destination_offset],
                    image.pixels[destination_offset + 1],
                    image.pixels[destination_offset + 2],
                    image.pixels[destination_offset + 3],
                ];
                image.pixels[destination_offset..destination_offset + 4]
                    .copy_from_slice(&blend_rgba(source, destination));
            } else {
                image.pixels[destination_offset..destination_offset + 4].copy_from_slice(&source);
            }
        }
    }
    Ok(())
}

fn clear_rect(
    image: &mut RgbaImage,
    layer: &WmlAnimationLayer,
    background: [u8; 4],
    cancel_probe: Option<DecodeCancelProbe<'_>>,
) -> CoreResult<()> {
    let dest_width = image.width as usize;
    let dest_height = image.height as usize;
    for y in 0..layer.height {
        if y % 64 == 0 {
            check_cancelled(cancel_probe)?;
        }
        let dest_y = layer.start_y + y as i32;
        if dest_y < 0 || dest_y >= dest_height as i32 {
            continue;
        }
        for x in 0..layer.width {
            let dest_x = layer.start_x + x as i32;
            if dest_x < 0 || dest_x >= dest_width as i32 {
                continue;
            }
            let offset = ((dest_y as usize * dest_width) + dest_x as usize) * 4;
            image.pixels[offset..offset + 4].copy_from_slice(&background);
        }
    }
    Ok(())
}

fn background_rgba(background: Option<&RGBA>) -> [u8; 4] {
    background
        .map(|color| [color.red, color.green, color.blue, color.alpha])
        .unwrap_or([0, 0, 0, 0])
}

fn blend_rgba(source: [u8; 4], destination: [u8; 4]) -> [u8; 4] {
    let source_alpha = source[3] as f32 / 255.0;
    let destination_alpha = destination[3] as f32 / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    if output_alpha <= f32::EPSILON {
        return [0, 0, 0, 0];
    }
    let mut output = [0_u8; 4];
    for channel in 0..3 {
        let source_value = source[channel] as f32 / 255.0;
        let destination_value = destination[channel] as f32 / 255.0;
        let value = (source_value * source_alpha
            + destination_value * destination_alpha * (1.0 - source_alpha))
            / output_alpha;
        output[channel] = (value * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    output[3] = (output_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    output
}
