//! JPEG encoder implementation.

use super::fdct::fdct_block;
use super::huffman::{HuffmanWriteTables, default_huffman_writer, encode_block, write_dht};
use super::quantize_table::scaled_quant_tables;

type Error = Box<dyn std::error::Error>;

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

pub(crate) use super::bitwriter::BitWriter;
pub use super::quantize_table::create_qt;

pub struct EncodeOptions<'a> {
    pub width: usize,
    pub height: usize,
    pub rgba: &'a [u8],
    pub quality: usize,
}

impl<'a> EncodeOptions<'a> {
    pub fn new(width: usize, height: usize, rgba: &'a [u8], quality: usize) -> Self {
        Self {
            width,
            height,
            rgba,
            quality,
        }
    }
}

fn rgb_to_ycbcr(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32;
    let g = g as f32;
    let b = b as f32;
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let cb = -0.168736 * r - 0.331264 * g + 0.5 * b + 128.0;
    let cr = 0.5 * r - 0.418688 * g - 0.081312 * b + 128.0;
    (y, cb, cr)
}

fn extract_block(image: &EncodeOptions<'_>, component: usize, x0: usize, y0: usize) -> [f32; 64] {
    let mut block = [0.0_f32; 64];

    for dy in 0..8 {
        let y = (y0 + dy).min(image.height.saturating_sub(1));
        for dx in 0..8 {
            let x = (x0 + dx).min(image.width.saturating_sub(1));
            let idx = (y * image.width + x) * 4;
            let (ycc, cb, cr) =
                rgb_to_ycbcr(image.rgba[idx], image.rgba[idx + 1], image.rgba[idx + 2]);
            let value = match component {
                0 => ycc,
                1 => cb,
                _ => cr,
            };
            block[dy * 8 + dx] = value - 128.0;
        }
    }

    block
}

fn quantize_block(coeffs: &[f32; 64], table: &[u8; 64]) -> [i32; 64] {
    let mut out = [0i32; 64];
    for i in 0..64 {
        let natural = ZIGZAG[i];
        out[i] = (coeffs[natural] / table[natural] as f32).round() as i32;
    }
    out
}

fn write_marker(buf: &mut Vec<u8>, marker: u8) {
    buf.push(0xff);
    buf.push(marker);
}

fn write_u16_be(buf: &mut Vec<u8>, value: u16) {
    buf.push((value >> 8) as u8);
    buf.push((value & 0xff) as u8);
}

fn write_soi(buf: &mut Vec<u8>) {
    write_marker(buf, 0xd8);
}

fn write_jfif(buf: &mut Vec<u8>) {
    write_marker(buf, 0xe0);
    write_u16_be(buf, 16);
    buf.extend_from_slice(b"JFIF\0");
    buf.push(1);
    buf.push(1);
    buf.push(0);
    write_u16_be(buf, 1);
    write_u16_be(buf, 1);
    buf.push(0);
    buf.push(0);
}

fn write_dqt(buf: &mut Vec<u8>, luma: &[u8; 64], chroma: &[u8; 64]) {
    write_marker(buf, 0xdb);
    write_u16_be(buf, 132);
    buf.push(0x00);
    for &index in &ZIGZAG {
        buf.push(luma[index]);
    }
    buf.push(0x01);
    for &index in &ZIGZAG {
        buf.push(chroma[index]);
    }
}

fn write_sof0(buf: &mut Vec<u8>, width: usize, height: usize) {
    write_marker(buf, 0xc0);
    write_u16_be(buf, 17);
    buf.push(8);
    write_u16_be(buf, height as u16);
    write_u16_be(buf, width as u16);
    buf.push(3);
    buf.push(1);
    buf.push(0x11);
    buf.push(0);
    buf.push(2);
    buf.push(0x11);
    buf.push(1);
    buf.push(3);
    buf.push(0x11);
    buf.push(1);
}

fn write_sos(buf: &mut Vec<u8>) {
    write_marker(buf, 0xda);
    write_u16_be(buf, 12);
    buf.push(3);
    buf.push(1);
    buf.push(0x00);
    buf.push(2);
    buf.push(0x11);
    buf.push(3);
    buf.push(0x11);
    buf.push(0);
    buf.push(63);
    buf.push(0);
}

fn write_eoi(buf: &mut Vec<u8>) {
    write_marker(buf, 0xd9);
}

fn encode_mcu_row(
    image: &EncodeOptions<'_>,
    y: usize,
    padded_width: usize,
    luma_q: &[u8; 64],
    chroma_q: &[u8; 64],
    entropy: &mut BitWriter,
    preds: &mut [i32; 3],
    huffman: &HuffmanWriteTables,
) -> Result<(), Error> {
    for x in (0..padded_width).step_by(8) {
        let y_block = quantize_block(&fdct_block(&extract_block(image, 0, x, y)), luma_q);
        let cb_block = quantize_block(&fdct_block(&extract_block(image, 1, x, y)), chroma_q);
        let cr_block = quantize_block(&fdct_block(&extract_block(image, 2, x, y)), chroma_q);

        encode_block(
            entropy,
            &y_block,
            &mut preds[0],
            &huffman.lum_dc,
            &huffman.lum_ac,
        )?;
        encode_block(
            entropy,
            &cb_block,
            &mut preds[1],
            &huffman.chrom_dc,
            &huffman.chrom_ac,
        )?;
        encode_block(
            entropy,
            &cr_block,
            &mut preds[2],
            &huffman.chrom_dc,
            &huffman.chrom_ac,
        )?;
    }

    Ok(())
}

pub fn encode(image: &EncodeOptions<'_>) -> Result<Vec<u8>, Error> {
    if image.width == 0 || image.height == 0 {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "image dimensions must be non-zero",
        )));
    }

    let expected_len = image
        .width
        .checked_mul(image.height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "image dimensions overflow",
            )) as Error
        })?;
    if image.rgba.len() != expected_len {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "rgba buffer length does not match width * height * 4",
        )));
    }

    if image.width > u16::MAX as usize || image.height > u16::MAX as usize {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "jpeg dimensions must fit in u16",
        )));
    }

    let (luma_q, chroma_q) = scaled_quant_tables(image.quality);
    let huffman = default_huffman_writer();
    let mut data = Vec::new();
    let mut entropy = BitWriter::new();
    let mut preds = [0_i32; 3];

    write_soi(&mut data);
    write_jfif(&mut data);
    write_dqt(&mut data, &luma_q, &chroma_q);
    write_sof0(&mut data, image.width, image.height);
    write_dht(&mut data);
    write_sos(&mut data);

    let padded_width = image.width.div_ceil(8) * 8;
    let padded_height = image.height.div_ceil(8) * 8;

    for y in (0..padded_height).step_by(8) {
        encode_mcu_row(
            image,
            y,
            padded_width,
            &luma_q,
            &chroma_q,
            &mut entropy,
            &mut preds,
            &huffman,
        )?;
    }

    entropy.flush()?;
    data.extend_from_slice(&entropy.buf);
    write_eoi(&mut data);

    Ok(data)
}
