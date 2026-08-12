//! PI decoder implementation.

use bin_rs::reader::BinaryReader;

use crate::draw::DecodeOptions;
use crate::error::ImgErrorKind;
use crate::metadata::DataMap;
use crate::retro::{BitReaderWordMsb, ByteCursor, draw_indexed, err, read_all};
use crate::warning::ImgWarnings;

type Error = Box<dyn std::error::Error>;

struct PiHeader {
    width: usize,
    height: usize,
    plane: usize,
    palette: Vec<(u8, u8, u8)>,
    comment: Vec<u8>,
    dummy_comment: Vec<u8>,
    start_x: usize,
    start_y: usize,
    data_offset: usize,
}

fn create_default_palette(plane: usize) -> Vec<(u8, u8, u8)> {
    let color_count = 1usize << plane;
    let mut palette = Vec::with_capacity(color_count);
    if plane == 4 {
        for i in 0..16 {
            let level = if (i & 0x08) != 0 { 0xff } else { 0x77 };
            palette.push((
                if (i & 0x02) != 0 { level } else { 0 },
                if (i & 0x04) != 0 { level } else { 0 },
                if (i & 0x01) != 0 { level } else { 0 },
            ));
        }
        return palette;
    }

    for i in 0..color_count {
        palette.push((
            (((i & 0x1c) >> 2) * 255 / 7) as u8,
            (((i & 0xe0) >> 5) * 255 / 7) as u8,
            ((i & 0x03) * 255 / 3) as u8,
        ));
    }
    palette
}

fn create_mtf_tables(color_count: usize) -> Vec<Vec<u8>> {
    let mut tables = vec![vec![0u8; color_count]; color_count];
    for i in 0..color_count {
        for j in 0..color_count {
            tables[i][j] =
                ((i as i32 - j as i32 + color_count as i32) as usize & (color_count - 1)) as u8;
        }
    }
    tables
}

fn take_from_table(
    tables: &mut [Vec<u8>],
    row_index: usize,
    color_index: usize,
) -> Result<u8, Error> {
    let row = tables
        .get_mut(row_index)
        .ok_or_else(|| err(ImgErrorKind::IllegalData, "PI table row out of range"))?;
    if color_index >= row.len() {
        return Err(err(
            ImgErrorKind::IllegalData,
            "PI color index out of range",
        ));
    }
    let value = row[color_index];
    for i in (1..=color_index).rev() {
        row[i] = row[i - 1];
    }
    row[0] = value;
    Ok(value)
}

fn parse_header(data: &[u8]) -> Result<PiHeader, Error> {
    let mut reader = ByteCursor::new(data, 0);
    let signature = [reader.read_u8()?, reader.read_u8()?];
    if &signature != b"Pi" {
        return Err(err(ImgErrorKind::IllegalData, "Not a PI image"));
    }

    let mut comment = Vec::new();
    loop {
        let c = reader.read_u8()?;
        if c == 0x1a {
            break;
        }
        comment.push(c);
    }

    let mut dummy_comment = Vec::new();
    loop {
        let c = reader.read_u8()?;
        if c == 0x00 {
            break;
        }
        dummy_comment.push(c);
    }

    let msb = reader.read_u8()?;
    let _x_aspect = reader.read_u8()?;
    let _y_aspect = reader.read_u8()?;
    let plane = reader.read_u8()? as usize;
    let _machine = reader.read_bytes(4)?;
    let system_length = reader.read_u16_be()? as usize;
    let system_data = reader.read_bytes(system_length)?.to_vec();

    let mut start_x = 0usize;
    let mut start_y = 0usize;
    let mut i = 0usize;
    while i < system_data.len() {
        if system_data[i] < 0x20 && i + 4 < system_data.len() {
            if system_data[i] == 1 {
                start_x = ((system_data[i + 1] as usize) << 8) | system_data[i + 2] as usize;
                start_y = ((system_data[i + 3] as usize) << 8) | system_data[i + 4] as usize;
            }
            i += 5;
        } else {
            i += 4;
            if i < system_data.len() {
                i += system_data[i] as usize;
            }
        }
    }

    let width = reader.read_u16_be()? as usize;
    let height = reader.read_u16_be()? as usize;
    let palette = if (msb & 0x80) != 0 {
        create_default_palette(plane)
    } else {
        let bytes = reader.read_bytes((1usize << plane) * 3)?;
        let mut palette = Vec::with_capacity(1usize << plane);
        for i in 0..(1usize << plane) {
            palette.push((bytes[i * 3], bytes[i * 3 + 1], bytes[i * 3 + 2]));
        }
        palette
    };

    Ok(PiHeader {
        width,
        height,
        plane,
        palette,
        comment,
        dummy_comment,
        start_x,
        start_y,
        data_offset: reader.tell(),
    })
}

fn pi_get_len(bits: &mut BitReaderWordMsb<'_>) -> usize {
    let mut i = 0usize;
    while bits.read_bit() != 0 {
        i += 1;
    }
    if i == 0 {
        1
    } else {
        (1usize << i) + bits.read_bits(i) as usize
    }
}

fn pi_get_col(bits: &mut BitReaderWordMsb<'_>, collength: usize) -> usize {
    if bits.read_bit() != 0 {
        return if bits.read_bit() != 0 { 1 } else { 0 };
    }
    let mut i = 1usize;
    while i < collength && bits.read_bit() != 0 {
        i += 1;
    }
    bits.read_bits(i) as usize + (1usize << i)
}

