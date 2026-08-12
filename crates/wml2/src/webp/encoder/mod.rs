//! Pure Rust WebP encoder helpers.
//!
//! Current scope covers still-image and animation-aware WebP encode.
//! The lossless path targets `VP8L` with transforms, adaptive Huffman
//! coding, simple backward references, and an optional color cache.
//! The lossy path targets opaque still images and emits a minimal
//! intra-only `VP8` bitstream.
//!
//! When used through [`crate::draw::image_encoder`] or [`crate::draw::convert`],
//! the built-in adapter reads `quality` and `optimize` from
//! [`crate::draw::EncodeOptions::options`]. Setting `quality` selects lossy
//! WebP; omitting it keeps the default lossless path. Animated input is encoded
//! as animated WebP by consuming the reserved animation metadata produced by
//! [`crate::draw::ImageBuffer`].

mod bit_writer;
mod container;
mod error;
mod huffman;
mod lossless;
mod lossy;
mod vp8_bool_writer;
mod writer;

use crate::color::RGBA;
use crate::draw::{
    ENCODE_ANIMATION_FRAMES_KEY, ENCODE_ANIMATION_LOOP_COUNT_KEY,
    EncodeOptions as DrawEncodeOptions, ImageProfiles, encode_animation_frame_key,
};
use crate::error::{ImgError, ImgErrorKind};
use crate::metadata::{DataMap, get_exif_option};

use self::container::{AnimationFrameChunk, wrap_animated_webp};
pub use error::EncoderError;
pub use lossless::{
    LosslessEncodingOptions, encode_lossless_image_to_webp,
    encode_lossless_image_to_webp_with_options,
    encode_lossless_image_to_webp_with_options_and_exif, encode_lossless_rgba_to_vp8l,
    encode_lossless_rgba_to_vp8l_with_options, encode_lossless_rgba_to_webp,
    encode_lossless_rgba_to_webp_with_options, encode_lossless_rgba_to_webp_with_options_and_exif,
};
pub use lossy::{
    LossyEncodingOptions, encode_lossy_image_to_webp, encode_lossy_image_to_webp_with_options,
    encode_lossy_image_to_webp_with_options_and_exif, encode_lossy_rgba_to_vp8,
    encode_lossy_rgba_to_vp8_with_options, encode_lossy_rgba_to_webp,
    encode_lossy_rgba_to_webp_with_options, encode_lossy_rgba_to_webp_with_options_and_exif,
};

type Error = Box<dyn std::error::Error>;

#[derive(Debug)]
struct AnimationFrame {
    width: usize,
    height: usize,
    x_offset: usize,
    y_offset: usize,
    delay_ms: usize,
    blend: bool,
    dispose: u8,
    buffer: Vec<u8>,
}

#[derive(Debug)]
struct AnimationInfo {
    background: RGBA,
    background_color: u32,
    loop_count: u16,
    frames: Vec<AnimationFrame>,
}

fn map_error(error: EncoderError) -> Error {
    let kind = match error {
        EncoderError::InvalidParam(_) => ImgErrorKind::InvalidParameter,
        EncoderError::Bitstream(_) => ImgErrorKind::EncodeError,
    };
    Box::new(ImgError::new_const(kind, error.to_string()))
}

fn rgba_to_argb(color: &RGBA) -> u32 {
    ((color.alpha as u32) << 24)
        | ((color.red as u32) << 16)
        | ((color.green as u32) << 8)
        | (color.blue as u32)
}

fn as_u64(value: Option<&DataMap>, key: &str) -> Result<u64, Error> {
    match value {
        Some(DataMap::UInt(value)) => Ok(*value),
        Some(DataMap::SInt(value)) if *value >= 0 => Ok(*value as u64),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            format!("{key} is not an unsigned integer"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} metadata not found"),
        ))),
    }
}

fn as_i64(value: Option<&DataMap>, key: &str) -> Result<i64, Error> {
    match value {
        Some(DataMap::SInt(value)) => Ok(*value),
        Some(DataMap::UInt(value)) => i64::try_from(*value).map_err(|_| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("{key} is too large"),
            )) as Error
        }),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            format!("{key} is not an integer"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} metadata not found"),
        ))),
    }
}

fn as_raw(value: Option<&DataMap>, key: &str) -> Result<Vec<u8>, Error> {
    match value {
        Some(DataMap::Raw(value)) => Ok(value.clone()),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            format!("{key} is not raw metadata"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} metadata not found"),
        ))),
    }
}

