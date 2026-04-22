    use super::*;
    use crate::drawers::canvas::Canvas;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn dummy_loaded_image(width: u32, height: u32) -> LoadedImage {
        LoadedImage {
            canvas: Canvas::new(width, height),
            animation: Vec::new(),
            loop_count: None,
        }
    }

    fn dummy_preloaded_entry(path: &str) -> PreloadedEntry {
        PreloadedEntry {
            navigation_path: PathBuf::from(path),
            load_path: Some(PathBuf::from(path)),
            display: DisplayedPageState {
                source: dummy_loaded_image(4, 4),
                rendered: dummy_loaded_image(4, 4),
                texture: None,
                texture_display_scale: 1.0,
            },
        }
    }

    fn dummy_filer_entry(path: &str) -> FilerEntry {
        FilerEntry {
            path: PathBuf::from(path),
            label: path.to_string(),
            is_container: false,
            sort_as_container: false,
            metadata: Default::default(),
        }
    }

    #[test]
    fn build_settings_draft_starts_from_effective_keymap() {
        let config = AppConfig::default();
        let draft = build_settings_draft(&config);
        let defaults = crate::options::default_key_mapping();

        assert_eq!(draft.key_mapping_rows.len(), defaults.len());
        assert!(
            draft
                .key_mapping_rows
                .iter()
                .any(|row| row.binding == KeyBinding::new("F5")
                    && row.action == ViewerAction::Reload)
        );
    }

    #[test]
    fn remember_preloaded_entry_in_cache_keeps_two_most_recent_entries() {
        let mut cache = VecDeque::new();
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("a"));
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("b"));
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("c"));

        let paths = cache
            .iter()
            .map(|entry| entry.navigation_path.clone())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec![PathBuf::from("c"), PathBuf::from("b")]);
    }

    #[test]
    fn remember_preloaded_entry_in_cache_refreshes_existing_entry_recency() {
        let mut cache = VecDeque::new();
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("a"));
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("b"));
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("a"));

        let paths = cache
            .iter()
            .map(|entry| entry.navigation_path.clone())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec![PathBuf::from("a"), PathBuf::from("b")]);
    }

    #[test]
    fn should_prioritize_companion_preload_until_visible_companion_is_ready() {
        let desired = Path::new("companion");

        assert!(should_prioritize_companion_preload(
            Some(desired),
            None,
            false,
        ));
        assert!(should_prioritize_companion_preload(
            Some(desired),
            Some(desired),
            false,
        ));
        assert!(!should_prioritize_companion_preload(
            Some(desired),
            Some(desired),
            true,
        ));
        assert!(!should_prioritize_companion_preload(None, None, false));
    }

    #[test]
    fn snapshot_only_clears_refresh_user_request() {
        assert!(should_clear_filer_user_request_after_snapshot(Some(
            &FilerUserRequest::Refresh {
                directory: PathBuf::from("dir"),
                selected: None,
            },
        )));
        assert!(!should_clear_filer_user_request_after_snapshot(Some(
            &FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir"),
            },
        )));
        assert!(!should_clear_filer_user_request_after_snapshot(Some(
            &FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir\\file"),
            },
        )));
    }

    #[test]
    fn zip_to_zip_bench_plan_is_available() {
        let (name, actions) = bench_automation_plan(Some("zip_to_zip"));

        assert_eq!(name, "zip_to_zip");
        assert!(actions.contains(&BenchAction::BrowseSiblingContainer));
    }

    #[test]
    fn zip_to_zip_random_bench_plan_is_available() {
        let (name, actions) = bench_automation_plan(Some("zip_to_zip_random"));

        assert_eq!(name, "zip_to_zip_random");
        assert!(actions.contains(&BenchAction::BrowseRandomContainer));
        assert!(actions.contains(&BenchAction::SelectRandomFileFromFiler));
        assert!(actions.contains(&BenchAction::Next));
        assert!(actions.contains(&BenchAction::Prev));
        assert_eq!(
            actions
                .iter()
                .filter(|action| **action == BenchAction::BrowseRandomContainer)
                .count(),
            ZIP_TO_ZIP_RANDOM_WALK_ROUNDS,
        );
        assert_eq!(
            actions
                .iter()
                .filter(|action| **action == BenchAction::RefreshFiler)
                .count(),
            ZIP_TO_ZIP_RANDOM_WALK_ROUNDS,
        );
    }

    #[test]
    fn snapshot_does_not_clear_browse_user_request_directly() {
        assert!(!should_clear_filer_user_request_after_snapshot(Some(
            &FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir"),
            },
        )));
    }

    #[test]
    fn branch_change_requires_filesystem_reinit_after_load() {
        assert!(should_reinitialize_filesystem_after_load(
            Path::new("a.zip\\__zipv__\\0001.jpg"),
            Path::new("b.zip\\__zipv__\\0001.jpg"),
        ));
        assert!(!should_reinitialize_filesystem_after_load(
            Path::new("a.zip\\__zipv__\\0001.jpg"),
            Path::new("a.zip\\__zipv__\\0002.jpg"),
        ));
    }

    #[test]
    fn load_failure_only_auto_advances_when_current_image_failed() {
        assert!(should_advance_after_load_failure(
            Path::new("dir\\current.png"),
            Some(Path::new("dir\\current.png")),
        ));
        assert!(!should_advance_after_load_failure(
            Path::new("dir\\current.png"),
            Some(Path::new("dir\\other.png")),
        ));
        assert!(!should_advance_after_load_failure(
            Path::new("dir\\current.png"),
            None,
        ));
    }

    #[test]
    fn clears_matching_filer_select_request_for_current_path() {
        assert!(should_clear_filer_select_request_for_current(
            Some(&FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir\\current.png"),
            }),
            Path::new("dir\\current.png"),
        ));
        assert!(!should_clear_filer_select_request_for_current(
            Some(&FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir\\other.png"),
            }),
            Path::new("dir\\current.png"),
        ));
        assert!(!should_clear_filer_select_request_for_current(
            Some(&FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir"),
            }),
            Path::new("dir\\current.png"),
        ));
    }

    #[test]
    fn clears_stale_filer_refresh_request_after_directory_change() {
        assert!(should_clear_stale_filer_refresh_request(
            Some(&FilerUserRequest::Refresh {
                directory: PathBuf::from("dir-a"),
                selected: Some(PathBuf::from("dir-a\\current.png")),
            }),
            Some(Path::new("dir-b")),
        ));
        assert!(!should_clear_stale_filer_refresh_request(
            Some(&FilerUserRequest::Refresh {
                directory: PathBuf::from("dir-a"),
                selected: Some(PathBuf::from("dir-a\\current.png")),
            }),
            Some(Path::new("dir-a")),
        ));
        assert!(!should_clear_stale_filer_refresh_request(
            Some(&FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir-a\\current.png"),
            }),
            Some(Path::new("dir-b")),
        ));
        assert!(!should_clear_stale_filer_refresh_request(
            Some(&FilerUserRequest::Refresh {
                directory: PathBuf::from("dir-a"),
                selected: None,
            }),
            None,
        ));
    }

    #[test]
    fn clears_stale_committed_browse_only_when_filer_is_hidden_and_idle() {
        assert!(should_clear_stale_committed_browse_for_viewer_navigation(
            false, None,
        ));
        assert!(!should_clear_stale_committed_browse_for_viewer_navigation(
            true, None,
        ));
        assert!(!should_clear_stale_committed_browse_for_viewer_navigation(
            false,
            Some(&FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir-a"),
            }),
        ));
    }

    #[test]
    fn clears_stale_committed_browse_when_filer_is_aligned_to_current_dir() {
        assert!(should_clear_stale_committed_browse_when_filer_aligned(
            Some(Path::new("dir-a")),
            Path::new("dir-a"),
            None,
        ));
        assert!(!should_clear_stale_committed_browse_when_filer_aligned(
            Some(Path::new("dir-b")),
            Path::new("dir-a"),
            None,
        ));
        assert!(!should_clear_stale_committed_browse_when_filer_aligned(
            Some(Path::new("dir-a")),
            Path::new("dir-a"),
            Some(&FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir-a"),
            }),
        ));
    }

    #[test]
    fn clears_browse_or_refresh_request_when_filer_is_hidden() {
        assert!(should_clear_filer_request_on_hide(Some(
            &FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir-a"),
            }
        )));
        assert!(should_clear_filer_request_on_hide(Some(
            &FilerUserRequest::Refresh {
                directory: PathBuf::from("dir-a"),
                selected: None,
            }
        )));
        assert!(!should_clear_filer_request_on_hide(Some(
            &FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir-a\\current.png"),
            }
        )));
        assert!(!should_clear_filer_request_on_hide(None));
    }

    #[test]
    fn hands_off_filer_control_when_viewer_navigation_starts() {
        assert!(should_handoff_filer_control_to_viewer_navigation(
            None,
            Some(Path::new("dir-a")),
        ));
        assert!(!should_handoff_filer_control_to_viewer_navigation(
            Some(&FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir-a"),
            }),
            Some(Path::new("dir-a")),
        ));
        assert!(!should_handoff_filer_control_to_viewer_navigation(
            None, None,
        ));
    }

    #[test]
    fn cancels_browse_or_refresh_request_when_viewer_navigation_starts() {
        assert!(should_cancel_filer_request_for_viewer_navigation(Some(
            &FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir-a"),
            }
        )));
        assert!(should_cancel_filer_request_for_viewer_navigation(Some(
            &FilerUserRequest::Refresh {
                directory: PathBuf::from("dir-a"),
                selected: Some(PathBuf::from("dir-a\\a.png")),
            }
        )));
        assert!(!should_cancel_filer_request_for_viewer_navigation(Some(
            &FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir-a\\a.png"),
            }
        )));
    }

    #[test]
    fn syncs_filer_selected_with_current_only_when_aligned_and_idle() {
        assert!(should_sync_filer_selected_with_current(
            None,
            Some(Path::new("dir-a")),
            Some(Path::new("dir-a")),
        ));
        assert!(!should_sync_filer_selected_with_current(
            Some(&FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir-a"),
            }),
            Some(Path::new("dir-a")),
            Some(Path::new("dir-a")),
        ));
        assert!(!should_sync_filer_selected_with_current(
            None,
            Some(Path::new("dir-a")),
            Some(Path::new("dir-b")),
        ));
    }

    #[test]
    fn skips_edge_navigation_when_target_is_current() {
        assert!(should_skip_edge_navigation_for_same_target(
            Path::new("dir-a\\a.png"),
            Path::new("dir-a\\a.png"),
            PendingViewerNavigation::First,
        ));
        assert!(!should_skip_edge_navigation_for_same_target(
            Path::new("dir-a\\a.png"),
            Path::new("dir-a\\b.png"),
            PendingViewerNavigation::Last,
        ));
    }

    #[test]
    fn skips_edge_navigation_when_container_edge_is_already_current() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("wml2viewer-edge-noop-{unique}"));
        let container = root.join("container");
        let first = container.join("001.png");
        let last = container.join("999.png");
        fs::create_dir_all(&container).unwrap();
        fs::write(&first, []).unwrap();
        fs::write(&last, []).unwrap();

        assert!(should_skip_edge_navigation_for_same_target(
            &first,
            &container,
            PendingViewerNavigation::First,
        ));
        assert!(should_skip_edge_navigation_for_same_target(
            &last,
            &container,
            PendingViewerNavigation::Last,
        ));
        assert!(!should_skip_edge_navigation_for_same_target(
            &first,
            &container,
            PendingViewerNavigation::Last,
        ));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn applies_edge_noop_only_when_filer_is_hidden_or_aligned() {
        assert!(should_apply_edge_noop(
            PendingViewerNavigation::Last,
            false,
            Some(Path::new("parent")),
            Some(Path::new("child")),
        ));
        assert!(should_apply_edge_noop(
            PendingViewerNavigation::First,
            true,
            Some(Path::new("same")),
            Some(Path::new("same")),
        ));
        assert!(!should_apply_edge_noop(
            PendingViewerNavigation::Last,
            true,
            Some(Path::new("parent")),
            Some(Path::new("child")),
        ));
        assert!(!should_apply_edge_noop(
            PendingViewerNavigation::Next,
            true,
            Some(Path::new("same")),
            Some(Path::new("same")),
        ));
    }

    #[test]
    fn maps_filer_sort_to_navigation_sort() {
        assert_eq!(
            navigation_sort_for_filer(FilerSortField::Name, NameSortMode::Os),
            NavigationSortOption::OsName,
        );
        assert_eq!(
            navigation_sort_for_filer(FilerSortField::Name, NameSortMode::CaseSensitive),
            NavigationSortOption::NameCaseSensitive,
        );
        assert_eq!(
            navigation_sort_for_filer(FilerSortField::Name, NameSortMode::CaseInsensitive),
            NavigationSortOption::NameCaseInsensitive,
        );
        assert_eq!(
            navigation_sort_for_filer(FilerSortField::Modified, NameSortMode::Os),
            NavigationSortOption::Date,
        );
        assert_eq!(
            navigation_sort_for_filer(FilerSortField::Size, NameSortMode::CaseInsensitive),
            NavigationSortOption::Size,
        );
    }

    #[test]
    fn queues_filesystem_init_when_request_is_already_active() {
        assert!(should_queue_filesystem_init(Some(1)));
        assert!(!should_queue_filesystem_init(None));
    }

    #[test]
    fn queued_filesystem_init_is_not_overwritten_by_navigation_queue() {
        let mut queued_init = None;
        queue_filesystem_init_path(&mut queued_init, PathBuf::from("dir-a"));
        let mut queued_navigation = None;
        queue_navigation_command(
            &mut queued_navigation,
            FilesystemCommand::Next {
                request_id: 0,
                policy: EndOfFolderOption::Recursive,
            },
        );
        queue_navigation_command(
            &mut queued_navigation,
            FilesystemCommand::Prev {
                request_id: 0,
                policy: EndOfFolderOption::Recursive,
            },
        );

        assert_eq!(queued_init, Some(PathBuf::from("dir-a")));
        assert!(matches!(
            queued_navigation,
            Some(FilesystemCommand::Prev {
                policy: EndOfFolderOption::Recursive,
                ..
            })
        ));
    }

    #[test]
    fn queued_filesystem_work_prioritizes_init_before_navigation() {
        let mut queued_init = Some(PathBuf::from("dir-a"));
        let mut queued_navigation = Some(FilesystemCommand::Next {
            request_id: 0,
            policy: EndOfFolderOption::Recursive,
        });

        let first = take_next_queued_filesystem_work(&mut queued_init, &mut queued_navigation);
        let second = take_next_queued_filesystem_work(&mut queued_init, &mut queued_navigation);

        assert!(matches!(
            first,
            Some(PendingFilesystemWork::Init(path)) if path == PathBuf::from("dir-a")
        ));
        assert!(matches!(
            second,
            Some(PendingFilesystemWork::Command(FilesystemCommand::Next {
                policy: EndOfFolderOption::Recursive,
                ..
            }))
        ));
        assert!(queued_init.is_none());
        assert!(queued_navigation.is_none());
    }

    #[test]
    fn defers_companion_sync_while_primary_load_is_active() {
        assert!(should_defer_companion_sync_during_primary_load(Some(
            ActiveRenderRequest::Load(7),
        )));
        assert!(!should_defer_companion_sync_during_primary_load(Some(
            ActiveRenderRequest::Resize(7),
        )));
        assert!(!should_defer_companion_sync_during_primary_load(None));
    }

    #[test]
    fn cancels_busy_filesystem_request_for_matching_filer_select() {
        let pending = FilerUserRequest::SelectFile {
            navigation_path: PathBuf::from("dir\\current.png"),
        };

        assert!(should_cancel_filesystem_request_for_filer_select(
            Some(&pending),
            Path::new("dir\\current.png"),
            Some(7),
        ));
        assert!(!should_cancel_filesystem_request_for_filer_select(
            Some(&pending),
            Path::new("dir\\other.png"),
            Some(7),
        ));
        assert!(!should_cancel_filesystem_request_for_filer_select(
            Some(&pending),
            Path::new("dir\\current.png"),
            None,
        ));
    }

    #[test]
    fn detects_filer_snapshot_change_in_same_directory_only() {
        assert!(!filer_snapshot_changed_in_same_directory(
            None,
            Path::new("dir-a"),
            10
        ));
        assert!(!filer_snapshot_changed_in_same_directory(
            Some((Path::new("dir-a"), 10)),
            Path::new("dir-a"),
            10,
        ));
        assert!(filer_snapshot_changed_in_same_directory(
            Some((Path::new("dir-a"), 10)),
            Path::new("dir-a"),
            11,
        ));
        assert!(!filer_snapshot_changed_in_same_directory(
            Some((Path::new("dir-a"), 10)),
            Path::new("dir-b"),
            10,
        ));
    }

    #[test]
    fn reinit_snapshot_only_when_current_is_missing_or_misaligned() {
        let entries = vec![
            dummy_filer_entry("dir\\001.png"),
            dummy_filer_entry("dir\\002.png"),
        ];
        assert!(!should_reinitialize_filesystem_from_filer_snapshot(
            Path::new("dir\\001.png"),
            Some(Path::new("dir")),
            Some(Path::new("dir")),
            &entries,
            Some(Path::new("dir\\001.png")),
        ));
        assert!(should_reinitialize_filesystem_from_filer_snapshot(
            Path::new("dir\\003.png"),
            Some(Path::new("dir")),
            Some(Path::new("dir")),
            &entries,
            Some(Path::new("dir\\001.png")),
        ));
        assert!(should_reinitialize_filesystem_from_filer_snapshot(
            Path::new("dir\\001.png"),
            Some(Path::new("dir")),
            Some(Path::new("dir")),
            &entries,
            Some(Path::new("dir\\002.png")),
        ));
        assert!(!should_reinitialize_filesystem_from_filer_snapshot(
            Path::new("dir\\001.png"),
            Some(Path::new("dir-a")),
            Some(Path::new("dir-b")),
            &entries,
            Some(Path::new("dir\\001.png")),
        ));
    }

    #[test]
    fn spread_companion_path_for_navigation_uses_same_branch_neighbor() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("wml2viewer-spread-{unique}"));
        let first = root.join("001.png");
        let second = root.join("002.png");
        fs::create_dir_all(&root).unwrap();
        fs::write(&first, []).unwrap();
        fs::write(&second, []).unwrap();

        let companion =
            spread_companion_path_for_navigation(&first, NavigationSortOption::Name, 1, true);

        assert_eq!(companion.as_deref(), Some(second.as_path()));

        let _ = fs::remove_dir_all(root);
    }
