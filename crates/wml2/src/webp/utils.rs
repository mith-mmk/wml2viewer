//! WebP metadata extraction helpers.

use crate::metadata::DataMap;
use crate::tiff::header::read_tags;
use crate::warning::ImgWarnings;
use crate::webp::decoder::{DecoderError, WebpFormat, get_features, parse_animation_webp};
use crate::webp::warning::{WebpWarning, WebpWarningKind};
use bin_rs::reader::BytesReader;
use std::collections::HashMap;

fn read_le32(bytes: &[u8]) -> usize {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize
}

fn scan_chunks<'a>(data: &'a [u8]) -> Result<Vec<([u8; 4], &'a [u8])>, DecoderError> {
    if data.len() < 12 {
        return Err(DecoderError::NotEnoughData("RIFF header"));
    }
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WEBP" {
        return Err(DecoderError::Bitstream("wrong RIFF WEBP signature"));
    }

    let riff_size = read_le32(&data[4..8]);
    let limit = riff_size + 8;
    if limit > data.len() {
        return Err(DecoderError::NotEnoughData("truncated RIFF payload"));
    }

    let mut offset = 12;
    let mut chunks = Vec::new();
    while offset + 8 <= limit {
        let size = read_le32(&data[offset + 4..offset + 8]);
        let padded_size = size + (size & 1);
        let chunk_end = offset + 8 + padded_size;
        if chunk_end > limit {
            return Err(DecoderError::NotEnoughData("chunk payload"));
        }

        let mut fourcc = [0_u8; 4];
        fourcc.copy_from_slice(&data[offset..offset + 4]);
        let payload = &data[offset + 8..offset + 8 + size];
        chunks.push((fourcc, payload));
        offset = chunk_end;
    }

    Ok(chunks)
}

fn webp_codec_name(format: WebpFormat, animated: bool) -> &'static str {
    if animated {
        "Animated"
    } else {
        match format {
            WebpFormat::Lossy => "Lossy",
            WebpFormat::Lossless => "Lossless",
            WebpFormat::Undefined => "Undefined",
        }
    }
}

pub(crate) fn make_metadata(
    data: &[u8],
) -> Result<(HashMap<String, DataMap>, Option<ImgWarnings>), DecoderError> {
    let features = get_features(data)?;
    let chunks = scan_chunks(data)?;
    let mut warnings = None;
    let mut map = HashMap::new();

    map.insert("Format".to_string(), DataMap::Ascii("WEBP".to_string()));
    map.insert("width".to_string(), DataMap::UInt(features.width as u64));
    map.insert("height".to_string(), DataMap::UInt(features.height as u64));
    map.insert(
        "WebP codec".to_string(),
        DataMap::Ascii(webp_codec_name(features.format, features.has_animation).to_string()),
    );
    map.insert(
        "WebP has alpha".to_string(),
        DataMap::Ascii(features.has_alpha.to_string()),
    );
    map.insert(
        "WebP animated".to_string(),
        DataMap::Ascii(features.has_animation.to_string()),
    );

    if let Some(vp8x) = features.vp8x {
        map.insert(
            "canvas width".to_string(),
            DataMap::UInt(vp8x.canvas_width as u64),
        );
        map.insert(
            "canvas height".to_string(),
            DataMap::UInt(vp8x.canvas_height as u64),
        );
    }

    if features.has_animation {
        let parsed = parse_animation_webp(data)?;
        map.insert(
            "Animation frames".to_string(),
            DataMap::UInt(parsed.frames.len() as u64),
        );
        map.insert(
            "Animation loop count".to_string(),
            DataMap::UInt(parsed.animation.loop_count as u64),
        );
        map.insert(
            "Animation background color".to_string(),
            DataMap::UInt(parsed.animation.background_color as u64),
        );
        map.insert(
            "Animation frame durations".to_string(),
            DataMap::UIntAllay(
                parsed
                    .frames
                    .iter()
                    .map(|frame| frame.duration as u64)
                    .collect(),
            ),
        );
    }

    for (fourcc, payload) in chunks {
        match &fourcc {
            b"ICCP" => {
                map.insert(
                    "ICC Profile".to_string(),
                    DataMap::ICCProfile(payload.to_vec()),
                );
            }
            b"EXIF" => {
                let exif_payload = if payload.starts_with(b"Exif\0\0") {
                    &payload[6..]
                } else {
                    payload
                };
                let mut reader = BytesReader::new(exif_payload);
                match read_tags(&mut reader) {
                    Ok(exif) => {
                        map.insert("EXIF".to_string(), DataMap::Exif(exif));
                    }
                    Err(_) => {
                        map.insert("EXIF Raw".to_string(), DataMap::Raw(payload.to_vec()));
                        warnings = ImgWarnings::add(
                            warnings,
                            Box::new(WebpWarning::new_const(
                                WebpWarningKind::MetadataCorruption,
                                "failed to parse EXIF chunk".to_string(),
                            )),
                        );
                    }
                }
            }
            b"XMP " => match String::from_utf8(payload.to_vec()) {
                Ok(xmp) => {
                    map.insert("XMP".to_string(), DataMap::Ascii(xmp));
                }
                Err(_) => {
                    map.insert("XMP Raw".to_string(), DataMap::Raw(payload.to_vec()));
                    warnings = ImgWarnings::add(
                        warnings,
                        Box::new(WebpWarning::new_const(
                            WebpWarningKind::MetadataEncoding,
                            "XMP chunk is not valid UTF-8".to_string(),
                        )),
                    );
                }
            },
            _ => {}
        }
    }

    Ok((map, warnings))
}
