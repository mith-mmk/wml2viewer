//! TIFF decoder implementation.

type Error = Box<dyn std::error::Error>;
#[cfg(feature = "tiff-jpeg")]
use self::jpeg::decode_jpeg_compresson;
use crate::color::RGBA;
use crate::decoder::lzw::Lzwdecode;
use crate::draw::*;
use crate::error::ImgError;
use crate::error::ImgErrorKind;
use crate::metadata::DataMap;
use crate::tiff::header::*;
use crate::tiff::warning::TiffWarning;
use crate::warning::ImgWarnings;
use bin_rs::io::read_u16;
use bin_rs::io::read_u32;
use bin_rs::reader::BinaryReader;
mod ccitt;
#[cfg(feature = "tiff-jpeg")]
mod jpeg;
mod packbits;

fn create_pallet(bits: usize, is_black_zero: bool) -> Vec<RGBA> {
    let color_max = 1 << bits;
    let mut pallet = Vec::with_capacity(color_max);

    if is_black_zero {
        for i in 0..color_max {
            let gray = (i * 255 / (color_max - 1)) as u8;
            pallet.push(RGBA {
                red: gray,
                green: gray,
                blue: gray,
                alpha: 0xff,
            });
        }
    } else {
        for i in 0..color_max {
            let gray = 255 - ((i * 255 / (color_max - 1)) as u8);
            pallet.push(RGBA {
                red: gray,
                green: gray,
                blue: gray,
                alpha: 0xff,
            });
        }
    }
    pallet
}

fn planar_to_chuncky(data: &[u8], header: &Tiff) -> Result<Vec<u8>, Error> {
    let mut buf = vec![];
    let mut total_length = 0;
    for bits in &header.bitspersamples {
        total_length += header.height as usize * header.width as usize * ((*bits as usize + 7) / 8);
    }

    if data.len() < total_length {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Data shotage.".to_string(),
        )));
    }

    let length = header.height as usize
        * header.width as usize
        * ((header.bitspersamples[0] as usize + 7) / 8);
    for i in 0..length {
        for j in 0..header.samples_per_pixel as usize {
            buf.push(data[i + j * length]);
        }
    }

    Ok(buf)
}

fn has_bytes(data: &[u8], offset: usize, count: usize) -> bool {
    offset
        .checked_add(count)
        .map(|end| end <= data.len())
        .unwrap_or(false)
}

pub fn draw_strip(
    data: &[u8],
    y: usize,
    strip: usize,
    option: &mut DecodeOptions,
    header: &Tiff,
) -> Result<Option<ImgWarnings>, Error> {
    draw_tile(data, y, strip, 0, header.width as usize, option, header)
}

