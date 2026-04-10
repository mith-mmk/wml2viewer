use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use crate::dependent::{default_temp_dir, path_is_probably_network};
use crate::options::ZipWorkaroundOptions;
use encoding_rs::SHIFT_JIS;
use serde::{Deserialize, Serialize};
use zip::{CompressionMethod, ZipArchive};

use super::{
    SourceSignature, compare_natural_str, is_supported_image, source_id_for_path,
    source_signature_for_path,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ZipEntryRecord {
    pub index: usize,
    pub name: String,
    pub size: u64,
}

#[derive(Clone)]
struct ZipIndexCacheEntry {
    signature: SourceSignature,
    entries: Vec<ZipEntryRecord>,
    profile: Option<ZipArchiveProfile>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PersistentZipIndexSnapshot {
    signature: SourceSignature,
    entries: Vec<ZipEntryRecord>,
    profile: Option<ZipArchiveProfile>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct ZipArchiveProfile {
    sampled_supported_entries: usize,
    stored_entries: usize,
    compressed_bytes: u64,
    uncompressed_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZipArchiveAccessKind {
    DirectOriginal,
    Sequential,
}

#[derive(Clone, Debug)]
pub(crate) struct ZipArchivePolicyDebug {
    pub access_kind: ZipArchiveAccessKind,
    pub is_network_path: bool,
    pub exceeds_size_threshold: bool,
    pub sampled_supported_entries: usize,
    pub stored_entries: usize,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    pub prefers_direct: bool,
}

#[derive(Clone)]
struct ZipProfileCacheEntry {
    signature: SourceSignature,
    profile: ZipArchiveProfile,
}

#[derive(Clone)]
struct ZipFirstEntryCacheEntry {
    signature: SourceSignature,
    entry: Option<ZipEntryRecord>,
}

#[derive(Clone)]
struct LocalArchiveCacheEntry {
    signature: SourceSignature,
    cached_path: PathBuf,
}

#[derive(Clone)]
enum ZipArchiveAccess {
    Direct(PathBuf),
    Sequential(PathBuf),
}

pub(crate) fn load_zip_entries(path: &Path) -> Option<Vec<ZipEntryRecord>> {
    let signature = archive_signature(path)?;
    let cache = zip_index_cache();
    if let Some(entry) = cache.lock().ok()?.get(path).cloned() {
        if entry.signature == signature {
            if let Some(profile) = entry.profile {
                cache_zip_profile(path, signature.clone(), profile);
            }
            return Some(entry.entries);
        }
    }

    let (mut entries, profile) = load_zip_entries_unsorted_with_profile(path)?;
    sort_zip_entries(&mut entries);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(
            path.to_path_buf(),
            ZipIndexCacheEntry {
                signature,
                entries: entries.clone(),
                profile,
            },
        );
    }
    Some(entries)
}

pub(crate) fn probe_first_supported_zip_entry(path: &Path) -> Option<ZipEntryRecord> {
    let signature = archive_signature(path)?;
    if let Ok(cache) = zip_first_entry_cache().lock()
        && let Some(entry) = cache.get(path)
        && entry.signature == signature
    {
        return entry.entry.clone();
    }

    let access = resolve_zip_archive_access(path)?;
    let entry = try_probe_first_supported_zip_entry_from_path(access.path()).or_else(|| {
        if access.path() != path {
            try_probe_first_supported_zip_entry_from_path(path)
        } else {
            None
        }
    });
    if let Ok(mut cache) = zip_first_entry_cache().lock() {
        cache.insert(
            path.to_path_buf(),
            ZipFirstEntryCacheEntry {
                signature,
                entry: entry.clone(),
            },
        );
    }
    entry
}

pub(crate) fn probe_adjacent_supported_zip_entry(
    path: &Path,
    entry_index: usize,
    step: isize,
) -> Option<ZipEntryRecord> {
    if step == 0 {
        return None;
    }
    let access = resolve_zip_archive_access(path)?;
    try_probe_adjacent_supported_zip_entry_from_path(access.path(), entry_index, step).or_else(
        || {
            if access.path() != path {
                try_probe_adjacent_supported_zip_entry_from_path(path, entry_index, step)
            } else {
                None
            }
        },
    )
}

pub(crate) fn load_zip_entries_unsorted(path: &Path) -> Option<Vec<ZipEntryRecord>> {
    load_zip_entries_unsorted_with_profile(path).map(|(entries, _)| entries)
}

fn load_zip_entries_unsorted_with_profile(
    path: &Path,
) -> Option<(Vec<ZipEntryRecord>, Option<ZipArchiveProfile>)> {
    let signature = archive_signature(path)?;
    if let Some(snapshot) = load_persistent_zip_index(path, &signature) {
        if let Some(profile) = snapshot.profile.clone() {
            cache_zip_profile(path, signature, profile);
        }
        return Some((snapshot.entries, snapshot.profile));
    }

    let access = resolve_zip_archive_access(path)?;
    let (entries, profile) = try_load_zip_entries_from_path(access.path()).or_else(|| {
        if access.path() != path {
            try_load_zip_entries_from_path(path)
        } else {
            None
        }
    })?;
    if let Some(profile) = profile.clone() {
        cache_zip_profile(path, signature.clone(), profile);
    }
    persist_zip_index(path, &signature, &entries, profile.clone());
    Some((entries, profile))
}

pub(crate) fn sort_zip_entries(entries: &mut [ZipEntryRecord]) {
    entries.sort_by(|left, right| compare_natural_str(&left.name, &right.name, false));
}

#[allow(dead_code)]
pub(crate) fn load_zip_entry_bytes_with_size(
    path: &Path,
    entry_index: usize,
) -> Option<(Vec<u8>, Option<u64>)> {
    load_zip_entry_bytes_with_size_with_cancel(path, entry_index, &|| false)
}

pub(crate) fn load_zip_entry_bytes_with_size_with_cancel<F: Fn() -> bool>(
    path: &Path,
    entry_index: usize,
    should_cancel: &F,
) -> Option<(Vec<u8>, Option<u64>)> {
    let access = resolve_zip_archive_access(path)?;
    let archive_path = access.path();
    if let Some((bytes, size_hint)) =
        read_zip_entry_bytes_from_path(archive_path, entry_index, None, should_cancel)
    {
        return Some((bytes, size_hint));
    }

    let fallback_name = zip_entry_record(path, entry_index).map(|entry| entry.name)?;
    read_zip_entry_bytes_from_path(
        archive_path,
        entry_index,
        Some(&fallback_name),
        should_cancel,
    )
    .or_else(|| {
        if archive_path != path {
            read_zip_entry_bytes_from_path(path, entry_index, Some(&fallback_name), should_cancel)
        } else {
            None
        }
    })
}

pub(crate) fn zip_entry_record(path: &Path, entry_index: usize) -> Option<ZipEntryRecord> {
    load_zip_entries(path)?
        .into_iter()
        .find(|entry| entry.index == entry_index)
}

pub(crate) fn zip_entry_size(path: &Path, entry_index: usize) -> Option<u64> {
    zip_entry_record(path, entry_index).map(|entry| entry.size)
}

fn read_zip_entry_bytes_from_path<F: Fn() -> bool>(
    archive_path: &Path,
    entry_index: usize,
    fallback_name: Option<&str>,
    should_cancel: &F,
) -> Option<(Vec<u8>, Option<u64>)> {
    if let Ok(mut archive) = open_zip_archive(archive_path) {
        if let Some(bytes) =
            read_entry_bytes(&mut archive, entry_index, fallback_name, should_cancel)
        {
            return Some(bytes);
        }
    }
    let mut archive = open_plain_zip_archive(archive_path).ok()?;
    read_entry_bytes(&mut archive, entry_index, fallback_name, should_cancel)
}

fn read_entry_bytes<R: Read + Seek, F: Fn() -> bool>(
    archive: &mut ZipArchive<R>,
    entry_index: usize,
    fallback_name: Option<&str>,
    should_cancel: &F,
) -> Option<(Vec<u8>, Option<u64>)> {
    if should_cancel() {
        return None;
    }
    if let Ok(mut entry) = archive.by_index(entry_index) {
        let size_hint = Some(entry.size());
        return read_zip_entry_to_end(&mut entry, should_cancel).map(|bytes| (bytes, size_hint));
    }
    let mut entry = archive.by_name(fallback_name?).ok()?;
    let size_hint = Some(entry.size());
    read_zip_entry_to_end(&mut entry, should_cancel).map(|bytes| (bytes, size_hint))
}

pub(crate) fn set_zip_workaround_options(options: ZipWorkaroundOptions) {
    if let Ok(mut config) = zip_workaround_config().lock() {
        *config = options;
    }
    clear_zip_caches();
}

pub(crate) fn zip_prefers_low_io(path: &Path) -> bool {
    matches!(
        resolve_zip_archive_access(path),
        Some(ZipArchiveAccess::Sequential(_))
    )
}

pub(crate) fn zip_archive_policy_debug(path: &Path) -> Option<ZipArchivePolicyDebug> {
    let metadata = std::fs::metadata(path).ok()?;
    let options = current_zip_workaround_options();
    let threshold_bytes = options.threshold_mb.saturating_mul(1024 * 1024);
    let is_network_path = path_is_probably_network(path);
    let exceeds_size_threshold = metadata.len() >= threshold_bytes;
    let needs_workaround = is_network_path || exceeds_size_threshold;
    let profile = if needs_workaround {
        probe_zip_archive_profile(path).unwrap_or_default()
    } else {
        ZipArchiveProfile::default()
    };
    let prefers_direct = !needs_workaround || zip_profile_is_direct_friendly(&profile);
    let access_kind = if !needs_workaround || prefers_direct {
        ZipArchiveAccessKind::DirectOriginal
    } else {
        ZipArchiveAccessKind::Sequential
    };

    Some(ZipArchivePolicyDebug {
        access_kind,
        is_network_path,
        exceeds_size_threshold,
        sampled_supported_entries: profile.sampled_supported_entries,
        stored_entries: profile.stored_entries,
        compressed_bytes: profile.compressed_bytes,
        uncompressed_bytes: profile.uncompressed_bytes,
        prefers_direct,
    })
}

fn open_zip_archive(path: &Path) -> std::io::Result<ZipArchive<ZipCacheReader>> {
    let file = File::open(path)?;
    let reader = ZipCacheReader::new(file)?;
    ZipArchive::new(reader).map_err(std::io::Error::other)
}

fn open_plain_zip_archive(path: &Path) -> std::io::Result<ZipArchive<BufReader<File>>> {
    let file = File::open(path)?;
    ZipArchive::new(BufReader::new(file)).map_err(std::io::Error::other)
}

impl ZipArchiveAccess {
    fn path(&self) -> &Path {
        match self {
            Self::Direct(path) | Self::Sequential(path) => path.as_path(),
        }
    }
}

fn try_load_zip_entries_from_path(
    path: &Path,
) -> Option<(Vec<ZipEntryRecord>, Option<ZipArchiveProfile>)> {
    if let Ok(mut archive) = open_zip_archive(path) {
        return Some(collect_zip_entries_and_profile(&mut archive));
    }
    let mut archive = open_plain_zip_archive(path).ok()?;
    Some(collect_zip_entries_and_profile(&mut archive))
}

fn try_probe_first_supported_zip_entry_from_path(path: &Path) -> Option<ZipEntryRecord> {
    if let Ok(mut archive) = open_zip_archive(path) {
        return probe_first_supported_zip_entry_from_archive(&mut archive);
    }
    let mut archive = open_plain_zip_archive(path).ok()?;
    probe_first_supported_zip_entry_from_archive(&mut archive)
}

fn try_probe_adjacent_supported_zip_entry_from_path(
    path: &Path,
    entry_index: usize,
    step: isize,
) -> Option<ZipEntryRecord> {
    if let Ok(mut archive) = open_zip_archive(path) {
        return probe_adjacent_supported_zip_entry_from_archive(&mut archive, entry_index, step);
    }
    let mut archive = open_plain_zip_archive(path).ok()?;
    probe_adjacent_supported_zip_entry_from_archive(&mut archive, entry_index, step)
}

fn probe_first_supported_zip_entry_from_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> Option<ZipEntryRecord> {
    for index in 0..archive.len() {
        let Ok(file) = archive.by_index(index) else {
            continue;
        };
        if file.is_dir() {
            continue;
        }

        let name = decode_zip_name(&file);
        let normalized = name.replace('\\', "/");
        let entry_path = PathBuf::from(&normalized);
        if !is_supported_image(&entry_path) {
            continue;
        }

        return Some(ZipEntryRecord {
            index,
            name: normalized,
            size: file.size(),
        });
    }
    None
}

fn probe_adjacent_supported_zip_entry_from_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry_index: usize,
    step: isize,
) -> Option<ZipEntryRecord> {
    let mut index = entry_index.checked_add_signed(step)?;
    while index < archive.len() {
        let Ok(file) = archive.by_index(index) else {
            index = index.checked_add_signed(step)?;
            continue;
        };
        if file.is_dir() {
            index = index.checked_add_signed(step)?;
            continue;
        }

        let name = decode_zip_name(&file);
        let normalized = name.replace('\\', "/");
        let entry_path = PathBuf::from(&normalized);
        if !is_supported_image(&entry_path) {
            index = index.checked_add_signed(step)?;
            continue;
        }

        return Some(ZipEntryRecord {
            index,
            name: normalized,
            size: file.size(),
        });
    }
    None
}

fn collect_zip_entries_and_profile<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> (Vec<ZipEntryRecord>, Option<ZipArchiveProfile>) {
    let mut entries = Vec::new();
    let mut profile = ZipArchiveProfile::default();
    const SAMPLE_LIMIT: usize = 8;
    for index in 0..archive.len() {
        let Ok(file) = archive.by_index(index) else {
            continue;
        };
        if file.is_dir() {
            continue;
        }

        let name = decode_zip_name(&file);
        let normalized = name.replace('\\', "/");
        let entry_path = PathBuf::from(&normalized);
        if !is_supported_image(&entry_path) {
            continue;
        }

        if profile.sampled_supported_entries < SAMPLE_LIMIT {
            profile.sampled_supported_entries += 1;
            if file.compression() == CompressionMethod::Stored {
                profile.stored_entries += 1;
            }
            profile.compressed_bytes = profile
                .compressed_bytes
                .saturating_add(file.compressed_size());
            profile.uncompressed_bytes = profile.uncompressed_bytes.saturating_add(file.size());
        }

        entries.push(ZipEntryRecord {
            index,
            name: normalized,
            size: file.size(),
        });
    }
    let profile = (profile.sampled_supported_entries > 0).then_some(profile);
    (entries, profile)
}

fn read_zip_entry_to_end<R: Read, F: Fn() -> bool>(
    entry: &mut R,
    should_cancel: &F,
) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 256 * 1024];
    loop {
        if should_cancel() {
            return None;
        }
        let read = entry.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..read]);
    }
    Some(buf)
}

