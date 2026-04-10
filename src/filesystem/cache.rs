use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::dependent::default_temp_dir;
use crate::options::{ArchiveBrowseOption, NavigationSortOption};
use serde::{Deserialize, Serialize};

use super::browser::BrowserMetadata;
use super::listed_file::load_listed_file_entries;
use super::path::{
    is_listed_file_name, is_listed_file_path, is_supported_image_name, is_zip_file_name,
    is_zip_file_path, listed_virtual_child_path, resolve_start_path, zip_virtual_child_path,
};
use super::sort_paths;
use super::source_signature_for_path;
use super::zip_file::{load_zip_entries, probe_first_supported_zip_entry};

pub(crate) struct FilesystemCache {
    listings_by_dir: HashMap<PathBuf, CachedDirectoryListing>,
    metadata_by_path: HashMap<PathBuf, CachedBrowserMetadata>,
    sort: NavigationSortOption,
    archive_mode: ArchiveBrowseOption,
}

pub(crate) type SharedFilesystemCache = Arc<Mutex<FilesystemCache>>;

impl Default for FilesystemCache {
    fn default() -> Self {
        Self::new(NavigationSortOption::OsName, ArchiveBrowseOption::Folder)
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct DirectoryListing {
    raw_files: Vec<PathBuf>,
    files: Vec<PathBuf>,
    files_ready: bool,
    dirs: Vec<PathBuf>,
    browser_entries: Vec<PathBuf>,
    first_file: Option<PathBuf>,
    last_file: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedDirectoryListing {
    signature: Option<super::SourceSignature>,
    listing: DirectoryListing,
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedBrowserMetadata {
    signature: Option<super::SourceSignature>,
    metadata: BrowserMetadata,
}

impl FilesystemCache {
    pub(crate) fn new(sort: NavigationSortOption, archive_mode: ArchiveBrowseOption) -> Self {
        let mut cache = load_persistent_cache().unwrap_or(Self {
            listings_by_dir: HashMap::new(),
            metadata_by_path: HashMap::new(),
            sort,
            archive_mode,
        });
        cache.ensure_settings(sort, archive_mode);
        cache
    }

    pub(crate) fn listing(&mut self, dir: &Path) -> &DirectoryListing {
        let current_signature = listing_signature(dir);
        let should_refresh = self
            .listings_by_dir
            .get(dir)
            .map(|cached| cached.signature != current_signature)
            .unwrap_or(true);
        if should_refresh {
            let mut listing = scan_directory_listing(dir, self.sort, self.archive_mode);
            normalize_directory_listing(&mut listing);
            self.listings_by_dir.insert(
                dir.to_path_buf(),
                CachedDirectoryListing {
                    signature: current_signature,
                    listing,
                },
            );
            persist_cache(self);
        } else if let Some(cached) = self.listings_by_dir.get_mut(dir) {
            if normalize_directory_listing(&mut cached.listing) {
                persist_cache(self);
            }
        }
        self.listings_by_dir
            .get(dir)
            .map(|cached| &cached.listing)
            .expect("directory listing inserted")
    }

    fn ensure_flat_files_ready(&mut self, dir: &Path) {
        let _ = self.listing(dir);
        let needs_flatten = self
            .listings_by_dir
            .get(dir)
            .map(|cached| !cached.listing.files_ready)
            .unwrap_or(false);
        if !needs_flatten {
            return;
        }
        let files = self
            .listings_by_dir
            .get(dir)
            .map(|cached| {
                expand_supported_files_from_raw(&cached.listing.raw_files, self.archive_mode)
            })
            .unwrap_or_default();
        if let Some(cached) = self.listings_by_dir.get_mut(dir) {
            cached.listing.first_file = files.first().cloned();
            cached.listing.last_file = files.last().cloned();
            cached.listing.files = files;
            cached.listing.files_ready = true;
        }
        persist_cache(self);
    }

    pub(crate) fn ensure_settings(
        &mut self,
        sort: NavigationSortOption,
        archive_mode: ArchiveBrowseOption,
    ) {
        if self.sort != sort || self.archive_mode != archive_mode {
            self.sort = sort;
            self.archive_mode = archive_mode;
            self.listings_by_dir.clear();
        }
    }

    pub(crate) fn browser_metadata_batch(
        &mut self,
        paths: &[PathBuf],
    ) -> HashMap<PathBuf, BrowserMetadata> {
        let mut changed = false;
        let mut result = HashMap::with_capacity(paths.len());
        for path in paths {
            let signature = metadata_signature(path);
            let metadata = match self.metadata_by_path.get(path) {
                Some(cached) if cached.signature == signature => cached.metadata.clone(),
                None => {
                    let metadata = load_browser_metadata(path);
                    self.metadata_by_path.insert(
                        path.clone(),
                        CachedBrowserMetadata {
                            signature,
                            metadata: metadata.clone(),
                        },
                    );
                    changed = true;
                    metadata
                }
                Some(_) => {
                    let metadata = load_browser_metadata(path);
                    self.metadata_by_path.insert(
                        path.clone(),
                        CachedBrowserMetadata {
                            signature,
                            metadata: metadata.clone(),
                        },
                    );
                    changed = true;
                    metadata
                }
            };
            result.insert(path.clone(), metadata);
        }
        if changed {
            persist_cache(self);
        }
        result
    }

    pub(crate) fn supported_entries(&mut self, dir: &Path) -> Vec<PathBuf> {
        self.ensure_flat_files_ready(dir);
        self.listing(dir).files.clone()
    }

    pub(crate) fn child_directories(&mut self, dir: &Path) -> Vec<PathBuf> {
        self.listing(dir).dirs.clone()
    }

    pub(crate) fn browser_entries(&mut self, dir: &Path) -> Vec<PathBuf> {
        self.listing(dir).browser_entries.clone()
    }

    pub(crate) fn raw_files(&mut self, dir: &Path) -> Vec<PathBuf> {
        self.listing(dir).raw_files.clone()
    }

    pub(crate) fn first_supported_file(&mut self, dir: &Path) -> Option<PathBuf> {
        self.ensure_flat_files_ready(dir);
        self.listing(dir).first_file.clone()
    }

    pub(crate) fn probe_first_supported_file(&self, path: &Path) -> Option<PathBuf> {
        probe_first_supported_path(path, self.sort, self.archive_mode)
    }

    pub(crate) fn last_supported_file(&mut self, dir: &Path) -> Option<PathBuf> {
        self.ensure_flat_files_ready(dir);
        self.listing(dir).last_file.clone()
    }
}

pub(crate) fn probe_first_supported_path(
    path: &Path,
    sort: NavigationSortOption,
    archive_mode: ArchiveBrowseOption,
) -> Option<PathBuf> {
    if archive_mode == ArchiveBrowseOption::Folder && is_zip_file_path(path) {
        return probe_first_supported_zip_entry(path)
            .map(|entry| zip_virtual_child_path(path, entry.index, &entry.name));
    }

    if archive_mode == ArchiveBrowseOption::Folder && is_listed_file_path(path) {
        return load_listed_file_entries(path)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
            .find_map(|(index, entry_path)| {
                resolve_start_path(&entry_path)
                    .map(|_| listed_virtual_child_path(path, index, &entry_path))
            });
    }

    if !path.is_dir() {
        return None;
    }

    let Some(entries) = fs::read_dir(path).ok() else {
        return None;
    };

    let mut raw_files = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let Some(candidate) = browser_entry_path_from_dir_entry(&entry) else {
            continue;
        };
        if dir_entry_is_browser_file(&entry, &candidate, archive_mode) {
            raw_files.push(candidate);
        }
    }
    sort_paths(&mut raw_files, sort);

    for candidate in &raw_files {
        match archive_mode {
            ArchiveBrowseOption::Folder => {
                if !is_listed_file_path(candidate) && !is_zip_file_path(candidate) {
                    return Some(candidate.clone());
                }
            }
            ArchiveBrowseOption::Skip => {
                if !is_listed_file_path(candidate) && !is_zip_file_path(candidate) {
                    return Some(candidate.clone());
                }
            }
            ArchiveBrowseOption::Archiver => return Some(candidate.clone()),
        }
    }

    if archive_mode == ArchiveBrowseOption::Folder {
        for candidate in &raw_files {
            if (is_listed_file_path(candidate) || is_zip_file_path(candidate))
                && let Some(path) = probe_first_supported_path(candidate, sort, archive_mode)
            {
                return Some(path);
            }
        }
    }

    None
}

pub(crate) fn new_shared_filesystem_cache(
    sort: NavigationSortOption,
    archive_mode: ArchiveBrowseOption,
) -> SharedFilesystemCache {
    Arc::new(Mutex::new(FilesystemCache::new(sort, archive_mode)))
}

#[allow(dead_code)]
pub fn list_openable_entries(dir: &Path, sort: NavigationSortOption) -> Vec<PathBuf> {
    let mut cache = FilesystemCache::new(sort, ArchiveBrowseOption::Folder);
    cache.supported_entries(dir)
}

pub fn list_browser_entries(dir: &Path, sort: NavigationSortOption) -> Vec<PathBuf> {
    let mut cache = FilesystemCache::new(sort, ArchiveBrowseOption::Folder);
    cache.browser_entries(dir)
}

pub fn is_browser_container(path: &Path) -> bool {
    path.is_dir() || is_zip_file_path(path) || is_listed_file_path(path)
}

pub(crate) fn scan_directory_listing(
    dir: &Path,
    sort: NavigationSortOption,
    archive_mode: ArchiveBrowseOption,
) -> DirectoryListing {
    if archive_mode == ArchiveBrowseOption::Folder && is_zip_file_path(dir) {
        return scan_zip_virtual_directory(dir);
    }

    if archive_mode == ArchiveBrowseOption::Folder && is_listed_file_path(dir) {
        return scan_listed_virtual_directory(dir);
    }

    scan_real_directory_listing(dir, sort, archive_mode)
}

pub(crate) fn browser_entry_path_from_dir_entry(entry: &fs::DirEntry) -> Option<PathBuf> {
    let file_name = entry.file_name();
    let path = entry.path();
    if is_supported_image_name(&file_name)
        || is_listed_file_name(&file_name)
        || is_zip_file_name(&file_name)
    {
        return Some(path);
    }

    dir_entry_is_directory(entry).then_some(path)
}

pub(crate) fn build_listed_virtual_children(listed_file: &Path) -> Vec<PathBuf> {
    load_listed_file_entries(listed_file)
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(index, entry_path)| {
            resolve_start_path(&entry_path)
                .map(|_| listed_virtual_child_path(listed_file, index, &entry_path))
        })
        .collect()
}

pub(crate) fn build_zip_virtual_children(zip_file: &Path) -> Vec<PathBuf> {
    load_zip_entries(zip_file)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| zip_virtual_child_path(zip_file, entry.index, &entry.name))
        .collect()
}

fn scan_listed_virtual_directory(listed_file: &Path) -> DirectoryListing {
    let files = build_listed_virtual_children(listed_file);
    let browser_entries = files.clone();

    DirectoryListing {
        raw_files: files.clone(),
        files_ready: true,
        first_file: files.first().cloned(),
        last_file: files.last().cloned(),
        browser_entries,
        files,
        dirs: Vec::new(),
    }
}

fn scan_zip_virtual_directory(zip_file: &Path) -> DirectoryListing {
    let entries = load_zip_entries(zip_file).unwrap_or_default();
    let files = entries
        .iter()
        .map(|entry| zip_virtual_child_path(zip_file, entry.index, &entry.name))
        .collect::<Vec<_>>();
    let browser_entries = files.clone();

    DirectoryListing {
        raw_files: files.clone(),
        files_ready: true,
        first_file: files.first().cloned(),
        last_file: files.last().cloned(),
        browser_entries,
        files,
        dirs: Vec::new(),
    }
}

fn scan_real_directory_listing(
    dir: &Path,
    sort: NavigationSortOption,
    archive_mode: ArchiveBrowseOption,
) -> DirectoryListing {
    let Some(entries) = fs::read_dir(dir).ok() else {
        return DirectoryListing::default();
    };

    let mut raw_files = Vec::new();
    let mut raw_dirs = Vec::new();

    for entry in entries.filter_map(Result::ok) {
        let Some(path) = browser_entry_path_from_dir_entry(&entry) else {
            continue;
        };
        if dir_entry_is_browser_file(&entry, &path, archive_mode) {
            raw_files.push(path.clone());
        }
        if dir_entry_is_browser_container(&entry, &path, archive_mode) {
            raw_dirs.push(path);
        }
    }

    sort_paths(&mut raw_files, sort);
    sort_paths(&mut raw_dirs, sort);
    let mut browser_entries = raw_dirs.clone();
    browser_entries.extend(
        raw_files
            .iter()
            .filter(|path| !raw_dirs.contains(path))
            .cloned(),
    );

    let (files, files_ready) = match archive_mode {
        ArchiveBrowseOption::Folder => (Vec::new(), false),
        _ => (
            expand_supported_files_from_raw(&raw_files, archive_mode),
            true,
        ),
    };

    DirectoryListing {
        raw_files,
        files_ready,
        first_file: files.first().cloned(),
        last_file: files.last().cloned(),
        browser_entries,
        files,
        dirs: raw_dirs,
    }
}

fn dir_entry_is_directory(entry: &fs::DirEntry) -> bool {
    entry
        .file_type()
        .map(|file_type| file_type.is_dir())
        .or_else(|_| entry.metadata().map(|metadata| metadata.is_dir()))
        .unwrap_or(false)
}

fn dir_entry_is_browser_file(
    entry: &fs::DirEntry,
    path: &Path,
    archive_mode: ArchiveBrowseOption,
) -> bool {
    let file_name = entry.file_name();
    is_supported_image_name(&file_name)
        || match archive_mode {
            ArchiveBrowseOption::Folder | ArchiveBrowseOption::Archiver => {
                is_listed_file_path(path) || is_zip_file_path(path)
            }
            ArchiveBrowseOption::Skip => false,
        }
}

fn dir_entry_is_browser_container(
    entry: &fs::DirEntry,
    path: &Path,
    archive_mode: ArchiveBrowseOption,
) -> bool {
    match archive_mode {
        ArchiveBrowseOption::Folder => {
            is_listed_file_path(path) || is_zip_file_path(path) || dir_entry_is_directory(entry)
        }
        ArchiveBrowseOption::Skip | ArchiveBrowseOption::Archiver => dir_entry_is_directory(entry),
    }
}

fn load_browser_metadata(path: &Path) -> BrowserMetadata {
    fs::metadata(path)
        .ok()
        .map(|metadata| BrowserMetadata {
            size: metadata.is_file().then_some(metadata.len()),
            modified: metadata.modified().ok(),
        })
        .unwrap_or_default()
}

fn expand_supported_files_from_raw(
    raw_files: &[PathBuf],
    archive_mode: ArchiveBrowseOption,
) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in raw_files {
        match archive_mode {
            ArchiveBrowseOption::Folder => {
                if is_listed_file_path(path) {
                    files.extend(build_listed_virtual_children(path));
                } else if is_zip_file_path(path) {
                    files.extend(build_zip_virtual_children(path));
                } else {
                    files.push(path.clone());
                }
            }
            ArchiveBrowseOption::Skip => {
                if !is_listed_file_path(path) && !is_zip_file_path(path) {
                    files.push(path.clone());
                }
            }
            ArchiveBrowseOption::Archiver => {
                files.push(path.clone());
            }
        }
    }
    files
}

