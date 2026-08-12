//! BMP decoder implementation.

/*
 *  bmp/decorder.rs (C) 2022 Mith@mmk
 *
 *
 */

use crate::metadata::DataMap;
use bin_rs::reader::BinaryReader;
type Error = Box<dyn std::error::Error>;
use crate::bmp::header::BitmapInfo::Windows;
use crate::draw::*;
use crate::error::{ImgError, ImgErrorKind};
use crate::warning::ImgWarnings;
use bin_rs::io::*;

use crate::bmp::header::Compressions;
use crate::bmp::header::{BitmapHeader, ColorTable};

fn get_color(c: &Option<&Vec<ColorTable>>, idx: usize) -> ColorTable {
    if let Some(c) = c {
        if idx < c.len() {
            c[idx]
        } else {
            // todo get default color table
            ColorTable {
                blue: idx as u8,
                green: idx as u8,
                red: idx as u8,
                reserved: 0,
            }
        }
    } else {
        // todo get default color table
        ColorTable {
            blue: idx as u8,
            green: idx as u8,
            red: idx as u8,
            reserved: 0,
        }
    }
}

fn convert_rgba32(
    buffer: &[u8],
    line: &mut Vec<u8>,
    header: &BitmapHeader,
    bit_count: usize,
) -> Result<(), Error> {
    let mut offset = 0;
    let width = header.width.unsigned_abs() as usize;
    let buffer_size = buffer.len();
    match bit_count {
        32 => {
            // bgra
            for x in 0..width {
                if offset + 3 >= buffer_size {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        "BMP scanline is truncated".to_string(),
                    )));
                }
                let b = buffer[offset];
                let g = buffer[offset + 1];
                let r = buffer[offset + 2];
                line[x * 4] = r;
                line[x * 4 + 1] = g;
                line[x * 4 + 2] = b;
                offset += 4;
            }
        }
        24 => {
            // bgra
            for x in 0..width {
                if offset + 2 >= buffer_size {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        "BMP scanline is truncated".to_string(),
                    )));
                }
                let b = buffer[offset];
                let g = buffer[offset + 1];
                let r = buffer[offset + 2];
                line[x * 4] = r;
                line[x * 4 + 1] = g;
                line[x * 4 + 2] = b;
                offset += 3;
            }
        }
        16 => {
            // rgb555
            for x in 0..width {
                if offset + 1 >= buffer_size {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        "BMP scanline is truncated".to_string(),
                    )));
                }
                let color = read_u16_le(buffer, offset);
                let r = ((color & 0x7c00) >> 10) as u8;
                let g = ((color & 0x03e0) >> 5) as u8;
                let b = (color & 0x001f) as u8;
                line[x * 4] = r << 3 | r >> 2;
                line[x * 4 + 1] = g << 3 | g >> 2;
                line[x * 4 + 2] = b << 3 | b >> 2;
                offset += 2;
            }
        }
        8 => {
            for x in 0..width {
                if offset >= buffer_size {
                    break;
                }
                let color = read_byte(buffer, offset) as usize;
                let color_tables = &header.color_table.as_ref();
                let c = get_color(color_tables, color);
                let r = c.red;
                let g = c.green;
                let b = c.blue;
                line[x * 4] = r;
                line[x * 4 + 1] = g;
                line[x * 4 + 2] = b;
                offset += 1;
            }
        }
        4 => {
            for x_ in 0..(width + 1) / 2 {
                let mut x = x_ * 2;
                if offset >= buffer_size {
                    break;
                }
                let color_ = read_byte(buffer, offset) as usize;
                let color = color_ >> 4;
                let c = get_color(&header.color_table.as_ref(), color);
                let r = c.red;
                let g = c.green;
                let b = c.blue;
                line[x * 4] = r;
                line[x * 4 + 1] = g;
                line[x * 4 + 2] = b;
                x += 1;
                if x >= width {
                    break;
                }
                let color = color_ & 0xf;
                let c = get_color(&header.color_table.as_ref(), color);
                let r = c.red;
                let g = c.green;
                let b = c.blue;
                line[x * 4] = r;
                line[x * 4 + 1] = g;
                line[x * 4 + 2] = b;
                offset += 1;
            }
        }

        1 => {
            for x_ in 0..(width + 7) / 8 {
                let mut x = x_ * 8;
                if offset >= buffer_size {
                    break;
                }
                let color_ = read_byte(buffer, offset) as usize;
                for i in [7, 6, 5, 4, 3, 2, 1, 0] {
                    if x >= width {
                        break;
                    }
                    let color = (color_ >> i) & 0x1;
                    let c = get_color(&header.color_table.as_ref(), color);
                    let r = c.red;
                    let g = c.green;
                    let b = c.blue;
                    line[x * 4] = r;
                    line[x * 4 + 1] = g;
                    line[x * 4 + 2] = b;
                    x += 1;
                }
                offset += 1;
            }
        }
        _ => {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::NoSupportFormat,
                "Not Support bit count".to_string(),
            )));
        }
    }
    Ok(())
}