fn current_zip_workaround_options() -> ZipWorkaroundOptions {
    zip_workaround_config()
        .lock()
        .map(|config| config.clone())
        .unwrap_or_default()
}

fn resolve_zip_archive_access(path: &Path) -> Option<ZipArchiveAccess> {
    let metadata = std::fs::metadata(path).ok()?;
    let options = current_zip_workaround_options();
    let threshold_bytes = options.threshold_mb.saturating_mul(1024 * 1024);
    let needs_workaround = path_is_probably_network(path) || metadata.len() >= threshold_bytes;
    if !needs_workaround {
        return Some(ZipArchiveAccess::Direct(path.to_path_buf()));
    }
    if zip_profile_prefers_direct(path, &metadata) {
        return Some(ZipArchiveAccess::Direct(path.to_path_buf()));
    }

    Some(ZipArchiveAccess::Sequential(path.to_path_buf()))
}

pub(crate) fn ensure_local_archive_cache(path: &Path) -> Option<PathBuf> {
    let metadata = std::fs::metadata(path).ok()?;
    if let Some(cached) = cached_local_archive_path(path, &metadata) {
        return Some(cached);
    }

    let destination = archive_cache_destination(path, &metadata)?;
    let temp_root = archive_cache_root()?;
    std::fs::create_dir_all(&temp_root).ok()?;
    if !destination.exists() {
        std::fs::copy(path, &destination).ok()?;
    }
    let signature = archive_signature_from_metadata(path, &metadata);
    if let Ok(mut cache) = local_archive_cache().lock() {
        cache.insert(
            path.to_path_buf(),
            LocalArchiveCacheEntry {
                signature,
                cached_path: destination.clone(),
            },
        );
    }
    Some(destination)
}

