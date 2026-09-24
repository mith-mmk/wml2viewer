use crate::filesystem::{
    browser_entry_display_name, browser_entry_path_from_dir_entry, compare_natural_str,
    compare_os_str, is_browser_container, list_browser_entries,
};
use crate::options::NavigationSortOption;
use crate::ui::menu::fileviewer::state::{FilerEntry, FilerMetadata, FilerSortField, NameSortMode};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub(crate) enum FilerCommand {
    Cancel {
        request_id: u64,
    },
    OpenDirectory {
        request_id: u64,
        dir: PathBuf,
        sort: NavigationSortOption,
        selected: Option<PathBuf>,
        sort_field: FilerSortField,
        ascending: bool,
        separate_dirs: bool,
        archive_as_container_in_sort: bool,
        filter_text: String,
        extension_filter: String,
        name_sort_mode: NameSortMode,
    },
}

pub(crate) enum FilerResult {
    Reset {
        request_id: u64,
        directory: PathBuf,
        selected: Option<PathBuf>,
    },
    Append {
        request_id: u64,
        entries: Vec<FilerEntry>,
    },
    Snapshot {
        request_id: u64,
        directory: PathBuf,
        entries: Vec<FilerEntry>,
        selected: Option<PathBuf>,
    },
}

pub(crate) fn spawn_filer_worker() -> (Sender<FilerCommand>, Receiver<FilerResult>) {
    let (command_tx, command_rx) = mpsc::channel::<FilerCommand>();
    let (result_tx, result_rx) = mpsc::channel::<FilerResult>();
    let latest_request_id = Arc::new(AtomicU64::new(0));

    thread::spawn(move || {
        while let Ok(command) = command_rx.recv() {
            let mut latest = command;
            while let Ok(next) = command_rx.try_recv() {
                latest = next;
            }
            match latest {
                FilerCommand::Cancel { request_id } => {
                    latest_request_id.store(request_id, Ordering::Relaxed);
                }
                FilerCommand::OpenDirectory {
                    request_id,
                    dir,
                    sort,
                    selected,
                    sort_field,
                    ascending,
                    separate_dirs,
                    archive_as_container_in_sort,
                    filter_text,
                    extension_filter,
                    name_sort_mode,
                } => {
                    latest_request_id.store(request_id, Ordering::Relaxed);
                    let result_tx = result_tx.clone();
                    let latest_request_id = latest_request_id.clone();
                    thread::spawn(move || {
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            scan_directory_request(
                                &result_tx,
                                &latest_request_id,
                                request_id,
                                dir.clone(),
                                sort,
                                selected.clone(),
                                sort_field,
                                ascending,
                                separate_dirs,
                                archive_as_container_in_sort,
                                filter_text,
                                extension_filter,
                                name_sort_mode,
                            )
                        }));
                        let entries = match result {
                            Ok(entries) => entries,
                            Err(_) => Vec::new(),
                        };
                        if request_is_stale(&latest_request_id, request_id) {
                            return;
                        }
                        let _ = result_tx.send(FilerResult::Snapshot {
                            request_id,
                            directory: dir,
                            entries,
                            selected,
                        });
                    });
                }
            }
        }
    });

    (command_tx, result_rx)
}

fn scan_directory_request(
    result_tx: &Sender<FilerResult>,
    latest_request_id: &AtomicU64,
    request_id: u64,
    dir: PathBuf,
    sort: NavigationSortOption,
    selected: Option<PathBuf>,
    sort_field: FilerSortField,
    ascending: bool,
    separate_dirs: bool,
    archive_as_container_in_sort: bool,
    filter_text: String,
    extension_filter: String,
    name_sort_mode: NameSortMode,
) -> Vec<FilerEntry> {
    if request_is_stale(latest_request_id, request_id) {
        return Vec::new();
    }
    let _ = result_tx.send(FilerResult::Reset {
        request_id,
        directory: dir.clone(),
        selected: selected.clone(),
    });

    if dir.is_dir() {
        return scan_real_directory_request(
            result_tx,
            latest_request_id,
            request_id,
            &dir,
            sort_field,
            ascending,
            separate_dirs,
            archive_as_container_in_sort,
            &filter_text,
            &extension_filter,
            name_sort_mode,
        );
    }

    let collected = collect_browser_entries(
        result_tx,
        latest_request_id,
        request_id,
        &dir,
        sort,
        archive_as_container_in_sort,
        &filter_text,
        &extension_filter,
    );
    if request_is_stale(latest_request_id, request_id) {
        return Vec::new();
    }

    let mut entries = Vec::with_capacity(collected.len());
    for path in collected {
        if request_is_stale(latest_request_id, request_id) {
            return Vec::new();
        }
        entries.push(build_filer_entry(path, archive_as_container_in_sort));
    }
    if request_is_stale(latest_request_id, request_id) {
        return Vec::new();
    }
    sort_entries(
        &mut entries,
        sort_field,
        ascending,
        separate_dirs,
        name_sort_mode,
    );
    entries
}

