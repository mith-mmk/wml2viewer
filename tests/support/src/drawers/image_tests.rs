use super::{LoadedImage, load_canvas_from_file, with_plugin_fallback};
use crate::dependent::plugins::{
    PluginConfig, PluginProviderConfig, discover_plugin_modules, set_runtime_plugin_config,
};
use crate::drawers::canvas::Canvas;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn tiny_image() -> LoadedImage {
    LoadedImage {
        canvas: Canvas::new(1, 1),
        animation: Vec::new(),
        loop_count: None,
    }
}

#[test]
fn failed_plugin_fallback_preserves_the_original_decoder_error() {
    let original = wml2viewer_core::CoreError::decode("synthetic decoder failure");
    let error = with_plugin_fallback(Err(Box::new(original.clone())), || None).unwrap_err();
    assert_eq!(error.to_string(), original.to_string());
    assert_eq!(
        error.downcast_ref::<wml2viewer_core::CoreError>(),
        Some(&original)
    );
}

#[test]
fn built_in_success_does_not_invoke_plugins() {
    let image = with_plugin_fallback(Ok(tiny_image()), || panic!("unexpected fallback")).unwrap();
    assert_eq!((image.canvas.width(), image.canvas.height()), (1, 1));
}

#[test]
fn successful_plugin_fallback_still_returns_its_image() {
    let image = with_plugin_fallback(
        Err(Box::new(wml2viewer_core::CoreError::decode(
            "synthetic failure",
        ))),
        || Some(tiny_image()),
    )
    .unwrap();
    assert_eq!((image.canvas.width(), image.canvas.height()), (1, 1));
}

#[cfg(target_os = "windows")]
#[test]
fn plugin_decode_is_visible_from_viewer_load_path() {
    let _guard = crate::test_support::plugin_runtime_lock()
        .lock()
        .unwrap_or_else(|p| p.into_inner());
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
