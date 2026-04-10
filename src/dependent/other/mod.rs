use crate::dependent::normalize_locale_tag;
use std::path::PathBuf;

pub fn default_config_dir() -> Option<PathBuf> {
    std::env::current_dir().ok().map(|dir| dir.join(".wml2"))
}

pub fn available_roots() -> Vec<PathBuf> {
    std::env::current_dir().ok().into_iter().collect()
}

pub fn system_locale() -> Option<String> {
    std::env::var("LC_ALL")
        .ok()
        .or_else(|| std::env::var("LC_MESSAGES").ok())
        .or_else(|| std::env::var("LANG").ok())
        .map(|locale| normalize_locale_tag(Some(&locale)))
}

pub fn locale_font_candidates(_locale: &str) -> Vec<PathBuf> {
    Vec::new()
}

pub fn emoji_font_candidates() -> Vec<PathBuf> {
    Vec::new()
}

pub fn last_resort_font_candidates() -> Vec<PathBuf> {
    Vec::new()
}

pub fn pick_directory_dialog() -> Option<PathBuf> {
    None
}

pub fn download_url_to_temp(_url: &str) -> Option<PathBuf> {
    None
}