fn scan_real_directory_request(
    result_tx: &Sender<FilerResult>,
    latest_request_id: &AtomicU64,
    request_id: u64,
    dir: &std::path::Path,
    sort_field: FilerSortField,
    ascending: bool,
    separate_dirs: bool,
    archive_as_container_in_sort: bool,
    filter_text: &str,
    extension_filter: &str,
    name_sort_mode: NameSortMode,
) -> Vec<FilerEntry> {
    let mut entries = Vec::new();
    let mut preview_chunk = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return entries;
    };
    for dir_entry in read_dir.filter_map(Result::ok) {
        if request_is_stale(latest_request_id, request_id) {
            return Vec::new();
        }
        let Some(path) = browser_entry_path_from_dir_entry(&dir_entry) else {
            continue;
        };
        let label = browser_entry_display_name(&path);
        if !matches_filters_for_path(&label, &path, filter_text, extension_filter) {
            continue;
        }
        let entry = build_real_filer_entry(&dir_entry, path, label, archive_as_container_in_sort);
        preview_chunk.push(entry.clone());
        entries.push(entry);
        if preview_chunk.len() >= 64 {
            if request_is_stale(latest_request_id, request_id) {
                return Vec::new();
            }
            let _ = result_tx.send(FilerResult::Append {
                request_id,
                entries: std::mem::take(&mut preview_chunk),
            });
        }
    }
    if request_is_stale(latest_request_id, request_id) {
        return Vec::new();
    }
    if !preview_chunk.is_empty() {
        let _ = result_tx.send(FilerResult::Append {
            request_id,
            entries: preview_chunk,
        });
    }
    sort_entries(
        &mut entries,
        sort_field,
        ascending,
        separate_dirs,
        name_sort_mode,
    );
    entries
}

fn build_real_filer_entry(
    dir_entry: &fs::DirEntry,
    path: PathBuf,
    label: String,
    archive_as_container_in_sort: bool,
) -> FilerEntry {
    let file_type = dir_entry.file_type().ok();
    let metadata = if file_type.as_ref().is_some_and(fs::FileType::is_symlink) {
        fs::metadata(&path).ok()
    } else {
        dir_entry.metadata().ok()
    };
    let is_dir = file_type.as_ref().is_some_and(fs::FileType::is_dir)
        || metadata.as_ref().is_some_and(fs::Metadata::is_dir);
    let extension = path.extension().and_then(|ext| ext.to_str());
    let is_archive = extension.is_some_and(|ext| {
        ext.eq_ignore_ascii_case("zip")
            || ext.eq_ignore_ascii_case("lha")
            || ext.eq_ignore_ascii_case("lzh")
            || ext.eq_ignore_ascii_case("wmltxt")
    });
    let is_listed = extension.is_some_and(|ext| ext.eq_ignore_ascii_case("wmltxt"));
    let is_container = is_dir || is_archive;
    let sort_as_container = is_dir || is_listed || (archive_as_container_in_sort && is_archive);
    let metadata = metadata
        .map(|metadata| FilerMetadata {
            size: metadata.is_file().then_some(metadata.len()),
            modified: metadata.modified().ok(),
        })
        .unwrap_or_default();
    FilerEntry {
        path,
        label,
        is_container,
        sort_as_container,
        metadata,
    }
}

fn collect_browser_entries(
    result_tx: &Sender<FilerResult>,
    latest_request_id: &AtomicU64,
    request_id: u64,
    dir: &std::path::Path,
    sort: NavigationSortOption,
    archive_as_container_in_sort: bool,
    filter_text: &str,
    extension_filter: &str,
) -> Vec<PathBuf> {
    let mut collected = Vec::new();
    let mut preview_chunk = Vec::new();
    for path in list_browser_entries(dir, sort) {
        if request_is_stale(latest_request_id, request_id) {
            return Vec::new();
        }
        let preview_entry = build_preview_entry(path.clone(), archive_as_container_in_sort);
        if !matches_filters(&preview_entry, filter_text, extension_filter) {
            continue;
        }
        collected.push(path);
        preview_chunk.push(preview_entry);
        if preview_chunk.len() >= 64 {
            if request_is_stale(latest_request_id, request_id) {
                return Vec::new();
            }
            let _ = result_tx.send(FilerResult::Append {
                request_id,
                entries: std::mem::take(&mut preview_chunk),
            });
        }
    }
    if !preview_chunk.is_empty() {
        if request_is_stale(latest_request_id, request_id) {
            return Vec::new();
        }
        let _ = result_tx.send(FilerResult::Append {
            request_id,
            entries: preview_chunk,
        });
    }
    collected
}