pub fn draw_tile(
    data: &[u8],
    y: usize,
    strip: usize,
    x: usize,
    width: usize,
    option: &mut DecodeOptions,
    header: &Tiff,
) -> Result<Option<ImgWarnings>, Error> {
    if data.is_empty() {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Data empty.".to_string(),
        )));
    }

    let mut data = data.to_owned();

    // no debug
    if header.planar_config == 2 && header.samples_per_pixel > 1 {
        data = planar_to_chuncky(&data, header)?;
    }

    let color_table: Option<Vec<RGBA>> = if let Some(color_table) = header.color_table.as_ref() {
        Some(color_table.to_vec())
    } else {
        let bitspersample = if header.bitspersample >= 8 {
            8
        } else {
            header.bitspersample
        };
        match header.photometric_interpretation {
            0 => {
                // WhiteIsZero
                Some(create_pallet(bitspersample as usize, false))
            }
            1 => {
                // BlackIsZero
                Some(create_pallet(bitspersample as usize, true))
            }
            2 => {
                if header.samples_per_pixel < 3 {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        "RGB image needs Sample per pixel >=3.".to_string(),
                    )));
                } else {
                    None
                }
            }
            3 => {
                // RGB Palette
                Some(create_pallet(bitspersample as usize, true))
            }
            5 => {
                if header.samples_per_pixel < 4 {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        "YMCK image needs Sample per pixel >=4.".to_string(),
                    )));
                } else {
                    None
                }
            }
            6 => {
                if header.samples_per_pixel < 3 {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::DecodeError,
                        "YCbCr image needs Sample per pixel >=3.".to_string(),
                    )));
                } else {
                    None
                }
            }
            _ => {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::DecodeError,
                    "Not Supprt Color model.".to_string(),
                )));
            }
        }
    };
    if header.bitspersample <= 8 && color_table.is_none() {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "This is an index color image,but A color table is empty.".to_string(),
        )));
    }
    let palette = match header.photometric_interpretation {
        0 | 1 | 3 => {
            let palette = color_table.as_deref().ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::DecodeError,
                    "This is an index color image,but A color table is empty.".to_string(),
                )) as Error
            })?;
            let required_len = 1usize << header.bitspersample as usize;
            if palette.len() < required_len {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::DecodeError,
                    format!(
                        "Color table is too short. expected at least {} entries, got {}",
                        required_len,
                        palette.len()
                    ),
                )));
            }
            palette
        }
        _ => &[],
    };

    let mut row_len = ((header.width as usize * header.bitspersample as usize) + 7) / 8;
    if header.bitspersample == 4 {
        row_len *= 2;
    } else if header.bitspersample == 2 {
        row_len *= 4;
    } else if header.bitspersample == 1 {
        row_len *= 8;
    }

    for (l, y) in (y..(y + strip)).enumerate() {
        let mut buf = vec![];
        let mut prevs = vec![0_u8; header.samples_per_pixel as usize];
        let mut i = l * row_len;

        for _ in 0..header.width as usize {
            match header.photometric_interpretation {
                0 | 1 | 3 => {
                    match header.bitspersamples[0] {
                        16 => {
                            // Illegal tiff(TOWNS TIFF)
                            if header.max_sample_values.len() == 1
                                && header.max_sample_values[0] == 32767
                                && header.tiff_headers.endian == bin_rs::Endian::LittleEndian
                            {
                                if !has_bytes(&data, i, 2) {
                                    return Ok(None);
                                }
                                let color = read_u16(&data, i, header.tiff_headers.endian) >> 8;
                                let temp_r = (color >> 5 & 0x1f) as u8;
                                let r = temp_r << 3 | temp_r >> 2;
                                let temp_g = (color >> 10 & 0x1f) as u8;
                                let g = temp_g << 3 | temp_g >> 2;
                                let temp_b = (color & 0x1f) as u8;
                                let b = temp_b << 3 | temp_b >> 2;
                                buf.push(r);
                                buf.push(g);
                                buf.push(b);
                                buf.push(0xff);
                                i += 2;
                            } else {
                                // 16 bit glayscale
                                if !has_bytes(&data, i, 1) {
                                    return Ok(None);
                                }
                                let mut color = data[i];
                                if header.predictor == 2 {
                                    color += prevs[0];
                                    prevs[0] = color;
                                }
                                buf.push(color);
                                buf.push(color);
                                buf.push(color);
                                buf.push(color);
                                i += header.bitspersamples[0] as usize / 8;
                            }
                        }
                        8 => {
                            if i >= data.len() {
                                //return Err(Box::new(ImgError::new_const(ImgErrorKind::DecodeError,"Buffer shotage".to_string())));
                                return Ok(None);
                            }
                            let mut color = data[i];
                            if header.predictor == 2 {
                                color += prevs[0];
                                prevs[0] = color;
                            }

                            let rgba = &palette[color as usize];

                            buf.push(rgba.red);
                            buf.push(rgba.green);
                            buf.push(rgba.blue);
                            buf.push(rgba.alpha);
                            i += header.samples_per_pixel as usize;
                        }
                        4 => {
                            if i / 2 >= data.len() {
                                return Ok(None);
                            }
                            let c;
                            let color = data[i / 2];
                            if i % 2 == 0 {
                                if header.fill_order == 1 {
                                    c = (color >> 4) as usize;
                                } else {
                                    c = (color.reverse_bits() & 0xf) as usize;
                                }
                            } else if header.fill_order == 1 {
                                c = (color & 0xf) as usize;
                            } else {
                                c = (color.reverse_bits() >> 4) as usize;
                            }

                            let rgba = &palette[c];

                            buf.push(rgba.red);
                            buf.push(rgba.green);
                            buf.push(rgba.blue);
                            buf.push(rgba.alpha);
                            i += 1; //  i += header.samples_per_pixel as usize; ?
                        }
                        2 => {
                            // usually illegal
                            if i / 4 >= data.len() {
                                return Ok(None);
                            }
                            let c;
                            let color = data[i / 4];
                            let shift = (i % 4) * 2;
                            if header.fill_order == 1 {
                                c = ((color >> (6 - shift)) & 0x3) as usize;
                            } else {
                                c = ((color.reverse_bits() >> (6 - shift)) & 0x3) as usize;
                            }

                            let rgba = &palette[c];

                            buf.push(rgba.red);
                            buf.push(rgba.green);
                            buf.push(rgba.blue);
                            buf.push(rgba.alpha);
                            i += 1;
                        }
                        1 => {
                            if i / 8 >= data.len() {
                                return Ok(None);
                            }
                            let c;
                            let color = data[i / 8];
                            let shift = i % 8;
                            if header.fill_order == 1 {
                                c = ((color >> (7 - shift)) & 0x1) as usize;
                            } else {
                                c = ((color.reverse_bits() >> (7 - shift)) & 0x1) as usize;
                            }

                            let rgba = &palette[c];

                            buf.push(rgba.red);
                            buf.push(rgba.green);
                            buf.push(rgba.blue);
                            buf.push(rgba.alpha);
                            i += 1;
                        }

                        _ => {
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::DecodeError,
                                "This bit per sample is not support.".to_string(),
                            )));
                        }
                    }
                }
                2 => {
                    //RGB
                    let (mut r, mut g, mut b, mut a) = (0, 0, 0, 0xff);
                    match header.bitspersamples[0] {
                        //bit per samples same (8,8,8), but also (8,16,8) pattern
                        8 => {
                            if i + 2 >= data.len() {
                                //                                return Err(Box::new(ImgError::new_const(ImgErrorKind::DecodeError,"Buffer shotage".to_string())));
                                return Ok(None);
                            }
                            r = data[i];
                            g = data[i + 1];
                            b = data[i + 2];
                            a = if !header.extra_samples.is_empty()
                                && header.extra_samples[0] == 2
                                && header.samples_per_pixel > 3
                            {
                                data[i + 3]
                            } else {
                                0xff
                            };
                            i += header.samples_per_pixel as usize;
                        }
                        16 => {
                            if header.samples_per_pixel >= 3 {
                                let bytes = header.samples_per_pixel as usize * 2;
                                if !has_bytes(&data, i, bytes) {
                                    return Ok(None);
                                }
                                r = (read_u16(&data, i, header.tiff_headers.endian) >> 8) as u8;
                                g = (read_u16(&data, i + 2, header.tiff_headers.endian) >> 8) as u8;
                                b = (read_u16(&data, i + 4, header.tiff_headers.endian) >> 8) as u8;
                                a = if !header.extra_samples.is_empty()
                                    && header.extra_samples[0] == 2
                                    && header.samples_per_pixel > 3
                                {
                                    (read_u16(&data, i + 6, header.tiff_headers.endian) >> 8) as u8
                                } else {
                                    0xff
                                };
                                i += header.samples_per_pixel as usize * 2;
                            }
                        }
                        32 => {
                            let bytes = header.samples_per_pixel as usize * 4;
                            if !has_bytes(&data, i, bytes) {
                                return Ok(None);
                            }
                            r = (read_u32(&data, i, header.tiff_headers.endian) >> 24) as u8;
                            g = (read_u32(&data, i + 4, header.tiff_headers.endian) >> 24) as u8;
                            b = (read_u32(&data, i + 8, header.tiff_headers.endian) >> 24) as u8;
                            a = if !header.extra_samples.is_empty()
                                && header.extra_samples[0] == 2
                                && header.samples_per_pixel > 3
                            {
                                (read_u32(&data, i + 12, header.tiff_headers.endian) >> 24) as u8
                            } else {
                                0xff
                            };
                            i += header.samples_per_pixel as usize * 4;
                        }
                        _ => {
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::DecodeError,
                                "This bit per sample is not support.".to_string(),
                            )));
                        }
                    }

                    if header.predictor == 2 {
                        r += prevs[0];
                        prevs[0] = r;
                        g += prevs[1];
                        prevs[1] = g;
                        b += prevs[2];
                        prevs[2] = b;
                        if !header.extra_samples.is_empty() && header.extra_samples[0] == 2 {
                            a += prevs[3];
                            prevs[3] = a;
                        }
                    }
                    buf.push(r);
                    buf.push(g);
                    buf.push(b);
                    buf.push(a);
                }
                // 4 : Transparentary musk is not support
                5 => {
                    //CMYK
                    let (mut c, mut m, mut y, mut k, mut a);
                    match header.bitspersamples[0] {
                        //bit per samples same (8,8,8), but also (8,16,8) pattern
                        8 => {
                            if !has_bytes(&data, i, header.samples_per_pixel as usize) {
                                return Ok(None);
                            }
                            c = data[i];
                            m = data[i + 1];
                            y = data[i + 2];
                            k = data[i + 3];
                            a = if !header.extra_samples.is_empty()
                                && header.extra_samples[0] == 2
                                && header.samples_per_pixel > 4
                            {
                                data[i + 4]
                            } else {
                                0xff
                            };
                            i += header.samples_per_pixel as usize;
                        }
                        16 => {
                            let bytes = header.samples_per_pixel as usize * 2;
                            if !has_bytes(&data, i, bytes) {
                                return Ok(None);
                            }
                            c = (read_u16(&data, i, header.tiff_headers.endian) >> 8) as u8;
                            m = (read_u16(&data, i + 2, header.tiff_headers.endian) >> 8) as u8;
                            y = (read_u16(&data, i + 4, header.tiff_headers.endian) >> 8) as u8;
                            k = (read_u16(&data, i + 6, header.tiff_headers.endian) >> 8) as u8;
                            a = if !header.extra_samples.is_empty()
                                && header.extra_samples[0] == 2
                                && header.samples_per_pixel > 4
                            {
                                (read_u16(&data, i + 8, header.tiff_headers.endian) >> 8) as u8
                            } else {
                                0xff
                            };
                            i += header.samples_per_pixel as usize * 2;
                        }
                        32 => {
                            let bytes = header.samples_per_pixel as usize * 4;
                            if !has_bytes(&data, i, bytes) {
                                return Ok(None);
                            }
                            c = (read_u32(&data, i, header.tiff_headers.endian) >> 24) as u8;
                            m = (read_u32(&data, i + 4, header.tiff_headers.endian) >> 24) as u8;
                            y = (read_u32(&data, i + 8, header.tiff_headers.endian) >> 24) as u8;
                            k = (read_u32(&data, i + 12, header.tiff_headers.endian) >> 24) as u8;
                            a = if !header.extra_samples.is_empty()
                                && header.extra_samples[0] == 2
                                && header.samples_per_pixel > 4
                            {
                                (read_u32(&data, i + 16, header.tiff_headers.endian) >> 24) as u8
                            } else {
                                0xff
                            };
                            i += header.samples_per_pixel as usize * 4;
                        }
                        _ => {
                            return Err(Box::new(ImgError::new_const(
                                ImgErrorKind::DecodeError,
                                "This bit per sample is not support.".to_string(),
                            )));
                        }
                    }

                    if header.predictor == 2 {
                        y += prevs[0];
                        prevs[0] = y;
                        m += prevs[1];
                        prevs[1] = m;
                        c += prevs[2];
                        prevs[2] = c;
                        k += prevs[3];
                        prevs[3] = k;
                        if !header.extra_samples.is_empty() && header.extra_samples[0] == 2 {
                            a += prevs[4];
                            prevs[4] = a;
                        }
                    }
                    let r = 255 - c;
                    let g = 255 - m;
                    let b = 255 - y;

                    buf.push(r);
                    buf.push(g);
                    buf.push(b);
                    buf.push(a);
                }
                // not support
                /*
                6 => {  // YCbCr ... this function is not suport YCbCrCoficients,positioning...,yet.  ....4:1:1 sampling
                    let (mut y, mut cb, mut cr, mut a);
                    match header.bitspersamples[0] {  //bit per samples same (8,8,8), but also (8,16,8) pattern
                        8 => {
                            y = data[i];
                            cb = data[i+1];
                            cr = data[i+2];
                            a = if header.extra_samples.len() > 0 && header.extra_samples[0] == 2
                                        && header.samples_per_pixel > 3 {
                                data[i+3] } else { 0xff };
                            i += header.samples_per_pixel as usize;
                        },
                        16 => {
                            y = (read_u16(&data,i,header.tiff_headers.endian) >> 8) as u8;
                            cb = (read_u16(&data,i+2,header.tiff_headers.endian) >> 8) as u8;
                            cr = (read_u16(&data,i+4,header.tiff_headers.endian) >> 8) as u8;
                            a = if header.extra_samples.len() > 0 && header.extra_samples[0] == 2
                                        && header.samples_per_pixel > 3 {
                                    (read_u16(&data,i+6,header.tiff_headers.endian) >> 8) as u8 } else { 0xff };
                            i += header.samples_per_pixel as usize * 2;
                        },
                        32 => {
                            y = (read_u32(&data,i,header.tiff_headers.endian) >> 24) as u8;
                            cb = (read_u32(&data,i+4,header.tiff_headers.endian) >> 24) as u8;
                            cr = (read_u32(&data,i+8,header.tiff_headers.endian) >> 24) as u8;
                            a = if header.extra_samples.len() > 0 && header.extra_samples[0] == 2
                                        && header.samples_per_pixel > 3 {
                                    (read_u32(&data,i+12,header.tiff_headers.endian) >> 24) as u8 } else { 0xff };
                            i += header.samples_per_pixel as usize * 4;
                        },
                        _ => {
                            return Err(Box::new(ImgError::new_const(ImgErrorKind::DecodeError,"This bit per sample is not support.".to_string())));
                        }
                    }

                    if header.predictor == 2 {
                        y += prevs[0];
                        prevs[0] = y;
                        cb += prevs[1];
                        prevs[1] = cb;
                        cr += prevs[2];
                        prevs[2] = cr;
                        if header.extra_samples.len() > 0 && header.extra_samples[0]  == 2 {
                            a += prevs[3];
                            prevs[3] = a;
                        }
                    }
                    // Bt601.1
                    let lr = 0.299;
                    let lg = 0.587;
                    let lb = 0.114;

                    let r = ((y as f32 + (2.0 - 2.0 * lr ) * cr as f32) as i16).clamp(0,255) as u8;
                    let b = ((y as f32 + (2.0 - 2.0 * lb ) * cr as f32) as i16).clamp(0,255) as u8;
                    let g = (((y as f32 - lb * b as f32 - lr * r as f32) / lg) as i16).clamp(0,255) as u8;

                    buf.push(r);
                    buf.push(g);
                    buf.push(b);
                    buf.push(a);

                }
                */
                _ => {
                    //                    return Err(Box::new(ImgError::new_const(ImgErrorKind::DecodeError,"Not support color space.".to_string())));
                }
            }
        }

        option.drawer.draw(x, y, width, 1, &buf, None)?;
    }
    Ok(None)
}

