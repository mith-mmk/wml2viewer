mod listed_file;
mod sort;
mod zip_file;

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::SystemTime;

use crate::dependent::plugins::path_supported_by_plugins;
use crate::options::{EndOfFolderOption, NavigationSortOption};
use listed_file::load_listed_file_entries;
pub(crate) use sort::{compare_natural_str, compare_os_str};
pub(crate) use zip_file::{load_zip_entries_unsorted, sort_zip_entries};
use zip_file::{
    load_zip_entries, load_zip_entry_bytes, set_zip_workaround_options, zip_entry_record,
    zip_prefers_low_io,
};

const SUPPORTED_EXTENSIONS: &[&str] = &[
    "webp","jpe", "jpg", "jpeg", "bmp", "gif", "png", "tif", "tiff", "mag", "mki", "pi", "pic",
];
const LISTED_FILE_EXTENSION: &str = "wmltxt";
const LISTED_VIRTUAL_MARKER: &str = "__wmlv__";
const ZIP_FILE_EXTENSION: &str = "zip";
const ZIP_VIRTUAL_MARKER: &str = "__zipv__";

#[derive(Clone, Debug)]
pub struct FileNavigator {
    current_path: PathBuf,
    files: Option<Vec<PathBuf>>,
    current: usize,
}

#[derive(Clone, Debug)]
struct NavigationTarget {
    navigation_path: PathBuf,
    load_path: PathBuf,
}

struct FilesystemCache {
    listings_by_dir: HashMap<PathBuf, DirectoryListing>,
    sort: NavigationSortOption,
}

impl Default for FilesystemCache {
    fn default() -> Self {
        Self {
            listings_by_dir: HashMap::new(),
            sort: NavigationSortOption::OsName,
        }
    }
}

#[derive(Clone, Default)]
struct DirectoryListing {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
    first_file: Option<PathBuf>,
    last_file: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum PendingDirection {
    Next,
    Prev,
}

#[derive(Clone)]
pub enum FilesystemCommand {
    Init {
        request_id: u64,
        path: PathBuf,
    },
    SetCurrent {
        request_id: u64,
        path: PathBuf,
    },
    Next {
        request_id: u64,
        policy: EndOfFolderOption,
    },
    Prev {
        request_id: u64,
        policy: EndOfFolderOption,
    },
    First {
        request_id: u64,
    },
    Last {
        request_id: u64,
    },
}

pub enum FilesystemResult {
    NavigatorReady {
        request_id: u64,
        navigation_path: Option<PathBuf>,
        load_path: Option<PathBuf>,
    },
    CurrentSet,
    PathResolved {
        request_id: u64,
        navigation_path: PathBuf,
        load_path: PathBuf,
    },
    NoPath {
        request_id: u64,
    },
}

impl FileNavigator {
    fn from_current_path(path: PathBuf, cache: &mut FilesystemCache) -> Self {
        let files = flat_container_entries(&path, cache).unwrap_or_else(|| vec![path.clone()]);
        let current = files
            .iter()
            .position(|candidate| candidate == &path)
            .unwrap_or(0);

        Self {
            current_path: path,
            files: Some(files),
            current,
        }
    }

    fn current(&self) -> &Path {
        &self.current_path
    }

    fn set_current_input(&mut self, path: PathBuf, cache: &mut FilesystemCache) {
        let Some(navigation_path) = resolve_navigation_path(&path, cache) else {
            return;
        };

        self.current_path = navigation_path;
        self.files = None;
        self.current = 0;
    }

    fn normalize_current_path(&mut self, cache: &mut FilesystemCache) {
        if let Some(navigation_path) = resolve_navigation_path(&self.current_path, cache) {
            if navigation_path != self.current_path {
                self.current_path = navigation_path;
                self.files = None;
                self.current = 0;
            }
        }
    }

    fn ensure_files<'a>(&'a mut self, cache: &mut FilesystemCache) -> &'a [PathBuf] {
        self.normalize_current_path(cache);
        if self.files.is_none() {
            let files = flat_container_entries(&self.current_path, cache)
                .unwrap_or_else(|| vec![self.current_path.clone()]);
            self.current = files
                .iter()
                .position(|candidate| candidate == &self.current_path)
                .unwrap_or(0);
            self.files = Some(files);
        }

