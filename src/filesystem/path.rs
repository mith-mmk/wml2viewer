use std::collections::hash_map::DefaultHasher;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::dependent::plugins::path_supported_by_plugins;
use crate::options::{ArchiveBrowseOption, NavigationSortOption};

use super::cache::{FilesystemCache, build_listed_virtual_children, probe_first_supported_path};
use super::listed_file::load_listed_file_entries;
use super::source::{
    OpenedImageSource, open_image_source, source_image_size, source_prefers_low_io,
};
use super::zip_file::set_zip_workaround_options;

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "webp", "jpe", "jpg", "jpeg", "bmp", "gif", "png", "tif", "tiff", "mag", "mki", "pi", "pic",
];
const LISTED_FILE_EXTENSION: &str = "wmltxt";
const LISTED_VIRTUAL_MARKER: &str = "__wmlv__";
const ZIP_FILE_EXTENSION: &str = "zip";
const ZIP_VIRTUAL_MARKER: &str = "__zipv__";

pub fn resolve_start_path(path: &Path) -> Option<PathBuf> {
    if is_virtual_zip_child(path) {
        return Some(path.to_path_buf());
    }

    if let Some(target) = resolve_virtual_listed_child(path) {
        return resolve_start_path(&target);
    }

    if is_zip_file_path(path) {
        let navigation_path = probe_first_supported_path(
            path,
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Folder,
        )?;
        return resolve_start_path(&navigation_path);
    }

    if is_listed_file_path(path) {
        let navigation_path = probe_first_supported_path(
            path,
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Folder,
        )?;
        return resolve_start_path(&navigation_path);
    }

    if path.is_dir() {
        let navigation_path = probe_first_supported_path(
            path,
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Folder,
        )?;
        return resolve_start_path(&navigation_path);
    }

    is_supported_image(path).then(|| path.to_path_buf())
}

pub fn load_virtual_image_bytes(path: &Path) -> Option<Vec<u8>> {
    match open_image_source(path)? {
        OpenedImageSource::Bytes { bytes, .. } => Some(bytes),
        OpenedImageSource::File { .. } => None,
    }
}

pub fn set_archive_zip_workaround(options: crate::options::ZipWorkaroundOptions) {
    set_zip_workaround_options(options);
}

pub fn archive_prefers_low_io(path: &Path) -> bool {
    source_prefers_low_io(path)
}

pub fn virtual_image_size(path: &Path) -> Option<u64> {
    source_image_size(path)
}

pub(crate) fn listed_virtual_child_path(
    listed_file: &Path,
    index: usize,
    entry_path: &Path,
) -> PathBuf {
    let mut path = listed_file.to_path_buf();
    path.push(LISTED_VIRTUAL_MARKER);

    let name = entry_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("entry");
    let identity = listed_virtual_identity(entry_path);
    path.push(format!("{index:08}__{identity:016x}__{name}"));
    path
}

pub(crate) fn zip_virtual_child_path(zip_file: &Path, index: usize, entry_name: &str) -> PathBuf {
    let mut path = zip_file.to_path_buf();
    path.push(ZIP_VIRTUAL_MARKER);
    let name = Path::new(entry_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("entry");
    path.push(format!("{index:08}__{name}"));
    path
}

pub(crate) fn listed_virtual_identity_from_virtual_path(path: &Path) -> Option<u64> {
    let file_name = path.file_name()?.to_string_lossy();
    let mut parts = file_name.splitn(3, "__");
    let _index = parts.next()?;
    let second = parts.next()?;
    if second.len() == 16 && second.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return u64::from_str_radix(second, 16).ok();
    }
    None
}

pub(crate) fn listed_virtual_name_from_virtual_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_string_lossy();
    let mut parts = file_name.splitn(3, "__");
    let _index = parts.next()?;
    let second = parts.next()?;
    let third = parts.next();
    Some(third.unwrap_or(second).to_string())
}

pub(crate) fn listed_virtual_root(path: &Path) -> Option<PathBuf> {
    listed_virtual_child_info(path).map(|(root, _)| root)
}

