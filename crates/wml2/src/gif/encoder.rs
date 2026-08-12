//! GIF encoder implementation.

use crate::color::RGBA;
use crate::draw::{
    ENCODE_ANIMATION_FRAMES_KEY, ENCODE_ANIMATION_LOOP_COUNT_KEY,
    EncodeOptions as DrawEncodeOptions, ImageProfiles, encode_animation_frame_key,
};
use crate::encoder::lzw::encode_gif;
use crate::error::{ImgError, ImgErrorKind};
use crate::metadata::DataMap;
use bin_rs::io::{write_bytes, write_u16_le};
use std::collections::HashMap;

type Error = Box<dyn std::error::Error>;

const TRANSPARENT_INDEX: u8 = 0;

#[derive(Debug)]
struct AnimationFrame {
    width: usize,
    height: usize,
    x_offset: usize,
    y_offset: usize,
    delay_ms: u64,
    blend: bool,
    dispose: u8,
    buffer: Vec<u8>,
}

#[derive(Debug)]
struct AnimationInfo {
    background: RGBA,
    loop_count: u32,
    frames: Vec<AnimationFrame>,
}

struct QuantizedFrame {
    palette: Vec<[u8; 3]>,
    indices: Vec<u8>,
    transparent_index: Option<u8>,
    table_len: usize,
    min_code_size: usize,
}

#[derive(Clone, Copy, Default)]
struct HistogramBin {
    count: u32,
    sum_r: u64,
    sum_g: u64,
    sum_b: u64,
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
        Some(DataMap::UInt(loop_count)) => u32::try_from(*loop_count).map_err(|_| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "animation loop_count must fit in u32".to_string(),
            )) as Error
        })?,
        Some(DataMap::SInt(loop_count)) if *loop_count >= 0 => {
            u32::try_from(*loop_count).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    "animation loop_count must fit in u32".to_string(),
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
        let delay_ms = as_u64(metadata.get(&delay_key), &delay_key)?;
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
        background,
        loop_count,
        frames,
    }))
}

