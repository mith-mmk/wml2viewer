use std::collections::hash_map::DefaultHasher;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_MAX_MATERIALIZATION_BYTES: u64 = 512 * 1024 * 1024;
pub const DEFAULT_CACHE_CAPACITY_BYTES: u64 = 512 * 1024 * 1024;

static PARTIAL_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SourceId {
    Local(PathBuf),
    Remote { provider: String, object: String },
}

impl SourceId {
    pub fn smb(
        share: impl Into<String>,
        object: impl Into<String>,
        version: Option<String>,
    ) -> Self {
        let share = share.into();
        let version = version.unwrap_or_default();
        // Length-prefix the identity components so delimiters inside a UNC path or
        // server-provided version cannot make two identities collide.
        Self::Remote {
            provider: format!("smb:{}:{share}:{}:{version}", share.len(), version.len()),
            object: object.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub object: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteMetadata {
    pub size: Option<u64>,
    pub version: Option<String>,
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn check(&self) -> Result<(), ProviderError> {
        if self.is_cancelled() {
            Err(ProviderError::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
pub enum ProviderError {
    Cancelled,
    InvalidSource(String),
    Authentication(String),
    Connection(String),
    TooLarge { size: u64, limit: u64 },
    Io(std::io::Error),
    Unsupported(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("operation cancelled"),
            Self::InvalidSource(message)
            | Self::Authentication(message)
            | Self::Connection(message)
            | Self::Unsupported(message) => formatter.write_str(message),
            Self::TooLarge { size, limit } => {
                write!(
                    formatter,
                    "remote object is too large ({size} bytes; limit {limit} bytes)"
                )
            }
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ProviderError {}

impl From<std::io::Error> for ProviderError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub trait ReadOnlyProvider: Send + Sync {
    /// Re-establish the provider session when the transport was interrupted.
    /// Providers that do not keep a session can use the default no-op.
    fn reconnect(&self) -> Result<(), ProviderError> {
        Ok(())
    }

    fn list(&self, object: &str, cancel: &CancelToken) -> Result<Vec<RemoteEntry>, ProviderError>;

    fn stat(&self, object: &str, cancel: &CancelToken) -> Result<RemoteMetadata, ProviderError>;

    fn materialize(
        &self,
        object: &str,
        version: Option<&str>,
        cache: &CacheStore,
        cancel: &CancelToken,
    ) -> Result<PathBuf, ProviderError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheEntry {
    pub path: PathBuf,
    pub size: u64,
    pub last_accessed: SystemTime,
}

#[derive(Clone, Debug)]
pub struct CacheStore {
    root: PathBuf,
    max_materialization_bytes: u64,
    capacity_bytes: u64,
}

pub enum CacheMaterialization {
    Hit(PathBuf),
    Write(CacheWriter),
}

pub struct CacheWriter {
    store: CacheStore,
    destination: PathBuf,
    partial: PathBuf,
    file: Option<File>,
    expected_size: Option<u64>,
    written: u64,
    committed: bool,
}

impl CacheStore {
    pub fn new(root: PathBuf) -> Result<Self, ProviderError> {
        Self::with_limits(
            root,
            DEFAULT_MAX_MATERIALIZATION_BYTES,
            DEFAULT_CACHE_CAPACITY_BYTES,
        )
    }

    pub fn with_limits(
        root: PathBuf,
        max_materialization_bytes: u64,
        capacity_bytes: u64,
    ) -> Result<Self, ProviderError> {
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            max_materialization_bytes,
            capacity_bytes,
        })
    }

    pub fn max_materialization_bytes(&self) -> u64 {
        self.max_materialization_bytes
    }

    pub fn capacity_bytes(&self) -> u64 {
        self.capacity_bytes
    }

    pub fn lookup(
        &self,
        key: &str,
        extension: Option<&str>,
        expected_size: Option<u64>,
    ) -> Result<Option<PathBuf>, ProviderError> {
        let destination = self.destination(key, extension);
        let metadata = match fs::metadata(&destination) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if expected_size.is_some_and(|size| size != metadata.len()) {
            self.remove_entry(&destination)?;
            return Ok(None);
        }
        self.write_metadata(&destination, metadata.len(), SystemTime::now())?;
        Ok(Some(destination))
    }

    pub fn begin_materialization(
        &self,
        key: &str,
        extension: Option<&str>,
        expected_size: Option<u64>,
        cancel: &CancelToken,
    ) -> Result<CacheMaterialization, ProviderError> {
        cancel.check()?;
        if let Some(size) = expected_size {
            self.check_size(size)?;
        }
        if let Some(path) = self.lookup(key, extension, expected_size)? {
            return Ok(CacheMaterialization::Hit(path));
        }

        let destination = self.destination(key, extension);
        let sequence = PARTIAL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut partial_name = destination
            .file_name()
            .map(OsString::from)
            .unwrap_or_else(|| OsString::from("source"));
        partial_name.push(format!(".partial-{}-{sequence}", std::process::id()));
        let partial = destination.with_file_name(partial_name);
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)?;
        Ok(CacheMaterialization::Write(CacheWriter {
            store: self.clone(),
            destination,
            partial,
            file: Some(file),
            expected_size,
            written: 0,
            committed: false,
        }))
    }

    pub fn materialize_bytes(
        &self,
        key: &str,
        extension: Option<&str>,
        bytes: &[u8],
        cancel: &CancelToken,
    ) -> Result<PathBuf, ProviderError> {
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        match self.begin_materialization(key, extension, Some(size), cancel)? {
            CacheMaterialization::Hit(path) => Ok(path),
            CacheMaterialization::Write(mut writer) => {
                writer.write_chunk(bytes, cancel)?;
                writer.commit(cancel)
            }
        }
    }

    pub fn entries(&self) -> Result<Vec<CacheEntry>, ProviderError> {
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.root)? {
            let path = item?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("meta") {
                continue;
            }
            let Some(data_path) = data_path_for_metadata(&path) else {
                continue;
            };
            let Ok(data_metadata) = fs::metadata(&data_path) else {
                let _ = fs::remove_file(path);
                continue;
            };
            if !data_metadata.is_file() {
                continue;
            }
            let cache_metadata = read_cache_metadata(&path).unwrap_or(CacheMetadata {
                size: data_metadata.len(),
                last_accessed: data_metadata.modified().unwrap_or(UNIX_EPOCH),
            });
            if cache_metadata.size != data_metadata.len() {
                self.remove_entry(&data_path)?;
                continue;
            }
            entries.push(CacheEntry {
                path: data_path,
                size: data_metadata.len(),
                last_accessed: cache_metadata.last_accessed,
            });
        }
        Ok(entries)
    }

    pub fn total_size(&self) -> Result<u64, ProviderError> {
        Ok(self
            .entries()?
            .into_iter()
            .fold(0_u64, |total, entry| total.saturating_add(entry.size)))
    }

    pub fn prune_lru(&self, protected: &[PathBuf]) -> Result<Vec<PathBuf>, ProviderError> {
        let mut entries = self.entries()?;
        let mut total = entries
            .iter()
            .fold(0_u64, |sum, entry| sum.saturating_add(entry.size));
        entries.sort_by_key(|entry| entry.last_accessed);
        let mut removed = Vec::new();
        for entry in entries {
            if total <= self.capacity_bytes {
                break;
            }
            if protected.iter().any(|path| path == &entry.path) {
                continue;
            }
            self.remove_entry(&entry.path)?;
            total = total.saturating_sub(entry.size);
            removed.push(entry.path);
        }
        Ok(removed)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn check_size(&self, size: u64) -> Result<(), ProviderError> {
        if size > self.max_materialization_bytes {
            Err(ProviderError::TooLarge {
                size,
                limit: self.max_materialization_bytes,
            })
        } else {
            Ok(())
        }
    }

    fn destination(&self, key: &str, extension: Option<&str>) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let digest = hasher.finish();
        let extension = extension
            .map(|value| value.trim_start_matches('.'))
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 16
                    && value
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric())
            });
        let name = match extension {
            Some(extension) => format!("source_{digest:016x}.{extension}"),
            None => format!("source_{digest:016x}.bin"),
        };
        self.root.join(name)
    }

    fn write_metadata(
        &self,
        destination: &Path,
        size: u64,
        last_accessed: SystemTime,
    ) -> Result<(), ProviderError> {
        let metadata_path = metadata_path(destination);
        let sequence = PARTIAL_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let partial = metadata_path.with_extension(format!("meta.partial-{sequence}"));
        let accessed = last_accessed
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis();
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&partial)?;
        write!(file, "version=1\nsize={size}\naccessed_ms={accessed}\n")?;
        file.flush()?;
        if let Err(error) = fs::rename(&partial, &metadata_path) {
            let _ = fs::remove_file(&partial);
            return Err(error.into());
        }
        Ok(())
    }

    fn remove_entry(&self, destination: &Path) -> Result<(), ProviderError> {
        remove_if_present(destination)?;
        remove_if_present(&metadata_path(destination))?;
        Ok(())
    }
}

impl CacheWriter {
    pub fn write_chunk(&mut self, bytes: &[u8], cancel: &CancelToken) -> Result<(), ProviderError> {
        cancel.check()?;
        let chunk_size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let next_size = self.written.saturating_add(chunk_size);
        self.store.check_size(next_size)?;
        self.file
            .as_mut()
            .ok_or_else(|| ProviderError::InvalidSource("cache writer is closed".to_string()))?
            .write_all(bytes)?;
        self.written = next_size;
        cancel.check()
    }

    pub fn bytes_written(&self) -> u64 {
        self.written
    }

    pub fn commit(mut self, cancel: &CancelToken) -> Result<PathBuf, ProviderError> {
        cancel.check()?;
        if self.expected_size.is_some_and(|size| size != self.written) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!(
                    "materialized {} bytes, expected {}",
                    self.written,
                    self.expected_size.unwrap_or_default()
                ),
            )
            .into());
        }
        if let Some(mut file) = self.file.take() {
            file.flush()?;
            file.sync_all()?;
        }
        cancel.check()?;
        if self.destination.is_file() {
            remove_if_present(&self.partial)?;
        } else {
            fs::rename(&self.partial, &self.destination)?;
        }
        self.store
            .write_metadata(&self.destination, self.written, SystemTime::now())?;
        self.committed = true;
        let destination = self.destination.clone();
        let _ = self.store.prune_lru(std::slice::from_ref(&destination))?;
        Ok(destination)
    }
}

