use super::associated_file_extensions;
use super::{available_save_formats, decoder_extensions, encoder_extensions};

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
