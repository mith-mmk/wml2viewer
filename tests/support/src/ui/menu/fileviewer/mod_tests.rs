use super::{
    ExifTagSets, ExifTagSpec, append_exif_tag_group, format_exif_tag_line, locale_datetime_pattern,
    normalize_backslash_display,
};
use wml2::tiff::header::{DataPack, TiffHeader};

#[test]
fn locale_datetime_pattern_supports_multiple_locales() {
    assert_eq!(locale_datetime_pattern("ja_JP"), "%Y/%m/%d %H:%M");
    assert_eq!(locale_datetime_pattern("en_US"), "%m/%d/%Y %I:%M %p");
    assert_eq!(locale_datetime_pattern("de_DE"), "%d.%m.%Y %H:%M");
    assert_eq!(locale_datetime_pattern("fr_FR"), "%d/%m/%Y %H:%M");
}

#[test]
fn normalize_backslash_display_collapses_escaped_sequences() {
    assert_eq!(
        normalize_backslash_display(r"C:\\Users\\misir\\image.png"),
        r"C:\Users\misir\image.png"
    );
}

#[test]
fn normalize_backslash_display_keeps_unc_prefix() {
    assert_eq!(
        normalize_backslash_display(r"\\server\\share\\folder\\a.png"),
        r"\\server\share\folder\a.png"
    );
}

#[test]
fn format_exif_tag_line_uses_wml2_tag_names_and_values() {
    let tags = ExifTagSets {
        primary: vec![TiffHeader {
            tagid: 0x010f,
            data: DataPack::Ascii("Canon\0".to_string()),
            length: 6,
        }],
        exif: vec![TiffHeader {
            tagid: 0x829a,
            data: DataPack::Rational(vec![wml2::tiff::header::Rational { n: 1, d: 125 }]),
            length: 1,
        }],
        gps: Vec::new(),
    };

    assert_eq!(
        format_exif_tag_line(&tags, ExifTagSpec::primary(0x010f)).as_deref(),
        Some("Make: Canon")
    );
    assert_eq!(
        format_exif_tag_line(&tags, ExifTagSpec::exif(0x829a)).as_deref(),
        Some("ExposureTime: 1/125")
    );
}

#[test]
fn append_exif_tag_group_reports_not_available_when_group_is_empty() {
    let mut lines = Vec::new();
    append_exif_tag_group(
        &mut lines,
        &ExifTagSets::default(),
        &[ExifTagSpec::gps(0x0002)],
    );
    assert_eq!(lines, vec!["(not available)".to_string()]);
}
