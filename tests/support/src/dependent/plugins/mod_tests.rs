use super::*;
use std::fs;
use std::path::PathBuf;

fn test_data_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data")
}

fn sample_path(name: &str) -> PathBuf {
    test_data_root().join("samples").join(name)
}

fn plugin_path(provider: &str) -> PathBuf {
    test_data_root().join("plugins").join(provider)
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
    let dir = crate::test_support::make_test_dir("plugins");
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
    let _guard = crate::test_support::plugin_runtime_lock()
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

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn susie64_decodes_jp2_sample() {
    let _guard = crate::test_support::plugin_runtime_lock()
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

#[test]
fn format_preferences_inherit_normalize_and_reset() {
    let mut config: PluginConfig =
        toml::from_str("internal_priority = 123\n[ffmpeg]\nenable = true\n").unwrap();
    assert!(config.formats.is_empty());
    assert!(preferred_candidates(&config, "image/avif").is_empty());
    config.formats.insert(".J2K".into(), "ffmpeg".into());
    assert_eq!(config.format_decoder("image/jp2"), Some("ffmpeg"));
    assert_eq!(
        preferred_candidates(&config, "image/jp2")[0].provider,
        ProviderKind::Ffmpeg
    );
    config.set_format_decoder("image/jp2", None);
    assert!(config.formats.is_empty());
    config.set_format_decoder("image/jpeg", Some("removed-plugin"));
    assert!(preferred_candidates(&config, "image/jpeg").is_empty());
    config.set_format_decoder("image/avif", Some("ffmpeg"));
    config.ffmpeg.enable = false;
    assert!(preferred_candidates(&config, "image/avif").is_empty());
    for (alias, expected) in [
        ("JPEG", "image/jpeg"),
        ("tif", "image/tiff"),
        ("jpf", "image/jp2"),
        ("HEIC", "image/heif"),
        ("wdp", "image/jxr"),
    ] {
        assert_eq!(canonical_format(alias), Some(expected));
    }
}

#[cfg(unix)]
#[test]
fn viewer_file_and_bytes_use_preference_then_fallback() {
    use crate::drawers::canvas::Canvas;
    use crate::drawers::image::{
        SaveFormat, load_canvas_from_bytes_with_hint, load_canvas_from_file, save_loaded_image,
    };
    use std::os::unix::fs::PermissionsExt;
    let _guard = crate::test_support::plugin_runtime_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    struct Restore(PluginConfig);
    impl Drop for Restore {
        fn drop(&mut self) {
            set_runtime_plugin_config(self.0.clone());
        }
    }
    let _restore = Restore(current_runtime_plugin_config());
    let dir = crate::test_support::make_test_dir("format-routing");
    let image = LoadedImage {
        canvas: Canvas::from_rgba(1, 1, vec![255, 0, 0, 255]).unwrap(),
        animation: vec![],
        loop_count: None,
    };
    let input = dir.join("input.jpg"); // Actual PNG takes precedence over the misleading extension.
    save_loaded_image(&input, &image, SaveFormat::Png).unwrap();
    let bytes = fs::read(&input).unwrap();
    let output = dir.join("output.bmp");
    save_loaded_image(
        &output,
        &LoadedImage {
            canvas: Canvas::from_rgba(1, 1, vec![0, 0, 255, 255]).unwrap(),
            ..image
        },
        SaveFormat::Bmp,
    )
    .unwrap();
    let marker = dir.join("called");
    let executable = dir.join("ffmpeg");
    // A deterministic decoder double: records invocation and emits a different colored pixel.
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\nfor arg do dest=\"$arg\"; done\ntouch '{}'\ncp '{}' \"$dest\"\n",
            marker.display(),
            output.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
    let mut config = PluginConfig::default();
    config.ffmpeg.enable = true;
    config.ffmpeg.search_path = vec![dir.clone()];
    config.ffmpeg.modules = vec![PluginModuleConfig {
        enable: true,
        plugin_name: "test".into(),
        ext: vec![PluginExtensionConfig {
            enable: true,
            mime: vec!["image/png".into()],
            modules: vec![],
        }],
        ..Default::default()
    }];
    set_runtime_plugin_config(config.clone());
    assert_eq!(
        load_canvas_from_file(&input).unwrap().canvas.buffer(),
        &[255, 0, 0, 255]
    );
    assert!(!marker.exists()); // Old settings still use internal first, regardless of plugin priority.
    config.set_format_decoder("image/png", Some("ffmpeg"));
    set_runtime_plugin_config(config.clone());
    assert_eq!(
        load_canvas_from_file(&input).unwrap().canvas.buffer(),
        &[0, 0, 255, 255]
    );
    assert_eq!(
        load_canvas_from_bytes_with_hint(&bytes, None)
            .unwrap()
            .canvas
            .buffer(),
        &[0, 0, 255, 255]
    );
    assert!(marker.exists());
    fs::remove_file(&executable).unwrap();
    assert_eq!(
        load_canvas_from_bytes_with_hint(&bytes, Some(&input))
            .unwrap()
            .canvas
            .buffer(),
        &[255, 0, 0, 255]
    );
    config.set_format_decoder("image/png", None);
    set_runtime_plugin_config(config);
    assert_eq!(
        load_canvas_from_file(&input).unwrap().canvas.buffer(),
        &[255, 0, 0, 255]
    );
    fs::remove_dir_all(dir).unwrap();
}
