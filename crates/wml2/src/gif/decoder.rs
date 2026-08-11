//! GIF decoder implementation.

type Error = Box<dyn std::error::Error>;

use super::header::*;
use crate::color::RGBA;
use crate::decoder::lzw::Lzwdecode;
use crate::draw::*;
use crate::error::ImgError;
use crate::error::ImgErrorKind;
use crate::metadata::DataMap;
use crate::warning::ImgWarnings;
use bin_rs::io::read_ascii_string;
use bin_rs::reader::BinaryReader;

const SEPARATOR: u8 = b','; // 0x2c
const EXTEND_BLOCK: u8 = b'!'; // 0x21
const COMMENT_LABEL: u8 = 0xfe;
const GRAPHIC_CONTROLE: u8 = 0xf9;
const END_MARKER: u8 = b';'; // 0x3c
const END: u8 = 0x00;

fn gif_dispose_option(dispose_method: u8) -> NextDispose {
    match dispose_method {
        2 => NextDispose::Background,
        3 => NextDispose::Previous,
        _ => NextDispose::None,
    }
}

fn gif_blend_option(is_transparent: bool) -> NextBlend {
    if is_transparent {
        NextBlend::Source
    } else {
        NextBlend::Override
    }
}

fn gif_delay_ms(delay_time_cs: usize) -> u64 {
    if delay_time_cs <= 1 {
        100
    } else {
        (delay_time_cs * 10) as u64
    }
}

const DRAW_CHUNK_PIXELS: usize = 64 * 1024;

fn draw_frame(
    option: &mut DecodeOptions,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    indices: &[u8],
    color_table: &[RGBA],
    interlaced: bool,
) -> Result<(), Error> {
    let mut interlace_mode = 0;
    let mut interlace_y = [0, 4, 2, 1][interlace_mode];
    for source_y in 0..height {
        option.drawer.check_decode_control()?;
        let destination_y = if interlaced {
            let destination_y = interlace_y;
            interlace_y += [8, 8, 4, 2][interlace_mode];
            if interlace_y >= height {
                interlace_mode += 1;
                if interlace_mode < 4 {
                    interlace_y = [0, 4, 2, 1][interlace_mode];
                }
            }
            destination_y
        } else {
            source_y
        };
        let row_offset = source_y.checked_mul(width).ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "GIF row offset overflow".to_string(),
            )) as Error
        })?;
        for chunk_x in (0..width).step_by(DRAW_CHUNK_PIXELS) {
            let chunk_width = (width - chunk_x).min(DRAW_CHUNK_PIXELS);
            let mut rgba = allocate_rgba(chunk_width, 1, None, "GIF draw chunk")?;
            for x in 0..chunk_width {
                let palette_index = *indices.get(row_offset + chunk_x + x).ok_or_else(|| {
                    Box::new(ImgError::new_const(
                        ImgErrorKind::UnexpectedEof,
                        "GIF LZW output is shorter than the frame".to_string(),
                    )) as Error
                })? as usize;
                let color = color_table.get(palette_index).ok_or_else(|| {
                    Box::new(ImgError::new_const(
                        ImgErrorKind::OutboundIndex,
                        "GIF palette index is out of range".to_string(),
                    )) as Error
                })?;
                let offset = x * 4;
                rgba[offset..offset + 4].copy_from_slice(&[
                    color.red,
                    color.green,
                    color.blue,
                    color.alpha,
                ]);
            }
            option.drawer.draw(
                start_x + chunk_x,
                start_y + destination_y,
                chunk_width,
                1,
                &rgba,
                None,
            )?;
        }
    }
    Ok(())
}

