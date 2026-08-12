//! PIC decoder implementation.

use bin_rs::reader::BinaryReader;

use crate::draw::DecodeOptions;
use crate::error::ImgErrorKind;
use crate::metadata::DataMap;
use crate::retro::{BitReaderWordMsb, draw_indexed, draw_rgb, err, read_all};
use crate::warning::ImgWarnings;

type Error = Box<dyn std::error::Error>;

const ASPNRM: u8 = 0;
const ASPX68: u8 = 1;

#[derive(Clone)]
struct PicParsed {
    width: usize,
    height: usize,
    color_bits: usize,
    comment: Vec<u8>,
    start_x: usize,
    start_y: usize,
    aspect: u8,
    mode8x2: bool,
    palette: Option<Vec<(u8, u8, u8)>>,
    decode_mode: &'static str,
}

struct PicDecoded {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    palette: Option<Vec<(u8, u8, u8)>>,
    rgb: bool,
}

#[derive(Clone, Copy)]
struct CacheNode {
    color: u32,
    prev: usize,
    next: usize,
}

struct CacheState {
    table: [CacheNode; 128],
    color_pointer: usize,
}

fn parse_mm_comment(comment: &[u8]) -> (usize, usize, bool) {
    let text = String::from_utf8_lossy(comment);
    if !text.starts_with("/MM/") {
        return (0, 0, false);
    }
    let mut start_x = 0usize;
    let mut start_y = 0usize;
    let mut has_xss = false;
    for part in text.split('/').filter(|part| !part.is_empty()) {
        if part == "MM" {
            continue;
        }
        if part == "XSS" {
            has_xss = true;
            continue;
        }
        if part.starts_with("XY") && part.len() >= 10 {
            if let Ok(x) = part[2..6].parse::<usize>() {
                start_x += x;
            }
            if let Ok(y) = part[6..10].parse::<usize>() {
                start_y += y;
            }
        }
    }
    (start_x, start_y, has_xss)
}

fn create_va256_palette() -> Vec<(u8, u8, u8)> {
    let mut palette = Vec::with_capacity(256);
    for i in 0..256usize {
        palette.push((
            (((i >> 2) & 0x07) * 255 / 7) as u8,
            (((i >> 5) & 0x07) * 255 / 7) as u8,
            ((i & 0x03) * 255 / 3) as u8,
        ));
    }
    palette
}

fn parse_header(data: &[u8]) -> Result<(BitReaderWordMsb<'_>, PicParsed), Error> {
    let mut bits = BitReaderWordMsb::new(data, 0);
    let signature = [
        bits.read_bits(8) as u8,
        bits.read_bits(8) as u8,
        bits.read_bits(8) as u8,
    ];
    if &signature != b"PIC" {
        return Err(err(ImgErrorKind::IllegalData, "Not a PIC image"));
    }

    let mut comment = Vec::new();
    loop {
        let c = bits.read_bits(8) as u8;
        if c == 0x1a {
            break;
        }
        comment.push(c);
    }
    let (mut start_x, mut start_y, has_xss) = parse_mm_comment(&comment);

    while bits.read_bits(8) != 0 {}
    if bits.read_bits(8) != 0 {
        return Err(err(ImgErrorKind::IllegalData, "Unsupported PIC header"));
    }

    let c = bits.read_bits(8) as u8;
    let color_bits = bits.read_bits(16) as usize;
    let width = bits.read_bits(16) as usize;
    let height = bits.read_bits(16) as usize;
    let mut aspect = ASPNRM;
    let mut mode8x2 = false;
    let mut palette = None;
    let mut decode_mode = "indexed";

    match color_bits {
        4 | 8 => {}
        15 => decode_mode = if (c & 0x0f) == 3 { "15-mac" } else { "15-rgb" },
        16 => {
            decode_mode = if (c & 0x2f) == 0x21 {
                "16-va8x2"
            } else if (c & 0x0f) == 1 {
                "16-va"
            } else {
                "16-rgb"
            };
        }
        24 => decode_mode = "24",
        _ => {
            return Err(err(
                ImgErrorKind::IllegalData,
                "Unsupported PIC color depth",
            ));
        }
    }

    match c & 0x0f {
        0 => {
            if color_bits >= 15 {
                if !has_xss {
                    aspect = ASPX68;
                }
            } else {
                let palette_size = 1usize << color_bits;
                let mut colors = Vec::with_capacity(palette_size);
                for _ in 0..palette_size {
                    let p = bits.read_bits(16) as u16;
                    let s = (p & 1) as u8 * 7;
                    colors.push((
                        (((p & 0x07c0) >> 3) as u8) | s,
                        (((p & 0xf800) >> 8) as u8) | s,
                        (((p & 0x003f) << 2) as u8) | s,
                    ));
                }
                palette = Some(colors);
            }
        }
        1 => {
            if (c & 0x20) != 0 || color_bits == 8 {
                palette = Some(create_va256_palette());
                if (c & 0x20) != 0 {
                    mode8x2 = true;
                }
            }
        }
        0x0f => {
            if (c & 0xf0) == 0x10 {
                aspect = ASPX68;
            }
            let mut sx = bits.read_bits(16) as usize;
            let mut sy = bits.read_bits(16) as usize;
            if sx == 0xffff {
                sx = 0;
            }
            if sy == 0xffff {
                sy = 0;
            }
            start_x += sx;
            start_y += sy;
            let _asp_y = bits.read_bits(8);
            let _asp_x = bits.read_bits(8);
            if color_bits <= 8 {
                let palette_size = 1usize << color_bits;
                let bit = bits.read_bits(8) as usize;
                let s = ((1usize << bit) - 1).max(1);
                let mut colors = Vec::with_capacity(palette_size);
                for _ in 0..palette_size {
                    let g = (bits.read_bits(bit) as usize * 255 / s) as u8;
                    let r = (bits.read_bits(bit) as usize * 255 / s) as u8;
                    let b = (bits.read_bits(bit) as usize * 255 / s) as u8;
                    colors.push((r, g, b));
                }
                palette = Some(colors);
            }
        }
        _ => {}
    }

    Ok((
        bits,
        PicParsed {
            width,
            height,
            color_bits,
            comment,
            start_x,
            start_y,
            aspect,
            mode8x2,
            palette,
            decode_mode,
        },
    ))
}

