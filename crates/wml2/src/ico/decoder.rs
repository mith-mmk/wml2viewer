//! ICO decoder implementation.

type Error = Box<dyn std::error::Error>;

use super::header::{IcoEntry, IcoHeader};
use crate::draw::{DecodeOptions, ImageBuffer, InitOptions};
use crate::error::{ImgError, ImgErrorKind};
use crate::metadata::DataMap;
use crate::warning::ImgWarnings;
use bin_rs::reader::{BinaryReader, BytesReader};
use std::io::SeekFrom;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

#[derive(Debug)]
struct DibLayout {
    width: usize,
    height: usize,
    stored_height: i32,
    bitmap_offset: usize,
    and_stride: usize,
    and_offset: usize,
}

fn invalid_data(message: impl Into<String>) -> Error {
    Box::new(ImgError::new_const(
        ImgErrorKind::IllegalData,
        message.into(),
    ))
}

fn decode_error(message: impl Into<String>) -> Error {
    Box::new(ImgError::new_const(
        ImgErrorKind::DecodeError,
        message.into(),
    ))
}

fn checked_stride(width: usize, bit_count: usize) -> Result<usize, Error> {
    let bits = width
        .checked_mul(bit_count)
        .ok_or_else(|| decode_error("ICO stride overflow"))?;
    let dwords = bits
        .checked_add(31)
        .ok_or_else(|| decode_error("ICO stride overflow"))?
        / 32;
    dwords
        .checked_mul(4)
        .ok_or_else(|| decode_error("ICO stride overflow"))
}

