//! MAG decoder implementation.

use bin_rs::reader::BinaryReader;

use crate::color::RGBA;
use crate::draw::*;
use crate::mag::header::{MAGFileHeader, MAGHeader};
use crate::metadata::DataMap;
use crate::warning::ImgWarnings;

type Error = Box<dyn std::error::Error>;

pub struct MAGDecoder {
    pub header: MAGHeader,
    pub pixels: Vec<RGBA>,
}

pub struct ChunckStream {
    base_offset: u32,
    xpoint: u32,
    left: u32,
    chunk_size: u32,
    chunk: Vec<u8>,
    pos: usize,
    loaded: usize,
}

impl ChunckStream {
    fn new(base_offset: u32, start_xpoint: u32, total_left: u32, chunk_size: u32) -> Self {
        Self {
            base_offset,
            xpoint: start_xpoint,
            left: total_left,
            chunk_size,
            chunk: vec![0; chunk_size as usize],
            pos: chunk_size as usize, // force first refill
            loaded: 0,
        }
    }

    fn refill<B: BinaryReader>(&mut self, reader: &mut B) -> Result<(), Error> {
        let abs = self.base_offset + self.xpoint;
        reader.seek(std::io::SeekFrom::Start(abs as u64))?;

        let copy_size = self.chunk_size.min(self.left) as usize;
        self.chunk.fill(0);

        if copy_size > 0 {
            reader.read_exact(&mut self.chunk[..copy_size])?;
        }

        self.xpoint += self.chunk_size;
        self.left = self.left.saturating_sub(self.chunk_size);
        self.pos = 0;
        self.loaded = copy_size;
        Ok(())
    }

    fn read_u8<B: BinaryReader>(&mut self, reader: &mut B) -> Result<u8, Error> {
        if self.pos >= self.loaded {
            self.refill(reader)?;
        }
        if self.pos >= self.loaded {
            return Ok(0);
        }
        let v = self.chunk[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u16_le<B: BinaryReader>(&mut self, reader: &mut B) -> Result<u16, Error> {
        let b0 = self.read_u8(reader)? as u16;
        let b1 = self.read_u8(reader)? as u16;
        Ok((b1 << 8) | b0)
    }
}

#[inline]
fn read_word_le(buf: &[u8], word_index: usize) -> u16 {
    let off = word_index * 2;
    if off + 1 >= buf.len() {
        0
    } else {
        (buf[off] as u16) | ((buf[off + 1] as u16) << 8)
    }
}

#[inline]
fn write_word_le(buf: &mut [u8], word_index: usize, value: u16) {
    let off = word_index * 2;
    if off + 1 < buf.len() {
        buf[off] = (value & 0x00FF) as u8;
        buf[off + 1] = (value >> 8) as u8;
    }
}

pub fn decode<B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let file_header = MAGFileHeader::new(reader)?;

    // MAGFileHeader::new() を読み終えた時点の位置が、そのまま Start_offset
    let start_offset = reader.seek(std::io::SeekFrom::Current(0))? as u32;

    let header = MAGHeader::new(reader)?;

    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("MAG".to_string()))?;
    option.drawer.set_metadata(
        "width",
        DataMap::UInt((header.end_x - header.start_x + 1) as u64),
    )?;
    option.drawer.set_metadata(
        "height",
        DataMap::UInt((header.end_y - header.start_y + 1) as u64),
    )?;
    option.drawer.set_metadata(
        "machine",
        DataMap::I18NString(format!("{:02X}", header.machine)),
    )?;
    option
        .drawer
        .set_metadata("user", DataMap::SJISString(file_header.get_user()))?;
    option.drawer.set_metadata(
        "comment",
        DataMap::SJISString(file_header.get_comment().unwrap_or_default()),
    )?;

