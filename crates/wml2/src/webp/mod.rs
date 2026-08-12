//! WebP support backed by the standalone `webp-rust` codec crate.
//!
//! `wml2` keeps its metadata and draw/pick integration locally, while the WebP
//! bitstream parser and still-image codec core live in the sibling repository.

type Error = Box<dyn std::error::Error>;

/// Lower-level WebP parsing and decoding APIs plus the `wml2` draw adapter.
pub mod decoder;
/// WebP encoding APIs plus the `wml2` draw/pick adapter.
pub mod encoder;
pub mod utils;
pub mod warning;

pub use webp_codec::{
    AnimationControl, AnimationFrame, ImageBuffer, WebpHeader, read_header, read_u24,
};

/// Decodes a still WebP image from memory into an RGBA buffer.
pub fn image_from_bytes(data: &[u8]) -> Result<ImageBuffer, webp_codec::DecoderError> {
    webp_codec::image_from_bytes(data)
}

#[cfg(not(target_family = "wasm"))]
/// Reads a still WebP image from disk and decodes it to RGBA.
pub fn image_from_file(filename: String) -> Result<ImageBuffer, Error> {
    webp_codec::image_from_file(filename)
}
