//! VSP decoder implementation.

use bin_rs::reader::BinaryReader;

use crate::draw::{
    DecodeOptions, ImageRect, InitOptions, NextOption, NextOptions, ResponseCommand,
};
use crate::error::ImgErrorKind;
use crate::metadata::DataMap;
use crate::retro::{ByteCursor, draw_indexed, err, read_all};
use crate::warning::ImgWarnings;

type Error = Box<dyn std::error::Error>;

#[derive(Clone)]
struct VspHeader {
    start_x: usize,
    start_y: usize,
    end_x: usize,
    end_y: usize,
    pixel: u8,
    palette_raw: Vec<u8>,
}

#[derive(Clone)]
struct DatEntry {
    index: usize,
    offset: usize,
}

fn normalize_vsp_color(value: u8) -> u8 {
    (value << 4) | value
}

fn build_vsp16_palette(raw: &[u8]) -> Vec<(u8, u8, u8)> {
    let mut palette = Vec::with_capacity(16);
    for i in 0..16 {
        palette.push((
            normalize_vsp_color(raw[i * 3 + 1]),
            normalize_vsp_color(raw[i * 3 + 2]),
            normalize_vsp_color(raw[i * 3]),
        ));
    }
    palette
}

fn parse_header(data: &[u8], offset: usize) -> Result<VspHeader, Error> {
    let mut reader = ByteCursor::new(data, offset);
    let start_x = reader.read_u16_le()? as usize;
    let start_y = reader.read_u16_le()? as usize;
    let end_x = reader.read_u16_le()? as usize;
    let end_y = reader.read_u16_le()? as usize;
    let pixel = reader.read_u8()?;
    let _reserve = reader.read_u8()?;
    let palette_raw = reader.read_bytes(48)?.to_vec();
    Ok(VspHeader {
        start_x,
        start_y,
        end_x,
        end_y,
        pixel,
        palette_raw,
    })
}

fn list_dat_entries(data: &[u8]) -> Result<Vec<DatEntry>, Error> {
    let mut page_reader = ByteCursor::new(data, 0);
    let page_count = page_reader.read_u16_le()? as usize;
    if page_count == 0 || page_count > 0x10 {
        return Err(err(
            ImgErrorKind::IllegalData,
            "Invalid VSP DAT index table",
        ));
    }

    let mut offsets = vec![0u16; page_count * 128];
    offsets[0] = page_count as u16;
    let mut fnum = 0usize;
    let mut table_offset = 0usize;

    for _ in 1..page_count {
        let mut reader = ByteCursor::new(data, table_offset);
        let mut j = 0usize;
        loop {
            let x = reader.read_u16_le()?;
            if x != 0 {
                offsets[fnum] = x;
                if fnum != 0 && offsets[fnum - 1] > offsets[fnum] {
                    return Err(err(
                        ImgErrorKind::IllegalData,
                        "Invalid VSP DAT entry order",
                    ));
                }
            }
            fnum += 1;
            j += 1;
            if x == 0 || j >= 128 {
                break;
            }
        }
        table_offset += 256;
    }

    fnum = fnum.saturating_sub(3);
    let mut entries = Vec::new();
    for i in 1..=fnum {
        entries.push(DatEntry {
            index: i - 1,
            offset: (offsets[i] as usize).saturating_sub(1) * 256,
        });
    }
    Ok(entries)
}