pub fn draw(
    data: &[u8],
    option: &mut DecodeOptions,
    header: &Tiff,
) -> Result<Option<ImgWarnings>, Error> {
    if data.is_empty() {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Data empty.".to_string(),
        )));
    }
    draw_strip(data, 0, header.height as usize, option, header)
}

fn init_canvas(option: &mut DecodeOptions, header: &Tiff, animation: bool) -> Result<(), Error> {
    let init = if animation {
        Some(InitOptions {
            loop_count: 1,
            background: None,
            animation: true,
        })
    } else {
        None
    };
    option
        .drawer
        .init(header.width as usize, header.height as usize, init)?;
    Ok(())
}

fn decoded_rows_bytes(header: &Tiff, rows: usize) -> Result<usize, Error> {
    let bits_per_pixel = if header.bitspersamples.is_empty() {
        (header.bitspersample as usize).checked_mul(header.samples_per_pixel as usize)
    } else {
        header
            .bitspersamples
            .iter()
            .try_fold(0_usize, |sum, bits| sum.checked_add(*bits as usize))
    }
    .ok_or_else(|| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "TIFF pixel size overflow".to_string(),
        )) as Error
    })?;
    let row_bits = (header.width as usize)
        .checked_mul(bits_per_pixel)
        .ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "TIFF row size overflow".to_string(),
            )) as Error
        })?;
    row_bits
        .checked_add(7)
        .and_then(|bits| bits.checked_div(8))
        .and_then(|bytes| bytes.checked_mul(rows))
        .ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "TIFF decoded size overflow".to_string(),
            )) as Error
        })
}