fn decode_rgb<B: BinaryReader>(
    reader: &mut B,
    header: &BitmapHeader,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let width = header.width.unsigned_abs() as usize;
    let height = header.height.unsigned_abs() as usize;
    option.drawer.init(width, height, InitOptions::new())?;
    let mut line: Vec<u8> = (0..width * 4)
        .map(|i| if i % 4 == 3 { 0xff } else { 0 })
        .collect();
    if header.bit_count <= 8 && header.color_table.is_none() {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "Not Support under 255 color and no color table".to_string(),
        )));
    }

    let line_size = ((width * header.bit_count + 31) / 32) * 4;
    for y_ in 0..height {
        let buffer = reader.read_bytes_as_vec(line_size)?;
        let y = height - 1 - y_;
        //        let offset = y_ * line_size;
        convert_rgba32(&buffer, &mut line, header, header.bit_count)?;
        if header.height > 0 {
            option.drawer.draw(0, y, width, 1, &line, None)?;
        } else {
            option.drawer.draw(0, y_, width, 1, &line, None)?;
        }
    }
    option.drawer.terminate(None)?;
    Ok(None)
}

fn decode_rle<B: BinaryReader>(
    reader: &mut B,
    header: &BitmapHeader,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let width = header.width.unsigned_abs() as usize;
    let height = header.height.unsigned_abs() as usize;
    if height == 0 {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::SizeZero,
            "BMP height is zero".to_string(),
        )));
    }
    option.drawer.init(width, height, InitOptions::new())?;
    let mut line: Vec<u8> = (0..width * 4)
        .map(|i| if i % 4 == 3 { 0xff } else { 0 })
        .collect();
    let mut y: usize = height - 1;
    let rev_bytes = 8 / header.bit_count;
    'y: loop {
        let mut x: usize = 0;
        let mut buf: Vec<u8> = (0..(width + 1)).map(|_| 0).collect();
        'x: loop {
            let data0 = reader.read_byte()?;
            let data1 = reader.read_byte()?;
            if data0 == 0 {
                if data1 == 0 {
                    break;
                } // EOL
                if data1 == 1 {
                    break 'y;
                } // EOB
                if data1 == 2 {
                    // Jump
                    let data0 = reader.read_byte()?;
                    let data1 = reader.read_byte()?;
                    if data1 == 0 {
                        x += data0 as usize;
                    } else {
                        convert_rgba32(&buf, &mut line, header, 8)?;
                        option.drawer.draw(0, y, width, 1, &line, None)?;
                        if y == 0 {
                            break;
                        }
                        y -= 1;
                        buf = (0..((width + rev_bytes - 1) / rev_bytes))
                            .map(|_| 0)
                            .collect();
                        for _ in 0..data1 as usize {
                            convert_rgba32(&buf, &mut line, header, 8)?;
                            option.drawer.draw(0, y, width, 1, &line, None)?;
                            if y == 0 {
                                break;
                            }
                            y -= 1;
                        }
                        x = data0 as usize;
                        continue 'x;
                    }
                }

                let bytes = (data1 as usize + rev_bytes - 1) / rev_bytes; // pixel
                let rbytes = (bytes + 1) / 2 * 2; // even bytes
                let rbuf = reader.read_bytes_as_vec(rbytes)?;

                if header.bit_count == 8 {
                    for i in 0..bytes {
                        if x >= buf.len() || i >= rbuf.len() {
                            break;
                        }
                        buf[x] = rbuf[i];
                        if x >= width {
                            break;
                        }
                        x += 1;
                    }
                } else if header.bit_count == 4 {
                    for i in 0..bytes {
                        if x >= buf.len() || i >= rbuf.len() {
                            break;
                        }
                        buf[x] = rbuf[i] >> 4;
                        if x + 1 < width {
                            buf[x + 1] = rbuf[i] & 0xf;
                            x += 2;
                        }
                    }
                } else {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::NoSupportFormat,
                        "Unknwon".to_string(),
                    )));
                }
            } else if header.bit_count == 8 {
                for _ in 0..data0 {
                    if x >= buf.len() {
                        // broken bmp
                        break 'x;
                    }
                    buf[x] = data1;
                    x += 1;
                }
            } else if header.bit_count == 4 {
                for _ in 0..data0 as usize / rev_bytes {
                    if x >= buf.len() {
                        break 'x;
                    }
                    buf[x] = data1 >> 4;
                    x += 1;
                    if x >= buf.len() {
                        break 'x;
                    }
                    buf[x] = data1 & 0xf;
                    x += 1;
                }
                if data0 % 2 == 1 {
                    if x >= buf.len() {
                        break 'x;
                    }
                    buf[x] = data1 >> 4;
                    x += 1;
                }
            } else {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::NoSupportFormat,
                    "Unknwon".to_string(),
                )));
            }
        }
        convert_rgba32(&buf, &mut line, header, 8)?;
        if header.height > 0 {
            option.drawer.draw(0, y, width, 1, &line, None)?;
        } else {
            option
                .drawer
                .draw(0, height - 1 - y, width, 1, &line, None)?;
        }
        if y == 0 {
            break;
        }
        y -= 1;
    }
    option.drawer.terminate(None)?;
    Ok(None)
}