fn init_cache() -> CacheState {
    let mut table = [CacheNode {
        color: 0,
        prev: 0,
        next: 0,
    }; 128];
    for (i, node) in table.iter_mut().enumerate() {
        node.prev = if i == 127 { 0 } else { i + 1 };
        node.next = if i == 0 { 127 } else { i - 1 };
    }
    CacheState {
        table,
        color_pointer: 0,
    }
}

fn move_cache_index(state: &mut CacheState, idx: usize) {
    let current = state.color_pointer;
    if current == idx {
        return;
    }
    let prev = state.table[idx].prev;
    let next = state.table[idx].next;
    state.table[prev].next = next;
    state.table[next].prev = prev;
    let current_prev = state.table[current].prev;
    state.table[current_prev].next = idx;
    state.table[idx].prev = current_prev;
    state.table[current].prev = idx;
    state.table[idx].next = current;
    state.color_pointer = idx;
}

fn get_cached_color(bits: &mut BitReaderWordMsb<'_>, cache: &mut CacheState) -> u32 {
    let idx = bits.read_bits(7) as usize;
    move_cache_index(cache, idx);
    cache.table[idx].color
}

fn set_new_cached_color(
    bits: &mut BitReaderWordMsb<'_>,
    cache: &mut CacheState,
    bit_depth: usize,
    mode: &str,
) -> u32 {
    cache.color_pointer = cache.table[cache.color_pointer].prev;
    let raw = bits.read_bits(bit_depth);
    let mut packed = match mode {
        "15-mac" => ((raw & 0x03e0) << 14) | ((raw & 0x7c00) << 1) | ((raw & 0x001f) << 3),
        "16-va" => ((raw & 0xfc00) << 8) | ((raw & 0x03e0) << 6) | ((raw & 0x001f) << 3),
        "16-va8x2" => raw,
        "16-rgb" => ((raw & 0xf800) << 8) | ((raw & 0x07c0) << 5) | ((raw & 0x003f) << 2),
        "24" => raw,
        _ => ((raw & 0x7c00) << 9) | ((raw & 0x03e0) << 6) | ((raw & 0x001f) << 3),
    };
    packed |= match mode {
        "15-mac" | "15-rgb" => (packed >> 5) & 0x00070707,
        "16-va" | "16-rgb" => (packed >> 5) & 0x00070307,
        _ => 0,
    };
    cache.table[cache.color_pointer].color = packed;
    packed
}

