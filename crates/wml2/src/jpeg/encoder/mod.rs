//! JPEG encoder support modules.

mod bitwriter;
mod encoder;
mod fdct;
mod huffman;
mod quantize_table;

use crate::draw::EncodeOptions as DrawEncodeOptions;
use crate::error::{ImgError, ImgErrorKind};
use crate::metadata::{DataMap, get_exif_option};

type Error = Box<dyn std::error::Error>;

pub use self::encoder::create_qt;

pub(crate) fn quality_from_draw_options(option: &DrawEncodeOptions<'_>) -> usize {
    option
        .options
        .as_ref()
        .and_then(|map| map.get("quality"))
        .and_then(|value| match value {
            DataMap::UInt(v) => Some(*v as usize),
            DataMap::SInt(v) if *v > 0 => Some(*v as usize),
            _ => None,
        })
        .unwrap_or(80)
        .clamp(1, 100)
}

pub(crate) fn encode_rgba(
    width: usize,
    height: usize,
    rgba: &[u8],
    quality: usize,
) -> Result<Vec<u8>, Error> {
    let inner = self::encoder::EncodeOptions::new(width, height, rgba, quality.clamp(1, 100));
    self::encoder::encode(&inner)
}

fn insert_exif_segment(jpeg: &mut Vec<u8>, exif: &[u8]) -> Result<(), Error> {
    if jpeg.len() < 2 || jpeg[..2] != [0xff, 0xd8] {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            "encoded JPEG is missing SOI".to_string(),
        )));
    }

    let payload_len = exif.len().checked_add(6).ok_or_else(|| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "EXIF payload is too large".to_string(),
        )) as Error
    })?;
    let segment_len = payload_len.checked_add(2).ok_or_else(|| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "EXIF payload is too large".to_string(),
        )) as Error
    })?;
    let segment_len = u16::try_from(segment_len).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "EXIF payload exceeds JPEG APP1 limits".to_string(),
        )) as Error
    })?;

    let mut segment = Vec::with_capacity(4 + payload_len);
    segment.extend_from_slice(&[0xff, 0xe1]);
    segment.extend_from_slice(&segment_len.to_be_bytes());
    segment.extend_from_slice(b"Exif\0\0");
    segment.extend_from_slice(exif);
    jpeg.splice(2..2, segment);
    Ok(())
}

/// Encodes an image source to JPEG.
///
/// Supported `EncodeOptions.options` keys:
/// - `quality`: lossy quality in `1..=100`
/// - `exif`: `Raw(bytes)`, `Exif(headers)`, or `Ascii("copy")`
pub fn encode(image: &mut DrawEncodeOptions<'_>) -> Result<Vec<u8>, Error> {
    let profile = image.drawer.encode_start(None)?;
    let profile = profile.ok_or_else(|| {
        Box::new(ImgError::new_const(
            ImgErrorKind::OutboundIndex,
            "Image profiles nothing".to_string(),
        )) as Error
    })?;

    let rgba = image
        .drawer
        .encode_pick(0, 0, profile.width, profile.height, None)?
        .ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::EncodeError,
                "Image buffer nothing".to_string(),
            )) as Error
        })?;

    let mut data = encode_rgba(
        profile.width,
        profile.height,
        &rgba,
        quality_from_draw_options(image),
    )?;
    if let Some(exif) = get_exif_option(image.options.as_ref(), profile.metadata.as_ref())? {
        insert_exif_segment(&mut data, &exif)?;
    }
    image.drawer.encode_end(None)?;
    Ok(data)
}