fn read_strips<'decode, B: BinaryReader>(reader: &mut B, header: &Tiff) -> Result<Vec<u8>, Error> {
    let mut data = vec![];
    if header.strip_offsets.len() != header.strip_byte_counts.len() {
        if header.strip_offsets.len() == 1
            && header.strip_byte_counts.is_empty()
            && header.compression == Compression::NoneCompression
        {
            let offset = header.strip_offsets[0] as u64;
            reader.seek(std::io::SeekFrom::Start(offset))?;
            let mut byte = 0;
            for sample in &header.bitspersamples {
                byte += (*sample as u32 + 7) / 8;
            }

            let strip_byte_counts = header.width * header.height * byte;
            let buf = reader.read_bytes_as_vec(strip_byte_counts as usize)?;
            return Ok(buf);
        }
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Mismach length, image strip offsets and strib byte counts.".to_string(),
        )));
    }
    for (i, offset) in header.strip_offsets.iter().enumerate() {
        reader.seek(std::io::SeekFrom::Start(*offset as u64))?;
        let mut buf = reader.read_bytes_as_vec(header.strip_byte_counts[i] as usize)?;
        data.append(&mut buf);
    }
    Ok(data)
}

pub fn decode_lzw_compresson<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
    header: &Tiff,
    initialize: bool,
    animation: bool,
) -> Result<Option<ImgWarnings>, Error> {
    let is_lsb = header.fill_order == 2; // 1: MSB 2: LSB
    if initialize {
        init_canvas(option, header, animation)?;
    }
    let mut y = 0;
    let mut strip = header.rows_per_strip as usize;
    for (i, offset) in header.strip_offsets.iter().enumerate() {
        reader.seek(std::io::SeekFrom::Start(*offset as u64))?;
        let buf = reader.read_bytes_as_vec(header.strip_byte_counts[i] as usize)?;
        let mut decoder = Lzwdecode::tiff(is_lsb);
        let maximum_output = decoded_rows_bytes(header, strip)?;
        check_decoded_output_bytes(option.drawer, maximum_output)?;
        option.drawer.check_decode_control()?;
        let data = if option.drawer.decode_limits().is_some() {
            decoder.decode_with_limit(&buf, maximum_output)?
        } else {
            decoder.decode(&buf)?
        };
        option.drawer.check_decode_control()?;
        draw_strip(&data, y, strip, option, header)?;
        y += strip;
        if y >= header.height as usize {
            break;
        }
        if y + strip >= header.height as usize {
            strip = header.height as usize - y;
        }
    }

    Ok(None)
}

