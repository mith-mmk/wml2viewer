//! ICO header parsing helpers.

type Error = Box<dyn std::error::Error>;

use crate::error::{ImgError, ImgErrorKind};
use bin_rs::reader::BinaryReader;

#[derive(Debug, Clone)]
pub struct IcoEntry {
    pub width: u8,
    pub height: u8,
    pub color_count: u8,
    pub planes: u16,
    pub bit_count: u16,
    pub bytes_in_res: u32,
    pub image_offset: u32,
}

impl IcoEntry {
    pub fn actual_width(&self) -> usize {
        if self.width == 0 {
            256
        } else {
            self.width as usize
        }
    }

    pub fn actual_height(&self) -> usize {
        if self.height == 0 {
            256
        } else {
            self.height as usize
        }
    }
}

#[derive(Debug, Clone)]
pub struct IcoHeader {
    pub resource_type: u16,
    pub image_count: u16,
    pub entries: Vec<IcoEntry>,
}

impl IcoHeader {
    pub fn new<B: BinaryReader>(reader: &mut B) -> Result<Self, Error> {
        let reserved = reader.read_u16_le()?;
        let resource_type = reader.read_u16_le()?;
        let image_count = reader.read_u16_le()?;

        if reserved != 0 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::UnknownFormat,
                "ICO reserved field must be zero".to_string(),
            )));
        }

        if resource_type != 1 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::NoSupportFormat,
                format!("ICO resource type {resource_type} is not supported"),
            )));
        }

        if image_count == 0 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::IllegalData,
                "ICO does not contain any images".to_string(),
            )));
        }

        let mut entries = Vec::with_capacity(image_count as usize);
        for _ in 0..image_count {
            let width = reader.read_byte()?;
            let height = reader.read_byte()?;
            let color_count = reader.read_byte()?;
            let reserved = reader.read_byte()?;
            if reserved != 0 {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::IllegalData,
                    "ICO entry reserved field must be zero".to_string(),
                )));
            }
            entries.push(IcoEntry {
                width,
                height,
                color_count,
                planes: reader.read_u16_le()?,
                bit_count: reader.read_u16_le()?,
                bytes_in_res: reader.read_u32_le()?,
                image_offset: reader.read_u32_le()?,
            });
        }

        Ok(Self {
            resource_type,
            image_count,
            entries,
        })
    }

    pub fn best_entry_index(&self) -> usize {
        self.entries
            .iter()
            .enumerate()
            .max_by_key(|(_, entry)| {
                (
                    entry.actual_width() * entry.actual_height(),
                    entry.bit_count,
                    entry.bytes_in_res,
                    entry.planes,
                )
            })
            .map(|(index, _)| index)
            .unwrap_or(0)
    }
}
