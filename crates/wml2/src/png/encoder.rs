//! PNG and APNG encoder implementation.

use crate::draw::{
    ENCODE_ANIMATION_FRAMES_KEY, ENCODE_ANIMATION_LOOP_COUNT_KEY, EncodeOptions, ImageProfiles,
    encode_animation_frame_key,
};
use crate::error::*;
use crate::metadata::{DataMap, get_exif_option};
use crate::png::header::*;
use crate::png::utils::*;
use bin_rs::io::*;
type Error = Box<dyn std::error::Error>;

struct ApngFrame {
    width: u32,
    height: u32,
    x_offset: u32,
    y_offset: u32,
    delay_num: u16,
    delay_den: u16,
    dispose_op: u8,
    blend_op: u8,
    buffer: Vec<u8>,
}

struct ApngInfo {
    loop_count: u32,
    frames: Vec<ApngFrame>,
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

fn fill_canvas(width: u32, height: u32, background: Option<crate::color::RGBA>) -> Vec<u8> {
    let background = background.unwrap_or(crate::color::RGBA {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });
    let mut canvas = vec![0u8; width as usize * height as usize * 4];
    for pixel in canvas.chunks_exact_mut(4) {
        pixel[0] = background.red;
        pixel[1] = background.green;
        pixel[2] = background.blue;
        pixel[3] = background.alpha;
    }
    canvas
}

fn apply_frame(canvas: &mut [u8], canvas_width: u32, frame: &ApngFrame) {
    let canvas_width = canvas_width as usize;
    for y in 0..frame.height as usize {
        let src_row = y * frame.width as usize * 4;
        let dst_row = (frame.y_offset as usize + y) * canvas_width * 4;
        for x in 0..frame.width as usize {
            let src_offset = src_row + x * 4;
            let dst_offset = dst_row + (frame.x_offset as usize + x) * 4;
            if frame.blend_op == 1 {
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

fn clear_frame(
    canvas: &mut [u8],
    canvas_width: u32,
    frame: &ApngFrame,
    background: Option<crate::color::RGBA>,
) {
    let background = background.unwrap_or(crate::color::RGBA {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });
    let canvas_width = canvas_width as usize;
    for y in 0..frame.height as usize {
        let dst_row = (frame.y_offset as usize + y) * canvas_width * 4;
        for x in 0..frame.width as usize {
            let dst_offset = dst_row + (frame.x_offset as usize + x) * 4;
            canvas[dst_offset] = background.red;
            canvas[dst_offset + 1] = background.green;
            canvas[dst_offset + 2] = background.blue;
            canvas[dst_offset + 3] = background.alpha;
        }
    }
}

fn normalize_apng_frames(
    width: u32,
    height: u32,
    background: Option<crate::color::RGBA>,
    apng: ApngInfo,
) -> Result<ApngInfo, Error> {
    let mut canvas = fill_canvas(width, height, background.clone());
    let mut normalized = Vec::with_capacity(apng.frames.len());

    for frame in apng.frames {
        let previous = (frame.dispose_op == 2).then(|| canvas.clone());
        apply_frame(&mut canvas, width, &frame);
        normalized.push(ApngFrame {
            width,
            height,
            x_offset: 0,
            y_offset: 0,
            delay_num: frame.delay_num,
            delay_den: frame.delay_den,
            dispose_op: 0,
            blend_op: 0,
            buffer: canvas.clone(),
        });

        match frame.dispose_op {
            1 => clear_frame(&mut canvas, width, &frame, background.clone()),
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

    Ok(ApngInfo {
        loop_count: apng.loop_count,
        frames: normalized,
    })
}

fn write_chunk(write_buffer: &mut Vec<u8>, crc32: &CRC32, chunk_type: &[u8; 4], data: &[u8]) {
    write_u32_be(data.len() as u32, write_buffer);
    let mut temp_buffer = Vec::with_capacity(4 + data.len());
    write_bytes(chunk_type, &mut temp_buffer);
    write_bytes(data, &mut temp_buffer);
    write_bytes(&temp_buffer, write_buffer);
    let crc = crc32.crc32(&temp_buffer);
    write_u32_be(crc, write_buffer);
}

fn to_u32(value: Option<&DataMap>, key: &str) -> Result<u32, Error> {
    match value {
        Some(DataMap::UInt(value)) => u32::try_from(*value).map_err(|_| {
            Box::new(ImgError::new_const(
                ImgErrorKind::EncodeError,
                format!("{key} exceeds u32"),
            )) as Error
        }),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} is not UInt metadata"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} metadata not found"),
        ))),
    }
}

fn to_i32(value: Option<&DataMap>, key: &str) -> Result<i32, Error> {
    match value {
        Some(DataMap::SInt(value)) => i32::try_from(*value).map_err(|_| {
            Box::new(ImgError::new_const(
                ImgErrorKind::EncodeError,
                format!("{key} exceeds i32"),
            )) as Error
        }),
        Some(DataMap::UInt(value)) => i32::try_from(*value).map_err(|_| {
            Box::new(ImgError::new_const(
                ImgErrorKind::EncodeError,
                format!("{key} exceeds i32"),
            )) as Error
        }),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} is not integer metadata"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} metadata not found"),
        ))),
    }
}