fn decode_vsp16(
    data: &[u8],
    offset: usize,
    header: &VspHeader,
) -> Result<(usize, usize, Vec<u8>, Vec<(u8, u8, u8)>), Error> {
    let width_cells = header.end_x.saturating_sub(header.start_x);
    let height = header.end_y.saturating_sub(header.start_y);
    let width = width_cells * 8;
    if width == 0 || height == 0 {
        return Err(err(
            ImgErrorKind::IllegalData,
            "VSP dimensions must be non-zero",
        ));
    }
    let mut pixels = vec![0u8; width * height];
    let mut reader = ByteCursor::new(data, offset + 0x3a);
    let mut planes = vec![vec![vec![0u8; height + 1]; 2]; 4];

    for x in 0..width_cells {
        let xb = if x == 0 { 1 } else { (x - 1) & 1 };
        let xw = x & 1;
        let mut s = 0u8;
        for pal in 0..4 {
            let mut y = 0usize;
            while y < height {
                let flag = reader.read_u8()?;
                match flag {
                    0x00 => {
                        let count = reader.read_u8()? as usize;
                        for _ in 0..=count {
                            if y >= height {
                                break;
                            }
                            planes[pal][xw][y] = planes[pal][xb][y];
                            y += 1;
                        }
                    }
                    0x01 => {
                        let count = reader.read_u8()? as usize;
                        let value = reader.read_u8()?;
                        for _ in 0..=count {
                            if y >= height {
                                break;
                            }
                            planes[pal][xw][y] = value;
                            y += 1;
                        }
                    }
                    0x02 => {
                        let count = reader.read_u8()? as usize;
                        let a = reader.read_u8()?;
                        let b = reader.read_u8()?;
                        for _ in 0..=count {
                            if y < height {
                                planes[pal][xw][y] = a;
                                y += 1;
                            }
                            if y < height {
                                planes[pal][xw][y] = b;
                                y += 1;
                            }
                        }
                    }
                    0x03..=0x05 => {
                        let count = reader.read_u8()? as usize;
                        let src_plane = (flag - 3) as usize;
                        for _ in 0..=count {
                            if y >= height {
                                break;
                            }
                            let value = planes[src_plane][xw][y];
                            planes[pal][xw][y] = if s == 0 { value } else { !value };
                            y += 1;
                        }
                        s = 0;
                    }
                    0x06 => s = 1,
                    0x07 => {
                        if y >= height {
                            break;
                        }
                        planes[pal][xw][y] = reader.read_u8()?;
                        y += 1;
                    }
                    _ => {
                        if y >= height {
                            break;
                        }
                        planes[pal][xw][y] = flag;
                        y += 1;
                    }
                }
            }
        }

        for y in 0..height {
            let b = planes[0][xw][y];
            let r = planes[1][xw][y];
            let g = planes[2][xw][y];
            let e = planes[3][xw][y];
            let row = y * width + x * 8;
            pixels[row] =
                ((b >> 7) & 0x01) | ((r >> 6) & 0x02) | ((g >> 5) & 0x04) | ((e >> 4) & 0x08);
            pixels[row + 1] =
                ((b >> 6) & 0x01) | ((r >> 5) & 0x02) | ((g >> 4) & 0x04) | ((e >> 3) & 0x08);
            pixels[row + 2] =
                ((b >> 5) & 0x01) | ((r >> 4) & 0x02) | ((g >> 3) & 0x04) | ((e >> 2) & 0x08);
            pixels[row + 3] =
                ((b >> 4) & 0x01) | ((r >> 3) & 0x02) | ((g >> 2) & 0x04) | ((e >> 1) & 0x08);
            pixels[row + 4] =
                ((b >> 3) & 0x01) | ((r >> 2) & 0x02) | ((g >> 1) & 0x04) | (e & 0x08);
            pixels[row + 5] =
                ((b >> 2) & 0x01) | ((r >> 1) & 0x02) | (g & 0x04) | ((e << 1) & 0x08);
            pixels[row + 6] =
                ((b >> 1) & 0x01) | (r & 0x02) | ((g << 1) & 0x04) | ((e << 2) & 0x08);
            pixels[row + 7] =
                (b & 0x01) | ((r << 1) & 0x02) | ((g << 2) & 0x04) | ((e << 3) & 0x08);
        }
    }

    Ok((
        width,
        height,
        pixels,
        build_vsp16_palette(&header.palette_raw),
    ))
}

