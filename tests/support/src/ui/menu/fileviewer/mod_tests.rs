    use super::{locale_datetime_pattern, normalize_backslash_display};

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

