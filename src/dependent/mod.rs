pub mod plugins;
mod thirdparty;
pub use thirdparty::{
    default_config_dir, default_download_dir, default_temp_dir, normalize_locale_tag,
    resource_locale_fallbacks,
};

#[cfg(target_os = "android")]
mod android;
#[cfg(target_os = "macos")]
mod darwin;
#[cfg(target_os = "ios")]
mod ios;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "macos",
    target_os = "android",
    target_os = "ios"
)))]
mod other;
#[cfg(target_os = "windows")]
mod windows;

//use eframe::egui::Direction;
#[cfg(target_os = "android")]
pub use android::*;
#[cfg(target_os = "macos")]
pub use darwin::*;
#[cfg(target_os = "ios")]
pub use ios::*;
#[cfg(target_os = "linux")]
pub use linux::*;
#[cfg(not(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "macos",
    target_os = "android",
    target_os = "ios"
)))]
pub use other::*;
#[cfg(target_os = "windows")]
pub use windows::*;

pub fn ui_available_roots() -> Vec<std::path::PathBuf> {
    available_roots()
}

pub fn pick_save_directory() -> Option<std::path::PathBuf> {
    pick_directory_dialog()
}

pub fn download_http_url(url: &str) -> Option<std::path::PathBuf> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("wml2viewer/0.0.1")
        .build()
        .ok()?;
    let response = client.get(url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_ascii_lowercase);
    let bytes = response.bytes().ok()?;
    let extension = infer_http_extension(&final_url, content_type.as_deref()).unwrap_or("bin");
    let temp_root = default_temp_dir().unwrap_or_else(std::env::temp_dir);
    let _ = std::fs::create_dir_all(&temp_root);
    let path = temp_root.join(format!(
        "wml2viewer_url_{}.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos(),
        extension
    ));
    std::fs::write(&path, &bytes).ok()?;
    Some(path)
}

pub fn register_system_file_associations(
    exe_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    register_file_associations(exe_path)
}

pub fn clean_system_integration() -> Result<(), Box<dyn std::error::Error>> {
    clean_file_associations()
}

fn infer_http_extension<'a>(url: &'a str, content_type: Option<&str>) -> Option<&'a str> {
    let from_url = url
        .rsplit_once('.')
        .map(|(_, ext)| ext)
        .and_then(|ext| ext.split('?').next())
        .filter(|ext| !ext.is_empty() && ext.len() <= 8);
    if from_url.is_some() {
        return from_url;
    }

    match content_type.unwrap_or_default() {
        value if value.contains("png") => Some("png"),
        value if value.contains("jpeg") || value.contains("jpg") => Some("jpg"),
        value if value.contains("webp") => Some("webp"),
        value if value.contains("gif") => Some("gif"),
        value if value.contains("bmp") => Some("bmp"),
        value if value.contains("tiff") => Some("tif"),
        value if value.contains("avif") => Some("avif"),
        _ => None,
    }
}

#[cfg(not(target_os = "windows"))]
fn register_file_associations(
    _exe_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    Err(std::io::Error::other("system integration is only supported on Windows").into())
}

#[cfg(not(target_os = "windows"))]
fn clean_file_associations() -> Result<(), Box<dyn std::error::Error>> {
    Err(std::io::Error::other("system integration is only supported on Windows").into())
}
