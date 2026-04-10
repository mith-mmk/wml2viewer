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

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn ui_available_roots() -> Vec<std::path::PathBuf> {
    available_roots()
}

pub fn pick_save_directory() -> Option<std::path::PathBuf> {
    pick_directory_dialog()
}

#[derive(Clone, Debug, Default)]
pub struct HttpFetchRequest {
    pub url: String,
    pub if_none_match: Option<String>,
    pub if_modified_since: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpFetchMetadata {
    pub final_url: String,
    pub content_type: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpFetchResult {
    Downloaded {
        path: PathBuf,
        metadata: HttpFetchMetadata,
    },
    NotModified {
        metadata: HttpFetchMetadata,
    },
    Cancelled,
    Failed,
}

pub fn download_http_url(url: &str) -> Option<std::path::PathBuf> {
    match fetch_http_url(
        &HttpFetchRequest {
            url: url.to_string(),
            ..HttpFetchRequest::default()
        },
        None,
    ) {
        HttpFetchResult::Downloaded { path, .. } => Some(path),
        HttpFetchResult::NotModified { .. }
        | HttpFetchResult::Cancelled
        | HttpFetchResult::Failed => None,
    }
}

pub fn fetch_http_url(request: &HttpFetchRequest, cancel: Option<&AtomicBool>) -> HttpFetchResult {
    let url = request.url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return HttpFetchResult::Failed;
    }
    if http_request_cancelled(cancel) {
        return HttpFetchResult::Cancelled;
    }

    let client = match reqwest::blocking::Client::builder()
        .user_agent("wml2viewer/0.0.1")
        .build()
    {
        Ok(client) => client,
        Err(_) => return HttpFetchResult::Failed,
    };

    let mut builder = client.get(url);
    if let Some(etag) = request.if_none_match.as_deref() {
        builder = builder.header(reqwest::header::IF_NONE_MATCH, etag);
    }
    if let Some(last_modified) = request.if_modified_since.as_deref() {
        builder = builder.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
    }

    let mut response = match builder.send() {
        Ok(response) => response,
        Err(_) => {
            return if http_request_cancelled(cancel) {
                HttpFetchResult::Cancelled
            } else {
                HttpFetchResult::Failed
            };
        }
    };
    let metadata = http_fetch_metadata(&response);
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return HttpFetchResult::NotModified { metadata };
    }
    if !response.status().is_success() {
        return HttpFetchResult::Failed;
    }

    let extension = infer_http_extension(&metadata.final_url, metadata.content_type.as_deref())
        .unwrap_or("bin");
    let Some(path) = create_http_temp_file_path(extension) else {
        return HttpFetchResult::Failed;
    };
    let Ok(mut file) = std::fs::File::create(&path) else {
        return HttpFetchResult::Failed;
    };
    let mut buffer = [0u8; 64 * 1024];
    loop {
        if http_request_cancelled(cancel) {
            let _ = std::fs::remove_file(&path);
            return HttpFetchResult::Cancelled;
        }
        let read = match response.read(&mut buffer) {
            Ok(read) => read,
            Err(_) => {
                let _ = std::fs::remove_file(&path);
                return HttpFetchResult::Failed;
            }
        };
        if read == 0 {
            break;
        }
        if file.write_all(&buffer[..read]).is_err() {
            let _ = std::fs::remove_file(&path);
            return HttpFetchResult::Failed;
        }
    }

    HttpFetchResult::Downloaded { path, metadata }
}

pub fn register_system_file_associations(
    exe_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    register_file_associations(exe_path)
}

pub fn clean_system_integration() -> Result<(), Box<dyn std::error::Error>> {
    clean_file_associations()
}

#[cfg(not(target_os = "windows"))]
pub fn path_is_probably_network(path: &std::path::Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\") || text.starts_with(r"//")
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

fn create_http_temp_file_path(extension: &str) -> Option<PathBuf> {
    let temp_root = default_temp_dir().unwrap_or_else(std::env::temp_dir);
    let _ = std::fs::create_dir_all(&temp_root);
    Some(temp_root.join(format!(
        "wml2viewer_url_{}.{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos(),
        extension
    )))
}

fn http_fetch_metadata(response: &reqwest::blocking::Response) -> HttpFetchMetadata {
    HttpFetchMetadata {
        final_url: response.url().to_string(),
        content_type: response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_ascii_lowercase),
        etag: response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
        last_modified: response
            .headers()
            .get(reqwest::header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned),
    }
}

fn http_request_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::Acquire))
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