fn option_u8(option: &DrawEncodeOptions<'_>, key: &str) -> Result<Option<u8>, Error> {
    let Some(value) = option.options.as_ref().and_then(|map| map.get(key)) else {
        return Ok(None);
    };

    match value {
        DataMap::UInt(value) => u8::try_from(*value).map(Some).map_err(|_| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("{key} must fit in u8"),
            )) as Error
        }),
        DataMap::SInt(value) if *value >= 0 => u8::try_from(*value).map(Some).map_err(|_| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("{key} must fit in u8"),
            )) as Error
        }),
        _ => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            format!("{key} must be an integer"),
        ))),
    }
}

fn webp_quality(option: &DrawEncodeOptions<'_>) -> Result<Option<u8>, Error> {
    let quality = option_u8(option, "quality")?;
    if quality.is_some_and(|quality| quality > 100) {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "quality must be in 0..=100".to_string(),
        )));
    }
    Ok(quality)
}

fn webp_optimize(option: &DrawEncodeOptions<'_>) -> Result<Option<u8>, Error> {
    let optimize = option_u8(option, "optimize")?;
    if optimize.is_some_and(|optimize| optimize > 9) {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "optimize must be in 0..=9".to_string(),
        )));
    }
    Ok(optimize)
}

fn parse_animation_info(profile: &ImageProfiles) -> Result<Option<AnimationInfo>, Error> {
    let Some(metadata) = &profile.metadata else {
        return Ok(None);
    };
    let Some(DataMap::UInt(frame_count)) = metadata.get(ENCODE_ANIMATION_FRAMES_KEY) else {
        return Ok(None);
    };
    if *frame_count == 0 {
        return Ok(None);
    }

    let loop_count = match metadata.get(ENCODE_ANIMATION_LOOP_COUNT_KEY) {
        Some(DataMap::UInt(loop_count)) => u16::try_from(*loop_count).map_err(|_| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "animation loop_count must fit in u16".to_string(),
            )) as Error
        })?,
        Some(DataMap::SInt(loop_count)) if *loop_count >= 0 => {
            u16::try_from(*loop_count).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    "animation loop_count must fit in u16".to_string(),
                )) as Error
            })?
        }
        Some(_) => {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "wml2.animation.loop_count is not an integer".to_string(),
            )));
        }
        None => 0,
    };

    let background = profile.background.clone().unwrap_or(RGBA {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });

    let mut frames = Vec::with_capacity(*frame_count as usize);
    for index in 0..*frame_count as usize {
        let width_key = encode_animation_frame_key(index, "width");
        let height_key = encode_animation_frame_key(index, "height");
        let start_x_key = encode_animation_frame_key(index, "start_x");
        let start_y_key = encode_animation_frame_key(index, "start_y");
        let delay_key = encode_animation_frame_key(index, "delay_ms");
        let dispose_key = encode_animation_frame_key(index, "dispose");
        let blend_key = encode_animation_frame_key(index, "blend");
        let buffer_key = encode_animation_frame_key(index, "buffer");

        let width =
            usize::try_from(as_u64(metadata.get(&width_key), &width_key)?).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    format!("{width_key} is too large"),
                )) as Error
            })?;
        let height =
            usize::try_from(as_u64(metadata.get(&height_key), &height_key)?).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    format!("{height_key} is too large"),
                )) as Error
            })?;
        let x_offset = as_i64(metadata.get(&start_x_key), &start_x_key)?;
        let y_offset = as_i64(metadata.get(&start_y_key), &start_y_key)?;
        let delay_ms =
            usize::try_from(as_u64(metadata.get(&delay_key), &delay_key)?).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    format!("{delay_key} is too large"),
                )) as Error
            })?;
        let dispose =
            u8::try_from(as_u64(metadata.get(&dispose_key), &dispose_key)?).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    format!("{dispose_key} is too large"),
                )) as Error
            })?;
        let blend = as_u64(metadata.get(&blend_key), &blend_key)? != 0;
        let buffer = as_raw(metadata.get(&buffer_key), &buffer_key)?;

        if width == 0 || height == 0 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} has zero size"),
            )));
        }
        if x_offset < 0 || y_offset < 0 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} has negative offset"),
            )));
        }
        let x_offset = x_offset as usize;
        let y_offset = y_offset as usize;
        let end_x = x_offset.checked_add(width).ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} x range overflows"),
            )) as Error
        })?;
        let end_y = y_offset.checked_add(height).ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} y range overflows"),
            )) as Error
        })?;
        if end_x > profile.width || end_y > profile.height {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} exceeds the canvas"),
            )));
        }
        let expected_len = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    format!("animation frame {index} buffer size overflows"),
                )) as Error
            })?;
        if buffer.len() != expected_len {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} buffer size mismatch"),
            )));
        }

        frames.push(AnimationFrame {
            width,
            height,
            x_offset,
            y_offset,
            delay_ms,
            blend,
            dispose,
            buffer,
        });
    }

    Ok(Some(AnimationInfo {
        background_color: rgba_to_argb(&background),
        background,
        loop_count,
        frames,
    }))
}