fn read_i32_le(buffer: &[u8], offset: usize) -> Result<i32, Error> {
    let bytes = buffer
        .get(offset..offset + 4)
        .ok_or_else(|| invalid_data("ICO DIB header is truncated"))?;
    Ok(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u16_le(buffer: &[u8], offset: usize) -> Result<u16, Error> {
    let bytes = buffer
        .get(offset..offset + 2)
        .ok_or_else(|| invalid_data("ICO DIB header is truncated"))?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(buffer: &[u8], offset: usize) -> Result<u32, Error> {
    let bytes = buffer
        .get(offset..offset + 4)
        .ok_or_else(|| invalid_data("ICO DIB header is truncated"))?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn parse_dib_layout(data: &[u8], entry: &IcoEntry) -> Result<DibLayout, Error> {
    if data.len() < 4 {
        return Err(invalid_data("ICO bitmap payload is too short"));
    }

    let header_size = read_u32_le(data, 0)? as usize;
    if data.len() < header_size {
        return Err(invalid_data("ICO bitmap header is truncated"));
    }

    let (width, stored_height, bit_count) = match header_size {
        12 => {
            let width = read_u16_le(data, 4)? as usize;
            let stored_height = read_u16_le(data, 6)? as i32;
            let bit_count = read_u16_le(data, 10)? as usize;
            (width, stored_height, bit_count)
        }
        size if size >= 40 => {
            let width = read_i32_le(data, 4)?.unsigned_abs() as usize;
            let stored_height = read_i32_le(data, 8)?;
            let bit_count = read_u16_le(data, 14)? as usize;
            (width, stored_height, bit_count)
        }
        _ => {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::UnsupportedFeature,
                format!("ICO DIB header size {header_size} is not supported"),
            )));
        }
    };

    let width = if width == 0 {
        entry.actual_width()
    } else {
        width
    };
    let full_height = stored_height.unsigned_abs() as usize;
    let height = if full_height >= 2 {
        full_height / 2
    } else {
        entry.actual_height()
    };
    if width == 0 || height == 0 {
        return Err(invalid_data("ICO image dimensions must be non-zero"));
    }

    let xor_stride = checked_stride(width, bit_count)?;
    let xor_size = xor_stride
        .checked_mul(height)
        .ok_or_else(|| decode_error("ICO bitmap data is too large"))?;
    let and_stride = checked_stride(width, 1)?;
    let and_size = and_stride
        .checked_mul(height)
        .ok_or_else(|| decode_error("ICO mask data is too large"))?;
    if data.len() < xor_size + and_size {
        return Err(invalid_data("ICO bitmap payload is truncated"));
    }

    let bitmap_offset = data.len() - xor_size - and_size;
    if bitmap_offset < header_size {
        return Err(invalid_data("ICO bitmap header size is inconsistent"));
    }
    let and_offset = bitmap_offset + xor_size;

    Ok(DibLayout {
        width,
        height,
        stored_height,
        bitmap_offset,
        and_stride,
        and_offset,
    })
}

fn make_fake_bmp(data: &[u8], layout: &DibLayout) -> Result<Vec<u8>, Error> {
    let mut dib = data.to_vec();
    let total_height = if layout.stored_height < 0 {
        -(layout.height as i32)
    } else {
        layout.height as i32
    };

    let header_size = read_u32_le(&dib, 0)? as usize;
    match header_size {
        12 => {
            let height = u16::try_from(layout.height)
                .map_err(|_| decode_error("ICO height does not fit BMP core header"))?;
            dib[6..8].copy_from_slice(&height.to_le_bytes());
        }
        size if size >= 40 => {
            dib[8..12].copy_from_slice(&total_height.to_le_bytes());
        }
        _ => return Err(invalid_data("ICO bitmap header is not supported")),
    }

    let file_size = (14usize)
        .checked_add(dib.len())
        .ok_or_else(|| decode_error("ICO bitmap payload is too large"))?;
    let off_bits = (14usize)
        .checked_add(layout.bitmap_offset)
        .ok_or_else(|| decode_error("ICO bitmap offset overflow"))?;
    let file_size =
        u32::try_from(file_size).map_err(|_| decode_error("ICO bitmap is too large"))?;
    let off_bits =
        u32::try_from(off_bits).map_err(|_| decode_error("ICO bitmap offset is too large"))?;

    let mut bmp = Vec::with_capacity(14 + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&0u16.to_le_bytes());
    bmp.extend_from_slice(&0u16.to_le_bytes());
    bmp.extend_from_slice(&off_bits.to_le_bytes());
    bmp.extend_from_slice(&dib);
    Ok(bmp)
}

fn apply_and_mask(buffer: &mut [u8], data: &[u8], layout: &DibLayout) {
    let mask_size = layout.and_stride * layout.height;
    let Some(mask) = data.get(layout.and_offset..layout.and_offset + mask_size) else {
        return;
    };

    for row in 0..layout.height {
        let src_row = &mask[row * layout.and_stride..(row + 1) * layout.and_stride];
        let y = if layout.stored_height < 0 {
            row
        } else {
            layout.height - 1 - row
        };
        for x in 0..layout.width {
            let byte = src_row[x / 8];
            let shift = 7 - (x % 8);
            if ((byte >> shift) & 0x1) == 1 {
                let alpha = (y * layout.width + x) * 4 + 3;
                if alpha < buffer.len() {
                    buffer[alpha] = 0;
                }
            }
        }
    }
}

fn copy_metadata(image: &ImageBuffer, option: &mut DecodeOptions) -> Result<(), Error> {
    if let Some(metadata) = &image.metadata {
        for (key, value) in metadata {
            option.drawer.set_metadata(key, value.clone())?;
        }
    }
    Ok(())
}

fn emit_image(
    image: &ImageBuffer,
    option: &mut DecodeOptions,
    header: &IcoHeader,
    entry: &IcoEntry,
    selected_index: usize,
    payload_format: &str,
) -> Result<(), Error> {
    let buffer = image.buffer.as_ref().ok_or_else(|| {
        Box::new(ImgError::new_const(
            ImgErrorKind::NotInitializedImageBuffer,
            "ICO payload decoder did not produce a bitmap".to_string(),
        )) as Error
    })?;

    option.drawer.init(
        image.width,
        image.height,
        Some(InitOptions {
            loop_count: 1,
            background: image.background_color.clone(),
            animation: false,
        }),
    )?;
    option
        .drawer
        .draw(0, 0, image.width, image.height, buffer, None)?;

    copy_metadata(image, option)?;
    option
        .drawer
        .set_metadata("Format", DataMap::Ascii("ICO".to_string()))?;
    option
        .drawer
        .set_metadata("width", DataMap::UInt(image.width as u64))?;
    option
        .drawer
        .set_metadata("height", DataMap::UInt(image.height as u64))?;
    option.drawer.set_metadata(
        "ICO payload format",
        DataMap::Ascii(payload_format.to_string()),
    )?;
    option.drawer.set_metadata(
        "ICO resource type",
        DataMap::UInt(header.resource_type as u64),
    )?;
    option
        .drawer
        .set_metadata("ICO image count", DataMap::UInt(header.image_count as u64))?;
    option
        .drawer
        .set_metadata("ICO selected index", DataMap::UInt(selected_index as u64))?;
    option.drawer.set_metadata(
        "ICO directory width",
        DataMap::UInt(entry.actual_width() as u64),
    )?;
    option.drawer.set_metadata(
        "ICO directory height",
        DataMap::UInt(entry.actual_height() as u64),
    )?;
    option.drawer.set_metadata(
        "ICO directory bit count",
        DataMap::UInt(entry.bit_count as u64),
    )?;
    option
        .drawer
        .set_metadata("ICO color count", DataMap::UInt(entry.color_count as u64))?;
    option.drawer.terminate(None)?;
    Ok(())
}

fn decode_png_payload(
    data: Vec<u8>,
    option: &mut DecodeOptions,
    header: &IcoHeader,
    entry: &IcoEntry,
    selected_index: usize,
) -> Result<Option<ImgWarnings>, Error> {
    #[cfg(feature = "ico-png")]
    {
        let mut image = ImageBuffer::new();
        let mut part_option = DecodeOptions {
            debug_flag: option.debug_flag,
            drawer: &mut image,
        };
        let mut reader = BytesReader::from(data);
        let warnings = crate::png::decoder::decode(&mut reader, &mut part_option)?;
        emit_image(&image, option, header, entry, selected_index, "PNG")?;
        Ok(warnings)
    }
    #[cfg(not(feature = "ico-png"))]
    {
        let _ = (data, option, header, entry, selected_index);
        Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "ICO PNG payload support is disabled by feature flags".to_string(),
        )))
    }
}