fn request_is_stale(latest_request_id: &AtomicU64, request_id: u64) -> bool {
    latest_request_id.load(Ordering::Relaxed) != request_id
}

fn build_filer_entry(path: PathBuf, archive_as_container_in_sort: bool) -> FilerEntry {
    let metadata = fs::metadata(&path)
        .ok()
        .map(|metadata| FilerMetadata {
            size: metadata.is_file().then_some(metadata.len()),
            modified: metadata.modified().ok(),
        })
        .unwrap_or_default();
    let is_container = is_browser_container(&path);
    let sort_as_container = sort_group_is_container(&path, archive_as_container_in_sort);
    let label = browser_entry_display_name(&path);
    FilerEntry {
        path,
        label,
        is_container,
        sort_as_container,
        metadata,
    }
}

fn build_preview_entry(path: PathBuf, archive_as_container_in_sort: bool) -> FilerEntry {
    let is_container = is_browser_container(&path);
    let sort_as_container = sort_group_is_container(&path, archive_as_container_in_sort);
    let label = browser_entry_display_name(&path);
    FilerEntry {
        path,
        label,
        is_container,
        sort_as_container,
        metadata: FilerMetadata::default(),
    }
}

fn sort_group_is_container(path: &std::path::Path, archive_as_container_in_sort: bool) -> bool {
    if path.is_dir() {
        return true;
    }
    if archive_as_container_in_sort {
        return is_browser_container(path);
    }
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("wmltxt"))
        .unwrap_or(false)
}

fn matches_filters(entry: &FilerEntry, filter_text: &str, extension_filter: &str) -> bool {
    matches_filters_for_path(&entry.label, &entry.path, filter_text, extension_filter)
}

fn matches_filters_for_path(
    label: &str,
    path: &std::path::Path,
    filter_text: &str,
    extension_filter: &str,
) -> bool {
    let text_ok = if filter_text.trim().is_empty() {
        true
    } else {
        label
            .to_ascii_lowercase()
            .contains(&filter_text.to_ascii_lowercase())
    };
    let ext_ok = if extension_filter.trim().is_empty() {
        true
    } else {
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.eq_ignore_ascii_case(extension_filter.trim().trim_start_matches('.')))
            .unwrap_or(false)
    };

    text_ok && ext_ok
}

fn sort_entries(
    entries: &mut [FilerEntry],
    sort_field: FilerSortField,
    ascending: bool,
    separate_dirs: bool,
    name_sort_mode: NameSortMode,
) {
    let compare = |left: &FilerEntry, right: &FilerEntry| {
        let primary = match sort_field {
            FilerSortField::Name => compare_name(&left.label, &right.label, name_sort_mode),
            FilerSortField::Modified => left.metadata.modified.cmp(&right.metadata.modified),
            FilerSortField::Size => left.metadata.size.cmp(&right.metadata.size),
        };
        let order = if primary == std::cmp::Ordering::Equal {
            compare_name(&left.label, &right.label, name_sort_mode)
        } else {
            primary
        }
        .then_with(|| left.label.cmp(&right.label))
        .then_with(|| left.path.cmp(&right.path));
        if ascending { order } else { order.reverse() }
    };

    if !separate_dirs {
        entries.sort_by(compare);
        return;
    }

    let mut containers = entries
        .iter()
        .filter(|entry| entry.sort_as_container)
        .cloned()
        .collect::<Vec<_>>();
    let mut files = entries
        .iter()
        .filter(|entry| !entry.sort_as_container)
        .cloned()
        .collect::<Vec<_>>();
    containers.sort_by(compare);
    files.sort_by(compare);

    for (index, entry) in containers.into_iter().chain(files.into_iter()).enumerate() {
        entries[index] = entry;
    }
}

fn compare_name(left: &str, right: &str, mode: NameSortMode) -> std::cmp::Ordering {
    match mode {
        NameSortMode::Os => compare_os_str(left, right),
        NameSortMode::CaseSensitive => compare_natural_str(left, right, true),
        NameSortMode::CaseInsensitive => compare_natural_str(left, right, false),
    }
}

#[cfg(test)]
#[path = "../../../../tests/support/src/ui/menu/fileviewer/worker_tests.rs"]
mod tests;
