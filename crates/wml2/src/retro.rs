//! Legacy retro-format module toggles.

use bin_rs::reader::BinaryReader;

use crate::draw::DecodeOptions;
use crate::error::{ImgError, ImgErrorKind};

type Error = Box<dyn std::error::Error>;

pub(crate) fn err(kind: ImgErrorKind, message: &str) -> Error {
    Box::new(ImgError::new_const(kind, message.to_string()))
}

pub(crate) fn read_all<B: BinaryReader>(reader: &mut B) -> Result<Vec<u8>, Error> {
    let mut buffer = Vec::new();
    while let Ok(byte) = reader.read_byte() {
        buffer.push(byte);
    }
    Ok(buffer)
}

pub(crate) fn clamp8(value: i32) -> u8 {
    if value < 0 {
        0
    } else if value > 255 {
        255
    } else {
        value as u8
    }
}

pub(crate) fn draw_indexed(
    option: &mut DecodeOptions,
    width: usize,
    height: usize,
    pixels: &[u8],
    palette: &[(u8, u8, u8)],
) -> Result<(), Error> {
    option
        .drawer
        .init(width, height, crate::draw::InitOptions::new())?;
    let mut rgba = vec![0u8; width * height * 4];
    for (i, &index) in pixels.iter().enumerate().take(width * height) {
        let (r, g, b) = palette.get(index as usize).copied().unwrap_or((0, 0, 0));
        let offset = i * 4;
        rgba[offset] = r;
        rgba[offset + 1] = g;
        rgba[offset + 2] = b;
        rgba[offset + 3] = 0xff;
    }
    option.drawer.draw(0, 0, width, height, &rgba, None)?;
    Ok(())
}

pub(crate) fn draw_rgb(
    option: &mut DecodeOptions,
    width: usize,
    height: usize,
    pixels: &[u8],
) -> Result<(), Error> {
    option
        .drawer
        .init(width, height, crate::draw::InitOptions::new())?;
    let mut rgba = vec![0u8; width * height * 4];
    for i in 0..(width * height) {
        let src = i * 3;
        let dst = i * 4;
        if src + 2 >= pixels.len() {
            break;
        }
        rgba[dst] = pixels[src];
        rgba[dst + 1] = pixels[src + 1];
        rgba[dst + 2] = pixels[src + 2];
        rgba[dst + 3] = 0xff;
    }
    option.drawer.draw(0, 0, width, height, &rgba, None)?;
    Ok(())
}

pub(crate) struct ByteCursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> ByteCursor<'a> {
    pub(crate) fn new(data: &'a [u8], offset: usize) -> Self {
        Self { data, offset }
    }

    pub(crate) fn tell(&self) -> usize {
        self.offset
    }

    pub(crate) fn seek(&mut self, offset: usize) -> Result<(), Error> {
        if offset > self.data.len() {
            return Err(err(ImgErrorKind::IllegalData, "Unexpected end of input"));
        }
        self.offset = offset;
        Ok(())
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, Error> {
        if self.offset >= self.data.len() {
            return Err(err(ImgErrorKind::IllegalData, "Unexpected end of input"));
        }
        let value = self.data[self.offset];
        self.offset += 1;
        Ok(value)
    }

    pub(crate) fn read_u16_le(&mut self) -> Result<u16, Error> {
        let lo = self.read_u8()? as u16;
        let hi = self.read_u8()? as u16;
        Ok(lo | (hi << 8))
    }

    pub(crate) fn read_u16_be(&mut self) -> Result<u16, Error> {
        let hi = self.read_u8()? as u16;
        let lo = self.read_u8()? as u16;
        Ok((hi << 8) | lo)
    }

    pub(crate) fn read_bytes(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let end = self.offset.saturating_add(length);
        if end > self.data.len() {
            return Err(err(ImgErrorKind::IllegalData, "Unexpected end of input"));
        }
        let slice = &self.data[self.offset..end];
        self.offset = end;
        Ok(slice)
    }
}

pub(crate) struct BitReaderWordMsb<'a> {
    data: &'a [u8],
    byte_offset: usize,
    word: u16,
    bits_remaining: u8,
}

impl<'a> BitReaderWordMsb<'a> {
    pub(crate) fn new(data: &'a [u8], offset: usize) -> Self {
        Self {
            data,
            byte_offset: offset,
            word: 0,
            bits_remaining: 0,
        }
    }

    pub(crate) fn read_bit(&mut self) -> u8 {
        if self.bits_remaining == 0 {
            let hi = self.data.get(self.byte_offset).copied().unwrap_or(0) as u16;
            let lo = self.data.get(self.byte_offset + 1).copied().unwrap_or(0) as u16;
            self.word = (hi << 8) | lo;
            self.byte_offset += 2;
            self.bits_remaining = 16;
        }
        self.bits_remaining -= 1;
        ((self.word >> self.bits_remaining) & 1) as u8
    }

    pub(crate) fn read_bits(&mut self, count: usize) -> u32 {
        let mut value = 0u32;
        for _ in 0..count {
            value = (value << 1) | self.read_bit() as u32;
        }
        value
    }
}
