//! Minimal RGBA canvas used by the viewer-side resampler.

use std::io;

use super::error::Result;

pub trait Screen {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn buffer(&self) -> &[u8];
    fn buffer_mut(&mut self) -> &mut [u8];
    fn clear_with_color(&mut self, color: u32);
}

#[derive(Clone, Debug)]
pub struct Canvas {
    width: u32,
    height: u32,
    buffer: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        let len = Self::buffer_len(width, height).unwrap_or(0);
        Self {
            width,
            height,
            buffer: vec![0; len],
        }
    }

    pub fn from_rgba(width: u32, height: u32, buffer: Vec<u8>) -> Result<Self> {
        let expected_len = Self::buffer_len(width, height)?;
        if buffer.len() != expected_len {
            return Err(Box::new(io::Error::other(format!(
                "invalid RGBA buffer length: expected {expected_len}, got {}",
                buffer.len()
            ))));
        }

        Ok(Self {
            width,
            height,
            buffer,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    fn buffer_len(width: u32, height: u32) -> Result<usize> {
        (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                Box::new(io::Error::other("canvas is too large")) as Box<dyn std::error::Error>
            })
    }
}

impl Screen for Canvas {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    fn buffer_mut(&mut self) -> &mut [u8] {
        &mut self.buffer
    }

    fn clear_with_color(&mut self, color: u32) {
        let red = ((color >> 16) & 0xff) as u8;
        let green = ((color >> 8) & 0xff) as u8;
        let blue = (color & 0xff) as u8;
        let alpha = ((color >> 24) & 0xff) as u8;

        for pixel in self.buffer.chunks_exact_mut(4) {
            pixel[0] = red;
            pixel[1] = green;
            pixel[2] = blue;
            pixel[3] = alpha;
        }
    }
}
