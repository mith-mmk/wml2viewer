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
#[path = "../tests/support/src/wml2_formats_tests.rs"]
mod tests;