fn cached_local_archive_path(path: &Path, metadata: &std::fs::Metadata) -> Option<PathBuf> {
    let signature = archive_signature_from_metadata(path, metadata);
    let cache = local_archive_cache();
    cache
        .lock()
        .ok()?
        .get(path)
        .cloned()
        .filter(|entry| entry.signature == signature && entry.cached_path.exists())
        .map(|entry| entry.cached_path)
}

fn archive_cache_destination(path: &Path, metadata: &std::fs::Metadata) -> Option<PathBuf> {
    let temp_root = archive_cache_root()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
        .hash(&mut hasher);
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("zip");
    Some(temp_root.join(format!("{:016x}.{ext}", hasher.finish())))
}

pub(crate) fn archive_cache_root() -> Option<PathBuf> {
    Some(default_temp_dir()?.join("archive-cache"))
}

fn zip_index_cache_path(path: &Path) -> Option<PathBuf> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    Some(
        archive_cache_root()?
            .join("zip-index")
            .join(format!("{:016x}.json", hasher.finish())),
    )
}

fn load_persistent_zip_index(
    path: &Path,
    signature: &SourceSignature,
) -> Option<PersistentZipIndexSnapshot> {
    let snapshot_path = zip_index_cache_path(path)?;
    let text = std::fs::read_to_string(snapshot_path).ok()?;
    let snapshot = serde_json::from_str::<PersistentZipIndexSnapshot>(&text).ok()?;
    (snapshot.signature == *signature).then_some(snapshot)
}

