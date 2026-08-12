//! RIFF/WebP container assembly helpers for still and animated images.

use crate::webp::decoder::vp8i::{ALPHA_FLAG, ANIMATION_FLAG};
use crate::webp::encoder::EncoderError;
use crate::webp::encoder::writer::ByteWriter;

const EXIF_FLAG: u32 = 0x0000_0008;

/// Metadata and payload for a still-image WebP chunk.
pub(crate) struct StillImageChunk<'a> {
    pub fourcc: [u8; 4],
    pub payload: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub has_alpha: bool,
}

/// Metadata and payload for one animated WebP frame.
pub(crate) struct AnimationFrameChunk {
    pub fourcc: [u8; 4],
    pub payload: Vec<u8>,
    pub duration_ms: usize,
    pub blend: bool,
    pub dispose_to_background: bool,
}

/// Internal helper for padded len.
fn padded_len(size: usize) -> Result<usize, EncoderError> {
    size.checked_add(size & 1)
        .ok_or(EncoderError::InvalidParam("encoded output is too large"))
}

/// Internal helper for append chunk.
fn append_chunk(
    data: &mut ByteWriter,
    fourcc: &[u8; 4],
    payload: &[u8],
) -> Result<(), EncoderError> {
    let _ = u32::try_from(payload.len())
        .map_err(|_| EncoderError::InvalidParam("encoded output is too large"))?;
    data.write_bytes(fourcc);
    data.write_u32_le(payload.len() as u32);
    data.write_bytes(payload);
    if payload.len() & 1 == 1 {
        data.write_byte(0);
    }
    Ok(())
}

/// Internal helper for extend riff.
fn extend_riff(body: ByteWriter) -> Result<Vec<u8>, EncoderError> {
    let body = body.into_bytes();
    let riff_size = u32::try_from(body.len())
        .map_err(|_| EncoderError::InvalidParam("encoded output is too large"))?;
    let mut data = ByteWriter::with_capacity(8 + body.len());
    data.write_bytes(b"RIFF");
    data.write_u32_le(riff_size);
    data.write_bytes(&body);
    Ok(data.into_bytes())
}

/// Encodes le24.
fn encode_le24(value: usize) -> Result<[u8; 3], EncoderError> {
    let encoded = value.checked_sub(1).ok_or(EncoderError::InvalidParam(
        "image dimensions must be non-zero",
    ))?;
    if encoded > 0x00ff_ffff {
        return Err(EncoderError::InvalidParam(
            "image dimensions exceed VP8X limits",
        ));
    }
    Ok([
        (encoded & 0xff) as u8,
        ((encoded >> 8) & 0xff) as u8,
        ((encoded >> 16) & 0xff) as u8,
    ])
}