fn get_shift(mask: u32) -> (u32, u32) {
    let mut temp = mask;
    let mut shift = 0;
    while temp & 0x1 == 0 {
        temp >>= 1;
        shift += 1;
        if shift > 32 {
            return (0, 8);
        }
    }
    let mut bits = 0;
    while temp & 0x1 == 1 {
        temp >>= 1;
        bits += 1;
        if bits + shift > 32 {
            return (0, 8);
        }
    }
    if bits >= 8 {
        shift += bits - 8;
        bits = 0;
    }
    (shift, bits)
}

fn decode_bit_fileds<B: BinaryReader>(
    reader: &mut B,
    header: &BitmapHeader,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let width = header.width.unsigned_abs() as usize;
    let height = header.height.unsigned_abs() as usize;
    if width == 0 || height == 0 {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::SizeZero,
            "BMP dimensions must be non-zero".to_string(),
        )));
    }

    let info;

    if header.bit_count != 16 && header.bit_count != 32 {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "Illigal bit field / bit count".to_string(),
        )));
    }
    if let Windows(info_) = &header.bitmap_info {
        info = info_;
    } else {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "Illigal bit field / not Windows Bitmap".to_string(),
        )));
    }

    if info.b_v4_header.is_none() {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "Illigal bit field / no V4 Header".to_string(),
        )));
    }
    let v4 = info.b_v4_header.as_ref().ok_or_else(|| {
        Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "Illigal bit field / no V4 Header".to_string(),
        )) as Box<dyn std::error::Error>
    })?;

    let red_mask = v4.b_v4_red_mask;
    let (red_shift, red_bits) = get_shift(red_mask);

    let green_mask = v4.b_v4_green_mask;
    let (green_shift, green_bits) = get_shift(green_mask);

    let blue_mask = v4.b_v4_blue_mask;
    let (blue_shift, blue_bits) = get_shift(blue_mask);

    let alpha_mask = v4.b_v4_alpha_mask;
    let (alpha_shift, alpha_bits) = get_shift(alpha_mask);

    option.drawer.init(width, height, InitOptions::new())?;
    let mut line: Vec<u8> = (0..width * 4)
        .map(|i| if i % 4 == 3 { 0xff } else { 0 })
        .collect();

    let line_size = ((width * header.bit_count + 31) / 32) * 4;

    for y_ in 0..height {
        let buffer = reader.read_bytes_as_vec(line_size)?;
        let y = height - 1 - y_;
        //        let offset = y_ * line_size;

        for x in 0..width {
            let color_offset = if header.bit_count == 32 { x * 4 } else { x * 2 };
            let needed = if header.bit_count == 32 { 4 } else { 2 };
            if color_offset + needed > buffer.len() {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::DecodeError,
                    "BMP bitfield scanline is truncated".to_string(),
                )));
            }
            let color = if header.bit_count == 32 {
                read_u32_le(&buffer, color_offset)
            } else {
                read_u16_le(&buffer, color_offset) as u32
            };
            let red = (color & red_mask) >> red_shift;
            let green = (color & green_mask) >> green_shift;
            let blue = (color & blue_mask) >> blue_shift;

            let alpha = if alpha_mask != 0 {
                (color & alpha_mask) >> alpha_shift
            } else {
                0xff
            };
            line[x * 4] = (red << (8 - red_bits) | red >> red_bits) as u8;
            line[x * 4 + 1] = (green << (8 - green_bits) | green >> green_bits) as u8;
            line[x * 4 + 2] = (blue << (8 - blue_bits) | blue >> blue_bits) as u8;
            line[x * 4 + 3] = (alpha << (8 - alpha_bits) | alpha >> alpha_bits) as u8;
        }
        if header.height > 0 {
            option.drawer.draw(0, y, width, 1, &line, None)?;
        } else {
            option.drawer.draw(0, y_, width, 1, &line, None)?;
        }
    }
    option.drawer.terminate(None)?;
    Ok(None)
}

