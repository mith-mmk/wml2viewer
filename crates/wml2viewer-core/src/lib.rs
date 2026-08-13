//! Platform-independent contracts shared by the desktop and mobile frontends.

pub mod archive;
mod error;
pub mod image;
#[path = "reading_model.rs"]
pub mod reading;

pub use error::{CoreError, CoreErrorKind, CoreResult};

/// Filename extensions accepted by the image decoders enabled in this build.
///
/// The list is normalized, sorted, and includes filename aliases whose contents are handled by
/// the same decoder (for example, `dib` uses the BMP decoder). Platform frontends should combine
/// this list with formats verified by their OS codec rather than maintaining another native-code
/// allowlist.
pub fn internal_decoder_extensions() -> Vec<String> {
    let mut extensions = wml2::get_decoder_extentions();
    if extensions.iter().any(|extension| extension == "bmp") {
        extensions.push("dib".to_string());
    }
    extensions.sort_unstable();
    extensions.dedup();
    extensions
}

#[cfg(test)]
mod archive_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
#[path = "reading_model_tests.rs"]
mod reading_tests;

#[cfg(test)]
mod capability_tests {
    use super::internal_decoder_extensions;

    #[test]
    fn internal_decoder_extensions_include_enabled_retro_formats_and_aliases() {
        let extensions = internal_decoder_extensions();
        for expected in ["dib", "mag", "mki", "pcd", "pi", "pic", "vsp"] {
            assert!(
                extensions.iter().any(|extension| extension == expected),
                "missing enabled decoder extension: {expected}"
            );
        }
        assert!(extensions.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