fn persist_zip_index(
    path: &Path,
    signature: &SourceSignature,
    entries: &[ZipEntryRecord],
    profile: Option<ZipArchiveProfile>,
) {
    let Some(snapshot_path) = zip_index_cache_path(path) else {
        return;
    };
    let Some(parent) = snapshot_path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let snapshot = PersistentZipIndexSnapshot {
        signature: signature.clone(),
        entries: entries.to_vec(),
        profile,
    };
    let Ok(text) = serde_json::to_string(&snapshot) else {
        return;
    };
    let _ = std::fs::write(snapshot_path, text);
}

fn clear_zip_caches() {
    if let Ok(mut cache) = zip_index_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = zip_first_entry_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = zip_profile_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = local_archive_cache().lock() {
        cache.clear();
    }
}

fn cache_zip_profile(path: &Path, signature: SourceSignature, profile: ZipArchiveProfile) {
    if let Ok(mut cache) = zip_profile_cache().lock() {
        cache.insert(
            path.to_path_buf(),
            ZipProfileCacheEntry { signature, profile },
        );
    }
}

pub(crate) fn zip_index_is_available(path: &Path) -> bool {
    let Some(signature) = archive_signature(path) else {
        return false;
    };
    if let Ok(cache) = zip_index_cache().lock()
        && cache
            .get(path)
            .is_some_and(|entry| entry.signature == signature)
    {
        return true;
    }
    load_persistent_zip_index(path, &signature).is_some()
}

