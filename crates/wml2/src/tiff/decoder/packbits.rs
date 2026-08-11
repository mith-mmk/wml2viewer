//! PackBits decoder for TIFF.

type Error = Box<dyn std::error::Error>;

use crate::draw::{DecodeLimitError, DecodeLimitKind};
use crate::error::{ImgError, ImgErrorKind};

pub fn decode(data: &[u8]) -> Result<Vec<u8>, Error> {
    decode_with_limit(data, usize::MAX)
}

pub fn decode_with_limit(data: &[u8], maximum_output: usize) -> Result<Vec<u8>, Error> {
    let mut buf = vec![];
    let mut i = 0;
    while i < data.len() {
        let run = data[i] as usize;
        i += 1;
        if run > 128 {
            let len = 256 - run;
            let byte = *data.get(i).ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::UnexpectedEof,
                    "truncated PackBits run".to_string(),
                )) as Error
            })?;
            extend_repeat(&mut buf, byte, len + 1, maximum_output)?;
            i += 1;
        } else if run < 128 {
            let count = run + 1;
            let end = i.checked_add(count).ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    "PackBits input offset overflow".to_string(),
                )) as Error
            })?;
            let literal = data.get(i..end).ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::UnexpectedEof,
                    "truncated PackBits literal".to_string(),
                )) as Error
            })?;
            extend_bytes(&mut buf, literal, maximum_output)?;
            i = end;
        }
    }
    Ok(buf)
}

fn checked_output_end(current: usize, additional: usize, maximum: usize) -> Result<usize, Error> {
    let attempted = current.checked_add(additional).ok_or_else(|| {
        Box::new(DecodeLimitError::new(
            DecodeLimitKind::DecodedBytes,
            u128::MAX,
            maximum as u128,
            0,
            0,
        )) as Error
    })?;
    if attempted > maximum {
        return Err(Box::new(DecodeLimitError::new(
            DecodeLimitKind::DecodedBytes,
            attempted as u128,
            maximum as u128,
            0,
            0,
        )));
    }
    Ok(attempted)
}

fn extend_bytes(output: &mut Vec<u8>, bytes: &[u8], maximum: usize) -> Result<(), Error> {
    checked_output_end(output.len(), bytes.len(), maximum)?;
    output.try_reserve(bytes.len()).map_err(|error| {
        Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            format!("PackBits allocation failed: {error}"),
        )) as Error
    })?;
    output.extend_from_slice(bytes);
    Ok(())
}

fn extend_repeat(
    output: &mut Vec<u8>,
    byte: u8,
    count: usize,
    maximum: usize,
) -> Result<(), Error> {
    let end = checked_output_end(output.len(), count, maximum)?;
    output.try_reserve(count).map_err(|error| {
        Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            format!("PackBits allocation failed: {error}"),
        )) as Error
    })?;
    output.resize(end, byte);
    Ok(())
}