fn decode_jpeg<B: BinaryReader>(
    reader: &mut B,
    _: &BitmapHeader,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    #[cfg(feature = "bmp-jpeg")]
    return crate::jpeg::decoder::decode(reader, option);
    #[cfg(not(feature = "bmp-jpeg"))]
    {
        let _ = (reader, option);
        Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "BMP JPEG payload support is disabled by feature flags".to_string(),
        )))
    }
}

fn decode_png<B: BinaryReader>(
    reader: &mut B,
    _header: &BitmapHeader,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    #[cfg(feature = "bmp-png")]
    return crate::png::decoder::decode(reader, option);
    #[cfg(not(feature = "bmp-png"))]
    {
        let _ = (reader, option);
        Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "BMP PNG payload support is disabled by feature flags".to_string(),
        )))
    }
}

pub fn decode<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let header = BitmapHeader::new(reader, option.debug_flag)?;

    // workaround
    if header.width > 32767 || header.height > 32767 {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::CannotDecode,
            "Too Large Bitmap".to_string(),
        )));
    }

    if option.debug_flag > 0 {
        let s1 = format!(
            "BITMAP Header size {}",
            header.bitmap_file_header.bf_offbits
        );
        let s2 = format!(
            "width {} height {}  {} bits per sample\n",
            header.width, header.height, header.bit_count
        );
        let s3 = format!("Compression {:?}\n", header.compression);
        let s = s1 + &s2 + &s3;
        option.drawer.verbose(&s, None)?;
    }

    let offset = header.image_offset;
    reader.seek(std::io::SeekFrom::Start(offset as u64))?;

    let result;
    if let Some(compression) = &header.compression {
        match compression {
            Compressions::BiRGB => {
                result = decode_rgb(reader, &header, option);
                option
                    .drawer
                    .set_metadata("compression", DataMap::Ascii("None".to_owned()))?;
            }
            Compressions::BiRLE8 => {
                result = decode_rle(reader, &header, option);
                option
                    .drawer
                    .set_metadata("compression", DataMap::Ascii("RLE".to_owned()))?;
            }
            Compressions::BiRLE4 => {
                result = decode_rle(reader, &header, option);
                option
                    .drawer
                    .set_metadata("compression", DataMap::Ascii("RLE".to_owned()))?;
            }
            Compressions::BiBitFileds => {
                result = decode_bit_fileds(reader, &header, option);
                option
                    .drawer
                    .set_metadata("compression", DataMap::Ascii("Bit fields".to_owned()))?;
            }
            Compressions::BiJpeg => {
                result = decode_jpeg(reader, &header, option);
                option
                    .drawer
                    .set_metadata("compression", DataMap::Ascii("Jpeg".to_owned()))?;
            }
            Compressions::BiPng => {
                result = decode_png(reader, &header, option);
                option
                    .drawer
                    .set_metadata("compression", DataMap::Ascii("PNG".to_owned()))?;
            }
        }
    } else {
        result = decode_rgb(reader, &header, option);
        option
            .drawer
            .set_metadata("compression", DataMap::Ascii("OS2".to_owned()))?;
    }

    if header.height < 0 {
        option
            .drawer
            .set_metadata("negative height", DataMap::Ascii("true".to_string()))?;
    }
    option
        .drawer
        .set_metadata("bits per pixel", DataMap::UInt(header.bit_count as u64))?;
    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("BMP".to_owned()))?;
    option
        .drawer
        .set_metadata("width", DataMap::UInt(header.width as u64))?;
    option
        .drawer
        .set_metadata("height", DataMap::UInt(header.height.unsigned_abs() as u64))?;

    result
}