fn read_pic_color(
    bits: &mut BitReaderWordMsb<'_>,
    cache: &mut CacheState,
    bit_depth: usize,
    mode: &str,
) -> u32 {
    if bits.read_bit() == 0 {
        set_new_cached_color(bits, cache, bit_depth, mode)
    } else {
        get_cached_color(bits, cache)
    }
}

fn read_pic_len(bits: &mut BitReaderWordMsb<'_>) -> usize {
    let mut a = 1usize;
    while bits.read_bit() != 0 {
        a += 1;
    }
    bits.read_bits(a) as usize + (1usize << a) - 1
}

fn expand_chain(
    bits: &mut BitReaderWordMsb<'_>,
    chain: &mut [Vec<u8>],
    _width: usize,
    height: usize,
    mut x: isize,
    mut y: usize,
) {
    let mut y_over = false;
    loop {
        let flag = if bits.read_bit() == 0 {
            if bits.read_bit() == 0 {
                if bits.read_bit() == 0 {
                    return;
                }
                if bits.read_bit() == 0 { -2 } else { 2 }
            } else {
                -1
            }
        } else if bits.read_bit() == 0 {
            0
        } else {
            1
        };
        x += flag;
        y += 1;
        if y >= height {
            y_over = true;
        }
        if !y_over && x >= 0 {
            let xi = x as usize;
            let byte_index = xi >> 1;
            if (xi & 1) != 0 {
                chain[y][byte_index] |= (flag + 4) as u8;
            } else {
                chain[y][byte_index] |= ((flag + 4) as u8) << 4;
            }
        }
    }
}

fn unpack_chain_value(row: &[u8], x: usize) -> u8 {
    let byte = row.get(x >> 1).copied().unwrap_or(0);
    if (x & 1) == 0 { byte >> 4 } else { byte & 0x0f }
}

fn decode_indexed(
    bits: &mut BitReaderWordMsb<'_>,
    parsed: &PicParsed,
) -> Result<PicDecoded, Error> {
    let width = parsed.width;
    let height = parsed.height;
    if width == 0 || height == 0 {
        return Err(err(
            ImgErrorKind::IllegalData,
            "PIC dimensions must be non-zero",
        ));
    }
    let mut chain = vec![vec![0u8; (width + 1) >> 1]; height];
    let mut current = vec![0u8; width];
    let mut previous = vec![0u8; width];
    let output_height = if parsed.mode8x2 {
        height
            .checked_mul(2)
            .ok_or_else(|| err(ImgErrorKind::IllegalData, "PIC output height overflow"))?
    } else {
        height
    };
    let mut pixels = vec![
        0u8;
        width.checked_mul(output_height).ok_or_else(|| err(
            ImgErrorKind::IllegalData,
            "PIC output size overflow"
        ))?
    ];
    let mut c = 0u8;
    let mut x = usize::MAX;
    let mut y = 0usize;

    loop {
        let mut len = read_pic_len(bits);
        loop {
            x = x.wrapping_add(1);
            if x == width {
                if parsed.mode8x2 {
                    let row_a = y * 2 * width;
                    let row_b = row_a + width;
                    for i in 0..width {
                        pixels[row_a + i] = current[i];
                        pixels[row_b + i] = current[i];
                    }
                } else {
                    pixels[y * width..(y + 1) * width].copy_from_slice(&current);
                }
                y += 1;
                if y == height {
                    return Ok(PicDecoded {
                        width,
                        height: output_height,
                        pixels,
                        palette: parsed.palette.clone(),
                        rgb: false,
                    });
                }
                x = 0;
                std::mem::swap(&mut previous, &mut current);
            }

            len -= 1;
            if len == 0 {
                break;
            }
            let a = unpack_chain_value(&chain[y], x);
            if a != 0 {
                let src_index = x + 4 - a as usize;
                c = previous.get(src_index).copied().unwrap_or(0);
            }
            current[x] = c;
        }

        c = bits.read_bits(parsed.color_bits) as u8;
        current[x] = c;
        if bits.read_bit() != 0 {
            expand_chain(bits, &mut chain, width, height, x as isize, y);
        }
    }
}

fn packed_to_rgb(color: u32) -> [u8; 3] {
    [(color >> 8) as u8, (color >> 16) as u8, color as u8]
}