fn decode_vsp256(
    data: &[u8],
    offset: usize,
    header: &VspHeader,
) -> Result<(usize, usize, Vec<u8>, Vec<(u8, u8, u8)>), Error> {
    let width = header.end_x.saturating_sub(header.start_x) + 1;
    let height = header.end_y.saturating_sub(header.start_y) + 1;
    let mut reader = ByteCursor::new(data, offset + 0x320);
    let mut pixels = vec![0u8; width * height];
    let mut rows = vec![vec![0u8; 640]; 3];

    for y in 0..height {
        let yb2 = y.wrapping_sub(2) % 3;
        let yb = y.wrapping_sub(1) % 3;
        let yw = y % 3;
        let row_prev = rows[yb].clone();
        let row_prev2 = rows[yb2].clone();
        let row = &mut rows[yw];
        let mut x = 0usize;
        while x < width {
            let flag = reader.read_u8()?;
            if flag < 0xf8 {
                row[x] = flag;
                x += 1;
                continue;
            }
            match flag {
                0xff => {
                    let count = reader.read_u8()? as usize + 3;
                    let span = count
                        .min(width.saturating_sub(x))
                        .min(row.len().saturating_sub(x));
                    row[x..x + span].copy_from_slice(&row_prev[x..x + span]);
                    x += span;
                }
                0xfe => {
                    let count = reader.read_u8()? as usize + 3;
                    let span = count
                        .min(width.saturating_sub(x))
                        .min(row.len().saturating_sub(x));
                    row[x..x + span].copy_from_slice(&row_prev2[x..x + span]);
                    x += span;
                }
                0xfd => {
                    let count = reader.read_u8()? as usize + 4;
                    let value = reader.read_u8()?;
                    let span = count
                        .min(width.saturating_sub(x))
                        .min(row.len().saturating_sub(x));
                    row[x..x + span].fill(value);
                    x += span;
                }
                0xfc => {
                    let count = reader.read_u8()? as usize + 3;
                    let a = reader.read_u8()?;
                    let b = reader.read_u8()?;
                    for _ in 0..count {
                        if x >= width || x >= row.len() {
                            break;
                        }
                        row[x] = a;
                        x += 1;
                        if x >= width || x >= row.len() {
                            break;
                        }
                        row[x] = b;
                        x += 1;
                    }
                }
                _ => {
                    row[x] = reader.read_u8()?;
                    x += 1;
                }
            }
        }
        pixels[y * width..(y + 1) * width].copy_from_slice(&row[..width]);
    }

    let mut palette_reader = ByteCursor::new(data, offset + 0x20);
    let palette_bytes = palette_reader.read_bytes(768)?;
    let mut palette = Vec::with_capacity(256);
    for i in 0..256 {
        palette.push((
            palette_bytes[i * 3],
            palette_bytes[i * 3 + 1],
            palette_bytes[i * 3 + 2],
        ));
    }
    Ok((width, height, pixels, palette))
}

fn decode_vsp_at(
    data: &[u8],
    offset: usize,
) -> Result<(VspHeader, usize, usize, Vec<u8>, Vec<(u8, u8, u8)>), Error> {
    let header = parse_header(data, offset)?;
    let decoded = if (header.start_x > 80 || header.end_x > 80) || header.pixel == 1 {
        decode_vsp256(data, offset, &header)?
    } else if header.pixel == 0 || header.pixel == 8 {
        decode_vsp16(data, offset, &header)?
    } else {
        return Err(err(ImgErrorKind::IllegalData, "Unsupported VSP format"));
    };
    Ok((header, decoded.0, decoded.1, decoded.2, decoded.3))
}

fn is_probable_dat(data: &[u8]) -> bool {
    list_dat_entries(data)
        .map(|entries| !entries.is_empty())
        .unwrap_or(false)
}

