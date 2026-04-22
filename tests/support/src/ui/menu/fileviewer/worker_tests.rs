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
        let mut paths = vec![
            PathBuf::from("pack10.zip"),
            PathBuf::from("pack2.zip"),
            PathBuf::from("pack1.zip"),
        ];
        sort_paths_for_navigation(&mut paths, NavigationSortOption::OsName);
        let labels = paths
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["pack1.zip", "pack2.zip", "pack10.zip"]);
    }

