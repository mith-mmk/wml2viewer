//! Format detection helpers and shared image format identifiers.

use bin_rs::io::read_string;

/// Image formats recognized by [`format_check`].
pub enum ImageFormat {
    Gif,  // GIF87a , GIF89a
    Jpeg, // 0xfffe
    Bmp,  // BM
    Ico,  // 00 00 01 00
    Tiff, // II/MM
    Png,  // [0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A]
    Webp, // RIFF . . . . WEBP
    //
    // Japanse old format
    Mag,
    #[cfg(not(feature = "noretoro"))]
    Maki,
    #[cfg(not(feature = "noretoro"))]
    Pi,
    #[cfg(not(feature = "noretoro"))]
    Pic,
    Pic2,
    #[cfg(not(feature = "noretoro"))]
    Vsp,
    #[cfg(not(feature = "noretoro"))]
    Pcd,
    RiffFormat(String),
    Unknown,
}

/// Detects an image format from the leading bytes of `buffer`.
pub fn format_check(buffer: &[u8]) -> ImageFormat {
    if buffer.len() >= 4 && buffer.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return ImageFormat::Ico;
    }
    if buffer.len() >= 6
        && buffer[0] == b'G'
        && buffer[1] == b'I'
        && buffer[2] == b'F'
        && buffer[3] == b'8'
        && (buffer[4] == b'7' || buffer[4] == b'9')
        && buffer[5] == b'a'
    {
        return ImageFormat::Gif;
    }
    if buffer.len() >= 2 && buffer[0] == b'B' && buffer[1] == b'M' {
        return ImageFormat::Bmp;
    }
    if buffer.len() >= 4 && buffer[0] == b'I' && buffer[1] == b'I' {
        let ver = bin_rs::io::read_u16_le(buffer, 2);
        if ver == 42 {
            return ImageFormat::Tiff;
        }
    }
    if buffer.len() >= 4 && buffer[0] == b'M' && buffer[1] == b'M' {
        let ver = bin_rs::io::read_u16_be(buffer, 2);
        if ver == 42 {
            return ImageFormat::Tiff;
        }
        return ImageFormat::Tiff;
    }
    if buffer.len() >= 6 && buffer.starts_with(b"MAKI02") {
        return ImageFormat::Mag;
    }
    #[cfg(not(feature = "noretoro"))]
    if buffer.len() >= 6 && buffer.starts_with(b"MAKI01") {
        return ImageFormat::Maki;
    }
    #[cfg(not(feature = "noretoro"))]
    if buffer.len() >= 2 && buffer[0] == b'P' && buffer[1] == b'i' {
        return ImageFormat::Pi;
    }
    #[cfg(not(feature = "noretoro"))]
    if buffer.len() >= 3 && buffer[0] == b'P' && buffer[1] == b'I' && buffer[2] == b'C' {
        return ImageFormat::Pic;
    }
    if buffer.len() >= 12 && buffer.starts_with(b"RIFF") {
        if &buffer[8..12] == b"WEBP" {
            return ImageFormat::Webp;
        }
        let s = read_string(buffer, 8, 4);
        return ImageFormat::RiffFormat(s);
    }
    if buffer.len() >= 8
        && buffer[0] == 0x89
        && buffer[1] == 0x50
        && buffer[2] == 0x4e
        && buffer[3] == 0x47
        && buffer[4] == 0x0d
        && buffer[5] == 0x0a
        && buffer[6] == 0x1a
        && buffer[7] == 0x0a
    {
        return ImageFormat::Png;
    }
    if buffer.len() >= 2 && buffer[0] == 0xff && buffer[1] == 0xd8 {
        return ImageFormat::Jpeg;
    }

    #[cfg(not(feature = "noretoro"))]
    if buffer.len() >= 10 {
        let pixel = buffer[8];
        let start_x = bin_rs::io::read_u16_le(buffer, 0);
        let end_x = bin_rs::io::read_u16_le(buffer, 4);
        if (pixel == 0 || pixel == 1 || pixel == 8) && start_x <= 80 && (end_x <= 80 || pixel == 1)
        {
            return ImageFormat::Vsp;
        }
        let page_count = bin_rs::io::read_u16_le(buffer, 0);
        if page_count > 0 && page_count <= 0x10 {
            return ImageFormat::Vsp;
        }
    }
    ImageFormat::Unknown
}

#[cfg(test)]
mod tests {
    use super::{ImageFormat, format_check};

    #[test]
    fn short_buffers_return_unknown_instead_of_panicking() {
        for len in 0..12 {
            let buffer = vec![0; len];
            let _ = format_check(&buffer);
        }
    }

    #[test]
    fn short_riff_header_does_not_panic() {
        let buffer = b"RIFF\x00\x00\x00\x00".to_vec();
        assert!(matches!(format_check(&buffer), ImageFormat::Unknown));
    }
}

/// Returns whether a decoder for `format` is enabled in the current build.
pub fn decoder_supports_format(format: &ImageFormat) -> bool {
    match format {
        #[cfg(feature = "gif")]
        ImageFormat::Gif => true,
        #[cfg(feature = "jpeg")]
        ImageFormat::Jpeg => true,
        #[cfg(feature = "bmp")]
        ImageFormat::Bmp => true,
        #[cfg(feature = "ico")]
        ImageFormat::Ico => true,
        #[cfg(feature = "tiff")]
        ImageFormat::Tiff => true,
        #[cfg(feature = "png")]
        ImageFormat::Png => true,
        #[cfg(feature = "webp")]
        ImageFormat::Webp => true,
        #[cfg(all(feature = "mag", not(feature = "noretoro")))]
        ImageFormat::Mag => true,
        #[cfg(all(feature = "maki", not(feature = "noretoro")))]
        ImageFormat::Maki => true,
        #[cfg(all(feature = "pi", not(feature = "noretoro")))]
        ImageFormat::Pi => true,
        #[cfg(all(feature = "pic", not(feature = "noretoro")))]
        ImageFormat::Pic => true,
        #[cfg(all(feature = "vsp", not(feature = "noretoro")))]
        ImageFormat::Vsp => true,
        #[cfg(all(feature = "pcd", not(feature = "noretoro")))]
        ImageFormat::Pcd => true,
        _ => false,
    }
}

/// Returns whether an encoder for `format` is enabled in the current build.
pub fn encoder_supports_format(format: &ImageFormat) -> bool {
    match format {
        #[cfg(feature = "gif")]
        ImageFormat::Gif => true,
        #[cfg(feature = "jpeg")]
        ImageFormat::Jpeg => true,
        #[cfg(feature = "bmp")]
        ImageFormat::Bmp => true,
        #[cfg(feature = "png")]
        ImageFormat::Png => true,
        #[cfg(feature = "tiff")]
        ImageFormat::Tiff => true,
        #[cfg(feature = "webp")]
        ImageFormat::Webp => true,
        _ => false,
    }
}