fn archive_signature(path: &Path) -> Option<SourceSignature> {
    source_signature_for_path(path)
}

fn archive_signature_from_metadata(path: &Path, metadata: &std::fs::Metadata) -> SourceSignature {
    source_signature_for_path(path).unwrap_or_else(|| SourceSignature {
        source: source_id_for_path(path),
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

fn zip_index_cache() -> &'static Mutex<HashMap<PathBuf, ZipIndexCacheEntry>> {
    static ZIP_INDEX_CACHE: OnceLock<Mutex<HashMap<PathBuf, ZipIndexCacheEntry>>> = OnceLock::new();
    ZIP_INDEX_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn zip_first_entry_cache() -> &'static Mutex<HashMap<PathBuf, ZipFirstEntryCacheEntry>> {
    static ZIP_FIRST_ENTRY_CACHE: OnceLock<Mutex<HashMap<PathBuf, ZipFirstEntryCacheEntry>>> =
        OnceLock::new();
    ZIP_FIRST_ENTRY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn zip_profile_cache() -> &'static Mutex<HashMap<PathBuf, ZipProfileCacheEntry>> {
    static ZIP_PROFILE_CACHE: OnceLock<Mutex<HashMap<PathBuf, ZipProfileCacheEntry>>> =
        OnceLock::new();
    ZIP_PROFILE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn local_archive_cache() -> &'static Mutex<HashMap<PathBuf, LocalArchiveCacheEntry>> {
    static LOCAL_ARCHIVE_CACHE: OnceLock<Mutex<HashMap<PathBuf, LocalArchiveCacheEntry>>> =
        OnceLock::new();
    LOCAL_ARCHIVE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn zip_workaround_config() -> &'static Mutex<ZipWorkaroundOptions> {
    static CONFIG: OnceLock<Mutex<ZipWorkaroundOptions>> = OnceLock::new();
    CONFIG.get_or_init(|| Mutex::new(ZipWorkaroundOptions::default()))
}

fn zip_profile_prefers_direct(path: &Path, metadata: &std::fs::Metadata) -> bool {
    let signature = archive_signature_from_metadata(path, metadata);
    let cache = zip_profile_cache();
    if let Some(entry) = cache.lock().ok().and_then(|cache| cache.get(path).cloned()) {
        if entry.signature == signature {
            return zip_profile_is_direct_friendly(&entry.profile);
        }
    }

    if let Some(snapshot) = load_persistent_zip_index(path, &signature)
        && let Some(profile) = snapshot.profile
    {
        cache_zip_profile(path, signature, profile.clone());
        return zip_profile_is_direct_friendly(&profile);
    }

    let profile = probe_zip_archive_profile(path).unwrap_or_default();
    cache_zip_profile(path, signature, profile.clone());
    zip_profile_is_direct_friendly(&profile)
}

fn probe_zip_archive_profile(path: &Path) -> Option<ZipArchiveProfile> {
    let mut archive = open_plain_zip_archive(path).ok()?;
    Some(sample_zip_archive_profile(&mut archive, 8))
}

fn sample_zip_archive_profile<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    sample_limit: usize,
) -> ZipArchiveProfile {
    let mut profile = ZipArchiveProfile::default();
    for index in 0..archive.len() {
        if profile.sampled_supported_entries >= sample_limit {
            break;
        }
        let Ok(file) = archive.by_index(index) else {
            continue;
        };
        if file.is_dir() {
            continue;
        }
        let entry_path = PathBuf::from(decode_zip_name(&file).replace('\\', "/"));
        if !is_supported_image(&entry_path) {
            continue;
        }
        profile.sampled_supported_entries += 1;
        if file.compression() == CompressionMethod::Stored {
            profile.stored_entries += 1;
        }
        profile.compressed_bytes = profile
            .compressed_bytes
            .saturating_add(file.compressed_size());
        profile.uncompressed_bytes = profile.uncompressed_bytes.saturating_add(file.size());
    }
    profile
}

fn zip_profile_is_direct_friendly(profile: &ZipArchiveProfile) -> bool {
    if profile.sampled_supported_entries == 0 {
        return false;
    }
    if profile.stored_entries != profile.sampled_supported_entries {
        return false;
    }
    if profile.uncompressed_bytes == 0 {
        return true;
    }
    let ratio = profile.compressed_bytes as f64 / profile.uncompressed_bytes as f64;
    ratio >= 0.98
}

struct ZipCacheReader {
    inner: File,
    pos: u64,
    len: u64,
    chunk_size: u64,
    max_chunks: usize,
    cache: HashMap<u64, Vec<u8>>,
    order: VecDeque<u64>,
}

impl ZipCacheReader {
    fn new(inner: File) -> std::io::Result<Self> {
        let len = inner.metadata()?.len();
        Ok(Self {
            inner,
            pos: 0,
            len,
            chunk_size: 8 * 1024 * 1024,
            max_chunks: 32,
            cache: HashMap::new(),
            order: VecDeque::new(),
        })
    }

    fn read_chunk(&mut self, chunk_index: u64) -> std::io::Result<&[u8]> {
        if !self.cache.contains_key(&chunk_index) {
            let offset = chunk_index.saturating_mul(self.chunk_size);
            self.inner.seek(SeekFrom::Start(offset))?;
            let remaining = self.len.saturating_sub(offset);
            let size = remaining.min(self.chunk_size) as usize;
            let mut buffer = vec![0u8; size];
            if size > 0 {
                self.inner.read_exact(&mut buffer)?;
            }
            self.cache.insert(chunk_index, buffer);
            self.order.push_back(chunk_index);
            while self.order.len() > self.max_chunks {
                if let Some(oldest) = self.order.pop_front() {
                    self.cache.remove(&oldest);
                }
            }
        }
        self.touch_chunk(chunk_index);
        Ok(self
            .cache
            .get(&chunk_index)
            .map(Vec::as_slice)
            .unwrap_or(&[]))
    }

    fn touch_chunk(&mut self, chunk_index: u64) {
        if let Some(index) = self.order.iter().position(|entry| *entry == chunk_index) {
            self.order.remove(index);
        }
        self.order.push_back(chunk_index);
    }
}

impl Read for ZipCacheReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if buf.is_empty() || self.pos >= self.len {
            return Ok(0);
        }

        let mut total = 0usize;
        while total < buf.len() && self.pos < self.len {
            let chunk_index = self.pos / self.chunk_size;
            let chunk_offset = (self.pos % self.chunk_size) as usize;
            let chunk = self.read_chunk(chunk_index)?;
            if chunk_offset >= chunk.len() {
                break;
            }
            let available = &chunk[chunk_offset..];
            let copy_len = available.len().min(buf.len() - total);
            buf[total..total + copy_len].copy_from_slice(&available[..copy_len]);
            total += copy_len;
            self.pos = self.pos.saturating_add(copy_len as u64);
        }
        Ok(total)
    }
}