pub fn decode<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let mut header = GifHeader::new(reader, option.debug_flag)?;
    let mut comment = "".to_string();
    let mut is_transparent = false;
    let mut transparent_color = 0x00;
    let mut delay_time = 0;
    let mut dispose_method = 0;
    let mut loop_count = 0;
    let mut is_inited = false;
    let mut is_first = true;
    let warnings: Option<ImgWarnings> = None;

    if option.debug_flag > 0 {
        option.drawer.verbose(&format!("{:?}", &header), None)?;
    }
    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("GIF".to_string()))?;
    option
        .drawer
        .set_metadata("version", DataMap::Ascii(header.version.to_string()))?;
    option
        .drawer
        .set_metadata("width", DataMap::UInt(header.width as u64))?;
    option
        .drawer
        .set_metadata("height", DataMap::UInt(header.height as u64))?;
    let mut comment_count = 0;

    loop {
        let c = reader.read_byte()?;

        match c {
            // BLOCK LOOP
            END => {}
            EXTEND_BLOCK => {
                let ext = reader.read_byte()?;
                match ext {
                    END => {}
                    COMMENT_LABEL => {
                        let mut s = "".to_string();
                        comment_count += 1;
                        loop {
                            let len = reader.read_byte()? as usize;
                            if len == 0 {
                                break;
                            }
                            if ext == COMMENT_LABEL {
                                let comment = reader.read_ascii_string(len)?;
                                s = s.to_owned() + &comment;
                            }
                        }
                        if option.debug_flag > 0 {
                            option
                                .drawer
                                .verbose(&("Comment: ".to_owned() + &s), None)?;
                        }
                        let string = format!("comment:{}", comment_count);
                        option
                            .drawer
                            .set_metadata(&string, DataMap::Ascii(s.to_string()))?;
                        comment += &s;
                    }
                    GRAPHIC_CONTROLE => {
                        let _len = reader.read_byte()? as usize; //5
                        let flag = reader.read_byte()?;
                        delay_time = reader.read_u16_le()?;

                        if is_transparent && transparent_color < header.color_table.len() {
                            header.color_table[transparent_color].alpha = 0xff;
                        }

                        is_transparent = flag & 0x1 == 1;
                        dispose_method = (flag >> 2) & 0x07;

                        transparent_color = reader.read_byte()? as usize;
                        if option.debug_flag > 0 {
                            let s = format!(
                                "Grahic Control {} delay {}ms  transpearent {:?}",
                                flag, delay_time, is_transparent
                            );
                            option.drawer.verbose(&s, None)?;
                        }
                        reader.read_byte()?; // is 00
                    }
                    0xff => {
                        // Netscape2.0 (Animation Flag)
                        let len = reader.read_byte()? as usize;
                        if len == 0 {
                            break;
                        }
                        let buf = reader.read_bytes_as_vec(len)?;
                        let s = read_ascii_string(&buf, 0, len);
                        loop {
                            let len = reader.read_byte()? as usize;
                            if len == 0 {
                                break;
                            }
                            if s == "NETSCAPE2.0" {
                                option.drawer.set_metadata(
                                    "Animation GIF",
                                    DataMap::Ascii("NETSCAPE2.0".to_string()),
                                )?;
                                if option.debug_flag > 0 {
                                    option
                                        .drawer
                                        .verbose(&("Animation tag: ".to_owned() + &s), None)?;
                                }
                                let id = reader.read_byte()?;
                                if id == 0x01 && len >= 3 {
                                    loop_count = reader.read_u16_le()? as u32;
                                    if len > 3 {
                                        reader.skip_ptr(len - 3)?;
                                    }
                                } else {
                                    reader.skip_ptr(len.saturating_sub(1))?;
                                }
                            } else {
                                reader.skip_ptr(len)?;
                            }
                        }
                    }
                    _ => loop {
                        let len = reader.read_byte()? as usize;
                        if len == 0 {
                            break;
                        }
                        reader.skip_ptr(len)?;
                    },
                }
            }

            SEPARATOR => {
                let lscd = GifLscd::new(reader)?;
                if !is_inited {
                    let background = if header.color_table.is_empty() {
                        None
                    } else {
                        Some(header.color_table[header.scd.color_index as usize].clone())
                    };
                    let init = InitOptions {
                        loop_count,
                        background,
                        animation: true,
                    };
                    option
                        .drawer
                        .init(header.width, header.height, Some(init))?;
                    is_inited = true;
                }

                let width = lscd.xsize as usize;
                let height = lscd.ysize as usize;
                option.drawer.preflight_frame(width, height)?;

                if !is_first {
                    let rect = ImageRect {
                        width: lscd.xsize as usize,
                        height: lscd.ysize as usize,
                        start_x: lscd.xstart as i32,
                        start_y: lscd.ystart as i32,
                    };

                    let await_time = gif_delay_ms(delay_time as usize);

                    let opt = NextOptions {
                        flag: NextOption::Next,
                        await_time,
                        image_rect: Some(rect),
                        dispose_option: Some(gif_dispose_option(dispose_method)),
                        blend: Some(gif_blend_option(is_transparent)),
                    };

                    let result = option.drawer.next(Some(opt))?;
                    if let Some(response) = result {
                        if response.response == ResponseCommand::Abort {
                            return Ok(None);
                        }
                    }
                }
                let has_local_palette;
                let mut local_color_table = Vec::new();
                if lscd.field & 0x80 == 0x80 {
                    has_local_palette = true;
                    let color_table_size = (1 << ((lscd.field & 0x07) + 1)) as usize;
                    for _ in 0..color_table_size {
                        let color = RGBA {
                            red: reader.read_byte()?,
                            green: reader.read_byte()?,
                            blue: reader.read_byte()?,
                            alpha: 0xff,
                        };
                        local_color_table.push(color);
                    }
                    if is_transparent {
                        local_color_table[transparent_color].alpha = 0x00;
                    }
                } else {
                    has_local_palette = false;
                    if is_transparent && transparent_color < header.color_table.len() {
                        header.color_table[transparent_color].alpha = 0x00;
                    }
                    // header.color_table[transparent_color].alpha = 0xff;
                }
                // LZW block
                let lzw_min_bits = reader.read_byte()? as usize;
                let mut buf: Vec<u8> = Vec::new();
                'lzw_read: loop {
                    let len = reader.read_byte()? as usize;
                    if len == 0 {
                        break 'lzw_read;
                    }
                    buf.append(&mut reader.read_bytes_as_vec(len)?);
                }
                let expected_indices = width.checked_mul(height).ok_or_else(|| {
                    Box::new(DecodeLimitError::new(
                        DecodeLimitKind::Pixels,
                        u128::MAX,
                        option
                            .drawer
                            .decode_limits()
                            .map(|limits| limits.maximum_frame_pixels as u128)
                            .unwrap_or(u128::MAX),
                        0,
                        0,
                    )) as Error
                })?;
                check_decoded_output_bytes(option.drawer, expected_indices)?;
                let mut decoder = Lzwdecode::gif(lzw_min_bits);
                option.drawer.check_decode_control()?;
                let data = if option.drawer.decode_limits().is_some() {
                    decoder.decode_with_limit(&buf, expected_indices)?
                } else {
                    decoder.decode(&buf)?
                };
                option.drawer.check_decode_control()?;
                if data.len() < expected_indices {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::UnexpectedEof,
                        "GIF LZW output is shorter than the frame".to_string(),
                    )));
                }
                let color_table = if has_local_palette {
                    &local_color_table
                } else {
                    &header.color_table
                };

                if option.debug_flag > 0 {
                    option.drawer.verbose(&format!("{:?}", lscd), None)?;
                    option.drawer.verbose(
                        &format!(
                            "{} {} {} data length {}",
                            width,
                            height,
                            width * height,
                            data.len()
                        ),
                        None,
                    )?;
                }

                let is_interlace = (lscd.field & 0x40) == 0x40;

                if is_first {
                    draw_frame(
                        option,
                        lscd.xstart as usize,
                        lscd.ystart as usize,
                        width,
                        height,
                        &data,
                        color_table,
                        is_interlace,
                    )?;

                    let opt = NextOptions {
                        flag: NextOption::Continue,
                        await_time: gif_delay_ms(delay_time as usize),
                        image_rect: Some(ImageRect {
                            width,
                            height,
                            start_x: lscd.xstart as i32,
                            start_y: lscd.ystart as i32,
                        }),
                        dispose_option: Some(gif_dispose_option(dispose_method)),
                        blend: Some(gif_blend_option(is_transparent)),
                    };
                    let result = option.drawer.next(Some(opt))?;
                    if let Some(response) = result {
                        if response.response == ResponseCommand::Continue {
                            draw_frame(
                                option,
                                0,
                                0,
                                width,
                                height,
                                &data,
                                color_table,
                                is_interlace,
                            )?;
                        }
                    }
                    is_first = false;
                } else {
                    draw_frame(
                        option,
                        0,
                        0,
                        width,
                        height,
                        &data,
                        color_table,
                        is_interlace,
                    )?;
                }
            }
            END_MARKER => {
                break;
            }
            _ => {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::IllegalData,
                    "read error in gif decode".to_string(),
                )));
            }
        };
    }

    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::gif_delay_ms;

    #[test]
    fn zero_and_one_centisecond_delays_are_normalized() {
        assert_eq!(gif_delay_ms(0), 100);
        assert_eq!(gif_delay_ms(1), 100);
        assert_eq!(gif_delay_ms(2), 20);
    }
}
