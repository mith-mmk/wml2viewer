//! MAKI decoder implementation.

use bin_rs::reader::BinaryReader;

use crate::draw::DecodeOptions;
use crate::error::ImgErrorKind;
use crate::metadata::DataMap;
use crate::retro::{ByteCursor, draw_indexed, err, read_all};
use crate::warning::ImgWarnings;

type Error = Box<dyn std::error::Error>;

struct MakiHeader {
    mode: usize,
    text: Vec<u8>,
    flag_b_size: usize,
    exp_flag: u16,
    start_x: usize,
    start_y: usize,
    width: usize,
    height: usize,
    palette: Vec<(u8, u8, u8)>,
    data_offset: usize,
}

fn parse_header(data: &[u8]) -> Result<MakiHeader, Error> {
    let mut reader = ByteCursor::new(data, 0);
    let header = reader.read_bytes(8)?;
    let text = reader.read_bytes(24)?.to_vec();
    let mode = match std::str::from_utf8(header).unwrap_or("") {
        "MAKI01A " => 2,
        "MAKI01B " => 4,
        _ => return Err(err(ImgErrorKind::IllegalData, "Not a MAKI image")),
    };

    let flag_b_size = reader.read_u16_be()? as usize;
    let _pixel_a_size = reader.read_u16_be()?;
    let _pixel_b_size = reader.read_u16_be()?;
    let exp_flag = reader.read_u16_be()?;
    let start_x = reader.read_u16_be()? as usize;
    let start_y = reader.read_u16_be()? as usize;
    let width = reader.read_u16_be()? as usize;
    let height = reader.read_u16_be()? as usize;

    let palette_bytes = reader.read_bytes(48)?;
    let mut palette = Vec::with_capacity(16);
    for i in 0..16 {
        palette.push((
            palette_bytes[i * 3 + 1],
            palette_bytes[i * 3],
            palette_bytes[i * 3 + 2],
        ));
    }

    Ok(MakiHeader {
        mode,
        text,
        flag_b_size,
        exp_flag,
        start_x,
        start_y,
        width,
        height,
        palette,
        data_offset: reader.tell(),
    })
}

pub fn decode<B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let data = read_all(reader)?;
    let header = parse_header(&data)?;
    if header.width == 0 || header.height == 0 {
        return Err(err(
            ImgErrorKind::IllegalData,
            "MAKI dimensions must be non-zero",
        ));
    }
    let mut cursor = ByteCursor::new(&data, header.data_offset);
    let ll = if (header.exp_flag & 0x01) != 0 {
        header.height.min(200)
    } else {
        header.height
    };

    let flag_a_size = (header.width * ll) / 256;
    let flag_a = cursor.read_bytes(flag_a_size)?.to_vec();
    let flag_b = cursor.read_bytes(header.flag_b_size)?.to_vec();

    let matrix_stride = header.width >> 4;
    let matrix_len = matrix_stride
        .checked_mul(ll)
        .ok_or_else(|| err(ImgErrorKind::IllegalData, "MAKI matrix size overflow"))?;
    let mut matrix = vec![0u8; matrix_len];
    let mut w = 0usize;
    let mut s = 0usize;
    let mut p = 0usize;

    for _ in (0..ll).step_by(4) {
        let mut t = s;
        for _ in (0..matrix_stride).step_by(4) {
            for mut i in [0x80u8, 0x20, 0x08, 0x02] {
                if p >= flag_a.len() {
                    break;
                }
                if (flag_a[p] & i) != 0 && w + 1 < flag_b.len() {
                    matrix[t] = flag_b[w] & 0xf0;
                    matrix[t + matrix_stride] = (flag_b[w] << 4) & 0xf0;
                    w += 1;
                    matrix[t + matrix_stride * 2] = flag_b[w] & 0xf0;
                    matrix[t + matrix_stride * 3] = (flag_b[w] << 4) & 0xf0;
                    w += 1;
                } else {
                    matrix[t] = 0;
                    matrix[t + matrix_stride] = 0;
                    matrix[t + matrix_stride * 2] = 0;
                    matrix[t + matrix_stride * 3] = 0;
                }
                i >>= 1;
                if (flag_a[p] & i) != 0 && w + 1 < flag_b.len() {
                    matrix[t] |= (flag_b[w] >> 4) & 0x0f;
                    matrix[t + matrix_stride] |= flag_b[w] & 0x0f;
                    w += 1;
                    matrix[t + matrix_stride * 2] |= (flag_b[w] >> 4) & 0x0f;
                    matrix[t + matrix_stride * 3] |= flag_b[w] & 0x0f;
                    w += 1;
                }
                t += 1;
            }
            p += 1;
        }
        s += matrix_stride * 4;
    }

    let mut lines = vec![vec![0u8; header.width]; 5];
    let pixel_count = header
        .width
        .checked_mul(header.height)
        .ok_or_else(|| err(ImgErrorKind::IllegalData, "MAKI pixel buffer size overflow"))?;
    let mut pixels = vec![0u8; pixel_count];
    let mut matrix_index = 0usize;

    for y in 0..ll {
        let mut x = 0usize;
        while x < header.width && matrix_index < matrix.len() {
            let value = matrix[matrix_index];
            for bit in (0..8).rev() {
                if x >= header.width {
                    break;
                }
                if ((value >> bit) & 1) != 0 {
                    let data = cursor.read_u8().unwrap_or(0);
                    lines[0][x] = ((data >> 4) & 0x0f) ^ lines[header.mode][x];
                    x += 1;
                    if x < header.width {
                        lines[0][x] = (data & 0x0f) ^ lines[header.mode][x];
                        x += 1;
                    }
                } else {
                    lines[0][x] = lines[header.mode][x];
                    x += 1;
                    if x < header.width {
                        lines[0][x] = lines[header.mode][x];
                        x += 1;
                    }
                }
            }
            matrix_index += 1;
        }

        let row = y * header.width;
        if row + header.width > pixels.len() {
            return Err(err(
                ImgErrorKind::IllegalData,
                "MAKI row output is out of range",
            ));
        }
        pixels[row..row + header.width].copy_from_slice(&lines[0]);
        lines.rotate_right(1);
    }

    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("MAKI".to_string()))?;
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
        .set_metadata("comment", DataMap::SJISString(header.text))?;

    draw_indexed(
        option,
        header.width,
        header.height,
        &pixels,
        &header.palette,
    )?;
    Ok(None)
}
