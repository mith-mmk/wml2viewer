//! LZW decoder used by GIF and TIFF.

type Error = Box<dyn std::error::Error>;
use crate::draw::{DecodeLimitError, DecodeLimitKind};
use crate::error::ImgError;
use crate::error::ImgErrorKind;
use bin_rs::io::read_byte;

const MAX_TABLE: usize = 4096;
const MAX_CBL: usize = 12;

pub struct Lzwdecode {
    buffer: Vec<u8>,
    cbl: usize,
    recovery_cbl: usize,
    last_byte: u32,
    left_bits: usize,
    bit_mask: u32,
    ptr: usize,
    clear: usize,
    end: usize,
    max_table: usize,
    dic: Vec<Vec<u8>>,
    prev_code: usize,
    is_init: bool,
    is_lsb: bool,
    is_tiff: usize,
}

impl Lzwdecode {
    fn insufficient_bits_error() -> Error {
        Box::new(ImgError::new_const(
            ImgErrorKind::IOError,
            "data shortage".to_string(),
        ))
    }

    /// for GIF LZW
    pub fn gif(lzw_min_bits: usize) -> Self {
        Self::new(lzw_min_bits, true, false)
    }

    /// for Tiff LZW
    pub fn tiff(is_lsb: bool) -> Self {
        Self::new(8, is_lsb, true)
    }

    pub fn new(lzw_min_bits: usize, is_lsb: bool, is_tiff: bool) -> Self {
        let cbl = lzw_min_bits + 1;
        let clear_code = 1 << lzw_min_bits;
        let max_table = MAX_TABLE;
        let is_tiff = if is_tiff { 1 } else { 0 };
        Self {
            buffer: Vec::new(),
            cbl,
            recovery_cbl: cbl,
            bit_mask: (1 << cbl) - 1,
            last_byte: 0,
            left_bits: 0,
            ptr: 0,
            clear: clear_code,
            end: clear_code + 1,
            max_table,
            dic: Vec::with_capacity(max_table),
            prev_code: clear_code,
            is_init: false,
            is_lsb,  // GIF Must True
            is_tiff, // if tiff set 1
        }
    }

    // use 32bit
    fn fill_bits(&mut self) {
        self.clear_dic();
        self.last_byte = 0;
        let ptr = self.ptr;
        if self.is_lsb {
            self.last_byte = read_byte(&self.buffer, ptr) as u32;
            self.last_byte |= (read_byte(&self.buffer, ptr + 1) as u32) << 8;
            self.last_byte |= (read_byte(&self.buffer, ptr + 2) as u32) << 16;
        } else {
            self.last_byte = (read_byte(&self.buffer, ptr) as u32) << 16;
            self.last_byte |= (read_byte(&self.buffer, ptr + 1) as u32) << 8;
            self.last_byte |= read_byte(&self.buffer, ptr + 2) as u32;
        }
        self.left_bits = 24;
        self.ptr = 3;
    }

    fn get_bits(&mut self) -> Result<usize, Error> {
        if self.is_lsb {
            return self.get_bits_lsb();
        } else {
            return self.get_bits_msb();
        }
    }

    fn get_bits_msb(&mut self) -> Result<usize, Error> {
        let size = self.cbl;
        while self.left_bits <= 16 {
            if self.ptr >= self.buffer.len() {
                if self.left_bits < size {
                    return Err(Self::insufficient_bits_error());
                }
                break;
            }

            self.last_byte = (self.last_byte << 8) | (self.buffer[self.ptr] as u32);
            self.ptr += 1;
            self.left_bits += 8;
        }
        if self.left_bits < size {
            return Err(Self::insufficient_bits_error());
        }
        let bits = (self.last_byte >> (self.left_bits - size)) & self.bit_mask;

        self.left_bits -= size;
        Ok(bits as usize)
    }

    fn get_bits_lsb(&mut self) -> Result<usize, Error> {
        let size = self.cbl;
        while self.left_bits <= 16 {
            if self.ptr >= self.buffer.len() {
                if self.left_bits < size {
                    return Err(Self::insufficient_bits_error());
                }
                break;
            }
            self.last_byte =
                (self.last_byte >> 8) & 0xffff | ((self.buffer[self.ptr] as u32) << 16);

            self.ptr += 1;
            self.left_bits += 8;
        }
        if self.left_bits < size {
            return Err(Self::insufficient_bits_error());
        }
        let bits = (self.last_byte >> (24 - self.left_bits)) & self.bit_mask;

        self.left_bits -= size;
        Ok(bits as usize)
    }

    fn clear_dic(&mut self) {
        self.dic = (0..self.end + 1)
            .map(|i| {
                if i < self.clear {
                    vec![i as u8]
                } else {
                    vec![]
                }
            })
            .collect();
        self.cbl = self.recovery_cbl;
        self.bit_mask = ((1_u64 << self.cbl) - 1) as u32;
    }