fn decode_direct(bits: &mut BitReaderWordMsb<'_>, parsed: &PicParsed) -> Result<PicDecoded, Error> {
    let width = parsed.width;
    let height = parsed.height;
    if width == 0 || height == 0 {
        return Err(err(
            ImgErrorKind::IllegalData,
            "PIC dimensions must be non-zero",
        ));
    }
    let mut chain = vec![vec![0u8; (width + 1) >> 1]; height];
    let mut current = vec![0u32; width];
    let mut previous = vec![0u32; width];
    let mut cache = init_cache();

    if parsed.mode8x2 {
        let output_width = width >> 1;
        let output_height = height
            .checked_mul(2)
            .ok_or_else(|| err(ImgErrorKind::IllegalData, "PIC output height overflow"))?;
        let mut pixels = vec![
            0u8;
            output_width.checked_mul(output_height).ok_or_else(|| err(
                ImgErrorKind::IllegalData,
                "PIC output size overflow"
            ))?
        ];
        let mut c = 0u32;
        let mut x = usize::MAX;
        let mut y = 0usize;
        loop {
            let mut len = read_pic_len(bits);
            loop {
                x = x.wrapping_add(1);
                if x == width {
                    let row_a = y * 2 * output_width;
                    let row_b = row_a + output_width;
                    let half = width >> 1;
                    for (i, value) in current.iter().take(half).enumerate() {
                        pixels[row_a + i] = (value >> 8) as u8;
                    }
                    for (i, value) in current.iter().enumerate().skip(half) {
                        pixels[row_b + (i - half)] = (value >> 8) as u8;
                    }
                    y += 1;
                    if y == height {
                        return Ok(PicDecoded {
                            width: output_width,
                            height: output_height,
                            pixels,
                            palette: parsed.palette.clone(),
                            rgb: false,
                        });
                    }
                    x = 0;
                    std::mem::swap(&mut previous, &mut current);
                }

                len -= 1;
                if len == 0 {
                    break;
                }
                let a = unpack_chain_value(&chain[y], x);
                if a != 0 {
                    let src_index = x + 4 - a as usize;
                    c = previous.get(src_index).copied().unwrap_or(0);
                }
                current[x] = c;
            }

            c = read_pic_color(bits, &mut cache, parsed.color_bits, parsed.decode_mode);
            current[x] = c;
            if bits.read_bit() != 0 {
                expand_chain(bits, &mut chain, width, height, x as isize, y);
            }
        }
    }

    let mut pixels =
        vec![
            0u8;
            width
                .checked_mul(height)
                .and_then(|px| px.checked_mul(3))
                .ok_or_else(|| err(ImgErrorKind::IllegalData, "PIC RGB output size overflow"))?
        ];
    let mut c = 0u32;
    let mut x = usize::MAX;
    let mut y = 0usize;
    loop {
        let mut len = read_pic_len(bits);
        loop {
            x = x.wrapping_add(1);
            if x == width {
                let mut out = y * width * 3;
                for value in &current {
                    let rgb = packed_to_rgb(*value);
                    pixels[out] = rgb[0];
                    pixels[out + 1] = rgb[1];
                    pixels[out + 2] = rgb[2];
                    out += 3;
                }
                y += 1;
                if y == height {
                    return Ok(PicDecoded {
                        width,
                        height,
                        pixels,
                        palette: None,
                        rgb: true,
                    });
                }
                x = 0;
                std::mem::swap(&mut previous, &mut current);
            }

            len -= 1;
            if len == 0 {
                break;
            }
            let a = unpack_chain_value(&chain[y], x);
            if a != 0 {
                let src_index = x + 4 - a as usize;
                c = previous.get(src_index).copied().unwrap_or(0);
            }
            current[x] = c;
        }

        c = read_pic_color(bits, &mut cache, parsed.color_bits, parsed.decode_mode);
        current[x] = c;
        if bits.read_bit() != 0 {
            expand_chain(bits, &mut chain, width, height, x as isize, y);
        }
    }
}

