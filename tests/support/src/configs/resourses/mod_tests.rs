    use super::resource_text_candidate_paths;
    use std::path::{Path, PathBuf};

    #[test]
    fn candidate_paths_include_builtin_and_config_i18() {
        let config_dir = PathBuf::from("C:/tmp/wml2-config");
        let paths = resource_text_candidate_paths("ja", Some(config_dir.clone()));
        assert_eq!(paths.len(), 2);
        assert_eq!(
            paths[0],
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("i18n")
                .join("ja.json")
        );
        assert_eq!(paths[1], config_dir.join("i18").join("ja.json"));
    }

    #[test]
    fn candidate_paths_fallback_to_builtin_when_config_dir_missing() {
        let paths = resource_text_candidate_paths("en", None);
        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths[0],
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("i18n")
                .join("en.json")
        );
    }

