use super::{
    ExifTagSets, ExifTagSpec, append_exif_tag_group, clamp_popup_position, filer_width_range,
    format_exif_tag_line, locale_datetime_pattern, normalize_backslash_display,
    subfiler_height_range,
};
use eframe::egui;
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

#[test]
fn popup_position_is_clamped_inside_tiny_initial_rect() {
    let content = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 240.0));
    let pos = clamp_popup_position(egui::pos2(300.0, 220.0), content, egui::vec2(280.0, 220.0));

    assert_eq!(pos, egui::pos2(40.0, 20.0));
}

#[test]
fn filer_width_leaves_room_for_viewer_on_tiny_initial_rect() {
    let content = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 240.0));
    let range = filer_width_range(content, 420.0);

    assert_eq!(range.min, 160.0);
    assert_eq!(range.max, 160.0);
    assert_eq!(range.default, 160.0);
}

#[test]
fn subfiler_height_leaves_room_for_viewer_on_tiny_initial_rect() {
    let content = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(320.0, 240.0));
    let range = subfiler_height_range(content);

    assert!(range.default <= 110.0);
    assert!(range.max < 120.0);
    assert!(range.min <= range.default);
}