fn decode_dib_payload(
    data: Vec<u8>,
    option: &mut DecodeOptions,
    header: &IcoHeader,
    entry: &IcoEntry,
    selected_index: usize,
) -> Result<Option<ImgWarnings>, Error> {
    #[cfg(feature = "ico-bmp")]
    {
        let layout = parse_dib_layout(&data, entry)?;
        let bmp = make_fake_bmp(&data, &layout)?;

        let mut image = ImageBuffer::new();
        let mut part_option = DecodeOptions {
            debug_flag: option.debug_flag,
            drawer: &mut image,
        };
        let mut reader = BytesReader::from(bmp);
        let warnings = crate::bmp::decoder::decode(&mut reader, &mut part_option)?;

        if let Some(buffer) = image.buffer.as_mut() {
            apply_and_mask(buffer, &data, &layout);
        }

        emit_image(&image, option, header, entry, selected_index, "BMP")?;
        Ok(warnings)
    }
    #[cfg(not(feature = "ico-bmp"))]
    {
        let _ = (data, option, header, entry, selected_index);
        Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "ICO BMP payload support is disabled by feature flags".to_string(),
        )))
    }
}

pub fn decode<B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let start = reader.offset()?;
    let end = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(start))?;

    let header = IcoHeader::new(reader)?;
    if option.debug_flag > 0 {
        option.drawer.verbose(&format!("{header:?}"), None)?;
    }

    let selected_index = header.best_entry_index();
    let entry = &header.entries[selected_index];
    let image_start = start
        .checked_add(entry.image_offset as u64)
        .ok_or_else(|| decode_error("ICO image offset overflow"))?;
    let image_end = image_start
        .checked_add(entry.bytes_in_res as u64)
        .ok_or_else(|| decode_error("ICO image size overflow"))?;
    if image_end > end {
        return Err(invalid_data(
            "ICO payload points outside of the input buffer",
        ));
    }

    reader.seek(SeekFrom::Start(image_start))?;
    let data = reader.read_bytes_as_vec(entry.bytes_in_res as usize)?;

    let result = if data.starts_with(&PNG_SIGNATURE) {
        decode_png_payload(data, option, &header, entry, selected_index)?
    } else {
        decode_dib_payload(data, option, &header, entry, selected_index)?
    };

    Ok(result)
}

#[cfg(all(test, feature = "ico-bmp", feature = "ico-png"))]
mod tests {
    use super::PNG_SIGNATURE;
    use crate::draw::{ImageBuffer, image_load, image_to};
    use crate::metadata::DataMap;
    use crate::util::{ImageFormat, format_check};

    fn wrap_ico(payload: &[u8], width: u8, height: u8, bit_count: u16) -> Vec<u8> {
        let mut ico = Vec::with_capacity(22 + payload.len());
        ico.extend_from_slice(&0u16.to_le_bytes());
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.push(width);
        ico.push(height);
        ico.push(0);
        ico.push(0);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&bit_count.to_le_bytes());
        ico.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        ico.extend_from_slice(&(22u32).to_le_bytes());
        ico.extend_from_slice(payload);
        ico
    }

    fn one_pixel_dib_with_mask(alpha_mask: bool) -> Vec<u8> {
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&1i32.to_le_bytes());
        dib.extend_from_slice(&2i32.to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&8u32.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&[0x00, 0x00, 0xff, 0xff]);
        dib.extend_from_slice(&[if alpha_mask { 0x80 } else { 0x00 }, 0x00, 0x00, 0x00]);
        dib
    }

    #[test]
    fn format_check_recognizes_ico() {
        let ico = wrap_ico(&PNG_SIGNATURE, 1, 1, 32);
        assert!(matches!(format_check(&ico), ImageFormat::Ico));
    }

    #[test]
    fn png_payload_icon_decodes() {
        let mut source = ImageBuffer::from_buffer(1, 1, vec![0x12, 0x34, 0x56, 0xff]);
        let png = image_to(&mut source, ImageFormat::Png, None).unwrap();
        let ico = wrap_ico(&png, 1, 1, 32);

        let image = image_load(&ico).unwrap();
        assert_eq!(image.width, 1);
        assert_eq!(image.height, 1);
        assert_eq!(image.buffer.unwrap(), vec![0x12, 0x34, 0x56, 0xff]);
        assert_eq!(
            image.metadata.unwrap().get("Format"),
            Some(&DataMap::Ascii("ICO".to_string()))
        );
    }

    #[test]
    fn bmp_payload_icon_applies_and_mask() {
        let dib = one_pixel_dib_with_mask(true);
        let ico = wrap_ico(&dib, 1, 1, 32);

        let image = image_load(&ico).unwrap();
        assert_eq!(image.width, 1);
        assert_eq!(image.height, 1);
        assert_eq!(image.buffer.unwrap(), vec![0xff, 0x00, 0x00, 0x00]);
    }
}
