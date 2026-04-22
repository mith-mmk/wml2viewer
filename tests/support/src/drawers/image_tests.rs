    use super::load_canvas_from_file;
    use crate::dependent::plugins::{
        PluginConfig, PluginProviderConfig, discover_plugin_modules, set_runtime_plugin_config,
    };
    use std::path::PathBuf;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn plugin_decode_is_visible_from_viewer_load_path() {
        let config = PluginConfig {
            ffmpeg: PluginProviderConfig {
                enable: true,
                priority: 100,
                search_path: vec![repo_root().join("test").join("plugins").join("ffmpeg")],
                modules: Vec::new(),
            },
            ..PluginConfig::default()
        };
        if discover_plugin_modules("ffmpeg", &config.ffmpeg).is_empty() {
            return;
        }
        set_runtime_plugin_config(config);

        let decoded = load_canvas_from_file(&repo_root().join("samples").join("WML2Viewer.avif"));
        assert!(decoded.is_ok());
    }

