//! GIF header parsing structures.

use crate::color::RGBA;
use crate::error::ImgError;
use crate::error::ImgErrorKind;
use bin_rs::reader::BinaryReader;
type Error = Box<dyn std::error::Error>;

#[derive(Debug)]
pub struct GifHeader {
    pub version: String,
    pub width: usize,
    pub height: usize,
    pub color_table: Vec<RGBA>,
    pub scd: GifScd,
    pub header_size: usize,
    pub transparent_color: Option<u8>,
    pub comment: Option<String>,
}

#[derive(Debug)]
pub struct GifScd {
    pub width: u16,
    pub height: u16,
    pub field: u8,
    pub color_index: u8,
    pub aspect: u8,
}

#[derive(Debug)]
pub struct GifLscd {
    pub xstart: u16,
    pub ystart: u16,
    pub xsize: u16,
    pub ysize: u16,
    pub field: u8,
}

#[derive(Debug)]
pub struct GifExtend {
    pub indent: u8,
    pub code: u8,
    pub length: u8,
    pub data: [u8; 256],
}

impl GifHeader {
    pub fn new<B: BinaryReader>(reader: &mut B, _opt: usize) -> Result<Self, Error> {
        let mut ptr = 0;
        let gif = reader.read_bytes_as_vec(6)?;

        if gif[0] != b'G' || gif[1] != b'I' || gif[2] != b'F' {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::UnknownFormat,
                "not Gif".to_string(),
            )));
        }
        if gif[3] != b'8' || (gif[4] != b'7' && gif[4] != b'9') || gif[5] != b'a' {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::UnknownFormat,
                "not Gif".to_string(),
            )));
        }
        ptr += 6;

        let scd = GifScd {
            width: reader.read_u16_le()?,
            height: reader.read_u16_le()?,
            field: reader.read_byte()?,
            color_index: reader.read_byte()?,
            aspect: reader.read_byte()?,
        };
        ptr += 7;

        let mut color_table: Vec<RGBA> = Vec::new();

        if (scd.field & 0x80) == 0x80 {
            let table_size = 1 << ((scd.field & 0x07) + 1);

            for _ in 0..table_size {
                let palette = RGBA {
                    red: reader.read_byte()?,
                    green: reader.read_byte()?,
                    blue: reader.read_byte()?,
                    alpha: 0xff,
                };
                color_table.push(palette);
                ptr += 3;
            }
        }

        return Ok(GifHeader {
            version: String::from_utf8_lossy(&gif[3..]).to_string(),
            width: scd.width as usize,
            height: scd.height as usize,
            color_table,
            scd,
            header_size: ptr,
            transparent_color: None,
            comment: None,
        });
    }
}

impl GifLscd {
    pub fn new<B: BinaryReader>(reader: &mut B) -> Result<Self, Error> {
        Ok(Self {
            xstart: reader.read_u16_le()?,
            ystart: reader.read_u16_le()?,
            xsize: reader.read_u16_le()?,
            ysize: reader.read_u16_le()?,
            field: reader.read_byte()?,
        })
    }
}