fn pi_get_pos(bits: &mut BitReaderWordMsb<'_>) -> usize {
    if bits.read_bit() == 0 {
        if bits.read_bit() == 0 { 0 } else { 1 }
    } else if bits.read_bit() == 0 {
        2
    } else if bits.read_bit() == 0 {
        3
    } else {
        4
    }
}

fn emit_row(pixels: &mut [u8], width: usize, height: usize, line: &[u8], y: usize) -> usize {
    if y >= height {
        return y;
    }
    pixels[y * width..(y + 1) * width].copy_from_slice(&line[..width]);
    y + 1
}

fn copy_span(buf: &mut [u8], width: usize, x: usize, offset: usize, span: usize) {
    let dst = width * 2 + x;
    let src = x + offset;
    for i in 0..span {
        buf[dst + i] = buf[src + i];
    }
}

fn decode_pixels(bits: &mut BitReaderWordMsb<'_>, parsed: &PiHeader) -> Result<Vec<u8>, Error> {
    let width = parsed.width;
    let height = parsed.height;
    let color_count = 1usize << parsed.plane;
    let collength = parsed.plane.saturating_sub(1);
    let mut tables = create_mtf_tables(color_count);

    if width <= 2 {
        let mut pixels = vec![0u8; width * height];
        let mut x = 0usize;
        let mut y = 0usize;
        let mut cc = 0usize;
        while y < height {
            let clr = pi_get_col(bits, collength);
            let cb = take_from_table(&mut tables, cc, clr)? as usize;
            pixels[y * width + x] = cb as u8;
            cc = cb;
            x += 1;
            if x == width {
                x = 0;
                y += 1;
            }
        }
        return Ok(pixels);
    }

    let mut pixels = vec![0u8; width * height];
    let lx = [width * 2 - 2, width, 0, width + 1, width - 1, width * 2 - 4];
    let mut buf = vec![0u8; width * 3 + 2];
    let c1_first = take_from_table(&mut tables, 0, pi_get_col(bits, collength))? as usize;
    let c2_first = take_from_table(&mut tables, c1_first, pi_get_col(bits, collength))?;

    let mut i = 0usize;
    while i < width * 3 {
        buf[i] = c1_first as u8;
        if i + 1 < buf.len() {
            buf[i + 1] = c2_first;
        }
        i += 2;
    }

    let mut x = 0usize;
    let mut y = 0usize;
    let mut bpos = 6usize;

    while y < height {
        loop {
            let pos = pi_get_pos(bits);
            if pos == bpos {
                break;
            }
            let mut flag = pos;
            let mut len = pi_get_len(bits) * 2;
            if pos == 0 && buf[width * 2 + x - 1] != buf[width * 2 + x - 2] {
                flag = 5;
            }
            let offset = lx[flag];
            while len > 0 {
                let span = len.min(width - x);
                copy_span(&mut buf, width, x, offset, span);
                x += span;
                len -= span;
                if x == width {
                    let line = buf[width * 2..width * 3].to_vec();
                    y = emit_row(&mut pixels, width, height, &line, y);
                    if y >= height {
                        return Ok(pixels);
                    }
                    buf.copy_within(width..width * 3, 0);
                    x = 0;
                }
            }
            bpos = pos;
        }

        bpos = 6;
        let mut c2 = buf[width * 2 + x - 1] as usize;
        loop {
            let clr1 = pi_get_col(bits, collength);
            let c1 = take_from_table(&mut tables, c2, clr1)? as usize;
            let clr2 = pi_get_col(bits, collength);
            c2 = take_from_table(&mut tables, c1, clr2)? as usize;
            buf[width * 2 + x] = c1 as u8;
            if x + 1 < width {
                buf[width * 2 + x + 1] = c2 as u8;
            }
            x += 2;
            if x >= width {
                let line = buf[width * 2..width * 3].to_vec();
                y = emit_row(&mut pixels, width, height, &line, y);
                if y >= height {
                    return Ok(pixels);
                }
                buf.copy_within(width..width * 3, 0);
                if x > width {
                    buf[width * 2] = c2 as u8;
                    x = 1;
                } else {
                    x = 0;
                }
            }
            if bits.read_bit() == 0 {
                break;
            }
        }
    }

    Ok(pixels)
}

pub fn decode<B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let data = read_all(reader)?;
    let header = parse_header(&data)?;
    let mut bits = BitReaderWordMsb::new(&data, header.data_offset);
    let pixels = decode_pixels(&mut bits, &header)?;

    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("PI".to_string()))?;
    option
        .drawer
        .set_metadata("width", DataMap::UInt(header.width as u64))?;
    option
        .drawer
        .set_metadata("height", DataMap::UInt(header.height as u64))?;
    option
        .drawer
        .set_metadata("start x", DataMap::UInt(header.start_x as u64))?;
    option
        .drawer
        .set_metadata("start y", DataMap::UInt(header.start_y as u64))?;
    option
        .drawer
        .set_metadata("comment", DataMap::SJISString(header.comment))?;
    option
        .drawer
        .set_metadata("dummy comment", DataMap::SJISString(header.dummy_comment))?;

    draw_indexed(
        option,
        header.width,
        header.height,
        &pixels,
        &header.palette,
    )?;
    Ok(None)
}
