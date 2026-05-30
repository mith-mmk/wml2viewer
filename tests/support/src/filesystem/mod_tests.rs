use super::*;
use crate::dependent::plugins::{
    PluginCapabilityConfig, PluginConfig, PluginExtensionConfig, PluginModuleConfig,
    PluginProviderConfig, set_runtime_plugin_config,
};
use oxiarc_archive::LzhWriter;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;

const TINY_PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E',
    b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
];

fn make_temp_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|path| {
                path.parent()
                    .and_then(|deps| deps.parent())
                    .map(Path::to_path_buf)
            })
        })
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .join(".test_wml2viewer");
    fs::create_dir_all(&base).unwrap();
    let dir = base.join(format!(".test_nav_{unique}"));
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

fn make_lha_with_entries(path: &Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let mut lha = LzhWriter::new(file);
    for (name, bytes) in entries {
        lha.add_file(name, bytes).unwrap();
    }
    lha.finish().unwrap();
}

#[test]
fn listed_file_is_expanded_as_virtual_children() {
    let dir = make_temp_dir();
    let before = dir.join("001_before.webp");
    let listed = dir.join("002_listedfile.wmltxt");
    let after = dir.join("003_after.gif");
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
    let listed_assets_root = make_temp_dir();
    let before = dir.join("001_before.webp");
    let listed = dir.join("002_listedfile.wmltxt");
    let after = dir.join("003_after.gif");
    let listed_assets = listed_assets_root.join("listed_assets");
    let listed_1 = listed_assets.join("listed_1.png");
    let listed_2 = listed_assets.join("listed_2.png");

    fs::write(&before, []).unwrap();
    fs::write(&after, []).unwrap();
    fs::create_dir_all(&listed_assets).unwrap();
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

    // Forward from before: listed file appears as opaque entry in flat navigation.
    // load_path resolves through the listed file to its first child.
    let NavigationOutcome::Resolved(target) =
        nav.next_with_policy(EndOfFolderOption::Next, &mut cache)
    else {
        panic!("expected listed file from next");
    };
    assert_eq!(target.navigation_path, listed);
    assert_eq!(target.load_path, listed_1);

    // After entering the listed file, navigation traverses its virtual children.
    nav.set_current_input(target.navigation_path.clone(), &mut cache);

    let NavigationOutcome::Resolved(target) =
        nav.next_with_policy(EndOfFolderOption::Next, &mut cache)
    else {
        panic!("expected second listed child");
    };
    assert!(is_virtual_listed_child(&target.navigation_path));
    assert_eq!(target.load_path, listed_2);

    nav.set_current_input(target.navigation_path.clone(), &mut cache);

    // At the end of virtual children, no adjacent directory → NoPath.
    assert!(matches!(
        nav.next_with_policy(EndOfFolderOption::Next, &mut cache),
        NavigationOutcome::NoPath
    ));

    // Backward from after: listed file appears as opaque entry.
    let mut nav = FileNavigator::from_current_path(after.clone(), &mut cache);
    let NavigationOutcome::Resolved(target) =
        nav.prev_with_policy(EndOfFolderOption::Next, &mut cache)
    else {
        panic!("expected listed file from prev");
    };
    assert_eq!(target.navigation_path, listed);
    assert_eq!(target.load_path, listed_1);

    let _ = fs::remove_dir_all(dir);
    let _ = fs::remove_dir_all(listed_assets_root);
}