pub fn decode_packbits_compresson<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
    header: &Tiff,
    initialize: bool,
    animation: bool,
) -> Result<Option<ImgWarnings>, Error> {
    if initialize {
        init_canvas(option, header, animation)?;
    }
    let data = read_strips(reader, header)?;
    let maximum_output = decoded_rows_bytes(header, header.height as usize)?;
    check_decoded_output_bytes(option.drawer, maximum_output)?;
    option.drawer.check_decode_control()?;
    let data = if option.drawer.decode_limits().is_some() {
        packbits::decode_with_limit(&data, maximum_output)?
    } else {
        packbits::decode(&data)?
    };
    option.drawer.check_decode_control()?;
    let warnings = draw(&data, option, header)?;
    Ok(warnings)
}

pub fn decode_deflate_compresson<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
    header: &Tiff,
    initialize: bool,
    animation: bool,
) -> Result<Option<ImgWarnings>, Error> {
    if initialize {
        init_canvas(option, header, animation)?;
    }
    let data = read_strips(reader, header)?;
    let maximum_output = decoded_rows_bytes(header, header.height as usize)?;
    check_decoded_output_bytes(option.drawer, maximum_output)?;
    option.drawer.check_decode_control()?;
    let res = if option.drawer.decode_limits().is_some() {
        miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data, maximum_output)
    } else {
        miniz_oxide::inflate::decompress_to_vec_zlib(&data)
    };
    match res {
        Ok(data) => {
            option.drawer.check_decode_control()?;
            let warnings = draw(&data, option, header)?;
            Ok(warnings)
        }
        Err(err) if err.status == miniz_oxide::inflate::TINFLStatus::HasMoreOutput => {
            Err(option.drawer.decode_limit_error(
                DecodeLimitKind::DecodedBytes,
                maximum_output.saturating_add(1) as u128,
                maximum_output as u128,
            ))
        }
        Err(err) => {
            let deflate_err = format!("{:?}", err);
            Err(Box::new(ImgError::new_const(
                ImgErrorKind::DecodeError,
                deflate_err,
            )))
        }
    }
}

