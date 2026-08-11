//! Shared LZW encoder used by TIFF and GIF.

type Error = Box<dyn std::error::Error>;

use crate::error::{ImgError, ImgErrorKind};
use std::collections::HashMap;

const MAX_CODE_BITS: usize = 12;
const MAX_TABLE_SIZE: usize = 1 << MAX_CODE_BITS;

#[derive(Clone, Copy, Debug)]
struct LzwFlavor {
    min_code_size: usize,
    is_lsb: bool,
    early_change: bool,
}

struct BitPacker {
    is_lsb: bool,
    bit_buffer: u64,
    bit_count: usize,
    bytes: Vec<u8>,
}

impl BitPacker {
    fn new(is_lsb: bool) -> Self {
        Self {
            is_lsb,
            bit_buffer: 0,
            bit_count: 0,
            bytes: Vec::new(),
        }
    }

    fn write(&mut self, code: usize, width: usize) {
        if self.is_lsb {
            self.bit_buffer |= (code as u64) << self.bit_count;
            self.bit_count += width;
            while self.bit_count >= 8 {
                self.bytes.push((self.bit_buffer & 0xff) as u8);
                self.bit_buffer >>= 8;
                self.bit_count -= 8;
            }
        } else {
            self.bit_buffer = (self.bit_buffer << width) | (code as u64);
            self.bit_count += width;
            while self.bit_count >= 8 {
                let shift = self.bit_count - 8;
                self.bytes.push((self.bit_buffer >> shift) as u8);
                self.bit_buffer &= (1_u64 << shift).saturating_sub(1);
                self.bit_count -= 8;
            }
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.bit_count > 0 {
            if self.is_lsb {
                self.bytes.push((self.bit_buffer & 0xff) as u8);
            } else {
                self.bytes
                    .push((self.bit_buffer << (8 - self.bit_count)) as u8);
            }
        }
        self.bytes
    }
}

fn invalid_parameter(message: &str) -> Error {
    Box::new(ImgError::new_const(
        ImgErrorKind::InvalidParameter,
        message.to_string(),
    ))
}

fn reset_dictionary(clear_code: usize) -> HashMap<Vec<u8>, usize> {
    let mut dictionary = HashMap::with_capacity(clear_code);
    for code in 0..clear_code {
        dictionary.insert(vec![code as u8], code);
    }
    dictionary
}

fn should_increase_code_size(next_code: usize, code_size: usize, early_change: bool) -> bool {
    if code_size >= MAX_CODE_BITS {
        return false;
    }
    let threshold = 1usize << code_size;
    if early_change {
        next_code == threshold
    } else {
        next_code > threshold
    }
}

fn should_increase_code_size_for_end(
    next_code: usize,
    code_size: usize,
    early_change: bool,
) -> bool {
    if code_size >= MAX_CODE_BITS {
        return false;
    }
    let threshold = 1usize << code_size;
    if early_change {
        next_code + 1 == threshold
    } else {
        next_code == threshold
    }
}

fn encode_with_flavor(data: &[u8], flavor: LzwFlavor) -> Result<Vec<u8>, Error> {
    let clear_code = 1usize << flavor.min_code_size;
    let end_code = clear_code + 1;
    let mut code_size = flavor.min_code_size + 1;
    let mut next_code = end_code + 1;
    let mut dictionary = reset_dictionary(clear_code);
    let mut packer = BitPacker::new(flavor.is_lsb);

    packer.write(clear_code, code_size);
    if data.is_empty() {
        packer.write(end_code, code_size);
        return Ok(packer.finish());
    }

    let mut current = vec![data[0]];
    for &byte in &data[1..] {
        let mut candidate = current.clone();
        candidate.push(byte);
        if dictionary.contains_key(&candidate) {
            current = candidate;
            continue;
        }

        let current_code = *dictionary.get(&current).ok_or_else(|| {
            invalid_parameter("LZW current code missing from dictionary during encode")
        })?;
        packer.write(current_code, code_size);

        if next_code < MAX_TABLE_SIZE {
            dictionary.insert(candidate, next_code);
            next_code += 1;
            if should_increase_code_size(next_code, code_size, flavor.early_change) {
                code_size += 1;
            }
        } else {
            packer.write(clear_code, code_size);
            dictionary = reset_dictionary(clear_code);
            code_size = flavor.min_code_size + 1;
            next_code = end_code + 1;
        }

        current.clear();
        current.push(byte);
    }

    let current_code = *dictionary
        .get(&current)
        .ok_or_else(|| invalid_parameter("LZW final code missing from dictionary during encode"))?;
    packer.write(current_code, code_size);
    if should_increase_code_size_for_end(next_code, code_size, flavor.early_change) {
        code_size += 1;
    }
    packer.write(end_code, code_size);
    Ok(packer.finish())
}

/// Encodes palette indices as a GIF-compatible LZW byte stream.
pub fn encode_gif(data: &[u8], min_code_size: usize) -> Result<Vec<u8>, Error> {
    if !(2..=8).contains(&min_code_size) {
        return Err(invalid_parameter("GIF LZW min_code_size must be in 2..=8"));
    }
    encode_with_flavor(
        data,
        LzwFlavor {
            min_code_size,
            is_lsb: true,
            early_change: false,
        },
    )
}

/// Encodes bytes as TIFF-compatible LZW data.
pub fn encode_tiff(data: &[u8], is_lsb: bool) -> Result<Vec<u8>, Error> {
    encode_with_flavor(
        data,
        LzwFlavor {
            min_code_size: 8,
            is_lsb,
            early_change: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{encode_gif, encode_tiff};
    use crate::decoder::lzw::Lzwdecode;

    fn patterned_bytes() -> Vec<u8> {
        let mut bytes = Vec::with_capacity(4096);
        for i in 0..4096usize {
            bytes.push(((i * 13) ^ (i >> 3) ^ (i * 7)) as u8);
        }
        bytes
    }

    #[test]
    fn gif_lzw_roundtrips_against_decoder() {
        let mut indices = Vec::with_capacity(1024);
        for i in 0..1024usize {
            indices.push(((i * 7 + i / 11) & 0x0f) as u8);
        }

        let encoded = encode_gif(&indices, 4).unwrap();
        let mut decoder = Lzwdecode::gif(4);
        let decoded = decoder.decode(&encoded).unwrap();
        assert_eq!(decoded, indices);
    }

    #[test]
    fn gif_lzw_roundtrips_small_sequence() {
        let indices = vec![0, 1, 2, 3];
        let encoded = encode_gif(&indices, 2).unwrap();
        let mut decoder = Lzwdecode::gif(2);
        let decoded = decoder.decode(&encoded).unwrap();
        assert_eq!(decoded, indices);
    }

    #[test]
    fn gif_lzw_roundtrips_repetitive_two_color_runs() {
        let indices = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 1];
        let encoded = encode_gif(&indices, 2).unwrap();
        let mut decoder = Lzwdecode::gif(2);
        let decoded = decoder.decode(&encoded).unwrap();
        assert_eq!(decoded, indices);
    }

    #[test]
    fn gif_lzw_roundtrips_sparse_two_color_runs() {
        let indices = vec![0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0];
        let encoded = encode_gif(&indices, 2).unwrap();
        let mut decoder = Lzwdecode::gif(2);
        let decoded = decoder.decode(&encoded).unwrap();
        assert_eq!(decoded, indices);
    }

    #[test]
    fn gif_lzw_roundtrips_three_color_transparent_canvas() {
        let indices = vec![1, 1, 2, 2, 1, 0, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0];
        let encoded = encode_gif(&indices, 2).unwrap();
        let mut decoder = Lzwdecode::gif(2);
        let decoded = decoder.decode(&encoded).unwrap();
        assert_eq!(decoded, indices);
    }

    #[test]
    fn tiff_lzw_msb_roundtrips_against_decoder() {
        let source = patterned_bytes();
        let encoded = encode_tiff(&source, false).unwrap();
        let mut decoder = Lzwdecode::tiff(false);
        let decoded = decoder.decode(&encoded).unwrap();
        assert_eq!(decoded, source);
    }

    #[test]
    fn tiff_lzw_lsb_roundtrips_against_decoder() {
        let source = patterned_bytes();
        let encoded = encode_tiff(&source, true).unwrap();
        let mut decoder = Lzwdecode::tiff(true);
        let decoded = decoder.decode(&encoded).unwrap();
        assert_eq!(decoded, source);
    }
}