        self.files.as_deref().unwrap_or(&[])
    }

    fn next(&mut self, cache: &mut FilesystemCache) -> Option<PathBuf> {
        let len = self.ensure_files(cache).len();
        if self.current + 1 >= len {
            return None;
        }

        self.current += 1;
        let path = self.files.as_ref()?.get(self.current)?.clone();
        self.current_path = path.clone();
        Some(path)
    }

    fn prev(&mut self, cache: &mut FilesystemCache) -> Option<PathBuf> {
        let _ = self.ensure_files(cache);
        if self.current == 0 {
            return None;
        }

        self.current -= 1;
        let path = self.files.as_ref()?.get(self.current)?.clone();
        self.current_path = path.clone();
        Some(path)
    }

    fn first(&mut self, cache: &mut FilesystemCache) -> Option<PathBuf> {
        let files = edge_entries(self.current(), cache)?;
        if files.is_empty() {
            return None;
        }

        self.current = 0;
        self.files = Some(files.clone());
        let path = files.first()?.clone();
        self.current_path = path.clone();
        Some(path)
    }

    fn last(&mut self, cache: &mut FilesystemCache) -> Option<PathBuf> {
        let files = edge_entries(self.current(), cache)?;
        let len = files.len();
        if len == 0 {
            return None;
        }

        self.current = len - 1;
        self.files = Some(files.clone());
        let path = files.get(self.current)?.clone();
        self.current_path = path.clone();
        Some(path)
    }

    fn current_target(&self) -> NavigationOutcome {
        let Some(load_path) = resolve_start_path(&self.current_path) else {
            return NavigationOutcome::NoPath;
        };

        NavigationOutcome::Resolved(NavigationTarget {
            navigation_path: self.current_path.clone(),
            load_path,
        })
    }

    fn next_with_policy(
        &mut self,
        policy: EndOfFolderOption,
        cache: &mut FilesystemCache,
    ) -> NavigationOutcome {
        if self.next(cache).is_some() {
            return self.current_target();
        }

        match policy {
            EndOfFolderOption::Stop => NavigationOutcome::NoPath,
            EndOfFolderOption::Loop => self
                .first(cache)
                .map(|_| self.current_target())
                .unwrap_or(NavigationOutcome::NoPath),
            EndOfFolderOption::Next => self
                .jump_to_adjacent_directory(true, cache)
                .unwrap_or(NavigationOutcome::NoPath),
            EndOfFolderOption::Recursive => find_recursive_next_path(cache, self.current())
                .map(|path| {
                    self.current_path = path;
                    self.files = None;
                    self.current = 0;
                    self.current_target()
                })
                .unwrap_or(NavigationOutcome::NoPath),
        }
    }

    fn prev_with_policy(
        &mut self,
        policy: EndOfFolderOption,
        cache: &mut FilesystemCache,
    ) -> NavigationOutcome {
        if self.prev(cache).is_some() {
            return self.current_target();
        }

        match policy {
            EndOfFolderOption::Stop => NavigationOutcome::NoPath,
            EndOfFolderOption::Loop => self
                .last(cache)
                .map(|_| self.current_target())
                .unwrap_or(NavigationOutcome::NoPath),
            EndOfFolderOption::Next => self
                .jump_to_adjacent_directory(false, cache)
                .unwrap_or(NavigationOutcome::NoPath),
            EndOfFolderOption::Recursive => find_recursive_prev_path(cache, self.current())
                .map(|path| {
                    self.current_path = path;
                    self.files = None;
                    self.current = 0;
                    self.current_target()
                })
                .unwrap_or(NavigationOutcome::NoPath),
        }
    }

    fn jump_to_adjacent_directory(
        &mut self,
        forward: bool,
        cache: &mut FilesystemCache,
    ) -> Option<NavigationOutcome> {
        let current_dir = next_policy_directory(self.current())?;
        let parent_dir = current_dir.parent()?;
        let directories = cache.child_directories(parent_dir);
        let current_index = directories.iter().position(|dir| dir == &current_dir)?;

        let target = if forward {
            directories.iter().skip(current_index + 1).find_map(|dir| {
                cache
                    .first_supported_file(dir)
                    .map(|path| (dir.clone(), path))
            })
        } else {
            directories[..current_index].iter().rev().find_map(|dir| {
                cache
                    .last_supported_file(dir)
                    .map(|path| (dir.clone(), path))
            })
        }?;

        let _ = target.0;
        self.current_path = target.1;
        self.files = None;
        self.current = 0;
        Some(self.current_target())
    }
}

enum NavigationOutcome {
    Resolved(NavigationTarget),
    NoPath,
}

pub fn resolve_start_path(path: &Path) -> Option<PathBuf> {
    if is_virtual_zip_child(path) {
        return Some(path.to_path_buf());
    }

    if let Some(target) = resolve_virtual_listed_child(path) {
        return resolve_start_path(&target);
    }

    if is_zip_file_path(path) {
        let mut cache = FilesystemCache::default();
        let navigation_path = cache.first_supported_file(path)?;
        return resolve_start_path(&navigation_path);
    }

    if is_listed_file_path(path) {
        let mut cache = FilesystemCache::default();
        let navigation_path = cache.first_supported_file(path)?;
        return resolve_start_path(&navigation_path);
    }

    if path.is_dir() {
        let mut cache = FilesystemCache::default();
        let navigation_path = cache.first_supported_file(path)?;
        return resolve_start_path(&navigation_path);
    }

    is_supported_image(path).then(|| path.to_path_buf())
}

pub fn load_virtual_image_bytes(path: &Path) -> Option<Vec<u8>> {
    resolve_virtual_zip_child(path)
        .and_then(|(archive, index)| load_zip_entry_bytes(&archive, index))
}

pub fn set_archive_zip_workaround(options: crate::options::ZipWorkaroundOptions) {
    set_zip_workaround_options(options);
}

pub fn archive_prefers_low_io(path: &Path) -> bool {
    if let Some((archive, _)) = resolve_virtual_zip_child(path) {
        return zip_prefers_low_io(&archive);
    }
    if is_zip_file_path(path) {
        return zip_prefers_low_io(path);
    }
    false
}

pub fn virtual_image_size(path: &Path) -> Option<u64> {
    resolve_virtual_zip_child(path)
        .and_then(|(archive, index)| zip_entry_record(&archive, index))
        .map(|entry| entry.size)
}

#[allow(dead_code)]
pub fn list_openable_entries(dir: &Path, sort: NavigationSortOption) -> Vec<PathBuf> {
    let mut cache = FilesystemCache {
        listings_by_dir: HashMap::new(),
        sort,
    };
    cache.supported_entries(dir)
}

pub fn list_browser_entries(dir: &Path, sort: NavigationSortOption) -> Vec<PathBuf> {
    if is_zip_file_path(dir) {
        return build_zip_virtual_children(dir);
    }

    if is_listed_file_path(dir) {
        return build_listed_virtual_children(dir);
    }

    let mut entries = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return entries;
    };

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in read_dir.filter_map(Result::ok) {
        let Some(path) = browser_entry_path_from_dir_entry(&entry) else {
            continue;
        };
        if dir_entry_is_browser_file(&entry, &path) {
            files.push(path.clone());
        }
        if dir_entry_is_browser_container(&entry, &path) {
            dirs.push(path);
        }
    }

    sort_paths(&mut dirs, sort);
    sort_paths(&mut files, sort);
    entries.extend(dirs);
    entries.extend(files);
    entries
}