fn fill_canvas(width: usize, height: usize, background: &RGBA) -> Vec<u8> {
    let mut canvas = Vec::with_capacity(width * height * 4);
    for _ in 0..(width * height) {
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

fn compose_animation_frames(
    profile: &ImageProfiles,
    animation: &AnimationInfo,
) -> Result<Vec<(Vec<u8>, u64)>, Error> {
    let mut canvas = fill_canvas(profile.width, profile.height, &animation.background);
    let mut frames = Vec::with_capacity(animation.frames.len());
    for frame in &animation.frames {
        let previous = matches!(frame.dispose, 2).then(|| canvas.clone());
        apply_animation_frame(&mut canvas, profile.width, frame);
        frames.push((canvas.clone(), frame.delay_ms));

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
    Ok(frames)
}

fn composite_rgb(pixel: &[u8], background: &RGBA) -> [u8; 3] {
    if pixel[3] == 255 {
        return [pixel[0], pixel[1], pixel[2]];
    }
    let alpha = pixel[3] as u32;
    let inv_alpha = 255 - alpha;
    [
        ((pixel[0] as u32 * alpha + background.red as u32 * inv_alpha + 127) / 255) as u8,
        ((pixel[1] as u32 * alpha + background.green as u32 * inv_alpha + 127) / 255) as u8,
        ((pixel[2] as u32 * alpha + background.blue as u32 * inv_alpha + 127) / 255) as u8,
    ]
}

fn gif_table_len(color_count: usize) -> usize {
    color_count.max(2).next_power_of_two().min(256)
}

fn gif_min_code_size(table_len: usize) -> usize {
    table_len.trailing_zeros() as usize
}

fn nearest_palette_index(color: [u8; 3], palette: &[[u8; 3]], start_index: usize) -> usize {
    let mut best_index = start_index;
    let mut best_distance = u32::MAX;
    for (index, entry) in palette.iter().enumerate().skip(start_index) {
        let dr = color[0] as i32 - entry[0] as i32;
        let dg = color[1] as i32 - entry[1] as i32;
        let db = color[2] as i32 - entry[2] as i32;
        let distance = (dr * dr + dg * dg + db * db) as u32;
        if distance < best_distance {
            best_distance = distance;
            best_index = index;
        }
    }
    best_index
}

fn exact_palette_quantization(
    rgba: &[u8],
    width: usize,
    height: usize,
    background: &RGBA,
) -> Option<QuantizedFrame> {
    let has_transparency = rgba.chunks_exact(4).any(|pixel| pixel[3] == 0);
    let palette_limit = if has_transparency { 255 } else { 256 };
    let mut palette = Vec::with_capacity(palette_limit + has_transparency as usize);
    let mut lookup = HashMap::new();
    let mut indices = Vec::with_capacity(width * height);

    if has_transparency {
        palette.push([0, 0, 0]);
    }

    for pixel in rgba.chunks_exact(4) {
        if pixel[3] == 0 {
            indices.push(TRANSPARENT_INDEX);
            continue;
        }

        let rgb = composite_rgb(pixel, background);
        let key = ((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32;
        let index = if let Some(index) = lookup.get(&key) {
            *index
        } else {
            if palette.len() >= palette_limit + has_transparency as usize {
                return None;
            }
            let index = palette.len() as u8;
            palette.push(rgb);
            lookup.insert(key, index);
            index
        };
        indices.push(index);
    }

    if palette.is_empty() {
        palette.push([0, 0, 0]);
    }
    let table_len = gif_table_len(palette.len());
    Some(QuantizedFrame {
        palette,
        indices,
        transparent_index: has_transparency.then_some(TRANSPARENT_INDEX),
        table_len,
        min_code_size: gif_min_code_size(table_len).max(2),
    })
}

fn histogram_palette(rgba: &[u8], background: &RGBA, has_transparency: bool) -> Vec<[u8; 3]> {
    let palette_limit = if has_transparency { 255 } else { 256 };
    let mut bins = vec![HistogramBin::default(); 32 * 32 * 32];
    for pixel in rgba.chunks_exact(4) {
        if pixel[3] == 0 {
            continue;
        }
        let rgb = composite_rgb(pixel, background);
        let index =
            ((rgb[0] as usize >> 3) << 10) | ((rgb[1] as usize >> 3) << 5) | (rgb[2] as usize >> 3);
        let bin = &mut bins[index];
        bin.count += 1;
        bin.sum_r += rgb[0] as u64;
        bin.sum_g += rgb[1] as u64;
        bin.sum_b += rgb[2] as u64;
    }

    let mut entries: Vec<_> = bins.into_iter().filter(|bin| bin.count > 0).collect();
    entries.sort_by(|left, right| right.count.cmp(&left.count));

    let mut palette = Vec::with_capacity(palette_limit + has_transparency as usize);
    if has_transparency {
        palette.push([0, 0, 0]);
    }

    for bin in entries.into_iter().take(palette_limit) {
        let count = bin.count as u64;
        palette.push([
            (bin.sum_r / count) as u8,
            (bin.sum_g / count) as u8,
            (bin.sum_b / count) as u8,
        ]);
    }

    if palette.is_empty() {
        palette.push([0, 0, 0]);
    }
    palette
}

fn dithered_palette_quantization(
    rgba: &[u8],
    width: usize,
    height: usize,
    background: &RGBA,
) -> QuantizedFrame {
    let has_transparency = rgba.chunks_exact(4).any(|pixel| pixel[3] == 0);
    let palette = histogram_palette(rgba, background, has_transparency);
    let start_index = has_transparency as usize;
    let table_len = gif_table_len(palette.len());
    let mut indices = vec![0_u8; width * height];
    let mut current_errors = vec![[0_i32; 3]; width + 2];
    let mut next_errors = vec![[0_i32; 3]; width + 2];

    for y in 0..height {
        for x in 0..width {
            let offset = (y * width + x) * 4;
            let pixel = &rgba[offset..offset + 4];
            if pixel[3] == 0 {
                indices[y * width + x] = TRANSPARENT_INDEX;
                continue;
            }

            let mut rgb = composite_rgb(pixel, background);
            for channel in 0..3 {
                rgb[channel] = (rgb[channel] as i32 + (current_errors[x + 1][channel] + 8) / 16)
                    .clamp(0, 255) as u8;
            }

            let palette_index = nearest_palette_index(rgb, &palette, start_index);
            let palette_color = palette[palette_index];
            indices[y * width + x] = palette_index as u8;

            let error = [
                rgb[0] as i32 - palette_color[0] as i32,
                rgb[1] as i32 - palette_color[1] as i32,
                rgb[2] as i32 - palette_color[2] as i32,
            ];

            for channel in 0..3 {
                current_errors[x + 2][channel] += error[channel] * 7;
                next_errors[x][channel] += error[channel] * 3;
                next_errors[x + 1][channel] += error[channel] * 5;
                next_errors[x + 2][channel] += error[channel];
            }
        }

        current_errors.fill([0, 0, 0]);
        std::mem::swap(&mut current_errors, &mut next_errors);
    }

    QuantizedFrame {
        palette,
        indices,
        transparent_index: has_transparency.then_some(TRANSPARENT_INDEX),
        table_len,
        min_code_size: gif_min_code_size(table_len).max(2),
    }
}

fn quantize_frame(
    rgba: &[u8],
    width: usize,
    height: usize,
    background: &RGBA,
) -> Result<QuantizedFrame, Error> {
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "GIF image dimensions overflow".to_string(),
            )) as Error
        })?;
    if rgba.len() != expected_len {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "GIF RGBA buffer size mismatch".to_string(),
        )));
    }

    if let Some(frame) = exact_palette_quantization(rgba, width, height, background) {
        return Ok(frame);
    }
    Ok(dithered_palette_quantization(
        rgba, width, height, background,
    ))
}

