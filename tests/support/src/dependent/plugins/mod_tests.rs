    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_data_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data")
    }

    fn runtime_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn sample_path(name: &str) -> PathBuf {
        test_data_root().join("samples").join(name)
    }

    fn plugin_path(provider: &str) -> PathBuf {
        test_data_root().join("plugins").join(provider)
    }

    fn make_temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = test_data_root();
        fs::create_dir_all(&base).unwrap();
        let dir = base.join(format!(".test_plugins_{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn candidate_order_prefers_high_priority_plugin_over_internal() {
        let input = DecodeInput {
            mime: Some("image/avif"),
        };
        let mut config = PluginConfig::default();
        config.ffmpeg.enable = true;
        config.ffmpeg.modules = vec![PluginModuleConfig {
            enable: true,
            path: Some(plugin_path("ffmpeg").join("ffmpeg.exe")),
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
        }];
        let candidates = decode_candidates(&config, &input);
        assert_eq!(
            candidates.first().map(|candidate| candidate.provider),
            Some(ProviderKind::Ffmpeg)
        );
    }

    #[test]
    fn discovers_ffmpeg_modules_from_test_plugins() {
        let dir = make_temp_dir();
        let ffmpeg_exe = dir.join("ffmpeg-test.exe");
        fs::write(&ffmpeg_exe, b"").unwrap();
        fs::write(dir.join("readme.txt"), b"").unwrap();
        let config = PluginProviderConfig {
            enable: true,
            priority: 100,
            search_path: vec![dir.clone()],
            modules: Vec::new(),
        };
        let modules = discover_plugin_modules("ffmpeg", &config);
        assert!(
            modules
                .iter()
                .any(|module| module.path.as_ref() == Some(&ffmpeg_exe))
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn ffmpeg_decodes_avif_sample() {
        let _guard = runtime_lock()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config = PluginConfig {
            ffmpeg: PluginProviderConfig {
                enable: true,
                priority: 100,
                search_path: vec![plugin_path("ffmpeg")],
                modules: Vec::new(),
            },
            ..PluginConfig::default()
        };
        if discover_plugin_modules("ffmpeg", &config.ffmpeg).is_empty() {
            return;
        }
        let sample = sample_path("WML2Viewer.avif");
        if !sample.exists() {
            return;
        }
        set_runtime_plugin_config(config);
        let decoded = decode_image_from_file_with_plugins(&sample);
        assert!(decoded.is_some());
        let decoded = decoded.unwrap();
        assert!(decoded.canvas.width() > 0);
        assert!(decoded.canvas.height() > 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn susie64_decodes_jp2_sample() {
        let _guard = runtime_lock()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let config = PluginConfig {
            susie64: PluginProviderConfig {
                enable: true,
                priority: 100,
                search_path: vec![plugin_path("susie64")],
                modules: Vec::new(),
            },
            ..PluginConfig::default()
        };
        if discover_plugin_modules("susie64", &config.susie64).is_empty() {
            return;
        }
        let sample = sample_path("WML2Viewer.jp2");
        if !sample.exists() {
            return;
        }
        set_runtime_plugin_config(config);
        let decoded = decode_image_from_file_with_plugins(&sample);
        assert!(decoded.is_some());
        let decoded = decoded.unwrap();
        assert!(decoded.canvas.width() > 0);
        assert!(decoded.canvas.height() > 0);
    }