impl Seek for ZipCacheReader {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let next = match pos {
            SeekFrom::Start(offset) => offset as i128,
            SeekFrom::End(offset) => self.len as i128 + offset as i128,
            SeekFrom::Current(offset) => self.pos as i128 + offset as i128,
        };
        self.pos = next.clamp(0, self.len as i128) as u64;
        Ok(self.pos)
    }
}

fn decode_zip_name(file: &zip::read::ZipFile<'_>) -> String {
    let raw = file.name_raw();
    if let Ok(utf8) = std::str::from_utf8(raw) {
        return utf8.to_string();
    }
    let (decoded, _, had_errors) = SHIFT_JIS.decode(raw);
    if !had_errors {
        return decoded.into_owned();
    }
    String::from_utf8_lossy(raw).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        PersistentZipIndexSnapshot, ZipArchiveAccess, ZipArchiveProfile, ZipCacheReader,
        ensure_local_archive_cache, load_zip_entries, load_zip_entries_unsorted,
        probe_first_supported_zip_entry, resolve_zip_archive_access, set_zip_workaround_options,
        zip_index_cache_path, zip_index_is_available, zip_profile_is_direct_friendly,
    };
    use crate::options::ZipWorkaroundOptions;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("wml2viewer-{name}-{unique}.bin"))
    }

    #[test]
    fn zip_cache_reader_supports_seek_and_read() {
        let path = temp_path("zip-cache");
        let mut file = File::create(&path).unwrap();
        for index in 0..(1024 * 32) {
            let value = (index % 251) as u8;
            file.write_all(&[value]).unwrap();
        }
        drop(file);

        let file = File::open(&path).unwrap();
        let mut reader = ZipCacheReader::new(file).unwrap();
        let mut buf = [0u8; 128];

        reader.seek(SeekFrom::Start(4093)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(buf[0], (4093 % 251) as u8);
        assert_eq!(buf[127], ((4093 + 127) % 251) as u8);

        reader.seek(SeekFrom::Start(32)).unwrap();
        reader.read_exact(&mut buf[..8]).unwrap();
        assert_eq!(&buf[..8], &[32, 33, 34, 35, 36, 37, 38, 39]);

        let _ = std::fs::remove_file(path);
    }

    fn write_zip(path: &std::path::Path, names: &[&str]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for name in names {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"data").unwrap();
        }
        zip.finish().unwrap();
    }

    fn write_zip_with_method(path: &std::path::Path, entries: &[(&str, CompressionMethod, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, method, bytes) in entries {
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(*method),
            )
            .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn zip_caches_are_invalidated_when_archive_changes() {
        let path = temp_path("zip-signature");
        write_zip(&path, &["001.png"]);
        set_zip_workaround_options(ZipWorkaroundOptions {
            threshold_mb: 0,
            local_cache: true,
        });

        let first = load_zip_entries(&path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_zip(&path, &["001.png", "002.png"]);
        let second = load_zip_entries(&path).unwrap();

        assert_eq!(first.len(), 1);
        assert_eq!(second.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn zip_index_is_persisted_on_disk() {
        let path = temp_path("zip-persistent-index");
        write_zip(&path, &["001.png", "002.png"]);

        let entries = load_zip_entries_unsorted(&path).unwrap();
        assert_eq!(entries.len(), 2);

        let snapshot_path = zip_index_cache_path(&path).unwrap();
        assert!(snapshot_path.exists());
        let snapshot = std::fs::read_to_string(&snapshot_path).unwrap();
        let snapshot = serde_json::from_str::<PersistentZipIndexSnapshot>(&snapshot).unwrap();
        assert!(snapshot.profile.is_some());

        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(snapshot_path);
    }

    #[test]
    fn probe_first_supported_zip_entry_does_not_require_full_index() {
        let path = temp_path("zip-probe-first");
        write_zip(&path, &["001.png", "002.png"]);

        let first = probe_first_supported_zip_entry(&path).unwrap();
        assert_eq!(first.index, 0);
        assert_eq!(first.name, "001.png");
        assert!(!zip_index_is_available(&path));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn probe_first_supported_zip_entry_cache_is_invalidated_when_archive_changes() {
        let path = temp_path("zip-probe-first-invalidate");
        write_zip(&path, &["001.png", "002.png"]);

        let first = probe_first_supported_zip_entry(&path).unwrap();
        assert_eq!(first.name, "001.png");

        std::thread::sleep(std::time::Duration::from_millis(20));
        write_zip(&path, &["000.png", "002.png"]);

        let first = probe_first_supported_zip_entry(&path).unwrap();
        assert_eq!(first.name, "000.png");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn direct_friendly_profile_prefers_uncompressed_entries() {
        assert!(zip_profile_is_direct_friendly(&ZipArchiveProfile {
            sampled_supported_entries: 4,
            stored_entries: 4,
            compressed_bytes: 9_900,
            uncompressed_bytes: 10_000,
        }));
        assert!(!zip_profile_is_direct_friendly(&ZipArchiveProfile {
            sampled_supported_entries: 4,
            stored_entries: 3,
            compressed_bytes: 9_900,
            uncompressed_bytes: 10_000,
        }));
        assert!(!zip_profile_is_direct_friendly(&ZipArchiveProfile {
            sampled_supported_entries: 4,
            stored_entries: 4,
            compressed_bytes: 5_000,
            uncompressed_bytes: 10_000,
        }));
    }

    #[test]
    fn load_zip_entries_reads_stored_and_deflated_archives() {
        let path = temp_path("zip-method");
        write_zip_with_method(
            &path,
            &[
                ("001.bmp", CompressionMethod::Stored, &[1u8; 1024]),
                ("002.bmp", CompressionMethod::Deflated, &[2u8; 1024]),
            ],
        );

        let entries = load_zip_entries(&path).unwrap();
        assert_eq!(entries.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn large_stored_zip_prefers_original_path_over_temp_copy() {
        let path = temp_path("zip-stored-direct");
        write_zip_with_method(
            &path,
            &[("001.bmp", CompressionMethod::Stored, &[1u8; 4096])],
        );
        set_zip_workaround_options(ZipWorkaroundOptions {
            threshold_mb: 0,
            local_cache: true,
        });

        let access = resolve_zip_archive_access(&path).unwrap();
        assert!(matches!(access, ZipArchiveAccess::Direct(ref direct) if direct == &path));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn compressed_zip_with_local_cache_stays_sequential_by_default() {
        let path = temp_path("zip-compressed-sequential");
        write_zip_with_method(
            &path,
            &[(
                "001.bmp",
                CompressionMethod::Deflated,
                &vec![7u8; 32 * 1024],
            )],
        );
        let _ = ensure_local_archive_cache(&path);
        set_zip_workaround_options(ZipWorkaroundOptions {
            threshold_mb: 0,
            local_cache: true,
        });

        let access = resolve_zip_archive_access(&path).unwrap();
        assert!(matches!(access, ZipArchiveAccess::Sequential(ref original) if original == &path));

        let _ = std::fs::remove_file(path);
    }
}