fn write_sub_blocks(buf: &mut Vec<u8>, data: &[u8]) {
    for chunk in data.chunks(255) {
        buf.push(chunk.len() as u8);
        write_bytes(chunk, buf);
    }
    buf.push(0);
}

fn write_graphic_control_extension(
    buf: &mut Vec<u8>,
    delay_ms: u64,
    transparent_index: Option<u8>,
) -> Result<(), Error> {
    let delay_cs = if delay_ms == 0 {
        0
    } else {
        ((delay_ms + 5) / 10).clamp(1, u16::MAX as u64)
    };

    buf.push(0x21);
    buf.push(0xf9);
    buf.push(0x04);
    let packed = if transparent_index.is_some() {
        0x01
    } else {
        0x00
    };
    buf.push(packed);
    write_u16_le(delay_cs as u16, buf);
    buf.push(transparent_index.unwrap_or(0));
    buf.push(0);
    Ok(())
}

fn write_netscape_loop_extension(buf: &mut Vec<u8>, loop_count: u32) -> Result<(), Error> {
    let loop_count = u16::try_from(loop_count).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "GIF loop_count must fit in u16".to_string(),
        )) as Error
    })?;

    buf.push(0x21);
    buf.push(0xff);
    buf.push(0x0b);
    write_bytes(b"NETSCAPE2.0", buf);
    buf.push(0x03);
    buf.push(0x01);
    write_u16_le(loop_count, buf);
    buf.push(0);
    Ok(())
}

fn write_image_descriptor(
    buf: &mut Vec<u8>,
    width: usize,
    height: usize,
    table_len: usize,
) -> Result<(), Error> {
    let width = u16::try_from(width).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "GIF width exceeds u16".to_string(),
        )) as Error
    })?;
    let height = u16::try_from(height).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "GIF height exceeds u16".to_string(),
        )) as Error
    })?;
    let size_code = gif_min_code_size(table_len).saturating_sub(1);

    buf.push(0x2c);
    write_u16_le(0, buf);
    write_u16_le(0, buf);
    write_u16_le(width, buf);
    write_u16_le(height, buf);
    buf.push(0x80 | (size_code as u8 & 0x07));
    Ok(())
}