pub(crate) fn zip_virtual_root(path: &Path) -> Option<PathBuf> {
    zip_virtual_child_info(path).map(|(root, _)| root)
}

pub(crate) fn resolve_virtual_listed_child(path: &Path) -> Option<PathBuf> {
    let (listed_root, index) = listed_virtual_child_info(path)?;
    let entries = load_listed_file_entries(&listed_root)?;
    let entry = entries.get(index)?.clone();
    resolve_navigation_leaf(entry)
}

pub(crate) fn resolve_virtual_zip_child(path: &Path) -> Option<(PathBuf, usize)> {
    zip_virtual_child_info(path)
}

pub(crate) fn is_virtual_listed_child(path: &Path) -> bool {
    listed_virtual_child_info(path).is_some()
}

pub(crate) fn is_virtual_zip_child(path: &Path) -> bool {
    zip_virtual_child_info(path).is_some()
}

pub(crate) fn is_supported_image(path: &Path) -> bool {
    is_supported_image_name(path.file_name().unwrap_or_else(|| path.as_os_str()))
        || path_supported_by_plugins(path)
}

pub(crate) fn is_supported_image_name(name: &OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext = ext.to_ascii_lowercase();
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| *supported == ext)
        })
        .unwrap_or(false)
}

pub(crate) fn is_listed_file_path(path: &Path) -> bool {
    is_listed_file_name(path.file_name().unwrap_or_else(|| path.as_os_str()))
}

pub(crate) fn is_listed_file_name(name: &OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(LISTED_FILE_EXTENSION))
        .unwrap_or(false)
}

pub(crate) fn is_zip_file_path(path: &Path) -> bool {
    is_zip_file_name(path.file_name().unwrap_or_else(|| path.as_os_str()))
}

pub(crate) fn is_zip_file_name(name: &OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(ZIP_FILE_EXTENSION))
        .unwrap_or(false)
}

fn listed_virtual_identity(entry_path: &Path) -> u64 {
    let target = resolve_start_path(entry_path).unwrap_or_else(|| entry_path.to_path_buf());
    let mut hasher = DefaultHasher::new();
    target.to_string_lossy().to_lowercase().hash(&mut hasher);
    hasher.finish()
}

fn resolve_navigation_leaf(path: PathBuf) -> Option<PathBuf> {
    if is_listed_file_path(&path) {
        let children = build_listed_virtual_children(&path);
        return children.first().cloned();
    }

    if path.is_dir() {
        let mut cache = FilesystemCache::default();
        return cache
            .probe_first_supported_file(&path)
            .or_else(|| cache.first_supported_file(&path));
    }

    resolve_start_path(&path).map(|_| path)
}

fn listed_virtual_child_info(path: &Path) -> Option<(PathBuf, usize)> {
    let file_name = path.file_name()?.to_string_lossy();
    let index_text = file_name
        .split_once("__")
        .map(|(index, _)| index)
        .unwrap_or(file_name.as_ref());
    let index = index_text.parse::<usize>().ok()?;

    let marker_dir = path.parent()?;
    if marker_dir.file_name()?.to_str()? != LISTED_VIRTUAL_MARKER {
        return None;
    }

    let listed_root = marker_dir.parent()?.to_path_buf();
    is_listed_file_path(&listed_root).then_some((listed_root, index))
}

fn zip_virtual_child_info(path: &Path) -> Option<(PathBuf, usize)> {
    let file_name = path.file_name()?.to_string_lossy();
    let index_text = file_name
        .split_once("__")
        .map(|(index, _)| index)
        .unwrap_or(file_name.as_ref());
    let index = index_text.parse::<usize>().ok()?;

    let marker_dir = path.parent()?;
    if marker_dir.file_name()?.to_str()? != ZIP_VIRTUAL_MARKER {
        return None;
    }

    let zip_root = marker_dir.parent()?.to_path_buf();
    is_zip_file_path(&zip_root).then_some((zip_root, index))
}