    let width = header.end_x as usize - header.start_x as usize + 1;
    let height = header.end_y as usize - header.start_y as usize + 1;
    if width == 0 || height == 0 {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "MAG dimensions must be non-zero",
        )));
    }

    option.drawer.init(width, height, InitOptions::new())?;

    let ncolors = header.get_number_of_colors();

    // Use the same `_startx` / `Xsize` alignment as the JS implementation.
    let (aligned_start_x, xsize) = if ncolors == 256 {
        let s = (header.start_x / 4) * 4;
        let xs = ((header.end_x + 4) / 4) * 4 - s;
        (s as usize, xs as usize)
    } else {
        let s = (header.start_x / 8) * 8;
        let mut xs = ((header.end_x + 8) / 8) * 8 - s;
        xs = (xs + 1) / 2;
        (s as usize, xs as usize)
    };

    let x_blocks = xsize / 4;
    let size_flag_a = (x_blocks * height + 7) / 8;

    let flag_a_offset = start_offset + header.flag_a_offset;
    reader.seek(std::io::SeekFrom::Start(flag_a_offset as u64))?;
    let flag_a = reader.read_bytes_as_vec(size_flag_a)?;

    let chunk_size = 0x8000;

    let mut flag_b_stream = ChunckStream::new(
        start_offset,
        header.flag_b_offset,
        header.flag_b_size,
        chunk_size,
    );
    flag_b_stream.refill(reader)?;

    let mut pixel_stream = ChunckStream::new(
        start_offset,
        header.pixel_d_offset,
        header.pixel_d_size,
        chunk_size,
    );
    pixel_stream.refill(reader)?;

    let mut flag_b_map = vec![0u8; x_blocks + 1];

    // Equivalent to the JS implementation's `ArrayBuffer(Xsize * 16)`.
    let mut line_buf = vec![0u8; xsize * 16];

    let flag_x = [0, -1, -2, -4, 0, -1, 0, -1, -2, 0, -1, -2, 0, -1, -2, 0];
    let flag_y = [0, 0, 0, 0, -1, -1, -2, -2, -2, -4, -4, -4, -8, -8, -8, 0];

    let mut flag_a_idx = 0usize;
    let mut ibit = 0x80u8;
    let mut flag_a_cur = flag_a.get(flag_a_idx).copied().unwrap_or(0);
    flag_a_idx += 1;

    let mut j = 15i32;
    let mut pixels = vec![0u8; width * height];

    for y in header.start_y..=header.end_y {
        j += 1;

        let mut flag = [0i32; 16];
        for i in 0..16 {
            flag[i] = (flag_x[i] * 2) + (((flag_y[i] + j) & 0xF) * xsize as i32);
        }
        let ftmp = flag[0];
        if j == 16 {
            j = 0;
        }

        let mut x = 0usize;
        let mut map_idx = 0usize;

        while x < xsize {
            if (flag_a_cur & ibit) != 0 {
                let b = flag_b_stream.read_u8(reader)?;
                if map_idx < flag_b_map.len() {
                    flag_b_map[map_idx] ^= b;
                }
            }

            ibit >>= 1;
            if ibit == 0 {
                ibit = 0x80;
                flag_a_cur = flag_a.get(flag_a_idx).copied().unwrap_or(0);
                flag_a_idx += 1;
            }

            let fr = *flag_b_map.get(map_idx).unwrap_or(&0);
            map_idx += 1;

            let hi = (fr >> 4) as usize;
            let dst1 = ftmp + x as i32;
            if dst1 >= 0 {
                let dst_word_index1 = (dst1 as usize) >> 1;
                let value = if hi == 0 {
                    pixel_stream.read_u16_le(reader)?
                } else {
                    let src = flag[hi] + x as i32;
                    if src >= 0 {
                        let src_word_index = (src as usize) >> 1;
                        read_word_le(&line_buf, src_word_index)
                    } else {
                        0
                    }
                };
                write_word_le(&mut line_buf, dst_word_index1, value);
            }
            x += 2;

            let lo = (fr & 0x0F) as usize;
            let dst2 = ftmp + x as i32;
            if dst2 >= 0 {
                let dst_word_index2 = (dst2 as usize) >> 1;
                let value = if lo == 0 {
                    pixel_stream.read_u16_le(reader)?
                } else {
                    let src = flag[lo] + x as i32;
                    if src >= 0 {
                        let src_word_index = (src as usize) >> 1;
                        read_word_le(&line_buf, src_word_index)
                    } else {
                        0
                    }
                };
                write_word_le(&mut line_buf, dst_word_index2, value);
            }
            x += 2;
        }

        let row_out = (y - header.start_y) as usize * width;

        if ncolors == 256 {
            let src_off = ftmp as usize + (header.start_x as usize - aligned_start_x);
            if src_off + width > line_buf.len() || row_out + width > pixels.len() {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "MAG scanline is out of range",
                )));
            }
            let src = &line_buf[src_off..src_off + width];
            pixels[row_out..row_out + width].copy_from_slice(src);
        } else {
            let mut expanded = vec![0u8; xsize * 2];
            for m in 0..xsize {
                let c = line_buf[ftmp as usize + m];
                expanded[m * 2] = c >> 4;
                expanded[m * 2 + 1] = c & 0x0F;
            }
            let pix_off = header.start_x as usize - aligned_start_x;
            if pix_off + width > expanded.len() || row_out + width > pixels.len() {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "MAG expanded scanline is out of range",
                )));
            }
            pixels[row_out..row_out + width].copy_from_slice(&expanded[pix_off..pix_off + width]);
        }
    }

    let mut rgba_pixels = vec![0u8; width * height * 4];
    for i in 0..(width * height) {
        let c = pixels[i] as usize;
        let Some(&(r, g, b)) = header.palette.get(c) else {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "MAG palette index out of range",
            )));
        };
        rgba_pixels[i * 4] = r;
        rgba_pixels[i * 4 + 1] = g;
        rgba_pixels[i * 4 + 2] = b;
        rgba_pixels[i * 4 + 3] = 0xFF;
    }

    option.drawer.draw(
        header.start_x as usize,
        header.start_y as usize,
        width,
        height,
        &rgba_pixels,
        None,
    )?;

    Ok(None)
}