fn scale_indexed_nearest(
    image: PicDecoded,
    target_width: usize,
    target_height: usize,
) -> Result<PicDecoded, Error> {
    if target_width == 0 || target_height == 0 {
        return Err(err(
            ImgErrorKind::IllegalData,
            "PIC target dimensions must be non-zero",
        ));
    }
    let mut pixels = vec![
        0u8;
        target_width.checked_mul(target_height).ok_or_else(|| err(
            ImgErrorKind::IllegalData,
            "PIC scaled output size overflow"
        ))?
    ];
    for y in 0..target_height {
        let src_y = (y * image.height / target_height).min(image.height.saturating_sub(1));
        for x in 0..target_width {
            let src_x = (x * image.width / target_width).min(image.width.saturating_sub(1));
            pixels[y * target_width + x] = image.pixels[src_y * image.width + src_x];
        }
    }
    Ok(PicDecoded {
        width: target_width,
        height: target_height,
        pixels,
        palette: image.palette,
        rgb: false,
    })
}

fn scale_rgb_nearest(
    image: PicDecoded,
    target_width: usize,
    target_height: usize,
) -> Result<PicDecoded, Error> {
    if target_width == 0 || target_height == 0 {
        return Err(err(
            ImgErrorKind::IllegalData,
            "PIC target dimensions must be non-zero",
        ));
    }
    let mut pixels = vec![
        0u8;
        target_width
            .checked_mul(target_height)
            .and_then(|px| px.checked_mul(3))
            .ok_or_else(|| err(
                ImgErrorKind::IllegalData,
                "PIC scaled RGB output size overflow"
            ))?
    ];
    for y in 0..target_height {
        let src_y = (y * image.height / target_height).min(image.height.saturating_sub(1));
        for x in 0..target_width {
            let src_x = (x * image.width / target_width).min(image.width.saturating_sub(1));
            let src = (src_y * image.width + src_x) * 3;
            let dst = (y * target_width + x) * 3;
            let src_end = src + 3;
            let dst_end = dst + 3;
            if src_end > image.pixels.len() || dst_end > pixels.len() {
                return Err(err(
                    ImgErrorKind::IllegalData,
                    "PIC scale copy is out of range",
                ));
            }
            pixels[dst..dst_end].copy_from_slice(&image.pixels[src..src_end]);
        }
    }
    Ok(PicDecoded {
        width: target_width,
        height: target_height,
        pixels,
        palette: None,
        rgb: true,
    })
}

fn apply_aspect(image: PicDecoded, parsed: &PicParsed) -> Result<PicDecoded, Error> {
    let height = image.height;
    if parsed.mode8x2 {
        let target_width = image
            .width
            .checked_mul(2)
            .ok_or_else(|| err(ImgErrorKind::IllegalData, "PIC target width overflow"))?;
        return if image.rgb {
            scale_rgb_nearest(image, target_width, height)
        } else {
            scale_indexed_nearest(image, target_width, height)
        };
    }
    if parsed.aspect == ASPX68 {
        let target_width = image
            .width
            .checked_mul(3)
            .map(|value| (value / 2).max(1))
            .ok_or_else(|| err(ImgErrorKind::IllegalData, "PIC target width overflow"))?;
        return if image.rgb {
            scale_rgb_nearest(image, target_width, height)
        } else {
            scale_indexed_nearest(image, target_width, height)
        };
    }
    Ok(image)
}

pub fn decode<B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let data = read_all(reader)?;
    let (mut bits, parsed) = parse_header(&data)?;
    let decoded = if parsed.color_bits <= 8 {
        decode_indexed(&mut bits, &parsed)
    } else {
        decode_direct(&mut bits, &parsed)
    }?;
    let decoded = apply_aspect(decoded, &parsed)?;

    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("PIC".to_string()))?;
    option
        .drawer
        .set_metadata("width", DataMap::UInt(decoded.width as u64))?;
    option
        .drawer
        .set_metadata("height", DataMap::UInt(decoded.height as u64))?;
    option
        .drawer
        .set_metadata("start x", DataMap::UInt(parsed.start_x as u64))?;
    option
        .drawer
        .set_metadata("start y", DataMap::UInt(parsed.start_y as u64))?;
    option
        .drawer
        .set_metadata("comment", DataMap::SJISString(parsed.comment))?;

    if decoded.rgb {
        draw_rgb(option, decoded.width, decoded.height, &decoded.pixels)?;
    } else {
        let palette = decoded
            .palette
            .ok_or_else(|| err(ImgErrorKind::IllegalData, "PIC palette is missing"))?;
        draw_indexed(
            option,
            decoded.width,
            decoded.height,
            &decoded.pixels,
            &palette,
        )?;
    }
    Ok(None)
}