pub fn is_browser_container(path: &Path) -> bool {
    path.is_dir() || is_zip_file_path(path) || is_listed_file_path(path)
}

pub fn navigation_branch_path(path: &Path) -> Option<PathBuf> {
    recursive_branch_dir(path)
}

pub fn adjacent_entry(path: &Path, sort: NavigationSortOption, step: isize) -> Option<PathBuf> {
    let mut cache = FilesystemCache {
        listings_by_dir: HashMap::new(),
        sort,
    };
    let start_path = resolve_navigation_path(path, &mut cache)?;
    let mut navigator = FileNavigator::from_current_path(start_path, &mut cache);

    if step == 0 {
        return Some(navigator.current().to_path_buf());
    }

    let count = step.unsigned_abs();
    let mut result = None;
    for _ in 0..count {
        result = if step > 0 {
            navigator.next(&mut cache)
        } else {
            navigator.prev(&mut cache)
        };
        result.as_ref()?;
    }
    result
}

pub fn resolve_navigation_entry_path(path: &Path) -> Option<PathBuf> {
    let mut cache = FilesystemCache::default();
    resolve_navigation_path(path, &mut cache)
}

pub fn spawn_filesystem_worker(
    sort: NavigationSortOption,
) -> (Sender<FilesystemCommand>, Receiver<FilesystemResult>) {
    let (command_tx, command_rx) = mpsc::channel::<FilesystemCommand>();
    let (result_tx, result_rx) = mpsc::channel::<FilesystemResult>();

    thread::spawn(move || {
        let mut navigator: Option<FileNavigator> = None;
        let mut cache = FilesystemCache {
            listings_by_dir: HashMap::new(),
            sort,
        };

        while let Ok(command) = command_rx.recv() {
            match command {
                FilesystemCommand::Init { request_id, path } => {
                    let Some(start_path) = resolve_navigation_path(&path, &mut cache) else {
                        let _ = result_tx.send(FilesystemResult::NoPath { request_id });
                        continue;
                    };

                    navigator = Some(FileNavigator::from_current_path(start_path, &mut cache));
                    let initial_target = navigator
                        .as_ref()
                        .and_then(|nav| navigation_outcome_to_target(nav.current_target()));
                    let _ = result_tx.send(FilesystemResult::NavigatorReady {
                        request_id,
                        navigation_path: initial_target.as_ref().map(|target| target.navigation_path.clone()),
                        load_path: initial_target.map(|target| target.load_path),
                    });
                }
                FilesystemCommand::SetCurrent { request_id, path } => {
                    if let Some(nav) = navigator.as_mut() {
                        nav.set_current_input(path, &mut cache);
                    } else if let Some(start_path) = resolve_navigation_path(&path, &mut cache) {
                        navigator = Some(FileNavigator::from_current_path(start_path, &mut cache));
                    }
                    let _ = request_id;
                    let _ = result_tx.send(FilesystemResult::CurrentSet);
                }
                FilesystemCommand::Next { request_id, policy } => {
                    handle_navigation_request(
                        &result_tx,
                        navigator.as_mut(),
                        &mut cache,
                        request_id,
                        policy,
                        PendingDirection::Next,
                    );
                }
                FilesystemCommand::Prev { request_id, policy } => {
                    handle_navigation_request(
                        &result_tx,
                        navigator.as_mut(),
                        &mut cache,
                        request_id,
                        policy,
                        PendingDirection::Prev,
                    );
                }
                FilesystemCommand::First { request_id } => {
                    let outcome = navigator
                        .as_mut()
                        .and_then(|nav| nav.first(&mut cache).map(|_| nav.current_target()))
                        .unwrap_or(NavigationOutcome::NoPath);
                    let _ = send_nav_result(
                        &result_tx,
                        request_id,
                        navigation_outcome_to_target(outcome),
                    );
                }
                FilesystemCommand::Last { request_id } => {
                    let outcome = navigator
                        .as_mut()
                        .and_then(|nav| nav.last(&mut cache).map(|_| nav.current_target()))
                        .unwrap_or(NavigationOutcome::NoPath);
                    let _ = send_nav_result(
                        &result_tx,
                        request_id,
                        navigation_outcome_to_target(outcome),
                    );
                }
            }
        }
    });

    (command_tx, result_rx)
}

fn send_nav_result(
    tx: &Sender<FilesystemResult>,
    request_id: u64,
    target: Option<NavigationTarget>,
) -> Result<(), mpsc::SendError<FilesystemResult>> {
    match target {
        Some(target) => tx.send(FilesystemResult::PathResolved {
            request_id,
            navigation_path: target.navigation_path,
            load_path: target.load_path,
        }),
        None => tx.send(FilesystemResult::NoPath { request_id }),
    }
}

fn handle_navigation_request(
    tx: &Sender<FilesystemResult>,
    navigator: Option<&mut FileNavigator>,
    cache: &mut FilesystemCache,
    request_id: u64,
    policy: EndOfFolderOption,
    direction: PendingDirection,
) {
    let outcome = match navigator {
        Some(nav) => match direction {
            PendingDirection::Next => nav.next_with_policy(policy, cache),
            PendingDirection::Prev => nav.prev_with_policy(policy, cache),
        },
        None => NavigationOutcome::NoPath,
    };

    let _ = send_nav_result(tx, request_id, navigation_outcome_to_target(outcome));
}

fn navigation_outcome_to_target(outcome: NavigationOutcome) -> Option<NavigationTarget> {
    match outcome {
        NavigationOutcome::Resolved(target) => Some(target),
        NavigationOutcome::NoPath => None,
    }
}