impl Drop for CacheWriter {
    fn drop(&mut self) {
        if !self.committed {
            self.file.take();
            let _ = fs::remove_file(&self.partial);
        }
    }
}

#[derive(Clone, Copy)]
struct CacheMetadata {
    size: u64,
    last_accessed: SystemTime,
}

fn metadata_path(destination: &Path) -> PathBuf {
    let mut name = destination
        .file_name()
        .map(OsString::from)
        .unwrap_or_else(|| OsString::from("source"));
    name.push(".meta");
    destination.with_file_name(name)
}

fn data_path_for_metadata(metadata: &Path) -> Option<PathBuf> {
    let name = metadata.file_name()?.to_str()?.strip_suffix(".meta")?;
    Some(metadata.with_file_name(name))
}

fn read_cache_metadata(path: &Path) -> Result<CacheMetadata, ProviderError> {
    let mut contents = String::new();
    File::open(path)?.read_to_string(&mut contents)?;
    let mut size = None;
    let mut accessed_ms = None;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("size=") {
            size = value.parse::<u64>().ok();
        } else if let Some(value) = line.strip_prefix("accessed_ms=") {
            accessed_ms = value.parse::<u64>().ok();
        }
    }
    let size = size
        .ok_or_else(|| ProviderError::InvalidSource("invalid cache metadata size".to_string()))?;
    let accessed_ms = accessed_ms.ok_or_else(|| {
        ProviderError::InvalidSource("invalid cache metadata access time".to_string())
    })?;
    Ok(CacheMetadata {
        size,
        last_accessed: UNIX_EPOCH + Duration::from_millis(accessed_ms),
    })
}

