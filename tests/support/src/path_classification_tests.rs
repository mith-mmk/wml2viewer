    use super::{
        bench_path_match, candidate_search_roots, is_bench_network_path, normalize_bench_path,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn normalize_bench_path_unifies_separator_and_case() {
        let normalized = normalize_bench_path(Path::new("Comics/Series"));
        assert_eq!(normalized, "comics\\series");
    }

    #[test]
    fn bench_path_match_matches_datapath_roots() {
        let Some((class_name, configured_root)) = first_datapath_root() else {
            return;
        };
        let path = PathBuf::from(&configured_root).join("archive\\test.zip");
        let entry = bench_path_match(&path).expect("bench path match");

        assert_eq!(entry.class_name, class_name);
        assert_eq!(entry.raw_root, configured_root);
    }

    #[test]
    fn bench_network_path_recognizes_configured_network_roots() {
        let Some((_, configured_root)) = first_datapath_root() else {
            return;
        };
        let positive = PathBuf::from(&configured_root).join("archive\\test.zip");
        assert!(is_bench_network_path(&positive));

        let outside = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        assert!(!is_bench_network_path(&outside));
    }

    #[test]
    fn candidate_search_roots_are_not_empty() {
        assert!(!candidate_search_roots().is_empty());
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