fn listing_signature(path: &Path) -> Option<super::SourceSignature> {
    source_signature_for_path(path)
}

fn metadata_signature(path: &Path) -> Option<super::SourceSignature> {
    source_signature_for_path(path)
}

fn normalize_directory_listing(listing: &mut DirectoryListing) -> bool {
    let original_len = listing.browser_entries.len();
    let mut deduped = Vec::with_capacity(original_len);
    for path in &listing.browser_entries {
        if !deduped.contains(path) {
            deduped.push(path.clone());
        }
    }
    if deduped.len() == original_len {
        return false;
    }
    listing.browser_entries = deduped;
    true
}

#[derive(Serialize, Deserialize)]
struct PersistentFilesystemCache {
    sort: NavigationSortOption,
    archive_mode: ArchiveBrowseOption,
    listings_by_dir: HashMap<PathBuf, CachedDirectoryListing>,
    metadata_by_path: HashMap<PathBuf, CachedBrowserMetadata>,
}

pub(crate) fn persistent_cache_path() -> Option<PathBuf> {
    Some(default_temp_dir()?.join("filesystem-cache.json"))
}

fn load_persistent_cache() -> Option<FilesystemCache> {
    let text = fs::read_to_string(persistent_cache_path()?).ok()?;
    let mut snapshot = serde_json::from_str::<PersistentFilesystemCache>(&text).ok()?;
    for cached in snapshot.listings_by_dir.values_mut() {
        normalize_directory_listing(&mut cached.listing);
    }
    Some(FilesystemCache {
        listings_by_dir: snapshot.listings_by_dir,
        metadata_by_path: snapshot.metadata_by_path,
        sort: snapshot.sort,
        archive_mode: snapshot.archive_mode,
    })
}