fn resolve_navigation_path(path: &Path, cache: &mut FilesystemCache) -> Option<PathBuf> {
    if is_virtual_zip_child(path) {
        return resolve_start_path(path).map(|_| path.to_path_buf());
    }

    if is_virtual_listed_child(path) {
        return rebase_virtual_listed_child_path(path, cache)
            .or_else(|| resolve_start_path(path).map(|_| path.to_path_buf()));
    }

    if is_listed_file_path(path) || is_zip_file_path(path) || path.is_dir() {
        return cache
            .first_supported_file(path)
            .or_else(|| Some(path.to_path_buf()));
    }

    resolve_start_path(path).map(|_| path.to_path_buf())
}

fn rebase_virtual_listed_child_path(
    path: &Path,
    cache: &mut FilesystemCache,
) -> Option<PathBuf> {
    let listed_root = listed_virtual_root(path)?;
    let expected_identity = listed_virtual_identity_from_virtual_path(path);
    cache
        .supported_entries(&listed_root)
        .into_iter()
        .find(|entry| {
            listed_virtual_identity_from_virtual_path(entry)
                .zip(expected_identity)
                .map(|(left, right)| left == right)
                .unwrap_or(false)
        })
        .or_else(|| {
            let expected_name = listed_virtual_name_from_virtual_path(path)?;
            cache.supported_entries(&listed_root).into_iter().find(|entry| {
                listed_virtual_name_from_virtual_path(entry)
                    .map(|name| name.eq_ignore_ascii_case(&expected_name))
                    .unwrap_or(false)
            })
        })
}

fn flat_container_entries(path: &Path, cache: &mut FilesystemCache) -> Option<Vec<PathBuf>> {
    if path.is_dir() || is_zip_file_path(path) || is_listed_file_path(path) {
        return Some(cache.supported_entries(path));
    }
    let dir = flat_container_dir(path)?;
    Some(cache.supported_entries(&dir))
}

fn edge_entries(path: &Path, cache: &mut FilesystemCache) -> Option<Vec<PathBuf>> {
    if let Some(zip_root) = zip_virtual_root(path) {
        return Some(cache.supported_entries(&zip_root));
    }

    if let Some(listed_root) = listed_virtual_root(path) {
        return Some(cache.supported_entries(&listed_root));
    }

    flat_container_entries(path, cache)
}

fn flat_container_dir(path: &Path) -> Option<PathBuf> {
    if let Some(zip_root) = zip_virtual_root(path) {
        return zip_root.parent().map(Path::to_path_buf);
    }

    if let Some(listed_root) = listed_virtual_root(path) {
        return listed_root.parent().map(Path::to_path_buf);
    }

    path.parent().map(Path::to_path_buf)
}

fn next_policy_directory(path: &Path) -> Option<PathBuf> {
    if path.is_dir() || is_zip_file_path(path) || is_listed_file_path(path) {
        return Some(path.to_path_buf());
    }

    if let Some(zip_root) = zip_virtual_root(path) {
        return zip_root.parent().map(Path::to_path_buf);
    }

    if let Some(listed_root) = listed_virtual_root(path) {
        return listed_root.parent().map(Path::to_path_buf);
    }

    path.parent().map(Path::to_path_buf)
}

fn recursive_branch_dir(path: &Path) -> Option<PathBuf> {
    if path.is_dir() || is_zip_file_path(path) || is_listed_file_path(path) {
        return Some(path.to_path_buf());
    }

    if let Some(zip_root) = zip_virtual_root(path) {
        return Some(zip_root);
    }

    if let Some(listed_root) = listed_virtual_root(path) {
        return Some(listed_root);
    }

    path.parent().map(Path::to_path_buf)
}

fn find_recursive_next_path(cache: &mut FilesystemCache, current_path: &Path) -> Option<PathBuf> {
    let mut branch_dir = recursive_branch_dir(current_path)?;

    loop {
        let parent_dir = branch_dir.parent()?.to_path_buf();
        let directories = cache.child_directories(&parent_dir);
        let current_index = directories.iter().position(|dir| dir == &branch_dir)?;

        for sibling_dir in directories.iter().skip(current_index + 1) {
            if let Some(path) = first_path_in_subtree(cache, sibling_dir) {
                return Some(path);
            }
        }

        branch_dir = parent_dir;
    }
}

fn find_recursive_prev_path(cache: &mut FilesystemCache, current_path: &Path) -> Option<PathBuf> {
    let mut branch_dir = recursive_branch_dir(current_path)?;

    loop {
        let parent_dir = branch_dir.parent()?.to_path_buf();
        let directories = cache.child_directories(&parent_dir);
        let current_index = directories.iter().position(|dir| dir == &branch_dir)?;

        for sibling_dir in directories[..current_index].iter().rev() {
            if let Some(path) = last_path_in_subtree(cache, sibling_dir) {
                return Some(path);
            }
        }

        branch_dir = parent_dir;
    }
}

fn first_path_in_subtree(cache: &mut FilesystemCache, dir: &Path) -> Option<PathBuf> {
    if let Some(path) = cache.first_supported_file(dir) {
        return Some(path);
    }

    for child_dir in cache.child_directories(dir) {
        if let Some(path) = first_path_in_subtree(cache, &child_dir) {
            return Some(path);
        }
    }

    None
}

fn last_path_in_subtree(cache: &mut FilesystemCache, dir: &Path) -> Option<PathBuf> {
    let child_dirs = cache.child_directories(dir);
    for child_dir in child_dirs.iter().rev() {
        if let Some(path) = last_path_in_subtree(cache, child_dir) {
            return Some(path);
        }
    }

    cache.last_supported_file(dir)
}

impl FilesystemCache {
    fn listing(&mut self, dir: &Path) -> &DirectoryListing {
        if is_listed_file_path(dir) {
            let listing = scan_directory_listing(dir, self.sort);
            self.listings_by_dir.insert(dir.to_path_buf(), listing);
            return self
                .listings_by_dir
                .get(dir)
                .expect("listed file listing inserted");
        }
        let sort = self.sort;
        self.listings_by_dir
            .entry(dir.to_path_buf())
            .or_insert_with(|| scan_directory_listing(dir, sort))
    }

