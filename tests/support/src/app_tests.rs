    use super::{bench_path_context, determine_startup_paths};
    use crate::path_classification::normalize_bench_path;
    use std::path::{Path, PathBuf};

    #[test]
    fn normalize_bench_path_unifies_separator_and_case() {
        let normalized = normalize_bench_path(Path::new("Comics/Series"));
        assert_eq!(normalized, "comics\\series");
    }

    #[test]
    fn bench_path_context_matches_datapath_roots() {
        let Some((class_name, configured_root)) = first_datapath_root() else {
            return;
        };
        let value = bench_path_context(&PathBuf::from(&configured_root).join("archive\\test.zip"));

        assert_eq!(value.get("matched").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(
            value.get("class").and_then(|v| v.as_str()),
            Some(class_name.as_str())
        );
        assert_eq!(
            value.get("configured_root").and_then(|v| v.as_str()),
            Some(configured_root.as_str())
        );
    }

    fn first_datapath_root() -> Option<(String, String)> {
        let datapath = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(".test")
            .join("datapath.md");
        let text = std::fs::read_to_string(datapath).ok()?;
        let mut class_name: Option<String> = None;
        for line in text.lines() {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix("## ") {
                class_name = Some(value.to_string());
                continue;
            }
            let Some(root) = trimmed.strip_prefix("- ") else {
                continue;
            };
            if root.is_empty() {
                continue;
            }
            return class_name.clone().map(|class| (class, root.to_string()));
        }
        None
    }

    #[test]
    fn startup_mode_directly_loads_virtual_zip_child() {
        let path_buf = PathBuf::from("comics")
            .join("sample.zip")
            .join("__zipv__")
            .join("00000000__001.jpg");
        let path = path_buf.as_path();

        let (_navigation_path, _start_path, startup_load_path, show_filer_on_start) =
            determine_startup_paths(path, false, false, true);

        assert_eq!(startup_load_path.as_deref(), Some(path));
        assert!(!show_filer_on_start);
    }

    #[test]
    fn startup_mode_shows_filer_for_missing_plain_file() {
        let path_buf = PathBuf::from("missing").join("image.png");
        let path = path_buf.as_path();

        let (_navigation_path, _start_path, startup_load_path, show_filer_on_start) =
            determine_startup_paths(path, false, false, false);

        assert!(startup_load_path.is_none());
        assert!(show_filer_on_start);
    }