fn fill_canvas(width: usize, height: usize, background: &RGBA) -> Vec<u8> {
    let pixel_count = width
        .checked_mul(height)
        .expect("validated WebP canvas dimensions should not overflow");
    let mut canvas = Vec::with_capacity(
        pixel_count
            .checked_mul(4)
            .expect("validated WebP canvas dimensions should not overflow"),
    );
    for _ in 0..pixel_count {
        canvas.push(background.red);
        canvas.push(background.green);
        canvas.push(background.blue);
        canvas.push(background.alpha);
    }
    canvas
}

fn source_over(dst: &mut [u8], src: &[u8]) {
    let src_alpha = src[3] as u32;
    if src_alpha == 0 {
        return;
    }
    if src_alpha == 255 {
        dst.copy_from_slice(src);
        return;
    }

    let dst_alpha = dst[3] as u32;
    let out_alpha = src_alpha + ((dst_alpha * (255 - src_alpha) + 127) / 255);
    if out_alpha == 0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }

    for channel in 0..3 {
        let src_premul = src[channel] as u32 * src_alpha;
        let dst_premul = dst[channel] as u32 * dst_alpha;
        let out_premul = src_premul + ((dst_premul * (255 - src_alpha) + 127) / 255);
        dst[channel] = ((out_premul * 255 + (out_alpha / 2)) / out_alpha) as u8;
    }
    dst[3] = out_alpha as u8;
}

fn apply_animation_frame(canvas: &mut [u8], canvas_width: usize, frame: &AnimationFrame) {
    for y in 0..frame.height {
        let src_row = y * frame.width * 4;
        let dst_row = (frame.y_offset + y) * canvas_width * 4;
        for x in 0..frame.width {
            let src_offset = src_row + x * 4;
            let dst_offset = dst_row + (frame.x_offset + x) * 4;
            if frame.blend {
                source_over(
                    &mut canvas[dst_offset..dst_offset + 4],
                    &frame.buffer[src_offset..src_offset + 4],
                );
            } else {
                canvas[dst_offset..dst_offset + 4]
                    .copy_from_slice(&frame.buffer[src_offset..src_offset + 4]);
            }
        }
    }
}

fn clear_animation_frame(
    canvas: &mut [u8],
    canvas_width: usize,
    frame: &AnimationFrame,
    background: &RGBA,
) {
    for y in 0..frame.height {
        let dst_row = (frame.y_offset + y) * canvas_width * 4;
        for x in 0..frame.width {
            let dst_offset = dst_row + (frame.x_offset + x) * 4;
            canvas[dst_offset] = background.red;
            canvas[dst_offset + 1] = background.green;
            canvas[dst_offset + 2] = background.blue;
            canvas[dst_offset + 3] = background.alpha;
        }
    }
}

fn rgba_has_alpha(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).any(|pixel| pixel[3] != 0xff)
}

fn encode_lossless_frame(
    width: usize,
    height: usize,
    rgba: &[u8],
    optimize: Option<u8>,
) -> Result<Vec<u8>, Error> {
    let mut options = LosslessEncodingOptions::default();
    if let Some(optimize) = optimize {
        options.optimization_level = optimize;
    }
    encode_lossless_rgba_to_vp8l_with_options(width, height, rgba, &options).map_err(map_error)
}

fn encode_lossy_frame(
    width: usize,
    height: usize,
    rgba: &[u8],
    quality: u8,
    optimize: Option<u8>,
) -> Result<Vec<u8>, Error> {
    let mut options = LossyEncodingOptions::default();
    options.quality = quality;
    if let Some(optimize) = optimize {
        options.optimization_level = optimize;
    }
    encode_lossy_rgba_to_vp8_with_options(width, height, rgba, &options).map_err(map_error)
}