#[test]
fn listed_file_prev_exits_to_previous_entry_even_if_first_item_matches_outer_file() {
    let root = make_temp_dir();
    let dir = root.join("case");
    fs::create_dir_all(&dir).unwrap();
    let listed_assets_root = make_temp_dir();
    let before = dir.join("before.webp");
    let listed = dir.join("listedfile.wmltxt");
    let after = dir.join("after.gif");
    let listed_assets = listed_assets_root.join("listed_assets");
    let listed_2 = listed_assets.join("listed_2.png");
    let listed_3 = listed_assets.join("listed_3.png");

    fs::write(&before, []).unwrap();
    fs::write(&after, []).unwrap();
    fs::create_dir_all(&listed_assets).unwrap();
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
    let listed_children = build_listed_virtual_children(&listed);
    assert_eq!(listed_children.len(), 3);
    let mut nav = FileNavigator::from_current_path(listed_children[2].clone(), &mut cache);

    let NavigationOutcome::Resolved(target) =
        nav.prev_with_policy(EndOfFolderOption::Next, &mut cache)
    else {
        panic!("expected previous listed entry");
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

    assert!(matches!(
        nav.prev_with_policy(EndOfFolderOption::Next, &mut cache),
        NavigationOutcome::NoPath
    ));

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(listed_assets_root);
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
fn lha_file_is_expanded_as_virtual_children() {
    let dir = make_temp_dir();
    let before = dir.join("before.webp");
    let archive = dir.join("images.lha");
    let after = dir.join("after.gif");

    fs::write(&before, []).unwrap();
    fs::write(&after, []).unwrap();
    make_lha_with_entries(
        &archive,
        &[
            ("001.png", b"first"),
            ("sub/002.jpg", b"second"),
            ("note.txt", b"ignored"),
        ],
    );

    let mut cache = FilesystemCache::default();
    let entries = cache.supported_entries(&dir);
    assert!(entries.contains(&before));
    assert!(entries.contains(&after));
    assert!(entries.iter().any(|entry| is_virtual_lha_child(entry)));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn lha_virtual_child_bytes_can_be_loaded() {
    let dir = make_temp_dir();
    let archive = dir.join("images.lzh");
    make_lha_with_entries(&archive, &[("001.png", b"first"), ("002.png", b"second")]);

    let children = build_lha_virtual_children(&archive);
    assert_eq!(children.len(), 2);
    assert_eq!(
        load_virtual_image_bytes(&children[0]),
        Some(b"first".to_vec())
    );
    assert_eq!(virtual_image_size(&children[1]), Some(6));

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn generated_lha_virtual_child_image_can_be_decoded() {
    let dir = make_temp_dir();
    let archive = dir.join("images.lzh");
    make_lha_with_entries(&archive, &[("001.png", TINY_PNG)]);

    let children = build_lha_virtual_children(&archive);
    assert_eq!(children.len(), 1);
    let bytes = load_virtual_image_bytes(&children[0]).expect("LZH child bytes should load");
    let image = crate::drawers::image::load_canvas_from_bytes_with_hint(&bytes, Some(&children[0]))
        .expect("LZH child image should decode");

    assert_eq!(image.canvas.width(), 1);
    assert_eq!(image.canvas.height(), 1);

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn error_lzh_sample_decodes_at_least_one_image_if_available() {
    let archive = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/errors/error.lzh");
    if !archive.exists() {
        return;
    }

    let children = build_lha_virtual_children(&archive);
    assert!(
        !children.is_empty(),
        "test_data/errors/error.lzh should expose image entries"
    );

    let decoded_count = children
        .iter()
        .filter_map(|child| {
            let bytes = load_virtual_image_bytes(child)?;
            crate::drawers::image::load_canvas_from_bytes_with_hint(&bytes, Some(child)).ok()
        })
        .count();

    assert!(
        decoded_count > 0,
        "at least one image in test_data/errors/error.lzh should extract and decode"
    );
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
fn home_and_end_in_parent_directory_do_not_dive_into_zip_children() {
    let root = make_temp_dir();
    let image_a = root.join("001.png");
    let image_b = root.join("002.png");
    let archive = root.join("inner.zip");
    fs::write(&image_a, []).unwrap();
    fs::write(&image_b, []).unwrap();
    make_zip_with_entries(&archive, &["100.png", "101.png"]);

    let mut cache = FilesystemCache::default();
    let mut nav = FileNavigator::from_current_path(image_b.clone(), &mut cache);
    let first = nav.first(&mut cache).expect("first parent image");
    let last = nav.last(&mut cache).expect("last parent image");

    assert_eq!(first, image_a);
    assert_eq!(last, image_b);
    assert!(resolve_virtual_zip_child(&first).is_none());
    assert!(resolve_virtual_zip_child(&last).is_none());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn flat_navigation_entries_do_not_expand_sibling_zip_children() {
    let root = make_temp_dir();
    let image = root.join("001.png");
    let archive = root.join("inner.zip");
    fs::write(&image, []).unwrap();
    make_zip_with_entries(&archive, &["100.png", "101.png"]);

    let mut cache = FilesystemCache::default();
    let entries = flat_container_entries(&image, &mut cache).unwrap_or_default();

    assert!(entries.contains(&image));
    assert!(entries.contains(&archive));
    assert!(!entries.iter().any(|entry| is_virtual_zip_child(entry)));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn zip_virtual_listing_respects_requested_os_sort() {
    let root = make_temp_dir();
    let archive = root.join("images.zip");
    make_zip_with_entries(&archive, &["b_10.png", "a_2.png"]);

    let entries = list_openable_entries(&archive, NavigationSortOption::OsName);
    assert_eq!(entries.len(), 2);
    assert_eq!(
        resolve_virtual_zip_child(&entries[0]),
        Some((archive.clone(), 0))
    );
    assert_eq!(resolve_virtual_zip_child(&entries[1]), Some((archive, 1)));

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
    assert_eq!(resolve_start_path(&first), Some(page1.clone()));
    assert_eq!(resolve_start_path(&last), Some(page3.clone()));
    assert_eq!(resolve_end_path(&first), Some(page1));
    assert_eq!(resolve_end_path(&last), Some(page3));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn virtual_zip_child_navigation_stays_inside_zip_entries() {
    let root = make_temp_dir();
    let archive = root.join("images.zip");
    make_zip_with_entries(&archive, &["001.png", "002.png", "003.png"]);

    let mut cache = FilesystemCache::default();
    let zip_children = build_zip_virtual_children(&archive);
    let mut nav = FileNavigator::from_current_path(zip_children[1].clone(), &mut cache);

    let next = nav.next(&mut cache).expect("next zip entry");
    let prev = nav.prev(&mut cache).expect("prev zip entry");

    assert_eq!(zip_virtual_root(&next), Some(archive.clone()));
    assert_eq!(zip_virtual_root(&prev), Some(archive.clone()));
    assert_eq!(resolve_virtual_zip_child(&next), Some((archive.clone(), 2)));
    assert_eq!(resolve_virtual_zip_child(&prev), Some((archive.clone(), 1)));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn virtual_listed_child_navigation_stays_inside_listed_entries() {
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
    let mut nav = FileNavigator::from_current_path(listed_children[1].clone(), &mut cache);

    let next = nav.next(&mut cache).expect("next listed entry");
    let prev = nav.prev(&mut cache).expect("prev listed entry");

    assert_eq!(listed_virtual_root(&next), Some(listed.clone()));
    assert_eq!(listed_virtual_root(&prev), Some(listed.clone()));
    assert_eq!(resolve_start_path(&next), Some(page3));
    assert_eq!(resolve_start_path(&prev), Some(page2));

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

    let rebased = resolve_navigation_entry_path(&old_page2).expect("rebased entry should exist");
    assert_eq!(resolve_start_path(&rebased), Some(page2));
    assert_ne!(rebased, old_page2);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn next_refreshes_stale_directory_listing_before_recursive_branch_change() {
    let parent = make_temp_dir();
    let current_dir = parent.join("000_current");
    let next_dir = parent.join("001_next");
    let current = current_dir.join("001_current.png");
    let stale_last = current_dir.join("002_last.png");
    let appended = current_dir.join("003_appended.png");
    let sibling_image = next_dir.join("000_sibling.png");

    fs::create_dir_all(&current_dir).unwrap();
    fs::create_dir_all(&next_dir).unwrap();
    fs::write(&current, []).unwrap();
    fs::write(&stale_last, []).unwrap();
    fs::write(&sibling_image, []).unwrap();

    let mut cache = FilesystemCache::default();
    let mut nav = FileNavigator::from_current_path(stale_last.clone(), &mut cache);

    fs::write(&appended, []).unwrap();

    let NavigationOutcome::Resolved(target) =
        nav.next_with_policy(EndOfFolderOption::Recursive, &mut cache)
    else {
        panic!("expected appended file from refreshed listing");
    };

    assert_eq!(target.navigation_path, appended);
    assert_eq!(target.load_path, appended);

    let _ = fs::remove_dir_all(parent);
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
