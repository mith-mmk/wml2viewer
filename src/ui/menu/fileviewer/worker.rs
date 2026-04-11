use crate::filesystem::{
    browser_entry_path_from_dir_entry, compare_natural_str, compare_os_str, is_browser_container,
    list_browser_entries,
};
use crate::options::NavigationSortOption;
use crate::ui::menu::fileviewer::state::{FilerEntry, FilerMetadata, FilerSortField, NameSortMode};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

pub(crate) enum FilerCommand {
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

    thread::spawn(move || {
        while let Ok(command) = command_rx.recv() {
            let mut latest = command;
            while let Ok(next) = command_rx.try_recv() {
                latest = next;
            }
            match latest {
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
                    let result_tx = result_tx.clone();
                    thread::spawn(move || {
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            scan_directory_request(
                                &result_tx,
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
    let _ = result_tx.send(FilerResult::Reset {
        request_id,
        directory: dir.clone(),
        selected: selected.clone(),
    });

    let collected = collect_browser_entries(
        result_tx,
        request_id,
        &dir,
        sort,
        archive_as_container_in_sort,
        &filter_text,
        &extension_filter,
    );

    let mut entries = collected
        .into_iter()
        .map(|path| build_filer_entry(path, archive_as_container_in_sort))
        .collect::<Vec<_>>();
    sort_entries(
        &mut entries,
        sort_field,
        ascending,
        separate_dirs,
        name_sort_mode,
    );
    entries
}

fn collect_browser_entries(
    result_tx: &Sender<FilerResult>,
    request_id: u64,
    dir: &std::path::Path,
    sort: NavigationSortOption,
    archive_as_container_in_sort: bool,
    filter_text: &str,
    extension_filter: &str,
) -> Vec<PathBuf> {
    if !dir.is_dir() {
        let mut collected = Vec::new();
        let mut preview_chunk = Vec::new();
        for path in list_browser_entries(dir, sort) {
            let preview_entry = build_preview_entry(path.clone(), archive_as_container_in_sort);
            if !matches_filters(&preview_entry, filter_text, extension_filter) {
                continue;
            }
            collected.push(path);
            preview_chunk.push(preview_entry);
            if preview_chunk.len() >= 64 {
                let _ = result_tx.send(FilerResult::Append {
                    request_id,
                    entries: std::mem::take(&mut preview_chunk),
                });
            }
        }
        if !preview_chunk.is_empty() {
            let _ = result_tx.send(FilerResult::Append {
                request_id,
                entries: preview_chunk,
            });
        }
        return collected;
    }

    let mut collected = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir) else {
        return collected;
    };

    let mut preview_chunk = Vec::new();
    for entry in read_dir.filter_map(Result::ok) {
        let Some(path) = browser_entry_path_from_dir_entry(&entry) else {
            continue;
        };
        let preview_entry = build_preview_entry(path.clone(), archive_as_container_in_sort);
        if !matches_filters(&preview_entry, filter_text, extension_filter) {
            continue;
        }
        collected.push(path);
        preview_chunk.push(preview_entry);
        if preview_chunk.len() >= 64 {
            let _ = result_tx.send(FilerResult::Append {
                request_id,
                entries: std::mem::take(&mut preview_chunk),
            });
        }
    }
    if !preview_chunk.is_empty() {
        let _ = result_tx.send(FilerResult::Append {
            request_id,
            entries: preview_chunk,
        });
    }
    collected
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
    let label = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "(entry)".to_string());
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
    let label = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "(entry)".to_string());
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
    let text_ok = if filter_text.trim().is_empty() {
        true
    } else {
        entry
            .label
            .to_ascii_lowercase()
            .contains(&filter_text.to_ascii_lowercase())
    };
    let ext_ok = if extension_filter.trim().is_empty() {
        true
    } else {
        entry
            .path
            .extension()
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
        };
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
mod tests {
    use super::*;

    #[test]
    fn natural_sort_orders_numeric_suffixes() {
        assert_eq!(
            compare_name("テスト10.jpg", "テスト2.jpg", NameSortMode::Os),
            std::cmp::Ordering::Greater
        );
    }

    #[test]
    fn natural_sort_orders_parenthesized_numbers() {
        assert_eq!(
            compare_name("テスト(5).jpg", "テスト(43).jpg", NameSortMode::Os),
            std::cmp::Ordering::Less
        );
    }

    #[test]
    fn separate_dirs_places_containers_before_files() {
        let mut entries = vec![
            FilerEntry {
                path: PathBuf::from("b.png"),
                label: "b.png".to_string(),
                is_container: false,
                sort_as_container: false,
                metadata: FilerMetadata::default(),
            },
            FilerEntry {
                path: PathBuf::from("a"),
                label: "a".to_string(),
                is_container: true,
                sort_as_container: true,
                metadata: FilerMetadata::default(),
            },
        ];

        sort_entries(
            &mut entries,
            FilerSortField::Name,
            true,
            true,
            NameSortMode::Os,
        );

        assert!(entries[0].is_container);
        assert!(!entries[1].is_container);
    }

    #[test]
    fn descending_sort_reverses_container_names() {
        let mut entries = vec![
            FilerEntry {
                path: PathBuf::from("a"),
                label: "a".to_string(),
                is_container: true,
                sort_as_container: true,
                metadata: FilerMetadata::default(),
            },
            FilerEntry {
                path: PathBuf::from("b"),
                label: "b".to_string(),
                is_container: true,
                sort_as_container: true,
                metadata: FilerMetadata::default(),
            },
        ];

        sort_entries(
            &mut entries,
            FilerSortField::Name,
            false,
            true,
            NameSortMode::Os,
        );

        assert_eq!(entries[0].label, "b");
        assert_eq!(entries[1].label, "a");
    }
}