pub fn decode_none_compresson<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
    header: &Tiff,
    initialize: bool,
    animation: bool,
) -> Result<Option<ImgWarnings>, Error> {
    if initialize {
        init_canvas(option, header, animation)?;
    }
    let data = read_strips(reader, header)?;
    let warnings = draw(&data, option, header)?;
    Ok(warnings)
}

pub fn decode_ccitt_compresson<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
    header: &Tiff,
    initialize: bool,
    animation: bool,
) -> Result<Option<ImgWarnings>, Error> {
    if initialize {
        init_canvas(option, header, animation)?;
    }
    let warnings = None;

    let header = &mut header.clone();

    header.bitspersample = 8;
    header.bitspersamples = [8].to_vec();

    if header.compression == Compression::CCITTGroup3Fax {
        let buf = read_strips(reader, header)?;
        let (data, _warning) = ccitt::decode(&buf, header)?;
        draw(&data, option, header)?;
    } else {
        // CCITGroup4FAX
        let mut y = 0;
        let mut strip = header.rows_per_strip as usize;
        for (i, offset) in header.strip_offsets.iter().enumerate() {
            reader.seek(std::io::SeekFrom::Start(*offset as u64))?;
            let buf = reader.read_bytes_as_vec(header.strip_byte_counts[i] as usize)?;
            let (data, _warning) = ccitt::decode(&buf, header)?;
            draw_strip(&data, y, strip, option, header)?;
            y += strip;
            if y >= header.height as usize {
                break;
            }
            if y + strip >= header.height as usize {
                strip = header.height as usize - y;
            }
        }
    }

    Ok(warnings)
}

