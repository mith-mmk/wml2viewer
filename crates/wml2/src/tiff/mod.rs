//! TIFF format support, including EXIF-oriented metadata parsing.

#[cfg(feature = "tiff")]
pub mod decoder;
#[cfg(feature = "tiff")]
pub mod encoder;
#[cfg(feature = "exif")]
pub mod header;
#[cfg(feature = "exif")]
pub mod tags;
#[cfg(feature = "exif")]
pub(crate) mod util;
#[cfg(feature = "tiff")]
pub mod warning;