    // Multi chuck image data decoding is not debug.
    pub fn decode(&mut self, buf: &[u8]) -> Result<Vec<u8>, Error> {
        self.decode_with_limit(buf, usize::MAX)
    }

    /// Decodes LZW while refusing to grow the output past `maximum_output`.
    pub fn decode_with_limit(
        &mut self,
        buf: &[u8],
        maximum_output: usize,
    ) -> Result<Vec<u8>, Error> {
        self.buffer.clear();
        self.buffer.try_reserve_exact(buf.len()).map_err(|error| {
            Box::new(ImgError::new_const(
                ImgErrorKind::DecodeError,
                format!("LZW input allocation failed: {error}"),
            )) as Error
        })?;
        self.buffer.extend_from_slice(buf);
        if !self.is_init {
            self.fill_bits();
            self.is_init = true;
        } else {
            self.ptr = 0;
        }

        let mut data: Vec<u8> = Vec::new();
        data.try_reserve(buf.len().min(maximum_output))
            .map_err(|error| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::DecodeError,
                    format!("LZW output allocation failed: {error}"),
                )) as Error
            })?;
        self.prev_code = self.clear; // NULL

        loop {
            let res = self.get_bits(); // GIF Lsb only Tiff use Lsb or Msb
            // If data is shotage,it returns values and waits a next buffer.
            let code = if let Ok(code) = res {
                code
            } else {
                return Ok(data);
            };

            if code == self.clear {
                append_limited(&mut data, &self.dic[self.prev_code], maximum_output)?;
                self.clear_dic();
            } else if code == self.end {
                let prev = self.dic.get(self.prev_code).ok_or_else(|| {
                    Box::new(ImgError::new_const(
                        ImgErrorKind::IllegalData,
                        "invalid previous LZW code".to_string(),
                    )) as Error
                })?;
                append_limited(&mut data, prev, maximum_output)?;
                return Ok(data);
            } else if code > self.dic.len() {
                let message = format!(
                    "Over table in LZW.Table size is {},but code is {}",
                    self.dic.len(),
                    code
                );
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::IllegalData,
                    message,
                )));
            } else {
                let append_code;
                if code == self.dic.len() {
                    append_code = *self
                        .dic
                        .get(self.prev_code)
                        .and_then(|value| value.first())
                        .ok_or_else(|| {
                            Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                "invalid previous LZW code".to_string(),
                            )) as Error
                        })?;
                } else {
                    append_code = *self
                        .dic
                        .get(code)
                        .and_then(|value| value.first())
                        .ok_or_else(|| {
                            Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                "invalid LZW code".to_string(),
                            )) as Error
                        })?;
                }
                if self.prev_code != self.end && self.prev_code != self.clear {
                    let mut table: Vec<u8> = Vec::new();
                    let prev = self.dic.get(self.prev_code).ok_or_else(|| {
                        Box::new(ImgError::new_const(
                            ImgErrorKind::IllegalData,
                            "invalid previous LZW code".to_string(),
                        )) as Error
                    })?;
                    append_limited(&mut data, prev, maximum_output)?;
                    table
                        .try_reserve_exact(prev.len().saturating_add(1))
                        .map_err(|error| {
                            Box::new(ImgError::new_const(
                                ImgErrorKind::DecodeError,
                                format!("LZW dictionary allocation failed: {error}"),
                            )) as Error
                        })?;
                    table.extend_from_slice(prev);
                    table.push(append_code);
                    self.dic.push(table);
                    // Tiff LZW is increment entry value before next loop.
                    let next = self.dic.len() + self.is_tiff;
                    if next == self.bit_mask as usize + 1
                        && next < self.max_table
                        && self.cbl < MAX_CBL
                    {
                        self.cbl += 1;
                        self.bit_mask = (self.bit_mask << 1) | 1;
                    }
                }
            }
            self.prev_code = code;
        }
    }
}

fn append_limited(output: &mut Vec<u8>, bytes: &[u8], maximum_output: usize) -> Result<(), Error> {
    let attempted = output.len().checked_add(bytes.len()).ok_or_else(|| {
        Box::new(DecodeLimitError::new(
            DecodeLimitKind::DecodedBytes,
            u128::MAX,
            maximum_output as u128,
            0,
            0,
        )) as Error
    })?;
    if attempted > maximum_output {
        return Err(Box::new(DecodeLimitError::new(
            DecodeLimitKind::DecodedBytes,
            attempted as u128,
            maximum_output as u128,
            0,
            0,
        )));
    }
    output.try_reserve(bytes.len()).map_err(|error| {
        Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            format!("LZW output allocation failed: {error}"),
        )) as Error
    })?;
    output.extend_from_slice(bytes);
    Ok(())
}