fn compression_decode<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
    header: &Tiff,
    initialize: bool,
    animation: bool,
) -> Result<Option<ImgWarnings>, Error> {
    match header.compression {
        Compression::NoneCompression => {
            return decode_none_compresson(reader, option, header, initialize, animation);
        }
        Compression::LZW => {
            return decode_lzw_compresson(reader, option, header, initialize, animation);
        }
        Compression::Jpeg => {
            #[cfg(feature = "tiff-jpeg")]
            return decode_jpeg_compresson(reader, option, header, initialize, animation);
            #[cfg(not(feature = "tiff-jpeg"))]
            {
                let _ = (reader, option, header, initialize, animation);
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::NoSupportFormat,
                    "TIFF JPEG compression support is disabled by feature flags".to_string(),
                )));
            }
        }
        Compression::Packbits => {
            return decode_packbits_compresson(reader, option, header, initialize, animation);
        }
        Compression::AdobeDeflate => {
            return decode_deflate_compresson(reader, option, header, initialize, animation);
        }
        Compression::CCITTHuffmanRLE
        | Compression::CCITTGroup3Fax
        | Compression::CCITTGroup4Fax => {
            return decode_ccitt_compresson(reader, option, header, initialize, animation);
        }
        _ => Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "Not suport compression".to_string(),
        ))),
    }
}