fn remove_if_present(path: &Path) -> Result<(), ProviderError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{CacheMaterialization, CacheStore, CancelToken, ProviderError, SourceId};
    use std::fs;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn smb_source_identity_includes_share_provider_and_version() {
        let first = SourceId::smb("//server/one", "folder/page.jpg", Some("v1".to_string()));
        let other_share = SourceId::smb("//server/two", "folder/page.jpg", Some("v1".to_string()));
        let other_version =
            SourceId::smb("//server/one", "folder/page.jpg", Some("v2".to_string()));
        assert_ne!(first, other_share);
        assert_ne!(first, other_version);
    }

    #[test]
    fn cache_materialization_is_atomic_and_reusable() {
        let root = crate::test_support::make_test_dir("provider_cache");
        let cache = CacheStore::new(root.clone()).expect("cache root");
        let cancel = CancelToken::default();
        let first = cache
            .materialize_bytes("remote/object:v1", Some("png"), b"image", &cancel)
            .expect("materialize");
        let second = cache
            .materialize_bytes("remote/object:v1", Some("png"), b"other", &cancel)
            .expect("reuse");
        assert_eq!(first, second);
        assert_eq!(fs::read(first).expect("cached bytes"), b"image");
        assert_eq!(cache.entries().expect("entries").len(), 1);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn cancelled_stream_removes_partial_file() {
        let root = crate::test_support::make_test_dir("provider_stream_cancel");
        let cache = CacheStore::new(root.clone()).expect("cache root");
        let cancel = CancelToken::default();
        let CacheMaterialization::Write(mut writer) = cache
            .begin_materialization("remote/object:v1", Some("jpg"), Some(10), &cancel)
            .expect("writer")
        else {
            panic!("unexpected cache hit");
        };
        writer.write_chunk(b"12345", &cancel).expect("first chunk");
        cancel.cancel();
        assert!(matches!(
            writer.write_chunk(b"67890", &cancel),
            Err(ProviderError::Cancelled)
        ));
        drop(writer);
        assert_eq!(fs::read_dir(&root).expect("cache listing").count(), 0);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn materialization_limit_applies_to_expected_and_streamed_sizes() {
        let root = crate::test_support::make_test_dir("provider_size_limit");
        let cache = CacheStore::with_limits(root.clone(), 5, 100).expect("cache root");
        let cancel = CancelToken::default();
        assert!(matches!(
            cache.begin_materialization("large", None, Some(6), &cancel),
            Err(ProviderError::TooLarge { size: 6, limit: 5 })
        ));
        let CacheMaterialization::Write(mut writer) = cache
            .begin_materialization("unknown", None, None, &cancel)
            .expect("writer")
        else {
            panic!("unexpected cache hit");
        };
        assert!(matches!(
            writer.write_chunk(b"123456", &cancel),
            Err(ProviderError::TooLarge { size: 6, limit: 5 })
        ));
        drop(writer);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn lru_pruning_keeps_recent_and_protected_entries() {
        let root = crate::test_support::make_test_dir("provider_lru");
        let writer_cache = CacheStore::with_limits(root.clone(), 100, 12).expect("cache root");
        let cancel = CancelToken::default();
        let oldest = writer_cache
            .materialize_bytes("oldest", None, b"1111", &cancel)
            .expect("oldest");
        thread::sleep(Duration::from_millis(2));
        let protected = writer_cache
            .materialize_bytes("protected", None, b"2222", &cancel)
            .expect("protected");
        thread::sleep(Duration::from_millis(2));
        let newest = writer_cache
            .materialize_bytes("newest", None, b"3333", &cancel)
            .expect("newest");
        let cache = CacheStore::with_limits(root.clone(), 100, 8).expect("pruning cache");
        let removed = cache
            .prune_lru(std::slice::from_ref(&protected))
            .expect("prune");
        assert!(removed.contains(&oldest));
        assert!(!oldest.exists());
        assert!(protected.exists());
        assert!(newest.exists());
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn size_mismatch_invalidates_cached_data() {
        let root = crate::test_support::make_test_dir("provider_size_mismatch");
        let cache = CacheStore::new(root.clone()).expect("cache root");
        let cancel = CancelToken::default();
        cache
            .materialize_bytes("object", None, b"1234", &cancel)
            .expect("materialize");
        assert!(
            cache
                .lookup("object", None, Some(5))
                .expect("lookup")
                .is_none()
        );
        assert_eq!(cache.entries().expect("entries").len(), 0);
        fs::remove_dir_all(root).expect("cleanup");
    }
}
