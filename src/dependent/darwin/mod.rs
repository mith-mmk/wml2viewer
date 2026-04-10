use crate::dependent::normalize_locale_tag;
use std::path::PathBuf;

pub fn system_locale() -> Option<String> {
    std::env::var("LC_ALL")
        .ok()
        .or_else(|| std::env::var("LC_MESSAGES").ok())
        .or_else(|| std::env::var("LANG").ok())
        .map(|locale| normalize_locale_tag(Some(&locale)))
}

pub fn locale_font_candidates(locale: &str) -> Vec<PathBuf> {
    let mut fonts = Vec::new();
    if locale.starts_with("ja") {
        fonts.extend([
            PathBuf::from("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc"),
            PathBuf::from("/System/Library/Fonts/ヒラギノ角ゴシック W6.ttc"),
            PathBuf::from("/System/Library/Fonts/ヒラギノ角ゴシック W5.ttc"),
            PathBuf::from("/System/Library/Fonts/Hiragino Sans GB.ttc"),
            PathBuf::from("/Library/Fonts/NotoSansJP-Regular.otf"),
            PathBuf::from("/Library/Fonts/NotoSansCJK-Regular.ttc"),
        ]);
    } else if locale.starts_with("zh") {
        fonts.extend([
            PathBuf::from("/System/Library/Fonts/PingFang.ttc"),
            PathBuf::from("/Library/Fonts/NotoSansTC-Regular.otf"),
            PathBuf::from("/Library/Fonts/NotoSansCJK-Regular.ttc"),
        ]);
    } else if locale.starts_with("ko") {
        fonts.extend([
            PathBuf::from("/System/Library/Fonts/AppleSDGothicNeo.ttc"),
            PathBuf::from("/Library/Fonts/NotoSansCJK-Regular.ttc"),
        ]);
    }
    fonts
}

pub fn emoji_font_candidates() -> Vec<PathBuf> {
    vec![PathBuf::from("/System/Library/Fonts/Apple Color Emoji.ttc")]
}

pub fn last_resort_font_candidates() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/System/Library/Fonts/SFNS.ttf"),
        PathBuf::from("/System/Library/Fonts/Supplemental/Arial Unicode.ttf"),
        PathBuf::from("/System/Library/Fonts/Supplemental/Arial.ttf"),
        PathBuf::from("/System/Library/Fonts/Supplemental/Apple Symbols.ttf"),
        PathBuf::from("/System/Library/Fonts/Supplemental/Menlo.ttc"),
    ]
}

pub fn pick_directory_dialog() -> Option<PathBuf> {
    None
}

pub fn available_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home));
    }
    roots
}

pub fn download_url_to_temp(_url: &str) -> Option<PathBuf> {
    None
}