fn write_local_color_table(buf: &mut Vec<u8>, quantized: &QuantizedFrame) {
    for color in &quantized.palette {
        buf.push(color[0]);
        buf.push(color[1]);
        buf.push(color[2]);
    }
    for _ in quantized.palette.len()..quantized.table_len {
        buf.extend_from_slice(&[0, 0, 0]);
    }
}

fn encode_frame(
    buf: &mut Vec<u8>,
    rgba: &[u8],
    width: usize,
    height: usize,
    background: &RGBA,
    delay_ms: u64,
) -> Result<(), Error> {
    let quantized = quantize_frame(rgba, width, height, background)?;
    write_graphic_control_extension(buf, delay_ms, quantized.transparent_index)?;
    write_image_descriptor(buf, width, height, quantized.table_len)?;
    write_local_color_table(buf, &quantized);
    buf.push(quantized.min_code_size as u8);
    let lzw = encode_gif(&quantized.indices, quantized.min_code_size)?;
    write_sub_blocks(buf, &lzw);
    Ok(())
}

fn encode_still(
    width: usize,
    height: usize,
    rgba: &[u8],
    background: &RGBA,
) -> Result<Vec<u8>, Error> {
    let width = u16::try_from(width).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "GIF width exceeds u16".to_string(),
        )) as Error
    })?;
    let height = u16::try_from(height).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "GIF height exceeds u16".to_string(),
        )) as Error
    })?;
    let mut data = Vec::new();
    write_bytes(b"GIF89a", &mut data);
    write_u16_le(width, &mut data);
    write_u16_le(height, &mut data);
    data.extend_from_slice(&[0x00, 0x00, 0x00]);
    encode_frame(
        &mut data,
        rgba,
        width as usize,
        height as usize,
        background,
        0,
    )?;
    data.push(0x3b);
    Ok(data)
}

fn encode_animation(profile: &ImageProfiles, animation: AnimationInfo) -> Result<Vec<u8>, Error> {
    let mut data = Vec::new();
    write_bytes(b"GIF89a", &mut data);
    let width = u16::try_from(profile.width).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "GIF width exceeds u16".to_string(),
        )) as Error
    })?;
    let height = u16::try_from(profile.height).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "GIF height exceeds u16".to_string(),
        )) as Error
    })?;
    write_u16_le(width, &mut data);
    write_u16_le(height, &mut data);
    data.extend_from_slice(&[0x00, 0x00, 0x00]);

    write_netscape_loop_extension(&mut data, animation.loop_count)?;
    for (canvas, delay_ms) in compose_animation_frames(profile, &animation)? {
        encode_frame(
            &mut data,
            &canvas,
            profile.width,
            profile.height,
            &animation.background,
            delay_ms,
        )?;
    }
    data.push(0x3b);
    Ok(data)
}

/// Encodes an image source to GIF89a.
///
/// Palette generation uses a two-pass histogram palette and error diffusion
/// when the frame exceeds 256 colors. Frames with 256 colors or fewer are kept
/// exact without dithering. Animated input is flattened to full-canvas GIF
/// frames with per-frame local palettes.
pub fn encode(image: &mut DrawEncodeOptions<'_>) -> Result<Vec<u8>, Error> {
    let profile = image.drawer.encode_start(None)?;
    let profile = profile.ok_or_else(|| {
        Box::new(ImgError::new_const(
            ImgErrorKind::OutboundIndex,
            "Image profiles nothing".to_string(),
        )) as Error
    })?;
    let background = profile.background.clone().unwrap_or(RGBA {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });

    let data = if let Some(animation) = parse_animation_info(&profile)? {
        encode_animation(&profile, animation)?
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
        encode_still(profile.width, profile.height, &rgba, &background)?
    };

    image.drawer.encode_end(None)?;
    Ok(data)
}
