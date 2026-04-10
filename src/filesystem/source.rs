use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::dependent::{
    HttpFetchMetadata, HttpFetchRequest, HttpFetchResult, default_temp_dir, fetch_http_url,
    path_is_probably_network,
};

use super::path::{
    is_listed_file_path, is_zip_file_path, listed_virtual_identity_from_virtual_path,
    listed_virtual_root, resolve_start_path, resolve_virtual_listed_child, zip_virtual_root,
};
use super::zip_file::{
    load_zip_entries, load_zip_entry_bytes_with_size_with_cancel, zip_entry_size,
    zip_prefers_low_io,
};

pub(crate) const HTTP_TEMP_PREFIX: &str = "wml2viewer_url_";
const HTTP_SOURCE_CACHE_DIR: &str = "http-source-cache";
const HTTP_SOURCE_CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 24);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum SourceKind {
    LocalPath,
    HttpTempFile,
    ListedFile,
    ListedVirtualChild,
    ZipArchive,
    ZipVirtualChild,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct SourceId {
    pub kind: SourceKind,
    pub path: PathBuf,
    pub entry_index: Option<usize>,
    pub listed_identity: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SourceSignature {
    pub source: SourceId,
    pub exists: bool,
    pub is_dir: bool,
    pub len: Option<u64>,
    pub modified_nanos: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OpenedImageSource {
    File {
        path: PathBuf,
        size_hint: Option<u64>,
    },
    Bytes {
        hint_path: PathBuf,
        bytes: Vec<u8>,
        size_hint: Option<u64>,
        prefers_low_io: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct HttpSourceCacheMetadata {
    url: String,
    final_url: String,
    content_type: Option<String>,
    etag: Option<String>,
    last_modified: Option<String>,
    fetched_nanos: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HttpSourceCacheEntry {
    path: PathBuf,
    metadata: Option<HttpSourceCacheMetadata>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SourceInputResolution {
    Resolved(PathBuf),
    Cancelled,
    Failed,
}

pub(crate) fn source_id_for_path(path: &Path) -> SourceId {
    if let Some((root, index)) = zip_virtual_child_source(path) {
        return SourceId {
            kind: SourceKind::ZipVirtualChild,
            path: root,
            entry_index: Some(index),
            listed_identity: None,
        };
    }
    if let Some((root, identity)) = listed_virtual_child_source(path) {
        return SourceId {
            kind: SourceKind::ListedVirtualChild,
            path: root,
            entry_index: None,
            listed_identity: identity,
        };
    }
    if is_zip_file_path(path) {
        return SourceId {
            kind: SourceKind::ZipArchive,
            path: path.to_path_buf(),
            entry_index: None,
            listed_identity: None,
        };
    }
    if is_listed_file_path(path) {
        return SourceId {
            kind: SourceKind::ListedFile,
            path: path.to_path_buf(),
            entry_index: None,
            listed_identity: None,
        };
    }
    SourceId {
        kind: if is_http_temp_file(path) {
            SourceKind::HttpTempFile
        } else {
            SourceKind::LocalPath
        },
        path: path.to_path_buf(),
        entry_index: None,
        listed_identity: None,
    }
}

pub(crate) fn source_signature_for_path(path: &Path) -> Option<SourceSignature> {
    let source = source_id_for_path(path);
    let metadata = fs::metadata(&source.path).ok()?;
    Some(SourceSignature {
        source,
        exists: true,
        is_dir: metadata.is_dir(),
        len: metadata.is_file().then_some(metadata.len()),
        modified_nanos: metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos()),
    })
}

pub fn resolve_source_input_path(path: &Path) -> Option<PathBuf> {
    match resolve_source_input_path_with_cancel(path, None) {
        SourceInputResolution::Resolved(path) => Some(path),
        SourceInputResolution::Cancelled | SourceInputResolution::Failed => None,
    }
}

pub(crate) fn resolve_source_input_path_with_cancel(
    path: &Path,
    cancel: Option<&AtomicBool>,
) -> SourceInputResolution {
    let Some(url) = source_url_from_input(path) else {
        return SourceInputResolution::Failed;
    };
    if url.starts_with("http://") || url.starts_with("https://") {
        return resolve_http_source_input_path(&url, cancel);
    }
    SourceInputResolution::Resolved(PathBuf::from(url))
}

fn source_url_from_input(path: &Path) -> Option<String> {
    let text = path.to_string_lossy().trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn resolve_http_source_input_path(url: &str, cancel: Option<&AtomicBool>) -> SourceInputResolution {
    resolve_http_source_input_path_with_fetcher(url, cancel, fetch_http_url)
}

fn resolve_http_source_input_path_with_fetcher(
    url: &str,
    cancel: Option<&AtomicBool>,
    fetch: impl Fn(&HttpFetchRequest, Option<&AtomicBool>) -> HttpFetchResult,
) -> SourceInputResolution {
    let cache_root = http_source_cache_root();
    let cached_entry = load_http_source_cache_entry(&cache_root, url);
    if let Some(entry) = cached_entry.as_ref() {
        if http_source_cache_is_fresh(&entry.path, entry.metadata.as_ref()) {
            remember_http_source_path(url, &entry.path);
            return SourceInputResolution::Resolved(entry.path.clone());
        }
        if let Some(metadata) = entry
            .metadata
            .as_ref()
            .filter(|metadata| metadata.has_validators())
        {
            match fetch(
                &HttpFetchRequest {
                    url: url.to_string(),
                    if_none_match: metadata.etag.clone(),
                    if_modified_since: metadata.last_modified.clone(),
                },
                cancel,
            ) {
                HttpFetchResult::Downloaded { path, metadata } => {
                    let resolved =
                        persist_downloaded_http_source(&cache_root, url, &path, &metadata)
                            .unwrap_or(path);
                    remember_http_source_path(url, &resolved);
                    return SourceInputResolution::Resolved(resolved);
                }
                HttpFetchResult::NotModified { metadata } => {
                    let merged = merge_http_source_cache_metadata(
                        url,
                        entry.metadata.clone(),
                        Some(metadata),
                    );
                    if let Some(metadata) = merged.as_ref() {
                        let _ = persist_http_source_metadata(&cache_root, url, metadata);
                    }
                    remember_http_source_path(url, &entry.path);
                    return SourceInputResolution::Resolved(entry.path.clone());
                }
                HttpFetchResult::Cancelled => return SourceInputResolution::Cancelled,
                HttpFetchResult::Failed => {
                    if entry.path.exists() {
                        remember_http_source_path(url, &entry.path);
                        return SourceInputResolution::Resolved(entry.path.clone());
                    }
                }
            }
        }
    }

    match fetch(
        &HttpFetchRequest {
            url: url.to_string(),
            ..HttpFetchRequest::default()
        },
        cancel,
    ) {
        HttpFetchResult::Downloaded { path, metadata } => {
            let resolved =
                persist_downloaded_http_source(&cache_root, url, &path, &metadata).unwrap_or(path);
            remember_http_source_path(url, &resolved);
            SourceInputResolution::Resolved(resolved)
        }
        HttpFetchResult::NotModified { .. } => cached_entry
            .map(|entry| {
                remember_http_source_path(url, &entry.path);
                SourceInputResolution::Resolved(entry.path)
            })
            .unwrap_or(SourceInputResolution::Failed),
        HttpFetchResult::Cancelled => SourceInputResolution::Cancelled,
        HttpFetchResult::Failed => cached_entry
            .map(|entry| {
                remember_http_source_path(url, &entry.path);
                SourceInputResolution::Resolved(entry.path)
            })
            .unwrap_or(SourceInputResolution::Failed),
    }
}

fn http_source_input_cache() -> &'static Mutex<HashMap<String, PathBuf>> {
    static CACHE: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn http_source_cache_root() -> PathBuf {
    default_temp_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(HTTP_SOURCE_CACHE_DIR)
}

fn load_http_source_cache_entry(cache_root: &Path, url: &str) -> Option<HttpSourceCacheEntry> {
    if let Some(path) = http_source_input_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(url).cloned())
        .filter(|path| path.exists())
    {
        return Some(HttpSourceCacheEntry {
            metadata: load_http_source_metadata(cache_root, url),
            path,
        });
    }

    let entry = find_persistent_http_source_cache(cache_root, url)?;
    remember_http_source_path(url, &entry.path);
    Some(entry)
}

fn find_persistent_http_source_cache(cache_root: &Path, url: &str) -> Option<HttpSourceCacheEntry> {
    let key = http_source_cache_key(url);
    let entries = fs::read_dir(cache_root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with(&format!("{key}.")) && path.is_file() && !name.ends_with(".json") {
            return Some(HttpSourceCacheEntry {
                metadata: load_http_source_metadata(cache_root, url),
                path,
            });
        }
    }
    None
}

fn persist_downloaded_http_source(
    cache_root: &Path,
    url: &str,
    downloaded: &Path,
    metadata: &HttpFetchMetadata,
) -> Option<PathBuf> {
    fs::create_dir_all(cache_root).ok()?;
    let key = http_source_cache_key(url);
    let ext = downloaded
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .unwrap_or("bin");
    let destination = cache_root.join(format!("{key}.{ext}"));
    remove_old_http_source_cache_files(cache_root, url, Some(&destination));
    if downloaded != destination {
        let _ = fs::remove_file(&destination);
        if fs::rename(downloaded, &destination).is_err() {
            fs::copy(downloaded, &destination).ok()?;
            let _ = fs::remove_file(downloaded);
        }
    }
    let metadata = http_source_cache_metadata(url, metadata);
    persist_http_source_metadata(cache_root, url, &metadata)?;
    Some(destination)
}

fn http_source_cache_key(url: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn http_source_cache_is_fresh(path: &Path, metadata: Option<&HttpSourceCacheMetadata>) -> bool {
    if let Some(metadata) = metadata {
        let fetched = UNIX_EPOCH
            .checked_add(Duration::from_nanos(
                metadata.fetched_nanos.min(u64::MAX as u128) as u64,
            ))
            .unwrap_or(UNIX_EPOCH);
        let Ok(elapsed) = SystemTime::now().duration_since(fetched) else {
            return true;
        };
        return elapsed <= HTTP_SOURCE_CACHE_TTL;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    let Ok(elapsed) = SystemTime::now().duration_since(modified) else {
        return true;
    };
    elapsed <= HTTP_SOURCE_CACHE_TTL
}

fn http_source_cache_metadata(url: &str, metadata: &HttpFetchMetadata) -> HttpSourceCacheMetadata {
    HttpSourceCacheMetadata {
        url: url.to_string(),
        final_url: metadata.final_url.clone(),
        content_type: metadata.content_type.clone(),
        etag: metadata.etag.clone(),
        last_modified: metadata.last_modified.clone(),
        fetched_nanos: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    }
}

fn merge_http_source_cache_metadata(
    url: &str,
    previous: Option<HttpSourceCacheMetadata>,
    latest: Option<HttpFetchMetadata>,
) -> Option<HttpSourceCacheMetadata> {
    match (previous, latest) {
        (Some(previous), Some(latest)) => {
            let mut merged = http_source_cache_metadata(url, &latest);
            if merged.etag.is_none() {
                merged.etag = previous.etag;
            }
            if merged.last_modified.is_none() {
                merged.last_modified = previous.last_modified;
            }
            Some(merged)
        }
        (None, Some(latest)) => Some(http_source_cache_metadata(url, &latest)),
        (Some(mut previous), None) => {
            previous.fetched_nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            Some(previous)
        }
        (None, None) => None,
    }
}

fn persist_http_source_metadata(
    cache_root: &Path,
    url: &str,
    metadata: &HttpSourceCacheMetadata,
) -> Option<()> {
    fs::create_dir_all(cache_root).ok()?;
    let path = http_source_metadata_path(cache_root, url);
    let text = serde_json::to_string(metadata).ok()?;
    fs::write(path, text).ok()?;
    Some(())
}

fn load_http_source_metadata(cache_root: &Path, url: &str) -> Option<HttpSourceCacheMetadata> {
    let path = http_source_metadata_path(cache_root, url);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn http_source_metadata_path(cache_root: &Path, url: &str) -> PathBuf {
    cache_root.join(format!("{}.json", http_source_cache_key(url)))
}

fn remove_old_http_source_cache_files(cache_root: &Path, url: &str, keep: Option<&Path>) {
    let key = http_source_cache_key(url);
    let Ok(entries) = fs::read_dir(cache_root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.starts_with(&format!("{key}.")) {
            continue;
        }
        if keep == Some(path.as_path()) || name.ends_with(".json") {
            continue;
        }
        let _ = fs::remove_file(path);
    }
}

fn remember_http_source_path(url: &str, path: &Path) {
    if let Ok(mut cache) = http_source_input_cache().lock() {
        cache.insert(url.to_string(), path.to_path_buf());
    }
}

impl HttpSourceCacheMetadata {
    fn has_validators(&self) -> bool {
        self.etag.is_some() || self.last_modified.is_some()
    }
}

pub(crate) fn open_image_source(path: &Path) -> Option<OpenedImageSource> {
    open_image_source_with_cancel(path, &|| false)
}

pub(crate) fn open_image_source_with_cancel<F: Fn() -> bool>(
    path: &Path,
    should_cancel: &F,
) -> Option<OpenedImageSource> {
    let resolved = normalize_open_path(path)?;
    if let Some((archive, index)) = zip_virtual_child_source(&resolved) {
        let (bytes, size_hint) =
            load_zip_entry_bytes_with_size_with_cancel(&archive, index, should_cancel)?;
        return Some(OpenedImageSource::Bytes {
            hint_path: resolved,
            bytes,
            size_hint,
            prefers_low_io: zip_prefers_low_io(&archive),
        });
    }

    let metadata = fs::metadata(&resolved).ok()?;
    metadata.is_file().then_some(OpenedImageSource::File {
        path: resolved,
        size_hint: Some(metadata.len()),
    })
}

pub(crate) fn source_image_size(path: &Path) -> Option<u64> {
    let resolved = normalize_open_path(path)?;
    if let Some((archive, index)) = zip_virtual_child_source(&resolved) {
        return zip_entry_size(&archive, index);
    }
    let metadata = fs::metadata(&resolved).ok()?;
    metadata.is_file().then_some(metadata.len())
}

pub(crate) fn source_entry_name(path: &Path) -> Option<String> {
    if let Some((archive, index)) = zip_virtual_child_source(path) {
        return load_zip_entries(&archive)
            .and_then(|entries| entries.into_iter().find(|entry| entry.index == index))
            .map(|entry| entry.name);
    }
    if let Some(target) = resolve_virtual_listed_child(path) {
        return source_entry_name(&target);
    }
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

pub(crate) fn source_metadata_path(path: &Path) -> Option<PathBuf> {
    if let Some((archive, _)) = zip_virtual_child_source(path) {
        return Some(archive);
    }
    if let Some(target) = resolve_virtual_listed_child(path) {
        return source_metadata_path(&target);
    }
    Some(path.to_path_buf())
}

pub(crate) fn source_prefers_low_io(path: &Path) -> bool {
    let Some(resolved) = normalize_open_path(path) else {
        return false;
    };
    if let Some((archive, _)) = zip_virtual_child_source(&resolved) {
        return zip_prefers_low_io(&archive);
    }
    if is_zip_file_path(&resolved) {
        return zip_prefers_low_io(&resolved);
    }
    path_is_probably_network(&resolved)
}

fn normalize_open_path(path: &Path) -> Option<PathBuf> {
    if zip_virtual_child_source(path).is_some() {
        return Some(path.to_path_buf());
    }
    if let Some(target) = resolve_virtual_listed_child(path) {
        return normalize_open_path(&target);
    }
    if is_zip_file_path(path) || is_listed_file_path(path) || path.is_dir() {
        let next = resolve_start_path(path)?;
        if next == path {
            return Some(next);
        }
        return normalize_open_path(&next);
    }
    Some(path.to_path_buf())
}

fn zip_virtual_child_source(path: &Path) -> Option<(PathBuf, usize)> {
    let root = zip_virtual_root(path)?;
    let name = path.file_name()?.to_string_lossy();
    let index = name
        .split_once("__")
        .map(|(index, _)| index)
        .unwrap_or(name.as_ref())
        .parse::<usize>()
        .ok()?;
    Some((root, index))
}

fn listed_virtual_child_source(path: &Path) -> Option<(PathBuf, Option<u64>)> {
    let root = listed_virtual_root(path)?;
    let identity = listed_virtual_identity_from_virtual_path(path);
    Some((root, identity))
}

fn is_http_temp_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with(HTTP_TEMP_PREFIX))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicBool;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn source_id_classifies_zip_virtual_children() {
        let path = PathBuf::from("archive.zip")
            .join("__zipv__")
            .join("00000003__page.png");

        let source = source_id_for_path(&path);

        assert_eq!(source.kind, SourceKind::ZipVirtualChild);
        assert_eq!(source.path, PathBuf::from("archive.zip"));
        assert_eq!(source.entry_index, Some(3));
    }

    #[test]
    fn source_id_classifies_http_temp_files() {
        let path = PathBuf::from("C:/temp/wml2viewer_url_12345.png");

        let source = source_id_for_path(&path);

        assert_eq!(source.kind, SourceKind::HttpTempFile);
    }

    #[test]
    fn resolve_source_input_path_keeps_local_paths() {
        let path = PathBuf::from("C:/images/sample.png");

        assert_eq!(resolve_source_input_path(&path), Some(path));
    }

    #[test]
    fn source_url_from_input_detects_http_urls() {
        let path = PathBuf::from("https://example.com/image.webp");

        assert_eq!(
            source_url_from_input(&path),
            Some("https://example.com/image.webp".to_string())
        );
    }

    #[test]
    fn source_prefers_low_io_for_unc_regular_files() {
        let path = PathBuf::from(r"\\server\share\images\sample.bmp");
        assert!(source_prefers_low_io(&path));
    }

    #[test]
    fn http_source_input_path_reuses_cached_download() {
        let dir = make_temp_dir();
        let cached = dir.join("cached.webp");
        fs::write(&cached, b"png").unwrap();
        let url = format!(
            "https://example.com/image-{}.webp",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        if let Ok(mut cache) = http_source_input_cache().lock() {
            cache.remove(&url);
        }

        let first_calls = calls.clone();
        let first = resolve_http_source_input_path_with_fetcher(&url, None, |_, _| {
            first_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            HttpFetchResult::Downloaded {
                path: cached.clone(),
                metadata: test_http_fetch_metadata(&url),
            }
        });
        let second = resolve_http_source_input_path_with_fetcher(&url, None, |_, _| {
            panic!("fresh cache should not be fetched again");
        });

        assert!(matches!(
            first,
            SourceInputResolution::Resolved(ref path) if path.exists()
        ));
        assert_eq!(first, second);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn http_source_input_path_reuses_persistent_cache_after_session_entry_is_cleared() {
        let dir = make_temp_dir();
        let downloaded = dir.join("downloaded.webp");
        fs::write(&downloaded, b"png").unwrap();
        let url = "https://example.com/persistent.webp";

        let first = {
            let root = dir.join("cache");
            let path = persist_downloaded_http_source(
                &root,
                url,
                &downloaded,
                &test_http_fetch_metadata(url),
            )
            .unwrap();
            if let Ok(mut cache) = http_source_input_cache().lock() {
                cache.insert(url.to_string(), path.clone());
                cache.remove(url);
            }
            find_persistent_http_source_cache(&root, url)
        };

        assert!(first.as_ref().is_some_and(|entry| entry.path.exists()));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_persistent_http_cache_is_revalidated_with_not_modified() {
        let cache_root = http_source_cache_root();
        fs::create_dir_all(&cache_root).unwrap();
        let url = format!(
            "https://example.com/stale-{}.webp",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        clear_http_source_cache_for(&cache_root, &url);
        let cached = cache_root.join(format!("{}.webp", http_source_cache_key(&url)));
        fs::write(&cached, b"png").unwrap();
        let previous = HttpSourceCacheMetadata {
            url: url.clone(),
            final_url: url.clone(),
            content_type: Some("image/webp".to_string()),
            etag: Some("\"abc\"".to_string()),
            last_modified: None,
            fetched_nanos: stale_fetched_nanos(),
        };
        persist_http_source_metadata(&cache_root, &url, &previous).unwrap();
        if let Ok(mut cache) = http_source_input_cache().lock() {
            cache.remove(&url);
        }

        let result = resolve_http_source_input_path_with_fetcher(&url, None, |request, _| {
            assert_eq!(request.if_none_match.as_deref(), Some("\"abc\""));
            HttpFetchResult::NotModified {
                metadata: test_http_fetch_metadata(&url),
            }
        });

        assert_eq!(result, SourceInputResolution::Resolved(cached.clone()));
        let refreshed = load_http_source_metadata(&cache_root, &url).unwrap();
        assert!(refreshed.fetched_nanos >= previous.fetched_nanos);

        clear_http_source_cache_for(&cache_root, &url);
    }

    #[test]
    fn stale_persistent_http_cache_is_replaced_on_download() {
        let dir = make_temp_dir();
        let cache_root = http_source_cache_root();
        fs::create_dir_all(&cache_root).unwrap();
        let url = format!(
            "https://example.com/replace-{}.webp",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        clear_http_source_cache_for(&cache_root, &url);
        let cached = cache_root.join(format!("{}.webp", http_source_cache_key(&url)));
        fs::write(&cached, b"old").unwrap();
        let previous = HttpSourceCacheMetadata {
            url: url.clone(),
            final_url: url.clone(),
            content_type: Some("image/webp".to_string()),
            etag: Some("\"old\"".to_string()),
            last_modified: None,
            fetched_nanos: stale_fetched_nanos(),
        };
        persist_http_source_metadata(&cache_root, &url, &previous).unwrap();
        if let Ok(mut cache) = http_source_input_cache().lock() {
            cache.remove(&url);
        }
        let downloaded = dir.join("downloaded.webp");
        fs::write(&downloaded, b"new").unwrap();

        let result = resolve_http_source_input_path_with_fetcher(&url, None, |request, _| {
            assert_eq!(request.if_none_match.as_deref(), Some("\"old\""));
            HttpFetchResult::Downloaded {
                path: downloaded.clone(),
                metadata: test_http_fetch_metadata(&url),
            }
        });

        assert_eq!(result, SourceInputResolution::Resolved(cached.clone()));
        assert_eq!(fs::read(&cached).unwrap(), b"new");

        let _ = fs::remove_dir_all(dir);
        clear_http_source_cache_for(&cache_root, &url);
    }

    #[test]
    fn cancelled_http_source_input_returns_cancelled() {
        let cancel = AtomicBool::new(true);
        let result = resolve_http_source_input_path_with_fetcher(
            "https://example.com/cancel.webp",
            Some(&cancel),
            |_, cancel| {
                assert!(
                    cancel.is_some_and(|cancel| cancel.load(std::sync::atomic::Ordering::Acquire))
                );
                HttpFetchResult::Cancelled
            },
        );

        assert_eq!(result, SourceInputResolution::Cancelled);
    }

    fn make_temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("wml2viewer-source-{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn test_http_fetch_metadata(url: &str) -> HttpFetchMetadata {
        HttpFetchMetadata {
            final_url: url.to_string(),
            content_type: Some("image/webp".to_string()),
            etag: Some("\"etag\"".to_string()),
            last_modified: Some("Wed, 21 Oct 2015 07:28:00 GMT".to_string()),
        }
    }

    fn stale_fetched_nanos() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            - (HTTP_SOURCE_CACHE_TTL + Duration::from_secs(60)).as_nanos()
    }

    fn clear_http_source_cache_for(cache_root: &Path, url: &str) {
        remove_old_http_source_cache_files(cache_root, url, None);
        let _ = fs::remove_file(http_source_metadata_path(cache_root, url));
        if let Ok(mut cache) = http_source_input_cache().lock() {
            cache.remove(url);
        }
    }

    #[test]
    fn open_image_source_resolves_listed_virtual_child_to_real_file() {
        let dir = make_temp_dir();
        let listed = dir.join("pages.wmltxt");
        let page = dir.join("001.png");
        fs::write(&page, b"png").unwrap();
        fs::write(
            &listed,
            format!("#!WMLViewer2 ListedFile 1.0\n{}\n", page.display()),
        )
        .unwrap();

        let virtual_child = listed.join("__wmlv__").join(format!(
            "{:08}__{:016x}__{}",
            0usize,
            1u64,
            page.file_name().unwrap().to_string_lossy()
        ));

        let source = open_image_source(&virtual_child).unwrap();

        assert_eq!(
            source,
            OpenedImageSource::File {
                path: page.clone(),
                size_hint: Some(3),
            }
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn source_metadata_path_points_zip_children_to_archive() {
        let path = PathBuf::from("archive.zip")
            .join("__zipv__")
            .join("00000003__page.png");

        assert_eq!(
            source_metadata_path(&path),
            Some(PathBuf::from("archive.zip"))
        );
    }
}