fn indexed_to_rgba(
    width: usize,
    height: usize,
    pixels: &[u8],
    palette: &[(u8, u8, u8)],
) -> Vec<u8> {
    let mut rgba = vec![0u8; width * height * 4];
    for (i, &index) in pixels.iter().enumerate().take(width * height) {
        let (r, g, b) = palette.get(index as usize).copied().unwrap_or((0, 0, 0));
        let offset = i * 4;
        rgba[offset] = r;
        rgba[offset + 1] = g;
        rgba[offset + 2] = b;
        rgba[offset + 3] = 0xff;
    }
    rgba
}

pub fn decode<B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let data = read_all(reader)?;
    if is_probable_dat(&data) {
        let entries = list_dat_entries(&data)?;
        let mut decoded_entries = Vec::new();
        for entry in entries {
            if let Ok((header, width, height, pixels, palette)) = decode_vsp_at(&data, entry.offset)
            {
                decoded_entries.push((entry.index, header, width, height, pixels, palette));
            }
        }
        if decoded_entries.is_empty() {
            return Err(err(
                ImgErrorKind::IllegalData,
                "No decodable VSP entry found in DAT",
            ));
        }

        let (_, first_header, first_width, first_height, first_pixels, first_palette) =
            decoded_entries.remove(0);
        option
            .drawer
            .set_metadata("Format", DataMap::Ascii("VSP".to_string()))?;
        option
            .drawer
            .set_metadata("container", DataMap::Ascii("DAT".to_string()))?;
        option.drawer.set_metadata(
            "image pages",
            DataMap::UInt((decoded_entries.len() + 1) as u64),
        )?;
        option
            .drawer
            .set_metadata("width", DataMap::UInt(first_width as u64))?;
        option
            .drawer
            .set_metadata("height", DataMap::UInt(first_height as u64))?;
        option
            .drawer
            .set_metadata("start x", DataMap::UInt(first_header.start_x as u64))?;
        option
            .drawer
            .set_metadata("start y", DataMap::UInt(first_header.start_y as u64))?;
        option
            .drawer
            .set_metadata("pixel mode", DataMap::UInt(first_header.pixel as u64))?;

        option.drawer.init(
            first_width,
            first_height,
            Some(InitOptions {
                loop_count: 0,
                background: None,
                animation: decoded_entries.len() > 0,
            }),
        )?;
        let rgba = indexed_to_rgba(first_width, first_height, &first_pixels, &first_palette);
        option
            .drawer
            .draw(0, 0, first_width, first_height, &rgba, None)?;

        for (entry_index, header, width, height, pixels, palette) in decoded_entries {
            let next = NextOptions {
                flag: NextOption::Next,
                await_time: 0,
                image_rect: Some(ImageRect {
                    start_x: header.start_x as i32,
                    start_y: header.start_y as i32,
                    width,
                    height,
                }),
                dispose_option: None,
                blend: None,
            };
            let result = option.drawer.next(Some(next))?;
            if let Some(response) = result {
                if response.response == ResponseCommand::Abort {
                    return Ok(None);
                }
            }
            option
                .drawer
                .set_metadata("dat entry index", DataMap::UInt(entry_index as u64))?;
            let rgba = indexed_to_rgba(width, height, &pixels, &palette);
            option.drawer.draw(0, 0, width, height, &rgba, None)?;
        }
        option.drawer.terminate(None)?;
        return Ok(None);
    }

    let (header, width, height, pixels, palette) = decode_vsp_at(&data, 0)?;
    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("VSP".to_string()))?;
    option
        .drawer
        .set_metadata("width", DataMap::UInt(width as u64))?;
    option
        .drawer
        .set_metadata("height", DataMap::UInt(height as u64))?;
    option
        .drawer
        .set_metadata("start x", DataMap::UInt(header.start_x as u64))?;
    option
        .drawer
        .set_metadata("start y", DataMap::UInt(header.start_y as u64))?;
    option
        .drawer
        .set_metadata("pixel mode", DataMap::UInt(header.pixel as u64))?;

    draw_indexed(option, width, height, &pixels, &palette)?;
    Ok(None)
}
