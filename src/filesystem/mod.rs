mod browser;
mod cache;
mod listed_file;
mod navigator;
mod path;
mod protocol;
mod sort;
mod source;
mod worker;
mod zip_file;

use std::fs;
use std::io;
use std::path::Path;

use crate::dependent::default_temp_dir;

#[cfg(test)]
use crate::options::NavigationSortOption;
pub(crate) use browser::benchmark_browser_scan_cached;
pub(crate) use browser::spawn_browser_query_worker;
pub use browser::{
    BrowserEntry, BrowserMetadata, BrowserNameSortMode, BrowserScanOptions, BrowserSnapshotState,
    BrowserSortField, browser_directory_for_path, browser_selected_path_for_directory,
    compare_browser_name, scan_browser_directory_with_preview, sort_browser_entries,
};
pub(crate) use browser::{SharedBrowserWorkerState, new_shared_browser_worker_state};
#[cfg(test)]
pub(crate) use cache::{
    FilesystemCache, SharedFilesystemCache, build_listed_virtual_children,
    build_zip_virtual_children, new_shared_filesystem_cache,
};
#[cfg(not(test))]
pub(crate) use cache::{FilesystemCache, SharedFilesystemCache, new_shared_filesystem_cache};
pub use cache::{is_browser_container, list_browser_entries, list_openable_entries};
#[cfg(test)]
pub(crate) use navigator::{FileNavigator, NavigationOutcome};
pub use navigator::{
    adjacent_entry, adjacent_entry_in_current_branch, adjacent_non_container_entry,
    navigation_branch_path, resolve_navigation_entry_path,
};
pub use path::{
    archive_prefers_low_io, load_virtual_image_bytes, resolve_start_path,
    set_archive_zip_workaround, virtual_image_size,
};
#[cfg(test)]
pub(crate) use path::{
    is_supported_image, is_virtual_listed_child, is_virtual_zip_child, listed_virtual_root,
    resolve_virtual_zip_child, zip_virtual_root,
};
#[cfg(not(test))]
pub(crate) use path::{is_supported_image, listed_virtual_root, zip_virtual_root};
pub use protocol::{BrowserQuery, BrowserQueryResult, FilesystemCommand, FilesystemResult};
pub(crate) use sort::{compare_natural_str, compare_os_str, sort_paths};
pub use source::resolve_source_input_path;
pub(crate) use source::{
    OpenedImageSource, SourceSignature, open_image_source, open_image_source_with_cancel,
    source_id_for_path, source_prefers_low_io, source_signature_for_path,
};
pub(crate) use worker::spawn_filesystem_worker;
pub(crate) use zip_file::{
    ZipArchiveAccessKind, ensure_local_archive_cache, load_zip_entries_unsorted,
    probe_adjacent_supported_zip_entry, sort_zip_entries, zip_archive_policy_debug,
    zip_index_is_available,
};

pub fn clean_cache_files() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = cache::persistent_cache_path() {
        remove_cache_path(&path)?;
    }
    if let Some(path) = zip_file::archive_cache_root() {
        remove_cache_path(&path)?;
    }
    remove_cache_path(&source::http_source_cache_root())?;
    if let Some(temp_root) = default_temp_dir() {
        remove_matching_temp_files(&temp_root, source::HTTP_TEMP_PREFIX)?;
    }
    Ok(())
}

fn remove_cache_path(path: &Path) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn remove_matching_temp_files(root: &Path, prefix: &str) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.is_file() && name.starts_with(prefix) {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependent::plugins::{
        PluginCapabilityConfig, PluginConfig, PluginExtensionConfig, PluginModuleConfig,
        PluginProviderConfig, set_runtime_plugin_config,
    };
    use crate::options::EndOfFolderOption;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
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

    #[test]
    fn remove_matching_temp_files_removes_prefixed_files_only() {
        let dir = make_temp_dir();
        let target = dir.join(format!("{}123.png", source::HTTP_TEMP_PREFIX));
        let keep = dir.join("keep.txt");
        fs::write(&target, []).unwrap();
        fs::write(&keep, []).unwrap();

        remove_matching_temp_files(&dir, source::HTTP_TEMP_PREFIX).unwrap();

        assert!(!target.exists());
        assert!(keep.exists());

        let _ = fs::remove_dir_all(dir);
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
        let listed_dir = dir.join("listed");
        let before = dir.join("00000-1796047615-Maid_san.jpg.webp");
        let listed = dir.join("listedfile.wmltxt");
        let after = dir.join("sample_animation.webp.gif");
        let listed_1 = listed_dir.join("test_f16.png");
        let listed_2 = listed_dir.join("test.png");

        fs::create_dir_all(&listed_dir).unwrap();
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
        let listed_dir = dir.join("listed");
        let before = dir.join("00000-1796047615-Maid_san.jpg.webp");
        let listed = dir.join("listedfile.wmltxt");
        let after = dir.join("sample_animation.webp.gif");
        let listed_2 = listed_dir.join("test.png");
        let listed_3 = listed_dir.join("test_f16.png");

        fs::create_dir_all(&listed_dir).unwrap();
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
        assert_eq!(
            resolve_virtual_zip_child(&first),
            Some((archive.clone(), 0))
        );
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
        assert!(
            second
                .iter()
                .any(|entry| resolve_start_path(entry) == Some(page3.clone()))
        );

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