/// Encodes a 24-bit little-endian integer.
fn encode_u24(value: usize, name: &'static str) -> Result<[u8; 3], EncoderError> {
    if value > 0x00ff_ffff {
        return Err(EncoderError::InvalidParam(name));
    }
    Ok([
        (value & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        ((value >> 16) & 0xff) as u8,
    ])
}

/// Encodes an animation frame offset stored in half-pixel units.
fn encode_frame_offset(value: usize) -> Result<[u8; 3], EncoderError> {
    if value & 1 == 1 {
        return Err(EncoderError::InvalidParam(
            "animation frame offsets must be even",
        ));
    }
    encode_u24(value >> 1, "animation frame offset exceeds WebP limits")
}

/// Wraps an encoded still-image payload in a RIFF/WebP container.
pub(crate) fn wrap_still_webp(
    image: StillImageChunk<'_>,
    exif: Option<&[u8]>,
) -> Result<Vec<u8>, EncoderError> {
    let padded_image_size = padded_len(image.payload.len())?;
    if exif.is_none() {
        let body_size = 4usize
            .checked_add(8)
            .and_then(|size| size.checked_add(padded_image_size))
            .ok_or(EncoderError::InvalidParam("encoded output is too large"))?;
        let mut body = ByteWriter::with_capacity(body_size);
        body.write_bytes(b"WEBP");
        append_chunk(&mut body, &image.fourcc, image.payload)?;
        return extend_riff(body);
    }

    let exif = exif.ok_or(EncoderError::InvalidParam("missing EXIF payload"))?;
    let vp8x_payload_size = 10usize;
    let padded_exif_size = padded_len(exif.len())?;
    let body_size = 4usize
        .checked_add(8 + vp8x_payload_size)
        .and_then(|size| size.checked_add(8 + padded_image_size))
        .and_then(|size| size.checked_add(8 + padded_exif_size))
        .ok_or(EncoderError::InvalidParam("encoded output is too large"))?;
    let mut body = ByteWriter::with_capacity(body_size);
    body.write_bytes(b"WEBP");

    let mut flags = EXIF_FLAG;
    if image.has_alpha {
        flags |= ALPHA_FLAG;
    }
    let width = encode_le24(image.width)?;
    let height = encode_le24(image.height)?;
    let mut vp8x_payload = ByteWriter::with_capacity(10);
    vp8x_payload.write_u32_le(flags);
    vp8x_payload.write_bytes(&width);
    vp8x_payload.write_bytes(&height);

    append_chunk(&mut body, b"VP8X", &vp8x_payload.into_bytes())?;
    append_chunk(&mut body, &image.fourcc, image.payload)?;
    append_chunk(&mut body, b"EXIF", exif)?;
    extend_riff(body)
}

/// Wraps full-canvas animation frames in an animated RIFF/WebP container.
pub(crate) fn wrap_animated_webp(
    canvas_width: usize,
    canvas_height: usize,
    background_color: u32,
    loop_count: u16,
    has_alpha: bool,
    frames: &[AnimationFrameChunk],
    exif: Option<&[u8]>,
) -> Result<Vec<u8>, EncoderError> {
    if frames.is_empty() {
        return Err(EncoderError::InvalidParam(
            "animated WebP requires at least one frame",
        ));
    }

    let canvas_width_le24 = encode_le24(canvas_width)?;
    let canvas_height_le24 = encode_le24(canvas_height)?;

    let mut body = ByteWriter::with_capacity(4);
    body.write_bytes(b"WEBP");

    let mut flags = ANIMATION_FLAG;
    if has_alpha {
        flags |= ALPHA_FLAG;
    }
    if exif.is_some() {
        flags |= EXIF_FLAG;
    }
    let mut vp8x_payload = ByteWriter::with_capacity(10);
    vp8x_payload.write_u32_le(flags);
    vp8x_payload.write_bytes(&canvas_width_le24);
    vp8x_payload.write_bytes(&canvas_height_le24);
    append_chunk(&mut body, b"VP8X", &vp8x_payload.into_bytes())?;

    let mut anim_payload = ByteWriter::with_capacity(6);
    anim_payload.write_u32_le(background_color);
    anim_payload.write_u16_le(loop_count);
    append_chunk(&mut body, b"ANIM", &anim_payload.into_bytes())?;

    for frame in frames {
        let mut frame_payload = ByteWriter::with_capacity(16 + 8 + frame.payload.len() + 1);
        frame_payload.write_bytes(&encode_frame_offset(0)?);
        frame_payload.write_bytes(&encode_frame_offset(0)?);
        frame_payload.write_bytes(&canvas_width_le24);
        frame_payload.write_bytes(&canvas_height_le24);
        frame_payload.write_bytes(&encode_u24(
            frame.duration_ms,
            "animation frame duration exceeds WebP limits",
        )?);

        let mut frame_flags = 0_u8;
        if frame.dispose_to_background {
            frame_flags |= 0x01;
        }
        if !frame.blend {
            frame_flags |= 0x02;
        }
        frame_payload.write_byte(frame_flags);
        append_chunk(&mut frame_payload, &frame.fourcc, &frame.payload)?;
        append_chunk(&mut body, b"ANMF", &frame_payload.into_bytes())?;
    }

    if let Some(exif) = exif {
        append_chunk(&mut body, b"EXIF", exif)?;
    }

    extend_riff(body)
}
