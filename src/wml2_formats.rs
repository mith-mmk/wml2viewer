use crate::drawers::image::SaveFormat;
use std::collections::BTreeSet;
use std::sync::OnceLock;

pub fn decoder_extensions() -> &'static BTreeSet<String> {
    static DECODER_EXTENSIONS: OnceLock<BTreeSet<String>> = OnceLock::new();
    DECODER_EXTENSIONS.get_or_init(|| normalize_extensions(wml2::get_decoder_extentions()))
}

pub fn encoder_extensions() -> &'static BTreeSet<String> {
    static ENCODER_EXTENSIONS: OnceLock<BTreeSet<String>> = OnceLock::new();
    ENCODER_EXTENSIONS.get_or_init(|| normalize_extensions(wml2::get_encode_extentions()))
}

pub fn supports_decoder_extension(ext: &str) -> bool {
    decoder_extensions().contains(&ext.to_ascii_lowercase())
}

pub fn available_save_formats() -> Vec<SaveFormat> {
    let encoder_extensions = encoder_extensions();
    let mut formats = Vec::new();

    for format in SaveFormat::all_known() {
        if encoder_extensions.contains(format.extension()) {
            formats.push(format);
        }
    }

    formats
}

pub fn associated_file_extensions() -> Vec<String> {
    let mut extensions: Vec<String> = decoder_extensions()
        .iter()
        .map(|ext| format!(".{ext}"))
        .collect();
    extensions.push(".zip".to_string());
    extensions.push(".wmltxt".to_string());
    extensions.sort();
    extensions.dedup();
    extensions
}

fn normalize_extensions(extensions: Vec<String>) -> BTreeSet<String> {
    extensions
        .into_iter()
        .map(|ext| ext.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{available_save_formats, decoder_extensions, encoder_extensions};
    use super::associated_file_extensions;

    #[test]
    fn decoder_extensions_include_common_formats_from_wml2() {
        let extensions = decoder_extensions();
        assert!(extensions.contains("png"));
        assert!(extensions.contains("jpg"));
    }

    #[test]
    fn encoder_extensions_include_common_formats_from_wml2() {
        let extensions = encoder_extensions();
        assert!(extensions.contains("png"));
        assert!(extensions.contains("jpg"));
    }

    #[test]
    fn available_save_formats_are_filtered_by_encoder_extensions() {
        let extensions = encoder_extensions();
        for format in available_save_formats() {
            assert!(extensions.contains(format.extension()));
        }
    }

    #[test]
    fn associated_file_extensions_include_viewer_specific_types() {
        let extensions = associated_file_extensions();
        assert!(extensions.iter().any(|ext| ext == ".zip"));
        assert!(extensions.iter().any(|ext| ext == ".wmltxt"));
        assert!(extensions.iter().any(|ext| ext == ".png"));
    }
}
