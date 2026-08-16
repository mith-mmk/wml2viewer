//! AVIF decoding backed by the standalone `avif-rust` crate.

pub mod decoder;

pub use avif_codec::{
    AvifInfo, ColorInformation, DecoderError, ImageSpatialExtents, PixelInformation,
};
