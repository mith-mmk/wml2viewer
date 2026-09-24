use super::*;
use std::sync::atomic::AtomicU64;

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

#[test]
fn request_is_stale_only_for_non_latest_request() {
    let latest_request_id = AtomicU64::new(42);

    assert!(!request_is_stale(&latest_request_id, 42));
    assert!(request_is_stale(&latest_request_id, 41));
}

#[test]
fn os_sort_orders_zip_names_naturally() {
    let mut entries = ["pack10.zip", "pack2.zip", "pack1.zip"]
        .into_iter()
        .map(|label| FilerEntry {
            path: PathBuf::from(label),
            label: label.to_string(),
            is_container: true,
            sort_as_container: true,
            metadata: FilerMetadata::default(),
        })
        .collect::<Vec<_>>();
    sort_entries(
        &mut entries,
        FilerSortField::Name,
        true,
        true,
        NameSortMode::Os,
    );
    let labels = entries
        .iter()
        .map(|entry| entry.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["pack1.zip", "pack2.zip", "pack10.zip"]);
}

#[test]
fn real_directory_preview_and_snapshot_keep_entries_and_metadata() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("wml2viewer-filer-{unique}"));
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("z.png"), [1_u8, 2, 3]).unwrap();
    std::fs::write(dir.join("a.png"), [4_u8]).unwrap();
    std::fs::create_dir(dir.join("folder")).unwrap();

    let (tx, rx) = mpsc::channel();
    let latest = AtomicU64::new(7);
    let entries = scan_real_directory_request(
        &tx,
        &latest,
        7,
        &dir,
        FilerSortField::Name,
        true,
        true,
        false,
        "",
        "",
        NameSortMode::Os,
    );
    let preview = rx
        .try_iter()
        .flat_map(|result| match result {
            FilerResult::Append { entries, .. } => entries,
            _ => Vec::new(),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.label.as_str())
            .collect::<Vec<_>>(),
        vec!["folder", "a.png", "z.png"]
    );
    let mut preview_names = preview
        .iter()
        .map(|entry| entry.label.as_str())
        .collect::<Vec<_>>();
    preview_names.sort_unstable();
    assert_eq!(preview_names, vec!["a.png", "folder", "z.png"]);
    assert_eq!(entries[1].metadata.size, Some(1));
    assert_eq!(entries[2].metadata.size, Some(3));
    std::fs::remove_dir_all(&dir).unwrap();
}