fn to_raw(value: Option<&DataMap>, key: &str) -> Result<Vec<u8>, Error> {
    match value {
        Some(DataMap::Raw(value)) => Ok(value.clone()),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} is not Raw metadata"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} metadata not found"),
        ))),
    }
}

fn encode_delay(delay_ms: u64) -> (u16, u16) {
    if delay_ms <= u16::MAX as u64 {
        (delay_ms as u16, 1000)
    } else {
        let seconds = ((delay_ms + 500) / 1000).min(u16::MAX as u64);
        (seconds as u16, 1)
    }
}

fn parse_apng_info(profile: &ImageProfiles) -> Result<Option<ApngInfo>, Error> {
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
        Some(DataMap::UInt(loop_count)) => *loop_count as u32,
        Some(_) => {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::EncodeError,
                "wml2.animation.loop_count is not UInt metadata".to_string(),
            )));
        }
        None => 0,
    };

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

        let width = to_u32(metadata.get(&width_key), &width_key)?;
        let height = to_u32(metadata.get(&height_key), &height_key)?;
        let x_offset = to_i32(metadata.get(&start_x_key), &start_x_key)?;
        let y_offset = to_i32(metadata.get(&start_y_key), &start_y_key)?;
        let delay_ms = to_u32(metadata.get(&delay_key), &delay_key)? as u64;
        let dispose_op = to_u32(metadata.get(&dispose_key), &dispose_key)? as u8;
        let blend_op = to_u32(metadata.get(&blend_key), &blend_key)? as u8;
        let buffer = to_raw(metadata.get(&buffer_key), &buffer_key)?;

        if width == 0 || height == 0 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::EncodeError,
                format!("animation frame {index} has zero size"),
            )));
        }
        if x_offset < 0 || y_offset < 0 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::EncodeError,
                format!("animation frame {index} has negative offset"),
            )));
        }
        let expected_len = usize::try_from(width)
            .ok()
            .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::EncodeError,
                    format!("animation frame {index} buffer size overflows"),
                )) as Error
            })?;
        if buffer.len() != expected_len {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::EncodeError,
                format!("animation frame {index} buffer size mismatch"),
            )));
        }

        let (delay_num, delay_den) = encode_delay(delay_ms);
        frames.push(ApngFrame {
            width,
            height,
            x_offset: x_offset as u32,
            y_offset: y_offset as u32,
            delay_num,
            delay_den,
            dispose_op,
            blend_op,
            buffer,
        });
    }

    let apng = ApngInfo { loop_count, frames };
    let needs_normalization = apng.frames.first().is_some_and(|frame| {
        frame.x_offset != 0
            || frame.y_offset != 0
            || frame.width != profile.width as u32
            || frame.height != profile.height as u32
    });

    if needs_normalization {
        Ok(Some(normalize_apng_frames(
            profile.width as u32,
            profile.height as u32,
            profile.background.clone(),
            apng,
        )?))
    } else {
        Ok(Some(apng))
    }
}

