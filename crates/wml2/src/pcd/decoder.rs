//! Photo CD decoder implementation.

use bin_rs::reader::BinaryReader;

use crate::draw::DecodeOptions;
use crate::metadata::DataMap;
use crate::retro::{ByteCursor, clamp8, draw_rgb, err, read_all};
use crate::warning::ImgWarnings;

type Error = Box<dyn std::error::Error>;

const RGB_F_LL: i32 = 1391;
const RGB_F_C1: i32 = 2271;
const RGB_O_C1: i32 = -353784;
const RGB_F_C2: i32 = 1865;
const RGB_O_C2: i32 = -255023;
const RGB_F_G1: i32 = -441;
const RGB_F_G2: i32 = -949;
const RGB_O_G: i32 = 199313;

struct PcdSize {
    width: usize,
    height: usize,
    seek: usize,
    name: &'static str,
}

const PCD_SIZES: [PcdSize; 3] = [
    PcdSize {
        width: 192,
        height: 128,
        seek: 0x2000,
        name: "base16",
    },
    PcdSize {
        width: 384,
        height: 256,
        seek: 0xb800,
        name: "base4",
    },
    PcdSize {
        width: 768,
        height: 512,
        seek: 0x30000,
        name: "base",
    },
];

fn rotate_rgb(rgb: &[u8], width: usize, height: usize, orientation: u8) -> (usize, usize, Vec<u8>) {
    if orientation == 0 {
        return (width, height, rgb.to_vec());
    }
    if orientation == 2 {
        let mut out = vec![0u8; rgb.len()];
        for y in 0..height {
            for x in 0..width {
                let src = (y * width + x) * 3;
                let dst = ((height - 1 - y) * width + (width - 1 - x)) * 3;
                out[dst..dst + 3].copy_from_slice(&rgb[src..src + 3]);
            }
        }
        return (width, height, out);
    }

    let out_width = height;
    let out_height = width;
    let mut out = vec![0u8; rgb.len()];
    for y in 0..height {
        for x in 0..width {
            let src = (y * width + x) * 3;
            let (dx, dy) = if orientation == 1 {
                (y, width - 1 - x)
            } else {
                (height - 1 - y, x)
            };
            let dst = (dy * out_width + dx) * 3;
            out[dst..dst + 3].copy_from_slice(&rgb[src..src + 3]);
        }
    }
    (out_width, out_height, out)
}

pub fn decode<B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let data = read_all(reader)?;
    let size = &PCD_SIZES[1];
    let mut cursor = ByteCursor::new(&data, 0x800);
    if cursor.read_bytes(7)? != b"PCD_IPI" {
        return Err(err(
            crate::error::ImgErrorKind::IllegalData,
            "Not a PhotoCD image",
        ));
    }
    let _version = cursor.read_u8()?;
    cursor.seek(0xe02)?;
    let orientation = cursor.read_u8()? & 0x03;

    let width = size.width;
    let height = size.height;
    let rgb_len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(3))
        .ok_or_else(|| {
            err(
                crate::error::ImgErrorKind::IllegalData,
                "PCD RGB buffer size overflow",
            )
        })?;
    let mut rgb = vec![0u8; rgb_len];
    cursor.seek(size.seek)?;
    let y_len = width.checked_mul(2).ok_or_else(|| {
        err(
            crate::error::ImgErrorKind::IllegalData,
            "PCD Y plane size overflow",
        )
    })?;
    let mut y_data = vec![0u8; y_len];
    let mut c1_data = vec![0u8; width / 2];
    let mut c2_data = vec![0u8; width / 2];

    for y in (0..height).step_by(2) {
        y_data.copy_from_slice(cursor.read_bytes(y_len)?);
        c1_data.copy_from_slice(cursor.read_bytes(width / 2)?);
        c2_data.copy_from_slice(cursor.read_bytes(width / 2)?);

        let mut xw = 0usize;
        for x in 0..(width * 2) {
            let cy = y_data[x] as i32;
            let cc1 = c1_data[xw] as i32;
            let cc2 = c2_data[xw] as i32;
            let l = cy * RGB_F_LL;
            let r = clamp8((l + cc2 * RGB_F_C2 + RGB_O_C2) >> 10);
            let g = clamp8((l + cc1 * RGB_F_G1 + cc2 * RGB_F_G2 + RGB_O_G) >> 10);
            let b = clamp8((l + cc1 * RGB_F_C1 + RGB_O_C1) >> 10);
            let row = y + (x / width);
            let col = x % width;
            let dst = (row * width + col) * 3;
            rgb[dst] = r;
            rgb[dst + 1] = g;
            rgb[dst + 2] = b;
            if (x & 1) != 0 {
                xw += 1;
            }
            if x + 1 == width {
                xw = 0;
            }
        }
    }

    let (out_width, out_height, rotated) = rotate_rgb(&rgb, width, height, orientation);
    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("PCD".to_string()))?;
    option
        .drawer
        .set_metadata("width", DataMap::UInt(out_width as u64))?;
    option
        .drawer
        .set_metadata("height", DataMap::UInt(out_height as u64))?;
    option
        .drawer
        .set_metadata("orientation", DataMap::UInt(orientation as u64))?;
    option
        .drawer
        .set_metadata("size", DataMap::Ascii(size.name.to_string()))?;

    draw_rgb(option, out_width, out_height, &rotated)?;
    Ok(None)
}
