//! JPEG-compressed TIFF decoding helpers.

type Error = Box<dyn std::error::Error>;
use crate::draw::DecodeOptions;
use crate::draw::ImageBuffer;
use crate::draw::InitOptions;
use crate::error::{ImgError, ImgErrorKind};
use crate::tiff::header::*;
use crate::warning::ImgWarnings;
use bin_rs::reader::BinaryReader;

fn draw_jpeg(
    data: Vec<u8>,
    x: usize,
    y: usize,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let mut image = ImageBuffer::new();
    let mut part_option = DecodeOptions {
        debug_flag: option.debug_flag,
        drawer: &mut image,
    };
    let mut reader = bin_rs::reader::BytesReader::from(data);
    let ws = crate::jpeg::decoder::decode(&mut reader, &mut part_option)?;
    let width = image.width;
    let height = image.height;

    if let Some(buffer) = image.buffer.as_ref() {
        option.drawer.draw(x, y, width, height, buffer, None)?;
    }

    Ok(ws)
}

// Tiff in JPEG is a multi parts image.
pub fn decode_jpeg_compresson<'decode, B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
    header: &Tiff,
    initialize: bool,
    animation: bool,
) -> Result<Option<ImgWarnings>, Error> {
    let jpeg_tables = &header.jpeg_tables;
    let metadata;
    if jpeg_tables.is_empty() {
        metadata = vec![0xff, 0xd8]; // SOI
    } else if jpeg_tables.len() < 2 {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            "JPEG tables are truncated".to_string(),
        )));
    } else {
        let len = jpeg_tables.len() - 2;
        metadata = jpeg_tables[..len].to_vec(); // remove EOI
    }
    let mut warnings: Option<ImgWarnings> = None;
    let mut x = 0;
    let mut y = 0;
    if initialize {
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
    }

    if header.tile_width != 0
        && header.tile_length != 0
        && !header.tile_byte_counts.is_empty()
        && !header.tile_offsets.is_empty()
    {
        for (i, offset) in header.tile_offsets.iter().enumerate() {
            reader.seek(std::io::SeekFrom::Start(*offset as u64))?;
            let mut data = vec![];
            data.append(&mut metadata.to_vec());
            let buf = reader.read_bytes_as_vec(header.tile_byte_counts[i] as usize)?;
            if buf.len() < 2 {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::DecodeError,
                    "JPEG tile payload is truncated".to_string(),
                )));
            }
            data.append(&mut buf[2..].to_vec()); // remove SOI

            let ws = draw_jpeg(data, x, y, option)?;
            warnings = ImgWarnings::append(warnings, ws);
            x += header.tile_width as usize;
            if x >= header.width as usize {
                x = 0;
                y += header.tile_length as usize;
            }
            if header.tile_length >= header.height {
                break;
            }
        }
    } else {
        for (i, offset) in header.strip_offsets.iter().enumerate() {
            reader.seek(std::io::SeekFrom::Start(*offset as u64))?;
            let mut data = vec![];
            data.append(&mut metadata.to_vec());
            let buf = reader.read_bytes_as_vec(header.strip_byte_counts[i] as usize)?;
            if buf.len() < 2 {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::DecodeError,
                    "JPEG strip payload is truncated".to_string(),
                )));
            }
            data.append(&mut buf[2..].to_vec()); // remove SOI

            let ws = draw_jpeg(data, 0, y, option)?;
            y += header.rows_per_strip as usize;
            warnings = ImgWarnings::append(warnings, ws);
        }
    }

    Ok(warnings)
}