fn filtered_scanlines<F>(width: u32, height: u32, mut row: F) -> Result<Vec<u8>, Error>
where
    F: FnMut(u32) -> Result<Vec<u8>, Error>,
{
    let mut prev_buf = Vec::new();
    let mut data = Vec::new();

    for y in 0..height {
        let buf = row(y)?;
        if buf.len() < width as usize * 4 {
            let boxstr = format!("data shotage width {} but {}", width, buf.len());
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::EncodeError,
                boxstr,
            )));
        }
        let mut inptr = 0;
        data.push(4_u8);
        for _ in 0..width {
            let mut red = buf[inptr];
            let mut green = buf[inptr + 1];
            let mut blue = buf[inptr + 2];
            let mut alpha = buf[inptr + 3];
            let (red_a, green_a, blue_a, alpha_a);
            if inptr > 0 {
                red_a = buf[inptr - 4] as i32;
                green_a = buf[inptr - 3] as i32;
                blue_a = buf[inptr - 2] as i32;
                alpha_a = buf[inptr - 1] as i32;
            } else {
                red_a = 0;
                green_a = 0;
                blue_a = 0;
                alpha_a = 0;
            }
            let (red_b, green_b, blue_b, alpha_b);
            if !prev_buf.is_empty() {
                red_b = prev_buf[inptr] as i32;
                green_b = prev_buf[inptr + 1] as i32;
                blue_b = prev_buf[inptr + 2] as i32;
                alpha_b = prev_buf[inptr + 3] as i32;
            } else {
                red_b = 0;
                green_b = 0;
                blue_b = 0;
                alpha_b = 0;
            }
            let (red_c, green_c, blue_c, alpha_c);
            if !prev_buf.is_empty() && inptr > 0 {
                red_c = prev_buf[inptr - 4] as i32;
                green_c = prev_buf[inptr - 3] as i32;
                blue_c = prev_buf[inptr - 2] as i32;
                alpha_c = prev_buf[inptr - 1] as i32;
            } else {
                red_c = 0;
                green_c = 0;
                blue_c = 0;
                alpha_c = 0;
            }
            red = paeth_enc(red, red_a, red_b, red_c);
            green = paeth_enc(green, green_a, green_b, green_c);
            blue = paeth_enc(blue, blue_a, blue_b, blue_c);
            alpha = paeth_enc(alpha, alpha_a, alpha_b, alpha_c);

            data.push(red);
            data.push(green);
            data.push(blue);
            data.push(alpha);
            inptr += 4;
        }
        prev_buf = buf;
    }

    Ok(miniz_oxide::deflate::compress_to_vec_zlib(&data, 8))
}

fn encode_main_idat(
    image: &mut EncodeOptions<'_>,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Error> {
    filtered_scanlines(width, height, |y| {
        Ok(image
            .drawer
            .encode_pick(0, y as usize, width as usize, 1, None)?
            .unwrap_or(vec![0]))
    })
}

fn encode_frame_data(width: u32, height: u32, buffer: &[u8]) -> Result<Vec<u8>, Error> {
    filtered_scanlines(width, height, |y| {
        let start = y as usize * width as usize * 4;
        let end = start + width as usize * 4;
        Ok(buffer[start..end].to_vec())
    })
}

fn write_ihdr(write_buffer: &mut Vec<u8>, crc32: &CRC32, width: u32, height: u32) {
    let mut temp_buffer: Vec<u8> = Vec::with_capacity(20);
    write_bytes(&IMAGE_HEADER, &mut temp_buffer);
    write_u32_be(width, &mut temp_buffer);
    write_u32_be(height, &mut temp_buffer);
    write_byte(8, &mut temp_buffer);
    write_byte(6, &mut temp_buffer);
    write_byte(0, &mut temp_buffer);
    write_byte(0, &mut temp_buffer);
    write_byte(0, &mut temp_buffer);

    write_u32_be(13, write_buffer);
    write_bytes(&temp_buffer, write_buffer);
    let crc = crc32.crc32(&temp_buffer);
    write_u32_be(crc, write_buffer);
}

fn write_background(write_buffer: &mut Vec<u8>, crc32: &CRC32, background: crate::color::RGBA) {
    let red = background.red as u16;
    let green = background.green as u16;
    let blue = background.blue as u16;
    let mut temp_buffer: Vec<u8> = Vec::with_capacity(10);
    write_u16_be(red, &mut temp_buffer);
    write_u16_be(green, &mut temp_buffer);
    write_u16_be(blue, &mut temp_buffer);
    write_chunk(write_buffer, crc32, &BACKGROUND_COLOR, &temp_buffer);
}

fn write_actl(write_buffer: &mut Vec<u8>, crc32: &CRC32, frame_count: u32, loop_count: u32) {
    let mut temp_buffer = Vec::with_capacity(8);
    write_u32_be(frame_count, &mut temp_buffer);
    write_u32_be(loop_count, &mut temp_buffer);
    write_chunk(write_buffer, crc32, &ANIMATION_CONTROLE, &temp_buffer);
}