    fn supported_entries(&mut self, dir: &Path) -> Vec<PathBuf> {
        self.listing(dir).files.clone()
    }

    fn child_directories(&mut self, dir: &Path) -> Vec<PathBuf> {
        self.listing(dir).dirs.clone()
    }

    fn first_supported_file(&mut self, dir: &Path) -> Option<PathBuf> {
        self.listing(dir).first_file.clone()
    }

    fn last_supported_file(&mut self, dir: &Path) -> Option<PathBuf> {
        self.listing(dir).last_file.clone()
    }
}

fn scan_directory_listing(dir: &Path, sort: NavigationSortOption) -> DirectoryListing {
    if is_zip_file_path(dir) {
        return scan_zip_virtual_directory(dir);
    }

    if is_listed_file_path(dir) {
        return scan_listed_virtual_directory(dir);
    }

    scan_real_directory_listing(dir, sort)
}

fn scan_listed_virtual_directory(listed_file: &Path) -> DirectoryListing {
    let files = build_listed_virtual_children(listed_file);

    DirectoryListing {
        first_file: files.first().cloned(),
        last_file: files.last().cloned(),
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

    DirectoryListing {
        first_file: files.first().cloned(),
        last_file: files.last().cloned(),
        files,
        dirs: Vec::new(),
    }
}

fn scan_real_directory_listing(dir: &Path, sort: NavigationSortOption) -> DirectoryListing {
    let Some(entries) = fs::read_dir(dir).ok() else {
        return DirectoryListing::default();
    };

    let mut raw_files = Vec::new();
    let mut raw_dirs = Vec::new();

    for entry in entries.filter_map(Result::ok) {
        let Some(path) = browser_entry_path_from_dir_entry(&entry) else {
            continue;
        };
        if dir_entry_is_browser_file(&entry, &path) {
            raw_files.push(path.clone());
        }
        if dir_entry_is_browser_container(&entry, &path) {
            raw_dirs.push(path);
        }
    }

    sort_paths(&mut raw_files, sort);
    sort_paths(&mut raw_dirs, sort);

    let mut files = Vec::new();
    for path in raw_files {
        if is_listed_file_path(&path) {
            files.extend(build_listed_virtual_children(&path));
        } else if is_zip_file_path(&path) {
            files.extend(build_zip_virtual_children(&path));
        } else {
            files.push(path);
        }
    }

    DirectoryListing {
        first_file: files.first().cloned(),
        last_file: files.last().cloned(),
        files,
        dirs: raw_dirs,
    }
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

fn dir_entry_is_directory(entry: &fs::DirEntry) -> bool {
    entry.file_type()
        .map(|file_type| file_type.is_dir())
        .or_else(|_| entry.metadata().map(|metadata| metadata.is_dir()))
        .unwrap_or(false)
}

fn dir_entry_is_browser_file(entry: &fs::DirEntry, path: &Path) -> bool {
    let file_name = entry.file_name();
    is_supported_image_name(&file_name) || is_listed_file_path(path) || is_zip_file_path(path)
}

fn dir_entry_is_browser_container(entry: &fs::DirEntry, path: &Path) -> bool {
    is_listed_file_path(path) || is_zip_file_path(path) || dir_entry_is_directory(entry)
}

fn build_listed_virtual_children(listed_file: &Path) -> Vec<PathBuf> {
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

fn build_zip_virtual_children(zip_file: &Path) -> Vec<PathBuf> {
    load_zip_entries(zip_file)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| zip_virtual_child_path(zip_file, entry.index, &entry.name))
        .collect()
}

fn listed_virtual_child_path(listed_file: &Path, index: usize, entry_path: &Path) -> PathBuf {
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

fn listed_virtual_identity(entry_path: &Path) -> u64 {
    let target = resolve_start_path(entry_path).unwrap_or_else(|| entry_path.to_path_buf());
    let mut hasher = DefaultHasher::new();
    target.to_string_lossy().to_lowercase().hash(&mut hasher);
    hasher.finish()
}

fn listed_virtual_identity_from_virtual_path(path: &Path) -> Option<u64> {
    let file_name = path.file_name()?.to_string_lossy();
    let mut parts = file_name.splitn(3, "__");
    let _index = parts.next()?;
    let second = parts.next()?;
    if second.len() == 16 && second.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return u64::from_str_radix(second, 16).ok();
    }
    None
}

fn listed_virtual_name_from_virtual_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_string_lossy();
    let mut parts = file_name.splitn(3, "__");
    let _index = parts.next()?;
    let second = parts.next()?;
    let third = parts.next();
    Some(third.unwrap_or(second).to_string())
}

fn zip_virtual_child_path(zip_file: &Path, index: usize, entry_name: &str) -> PathBuf {
    let mut path = zip_file.to_path_buf();
    path.push(ZIP_VIRTUAL_MARKER);
    let name = Path::new(entry_name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("entry");
    path.push(format!("{index:08}__{name}"));
    path
}

fn listed_virtual_root(path: &Path) -> Option<PathBuf> {
    listed_virtual_child_info(path).map(|(root, _)| root)
}

fn zip_virtual_root(path: &Path) -> Option<PathBuf> {
    zip_virtual_child_info(path).map(|(root, _)| root)
}

fn resolve_virtual_listed_child(path: &Path) -> Option<PathBuf> {
    let (listed_root, index) = listed_virtual_child_info(path)?;
    let entries = load_listed_file_entries(&listed_root)?;
    let entry = entries.get(index)?.clone();
    resolve_navigation_leaf(entry)
}

fn resolve_virtual_zip_child(path: &Path) -> Option<(PathBuf, usize)> {
    zip_virtual_child_info(path)
}

fn resolve_navigation_leaf(path: PathBuf) -> Option<PathBuf> {
    if is_listed_file_path(&path) {
        let children = build_listed_virtual_children(&path);
        return children.first().cloned();
    }

    if path.is_dir() {
        let mut cache = FilesystemCache::default();
        return cache.first_supported_file(&path);
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

fn is_virtual_listed_child(path: &Path) -> bool {
    listed_virtual_child_info(path).is_some()
}

fn is_virtual_zip_child(path: &Path) -> bool {
    zip_virtual_child_info(path).is_some()
}

fn is_supported_image(path: &Path) -> bool {
    is_supported_image_name(path.file_name().unwrap_or_else(|| path.as_os_str()))
        || path_supported_by_plugins(path)
}

fn is_supported_image_name(name: &OsStr) -> bool {
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

fn is_listed_file_path(path: &Path) -> bool {
    is_listed_file_name(path.file_name().unwrap_or_else(|| path.as_os_str()))
}

fn is_listed_file_name(name: &OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(LISTED_FILE_EXTENSION))
        .unwrap_or(false)
}

fn is_zip_file_path(path: &Path) -> bool {
    is_zip_file_name(path.file_name().unwrap_or_else(|| path.as_os_str()))
}

fn is_zip_file_name(name: &OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case(ZIP_FILE_EXTENSION))
        .unwrap_or(false)
}

fn file_name_sort_key(path: &Path) -> String {
    if let Some((archive, index)) = resolve_virtual_zip_child(path) {
        return load_zip_entries(&archive)
            .and_then(|entries| entries.into_iter().find(|entry| entry.index == index))
            .map(|entry| entry.name.to_lowercase())
            .unwrap_or_default();
    }

    if let Some(target) = resolve_virtual_listed_child(path) {
        return file_name_sort_key(&target);
    }

    path.file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn os_name_sort_key(path: &Path) -> String {
    if let Some((archive, index)) = resolve_virtual_zip_child(path) {
        return load_zip_entries(&archive)
            .and_then(|entries| entries.into_iter().find(|entry| entry.index == index))
            .map(|entry| entry.name)
            .unwrap_or_default();
    }

    if let Some(target) = resolve_virtual_listed_child(path) {
        return os_name_sort_key(&target);
    }

    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn sort_paths(paths: &mut [PathBuf], sort: NavigationSortOption) {
    match sort {
        NavigationSortOption::OsName => {
            paths.sort_by(|left, right| {
                compare_os_str(&os_name_sort_key(left), &os_name_sort_key(right))
            });
        }
        NavigationSortOption::Name => {
            paths.sort_by(|left, right| {
                compare_natural_str(&file_name_sort_key(left), &file_name_sort_key(right), true)
            });
        }
        NavigationSortOption::Date => {
            paths
                .sort_by_cached_key(|path| (metadata_modified_key(path), file_name_sort_key(path)));
        }
        NavigationSortOption::Size => {
            paths.sort_by_cached_key(|path| (metadata_size_key(path), file_name_sort_key(path)));
        }
    }
}

fn metadata_modified_key(path: &Path) -> SystemTime {
    if let Some((archive, _)) = resolve_virtual_zip_child(path) {
        return fs::metadata(archive)
            .and_then(|metadata| metadata.modified())
            .unwrap_or(SystemTime::UNIX_EPOCH);
    }

    let metadata_path = resolve_virtual_listed_child(path).unwrap_or_else(|| path.to_path_buf());
    fs::metadata(metadata_path)
        .and_then(|metadata| metadata.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

fn metadata_size_key(path: &Path) -> u64 {
    if let Some((archive, _)) = resolve_virtual_zip_child(path) {
        return fs::metadata(archive)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
    }

    let metadata_path = resolve_virtual_listed_child(path).unwrap_or_else(|| path.to_path_buf());
    fs::metadata(metadata_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependent::plugins::{
        PluginCapabilityConfig, PluginConfig, PluginExtensionConfig, PluginModuleConfig,
        PluginProviderConfig, set_runtime_plugin_config,
    };
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::SimpleFileOptions;

    fn make_temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("wml2viewer_nav_{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn plugin_runtime_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn make_zip_with_entries(path: &Path, names: &[&str]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for name in names {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(b"not-a-real-image").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn listed_file_is_expanded_as_virtual_children() {
        let dir = make_temp_dir();
        let before = dir.join("before.webp");
        let listed = dir.join("listedfile.wmltxt");
        let after = dir.join("after.gif");
        let listed_1 = dir.join("test_f16.png");
        let listed_2 = dir.join("test.png");

        fs::write(&before, []).unwrap();
        fs::write(&after, []).unwrap();
        fs::write(&listed_1, []).unwrap();
        fs::write(&listed_2, []).unwrap();
        fs::write(
            &listed,
            format!(
                "#!WMLViewer2 ListedFile 1.0\n{}\n{}\n",
                listed_1.display(),
                listed_2.display()
            ),
        )
        .unwrap();

        let mut cache = FilesystemCache::default();
        let entries = cache.supported_entries(&dir);
        assert!(entries.contains(&before));
        assert!(entries.contains(&after));
        assert!(entries.iter().any(|entry| {
            is_virtual_listed_child(entry) && resolve_start_path(entry) == Some(listed_1.clone())
        }));
        assert!(entries.iter().any(|entry| {
            is_virtual_listed_child(entry) && resolve_start_path(entry) == Some(listed_2.clone())
        }));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn listed_file_returns_to_directory_on_next_and_prev() {
        let dir = make_temp_dir();
        let before = dir.join("00000-1796047615-Maid_san.jpg.webp");
        let listed = dir.join("listedfile.wmltxt");
        let after = dir.join("sample_animation.webp.gif");
        let listed_1 = dir.join("test_f16.png");
        let listed_2 = dir.join("test.png");

        fs::write(&before, []).unwrap();
        fs::write(&after, []).unwrap();
        fs::write(&listed_1, []).unwrap();
        fs::write(&listed_2, []).unwrap();
        fs::write(
            &listed,
            format!(
                "#!WMLViewer2 ListedFile 1.0\n{}\n{}\n",
                listed_1.display(),
                listed_2.display()
            ),
        )
        .unwrap();

        let mut cache = FilesystemCache::default();
        let mut nav = FileNavigator::from_current_path(before.clone(), &mut cache);

        let NavigationOutcome::Resolved(target) =
            nav.next_with_policy(EndOfFolderOption::Next, &mut cache)
        else {
            panic!("expected first listed child from next");
        };
        assert!(is_virtual_listed_child(&target.navigation_path));
        assert_eq!(
            listed_virtual_root(&target.navigation_path),
            Some(listed.clone())
        );
        assert_eq!(target.load_path, listed_1);

        nav.set_current_input(target.navigation_path.clone(), &mut cache);

        let NavigationOutcome::Resolved(target) =
            nav.next_with_policy(EndOfFolderOption::Next, &mut cache)
        else {
            panic!("expected second listed child");
        };
        assert!(is_virtual_listed_child(&target.navigation_path));
        assert_eq!(target.load_path, listed_2);

        nav.set_current_input(target.navigation_path.clone(), &mut cache);

        let NavigationOutcome::Resolved(target) =
            nav.next_with_policy(EndOfFolderOption::Next, &mut cache)
        else {
            panic!("expected directory item after listed file");
        };
        assert_eq!(target.navigation_path, after);
        assert_eq!(target.load_path, after);

        let mut nav = FileNavigator::from_current_path(after.clone(), &mut cache);
        let NavigationOutcome::Resolved(target) =
            nav.prev_with_policy(EndOfFolderOption::Next, &mut cache)
        else {
            panic!("expected listed file child from prev");
        };
        assert!(is_virtual_listed_child(&target.navigation_path));
        assert_eq!(listed_virtual_root(&target.navigation_path), Some(listed));
        assert_eq!(target.load_path, listed_2);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn listed_file_prev_exits_to_previous_entry_even_if_first_item_matches_outer_file() {
        let dir = make_temp_dir();
        let before = dir.join("00000-1796047615-Maid_san.jpg.webp");
        let listed = dir.join("listedfile.wmltxt");
        let after = dir.join("sample_animation.webp.gif");
        let listed_2 = dir.join("test.png");
        let listed_3 = dir.join("test_f16.png");

        fs::write(&before, []).unwrap();
        fs::write(&after, []).unwrap();
        fs::write(&listed_2, []).unwrap();
        fs::write(&listed_3, []).unwrap();
        fs::write(
            &listed,
            format!(
                "#!WMLViewer2 ListedFile 1.0\n{}\n{}\n{}\n",
                after.display(),
                listed_2.display(),
                listed_3.display()
            ),
        )
        .unwrap();

        let mut cache = FilesystemCache::default();
        let mut nav = FileNavigator::from_current_path(after.clone(), &mut cache);

        let NavigationOutcome::Resolved(target) =
            nav.prev_with_policy(EndOfFolderOption::Next, &mut cache)
        else {
            panic!("expected listed file from prev");
        };
        assert_eq!(target.load_path, listed_3);
        nav.set_current_input(target.navigation_path.clone(), &mut cache);

        let NavigationOutcome::Resolved(target) =
            nav.prev_with_policy(EndOfFolderOption::Next, &mut cache)
        else {
            panic!("expected middle listed entry");
        };
        assert_eq!(target.load_path, listed_2);
        nav.set_current_input(target.navigation_path.clone(), &mut cache);

        let NavigationOutcome::Resolved(target) =
            nav.prev_with_policy(EndOfFolderOption::Next, &mut cache)
        else {
            panic!("expected first listed entry");
        };
        assert_eq!(target.load_path, after);
        nav.set_current_input(target.navigation_path.clone(), &mut cache);

        let NavigationOutcome::Resolved(target) =
            nav.prev_with_policy(EndOfFolderOption::Next, &mut cache)
        else {
            panic!("expected exit to previous outer entry");
        };
        assert_eq!(target.navigation_path, before);
        assert_eq!(target.load_path, before);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn zip_file_is_expanded_as_virtual_children() {
        let dir = make_temp_dir();
        let before = dir.join("before.webp");
        let archive = dir.join("images.zip");
        let after = dir.join("after.gif");

        fs::write(&before, []).unwrap();
        fs::write(&after, []).unwrap();
        make_zip_with_entries(&archive, &["001.png", "sub/002.jpg", "note.txt"]);

        let mut cache = FilesystemCache::default();
        let entries = cache.supported_entries(&dir);
        assert!(entries.contains(&before));
        assert!(entries.contains(&after));
        assert!(entries.iter().any(|entry| is_virtual_zip_child(entry)));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn empty_folder_can_navigate_to_next_folder() {
        let root = make_temp_dir();
        let empty = root.join("000_empty");
        let next = root.join("001_next");
        let image = next.join("page01.png");

        fs::create_dir_all(&empty).unwrap();
        fs::create_dir_all(&next).unwrap();
        fs::write(&image, []).unwrap();

        let mut cache = FilesystemCache::default();
        let mut nav = FileNavigator::from_current_path(empty.clone(), &mut cache);

        let NavigationOutcome::Resolved(target) =
            nav.next_with_policy(EndOfFolderOption::Next, &mut cache)
        else {
            panic!("expected next folder image");
        };
        assert_eq!(target.navigation_path, image);
        assert_eq!(target.load_path, image);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn home_and_end_stay_inside_current_zip_virtual_folder() {
        let root = make_temp_dir();
        let archive = root.join("images.zip");
        make_zip_with_entries(&archive, &["001.png", "002.png", "003.png"]);

        let mut cache = FilesystemCache::default();
        let zip_children = build_zip_virtual_children(&archive);
        assert_eq!(zip_children.len(), 3);

        let mut nav = FileNavigator::from_current_path(zip_children[1].clone(), &mut cache);
        let first = nav.first(&mut cache).expect("first zip entry");
        let last = nav.last(&mut cache).expect("last zip entry");

        assert_eq!(zip_virtual_root(&first), Some(archive.clone()));
        assert_eq!(zip_virtual_root(&last), Some(archive.clone()));
        assert_eq!(resolve_virtual_zip_child(&first), Some((archive.clone(), 0)));
        assert_eq!(resolve_virtual_zip_child(&last), Some((archive.clone(), 2)));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn home_and_end_stay_inside_current_listed_virtual_folder() {
        let root = make_temp_dir();
        let listed = root.join("pages.wmltxt");
        let page1 = root.join("001.png");
        let page2 = root.join("002.png");
        let page3 = root.join("003.png");

        fs::write(&page1, []).unwrap();
        fs::write(&page2, []).unwrap();
        fs::write(&page3, []).unwrap();
        fs::write(
            &listed,
            format!(
                "#!WMLViewer2 ListedFile 1.0\n{}\n{}\n{}\n",
                page1.display(),
                page2.display(),
                page3.display()
            ),
        )
        .unwrap();

        let mut cache = FilesystemCache::default();
        let listed_children = build_listed_virtual_children(&listed);
        assert_eq!(listed_children.len(), 3);

        let mut nav = FileNavigator::from_current_path(listed_children[1].clone(), &mut cache);
        let first = nav.first(&mut cache).expect("first listed entry");
        let last = nav.last(&mut cache).expect("last listed entry");

        assert_eq!(listed_virtual_root(&first), Some(listed.clone()));
        assert_eq!(listed_virtual_root(&last), Some(listed.clone()));
        assert_eq!(resolve_start_path(&first), Some(page1));
        assert_eq!(resolve_start_path(&last), Some(page3));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn listed_file_cache_is_refreshed_after_file_update() {
        let root = make_temp_dir();
        let listed = root.join("pages.wmltxt");
        let page1 = root.join("001.png");
        let page2 = root.join("002.png");
        let page3 = root.join("003.png");

        fs::write(&page1, []).unwrap();
        fs::write(&page2, []).unwrap();
        fs::write(&page3, []).unwrap();
        fs::write(
            &listed,
            format!(
                "#!WMLViewer2 ListedFile 1.0\n{}\n{}\n",
                page1.display(),
                page2.display()
            ),
        )
        .unwrap();

        let mut cache = FilesystemCache::default();
        let first = cache.supported_entries(&listed);
        assert_eq!(first.len(), 2);

        fs::write(
            &listed,
            format!(
                "#!WMLViewer2 ListedFile 1.0\n{}\n{}\n{}\n",
                page1.display(),
                page2.display(),
                page3.display()
            ),
        )
        .unwrap();

        let second = cache.supported_entries(&listed);
        assert_eq!(second.len(), 3);
        assert!(second.iter().any(|entry| resolve_start_path(entry) == Some(page3.clone())));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn listed_virtual_child_rebases_to_same_actual_file_after_update() {
        let root = make_temp_dir();
        let listed = root.join("pages.wmltxt");
        let page1 = root.join("001.png");
        let page2 = root.join("002.png");
        let page3 = root.join("003.png");

        fs::write(&page1, []).unwrap();
        fs::write(&page2, []).unwrap();
        fs::write(&page3, []).unwrap();
        fs::write(
            &listed,
            format!(
                "#!WMLViewer2 ListedFile 1.0\n{}\n{}\n",
                page1.display(),
                page2.display()
            ),
        )
        .unwrap();

        let mut cache = FilesystemCache::default();
        let before = cache.supported_entries(&listed);
        let old_page2 = before
            .into_iter()
            .find(|entry| resolve_start_path(entry) == Some(page2.clone()))
            .expect("old page2 entry");

        fs::write(
            &listed,
            format!(
                "#!WMLViewer2 ListedFile 1.0\n{}\n{}\n{}\n",
                page1.display(),
                page3.display(),
                page2.display()
            ),
        )
        .unwrap();

        let rebased =
            resolve_navigation_entry_path(&old_page2).expect("rebased entry should exist");
        assert_eq!(resolve_start_path(&rebased), Some(page2));
        assert_ne!(rebased, old_page2);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn plugin_enabled_extensions_are_visible_to_filer() {
        let _guard = plugin_runtime_lock()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        set_runtime_plugin_config(PluginConfig {
            internal_priority: 300,
            ffmpeg: PluginProviderConfig {
                enable: true,
                priority: 100,
                search_path: Vec::new(),
                modules: vec![PluginModuleConfig {
                    enable: true,
                    path: None,
                    plugin_name: "ffmpeg".to_string(),
                    plugin_type: "image".to_string(),
                    ext: vec![PluginExtensionConfig {
                        enable: true,
                        mime: vec!["image/avif".to_string()],
                        modules: vec![PluginCapabilityConfig {
                            capability_type: "decode".to_string(),
                            priority: "high".to_string(),
                        }],
                    }],
                }],
            },
            ..PluginConfig::default()
        });

        assert!(is_supported_image(Path::new("sample.avif")));
    }

    #[test]
    fn browser_listing_includes_webp_files() {
        let dir = make_temp_dir();
        let webp = dir.join("network_like.webp");
        let png = dir.join("other.png");
        let txt = dir.join("note.txt");

        fs::write(&webp, []).unwrap();
        fs::write(&png, []).unwrap();
        fs::write(&txt, []).unwrap();

        let entries = list_browser_entries(&dir, NavigationSortOption::OsName);
        assert!(entries.contains(&webp));
        assert!(entries.contains(&png));
        assert!(!entries.contains(&txt));

        let _ = fs::remove_dir_all(dir);
    }
}