pub fn decode<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let mut header = Tiff::new(reader)?;

    let mut count = 1;
    if header.multi_page.len() > 0 {
        for append in header.multi_page.iter() {
            if append.newsubfiletype == 0 && append.subfiletype == 0 {
                count += 1;
            }
        }
    }
    option
        .drawer
        .set_metadata("image pages", DataMap::UInt(count))?;

    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("Tiff".to_owned()))?;
    option
        .drawer
        .set_metadata("width", DataMap::UInt(header.width as u64))?;
    option
        .drawer
        .set_metadata("height", DataMap::UInt(header.height as u64))?;
    option
        .drawer
        .set_metadata("bits per pixel", DataMap::UInt(header.bitspersample as u64))?;
    option
        .drawer
        .set_metadata("Tiff headers", DataMap::Exif(header.tiff_headers.clone()))?;
    option.drawer.set_metadata(
        "compression",
        DataMap::Ascii(header.compression.to_string()),
    )?;
    if let Some(ref icc_profile) = header.icc_profile {
        option
            .drawer
            .set_metadata("ICC Profile", DataMap::ICCProfile(icc_profile.to_vec()))?;
    }
    let mut warnings = None;

    if count > 1 {
        check_declared_animation_budget(
            option.drawer,
            header.width as usize,
            header.height as usize,
            header
                .multi_page
                .iter()
                .filter(|page| page.newsubfiletype == 0 && page.subfiletype == 0)
                .map(|page| (page.width as usize, page.height as usize)),
        )?;
    }

    let warn = compression_decode(reader, option, &mut header, true, count > 1)?;

    warnings = ImgWarnings::append(warnings, warn);

    if count > 1 {
        for append in header.multi_page.iter() {
            if append.newsubfiletype == 0 && append.subfiletype == 0 {
                let rect = ImageRect {
                    width: append.width as usize,
                    height: append.height as usize,
                    start_x: append.startx as i32,
                    start_y: append.starty as i32,
                };
                let opt = NextOptions {
                    flag: NextOption::Next,
                    await_time: 0,
                    image_rect: Some(rect),
                    dispose_option: None,
                    blend: None,
                };

                let result = option.drawer.next(Some(opt))?;
                if let Some(response) = result {
                    if response.response == ResponseCommand::Abort {
                        return Ok(warnings);
                    }
                }
                let header = &mut append.clone();
                let result = compression_decode(reader, option, header, false, false);
                match result {
                    Ok(warn) => {
                        warnings = ImgWarnings::append(warnings, warn);
                    }
                    Err(error) => {
                        let warning = TiffWarning::new(error.to_string());
                        warnings = ImgWarnings::add(warnings, Box::new(warning));
                    }
                }
            }
        }
    }

    option.drawer.terminate(None)?;
    Ok(warnings)
}