fn write_fctl(
    write_buffer: &mut Vec<u8>,
    crc32: &CRC32,
    sequence_number: u32,
    width: u32,
    height: u32,
    x_offset: u32,
    y_offset: u32,
    delay_num: u16,
    delay_den: u16,
    dispose_op: u8,
    blend_op: u8,
) {
    let mut temp_buffer = Vec::with_capacity(26);
    write_u32_be(sequence_number, &mut temp_buffer);
    write_u32_be(width, &mut temp_buffer);
    write_u32_be(height, &mut temp_buffer);
    write_u32_be(x_offset, &mut temp_buffer);
    write_u32_be(y_offset, &mut temp_buffer);
    write_u16_be(delay_num, &mut temp_buffer);
    write_u16_be(delay_den, &mut temp_buffer);
    write_byte(dispose_op, &mut temp_buffer);
    write_byte(blend_op, &mut temp_buffer);
    write_chunk(write_buffer, crc32, &FRAME_CONTROLE, &temp_buffer);
}

/// Encodes a still PNG or APNG stream.
///
/// Supported `EncodeOptions.options` keys:
/// - `exif`: `Raw(bytes)`, `Exif(headers)`, or `Ascii("copy")`
pub fn encode(image: &mut EncodeOptions<'_>) -> Result<Vec<u8>, Error> {
    let profile = image.drawer.encode_start(None)?;
    let (width, height, background, apng_info, exif) = if let Some(profile) = profile {
        let apng_info = parse_apng_info(&profile)?;
        let exif = get_exif_option(image.options.as_ref(), profile.metadata.as_ref())?;
        (
            profile.width as u32,
            profile.height as u32,
            profile.background,
            apng_info,
            exif,
        )
    } else {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::OutboundIndex,
            "Image profiles nothing".to_string(),
        )));
    };

    let crc32 = CRC32::new();
    let mut write_buffer: Vec<u8> = Vec::new();
    write_bytes(&SIGNATURE, &mut write_buffer);

    write_ihdr(&mut write_buffer, &crc32, width, height);
    if let Some(exif) = exif {
        write_chunk(&mut write_buffer, &crc32, &EXIF_PROFILE, &exif);
    }

    if let Some(background) = background {
        write_background(&mut write_buffer, &crc32, background);
    }

    if let Some(apng) = apng_info {
        write_actl(
            &mut write_buffer,
            &crc32,
            apng.frames.len() as u32,
            apng.loop_count,
        );

        let mut sequence_number = 0;
        let first_frame = &apng.frames[0];
        write_fctl(
            &mut write_buffer,
            &crc32,
            sequence_number,
            first_frame.width,
            first_frame.height,
            first_frame.x_offset,
            first_frame.y_offset,
            first_frame.delay_num,
            first_frame.delay_den,
            first_frame.dispose_op,
            first_frame.blend_op,
        );
        sequence_number += 1;

        let idat = encode_frame_data(first_frame.width, first_frame.height, &first_frame.buffer)?;
        write_chunk(&mut write_buffer, &crc32, &IMAGE_DATA, &idat);

        for frame in apng.frames.iter().skip(1) {
            if frame.x_offset + frame.width > width || frame.y_offset + frame.height > height {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::EncodeError,
                    "animation frame exceeds PNG canvas".to_string(),
                )));
            }
            write_fctl(
                &mut write_buffer,
                &crc32,
                sequence_number,
                frame.width,
                frame.height,
                frame.x_offset,
                frame.y_offset,
                frame.delay_num,
                frame.delay_den,
                frame.dispose_op,
                frame.blend_op,
            );
            sequence_number += 1;

            let fd_at = encode_frame_data(frame.width, frame.height, &frame.buffer)?;
            let mut temp_buffer = Vec::with_capacity(fd_at.len() + 4);
            write_u32_be(sequence_number, &mut temp_buffer);
            write_bytes(&fd_at, &mut temp_buffer);
            write_chunk(&mut write_buffer, &crc32, &FRAME_DATA, &temp_buffer);
            sequence_number += 1;
        }
    } else {
        let idat = encode_main_idat(image, width, height)?;
        write_chunk(&mut write_buffer, &crc32, &IMAGE_DATA, &idat);
    }

    write_chunk(&mut write_buffer, &crc32, &IMAGE_END, &[]);
    image.drawer.encode_end(None)?;
    Ok(write_buffer)
}