fn persist_cache(cache: &FilesystemCache) {
    let Some(path) = persistent_cache_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let snapshot = PersistentFilesystemCache {
        sort: cache.sort,
        archive_mode: cache.archive_mode,
        listings_by_dir: cache.listings_by_dir.clone(),
        metadata_by_path: cache.metadata_by_path.clone(),
    };
    let Ok(text) = serde_json::to_string(&snapshot) else {
        return;
    };
    let _ = fs::write(path, text);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::SimpleFileOptions;

    fn make_temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("wml2viewer_cache_{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_zip_with_entries(path: &Path, names: &[&str]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for name in names {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            use std::io::Write;
            zip.write_all(b"data").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn browser_metadata_batch_is_invalidated_when_file_changes() {
        let dir = make_temp_dir();
        let file = dir.join("page.png");
        fs::write(&file, [1u8]).unwrap();

        let mut cache = FilesystemCache::default();
        let first = cache.browser_metadata_batch(std::slice::from_ref(&file));
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&file, [1u8, 2, 3, 4]).unwrap();
        let second = cache.browser_metadata_batch(std::slice::from_ref(&file));

        assert_eq!(first.get(&file).and_then(|meta| meta.size), Some(1));
        assert_eq!(second.get(&file).and_then(|meta| meta.size), Some(4));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn scan_real_directory_listing_respects_archive_mode() {
        let dir = make_temp_dir();
        let image = dir.join("001.png");
        let archive = dir.join("images.zip");
        fs::write(&image, []).unwrap();
        fs::write(&archive, []).unwrap();

        let folder_listing = scan_real_directory_listing(
            &dir,
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Folder,
        );
        let skip_listing = scan_real_directory_listing(
            &dir,
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Skip,
        );
        let archiver_listing = scan_real_directory_listing(
            &dir,
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Archiver,
        );

        assert!(folder_listing.browser_entries.contains(&archive));
        assert!(!skip_listing.browser_entries.contains(&archive));
        assert!(archiver_listing.browser_entries.contains(&archive));
        assert!(archiver_listing.files.contains(&archive));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn directory_listing_is_invalidated_when_directory_changes() {
        let dir = make_temp_dir();
        let first = dir.join("001.png");
        let second = dir.join("002.png");
        fs::write(&first, []).unwrap();

        let mut cache = FilesystemCache::default();
        let before = cache.browser_entries(&dir);
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&second, []).unwrap();
        let after = cache.browser_entries(&dir);

        assert_eq!(before, vec![first.clone()]);
        assert!(after.contains(&first));
        assert!(after.contains(&second));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn probe_first_supported_path_returns_first_zip_child() {
        let dir = make_temp_dir();
        let zip_path = dir.join("001.zip");
        make_zip_with_entries(&zip_path, &["002.png", "010.png"]);

        let first = probe_first_supported_path(
            &dir,
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Folder,
        );

        assert_eq!(first, Some(zip_virtual_child_path(&zip_path, 0, "002.png")));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn folder_browser_listing_does_not_duplicate_archives() {
        let dir = make_temp_dir();
        let archive = dir.join("pages.zip");
        let image = dir.join("cover.png");

        make_zip_with_entries(&archive, &["001.png"]);
        fs::write(&image, []).unwrap();

        let listing = scan_real_directory_listing(
            &dir,
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Folder,
        );

        assert_eq!(
            listing
                .browser_entries
                .iter()
                .filter(|path| *path == &archive)
                .count(),
            1
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn probe_first_supported_path_prefers_direct_images_before_archives() {
        let dir = make_temp_dir();
        let archive = dir.join("000_pages.zip");
        let image = dir.join("zzz_cover.png");

        fs::write(&archive, b"not-a-real-zip").unwrap();
        fs::write(&image, []).unwrap();

        let first = probe_first_supported_path(
            &dir,
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Folder,
        );

        assert_eq!(first, Some(image));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn normalize_directory_listing_deduplicates_browser_entries() {
        let archive = PathBuf::from("pages.zip");
        let image = PathBuf::from("cover.png");
        let mut listing = DirectoryListing {
            raw_files: vec![archive.clone(), image],
            files: Vec::new(),
            files_ready: false,
            dirs: vec![archive.clone()],
            browser_entries: vec![archive.clone(), archive, PathBuf::from("cover.png")],
            first_file: None,
            last_file: None,
        };

        assert!(normalize_directory_listing(&mut listing));
        assert_eq!(listing.browser_entries.len(), 2);
    }
}