fn encode_still(
    width: usize,
    height: usize,
    rgba: &[u8],
    quality: Option<u8>,
    optimize: Option<u8>,
    exif: Option<&[u8]>,
) -> Result<Vec<u8>, Error> {
    if let Some(quality) = quality {
        let mut options = LossyEncodingOptions::default();
        options.quality = quality;
        if let Some(optimize) = optimize {
            options.optimization_level = optimize;
        }
        encode_lossy_rgba_to_webp_with_options_and_exif(width, height, rgba, &options, exif)
            .map_err(map_error)
    } else {
        let mut options = LosslessEncodingOptions::default();
        if let Some(optimize) = optimize {
            options.optimization_level = optimize;
        }
        encode_lossless_rgba_to_webp_with_options_and_exif(width, height, rgba, &options, exif)
            .map_err(map_error)
    }
}

fn encode_animation(
    profile: &ImageProfiles,
    animation: AnimationInfo,
    quality: Option<u8>,
    optimize: Option<u8>,
    exif: Option<&[u8]>,
) -> Result<Vec<u8>, Error> {
    let mut canvas = fill_canvas(profile.width, profile.height, &animation.background);
    let mut has_alpha = canvas.chunks_exact(4).any(|pixel| pixel[3] != 0xff);
    let mut encoded_frames = Vec::with_capacity(animation.frames.len());

    for frame in &animation.frames {
        let previous = matches!(frame.dispose, 2).then(|| canvas.clone());
        apply_animation_frame(&mut canvas, profile.width, frame);

        if rgba_has_alpha(&canvas) {
            has_alpha = true;
        }

        let (fourcc, payload) = if let Some(quality) = quality {
            if rgba_has_alpha(&canvas) {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    "lossy WebP encoder does not support alpha".to_string(),
                )));
            }
            (
                *b"VP8 ",
                encode_lossy_frame(profile.width, profile.height, &canvas, quality, optimize)?,
            )
        } else {
            (
                *b"VP8L",
                encode_lossless_frame(profile.width, profile.height, &canvas, optimize)?,
            )
        };
        encoded_frames.push(AnimationFrameChunk {
            fourcc,
            payload,
            duration_ms: frame.delay_ms,
            blend: false,
            dispose_to_background: false,
        });

        match frame.dispose {
            1 => clear_animation_frame(&mut canvas, profile.width, frame, &animation.background),
            2 => {
                canvas = previous.ok_or_else(|| {
                    Box::new(ImgError::new_const(
                        ImgErrorKind::EncodeError,
                        "missing previous canvas for dispose=previous".to_string(),
                    )) as Error
                })?;
            }
            _ => {}
        }
    }

    wrap_animated_webp(
        profile.width,
        profile.height,
        animation.background_color,
        animation.loop_count,
        has_alpha,
        &encoded_frames,
        exif,
    )
    .map_err(map_error)
}

/// Encodes an image source to still or animated WebP.
///
/// Supported `EncodeOptions.options` keys:
/// - `quality`: lossy quality in `0..=100`
/// - `optimize`: search effort in `0..=9`
/// - `exif`: `Raw(bytes)`, `Exif(headers)`, or `Ascii("copy")`
///
/// If animation metadata is present, this emits animated WebP; otherwise it
/// encodes a still image.
pub fn encode(image: &mut DrawEncodeOptions<'_>) -> Result<Vec<u8>, Error> {
    let profile = image.drawer.encode_start(None)?;
    let profile = profile.ok_or_else(|| {
        Box::new(ImgError::new_const(
            ImgErrorKind::OutboundIndex,
            "Image profiles nothing".to_string(),
        )) as Error
    })?;
    let quality = webp_quality(image)?;
    let optimize = webp_optimize(image)?;
    let exif = get_exif_option(image.options.as_ref(), profile.metadata.as_ref())?;

    let data = if let Some(animation) = parse_animation_info(&profile)? {
        encode_animation(&profile, animation, quality, optimize, exif.as_deref())?
    } else {
        let rgba = image
            .drawer
            .encode_pick(0, 0, profile.width, profile.height, None)?
            .ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::EncodeError,
                    "Image buffer nothing".to_string(),
                )) as Error
            })?;
        encode_still(
            profile.width,
            profile.height,
            &rgba,
            quality,
            optimize,
            exif.as_deref(),
        )?
    };

    image.drawer.encode_end(None)?;
    Ok(data)
}
