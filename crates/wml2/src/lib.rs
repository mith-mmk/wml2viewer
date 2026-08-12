/*
 * WML2 - Web graphic Multi format Library To Rust
 *  (C)Mith@mmk 2022
 *
 *  use MIT Licnce
 */
// Preserve the published 0.0.23 source style in this vendored compatibility
// snapshot. Product crates remain checked with workspace `-D warnings`.
#![allow(clippy::all)]

//! Multi-format image decoding and encoding for RGBA buffers.
//!
//! `wml2` exposes a callback-based decoding API in [`draw`] and built-in
//! buffer-backed helpers via [`draw::ImageBuffer`]. The crate can decode still
//! images and animations into RGBA buffers, preserve metadata, and encode BMP,
//! GIF, JPEG, PNG/APNG, TIFF, and WebP output.
//!
//! # Example
//! ```rust
//! use wml2::draw::*;
//! use wml2::metadata::DataMap;
//! use std::error::Error;
//! use std::env;
//!
//! pub fn main()-> Result<(),Box<dyn Error>> {
//!     let args: Vec<String> = env::args().collect();
//!     if args.len() < 2 {
//!         println!("usage: metadata <inputfilename>");
//!         return Ok(())
//!     }
//!
//!     let filename = &args[1];
//!     let mut image = image_from_file(filename.to_string())?;
//!     let metadata = image.metadata()?;
//!     if let Some(metadata) = metadata {
//!         for (key,value) in metadata {
//!             match value {
//!                 DataMap::None => {
//!                     println!("{}",key);
//!                 },
//!                 DataMap::Raw(value) => {
//!                     println!("{}: {}bytes",key,value.len());
//!                 },
//!                 DataMap::Ascii(string) => {
//!                     println!("{}: {}",key,string);
//!                 },
//!                 DataMap::Exif(value) => {
//!                     println!("=============== EXIF START ==============");
//!                     let string = value.to_string();
//!                     println!("{}", string);
//!                     println!("================ EXIF END ===============");
//!                 },
//!                 DataMap::ICCProfile(data) => {
//!                     println!("{}: {}bytes",key,data.len());
//!                 },
//!                 _ => {
//!                     println!("{}: {:?}",key,value);
//!                 }
//!             }
//!         }        
//!     }
//!     Ok(())
//! }
//! ```

use crate::util::ImageFormat;
use crate::util::decoder_supports_format;

// 0.0.20 new!
/// get_version get WML2 crate version
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// 0.0.20 new!
/// get_decoder_extentions get extentions of WML2's decoders
pub fn get_decoder_extentions() -> Vec<String> {
    let mut exts = Vec::new();
    #[cfg(feature = "bmp")]
    exts.push("bmp".to_string());
    #[cfg(feature = "gif")]
    exts.push("gif".to_string());
    #[cfg(feature = "ico")]
    exts.push("ico".to_string());
    #[cfg(feature = "jpeg")]
    {
        exts.push("jpg".to_string());
        exts.push("jpe".to_string());
        exts.push("jpeg".to_string());
    }
    #[cfg(feature = "png")]
    exts.push("png".to_string());
    #[cfg(feature = "tiff")]
    {
        exts.push("tif".to_string());
        exts.push("tiff".to_string());
    }
    #[cfg(feature = "webp")]
    exts.push("webp".to_string());
    #[cfg(all(feature = "mag", not(feature = "noretoro")))]
    exts.push("mag".to_string());
    #[cfg(all(feature = "maki", not(feature = "noretoro")))]
    exts.push("mki".to_string());
    #[cfg(all(feature = "pi", not(feature = "noretoro")))]
    exts.push("pi".to_string());
    #[cfg(all(feature = "pic", not(feature = "noretoro")))]
    exts.push("pic".to_string());
    exts
}

// 0.0.20 new!
/// get_can_decode get WML2 crate decoder header check
pub fn get_can_decode(buffer: &[u8]) -> Result<bool, Box<dyn std::error::Error>> {
    let result = crate::util::format_check(buffer);
    match result {
        ImageFormat::Unknown => Ok(false),
        ImageFormat::RiffFormat(_) => Ok(false),
        _ => Ok(decoder_supports_format(&result)),
    }
}

// 0.0.20 new!
/// get_encode_extentions get extentions of WML2's encoders
pub fn get_encode_extentions() -> Vec<String> {
    let mut exts = Vec::new();
    #[cfg(feature = "bmp")]
    exts.push("bmp".to_string());
    #[cfg(feature = "gif")]
    exts.push("gif".to_string());
    #[cfg(feature = "jpeg")]
    {
        exts.push("jpg".to_string());
        exts.push("jpe".to_string());
        exts.push("jpeg".to_string());
    }
    #[cfg(feature = "png")]
    exts.push("png".to_string());
    #[cfg(feature = "tiff")]
    {
        exts.push("tif".to_string());
        exts.push("tiff".to_string());
    }
    #[cfg(feature = "webp")]
    exts.push("webp".to_string());
    exts
}

//pub(crate) mod io; // move bin_rs crate
#[cfg(feature = "bmp")]
pub mod bmp;
pub mod draw;
pub mod encoder;
pub mod error;
#[cfg(feature = "gif")]
pub mod gif;
#[cfg(feature = "ico")]
pub mod ico;
#[cfg(feature = "jpeg")]
pub mod jpeg;
#[cfg(all(feature = "mag", not(feature = "noretoro")))]
pub mod mag;
#[cfg(all(feature = "maki", not(feature = "noretoro")))]
pub mod maki;
#[cfg(all(feature = "pcd", not(feature = "noretoro")))]
pub mod pcd;
#[cfg(all(feature = "pi", not(feature = "noretoro")))]
pub mod pi;
#[cfg(all(feature = "pic", not(feature = "noretoro")))]
pub mod pic;
#[cfg(feature = "png")]
pub mod png;
#[cfg(any(
    all(feature = "maki", not(feature = "noretoro")),
    all(feature = "pcd", not(feature = "noretoro")),
    all(feature = "pi", not(feature = "noretoro")),
    all(feature = "pic", not(feature = "noretoro")),
    all(feature = "vsp", not(feature = "noretoro"))
))]
mod retro;
#[cfg(any(feature = "tiff", feature = "exif"))]
pub mod tiff;
pub mod util;
#[cfg(all(feature = "vsp", not(feature = "noretoro")))]
pub mod vsp;
pub mod warning;
//pub mod iccprofile;
pub mod color;
pub mod decoder;
pub mod metadata;
#[cfg(feature = "webp")]
pub mod webp;
