use crate::benchlog::{BenchLogger, set_global_bench_logger};
use crate::configs::config::{load_app_config, load_startup_path};
use crate::configs::resourses::apply_resources;
use crate::dependent::plugins::set_runtime_plugin_config;
use crate::drawers::canvas::Canvas;
use crate::drawers::image::LoadedImage;
use crate::filesystem::{
    archive_prefers_low_io, is_browser_container, resolve_start_path, set_archive_zip_workaround,
};
use crate::options::*;
use crate::path_classification::bench_path_match;
use crate::ui::menu::fileviewer::thumbnail::set_thumbnail_workaround;
use crate::ui::viewer::ViewerApp;
use eframe::egui::{self};
use std::error::Error;
use std::path::Path;
use std::path::PathBuf;

const APP_ICON_PNG: &[u8] = include_bytes!("../resources/wml2viwer.png");

pub fn run(
    image_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    bench_enabled: bool,
    log_enabled: bool,
    bench_scenario: Option<String>,
) -> Result<(), Box<dyn Error>> {
    let config = load_app_config(config_path.as_deref()).unwrap_or_default();
    set_runtime_plugin_config(config.plugins.clone());
    set_archive_zip_workaround(config.runtime.workaround.archive.zip.clone());
    set_thumbnail_workaround(config.runtime.workaround.thumbnail.clone());
    let image_path = image_path
        .unwrap_or(load_startup_path(config_path.as_deref()).unwrap_or(std::env::current_dir()?));
    let path_exists = image_path.exists();
    let is_container = is_browser_container(&image_path);
    let can_load_directly = resolve_start_path(&image_path).is_some();
    let (navigation_path, start_path, startup_load_path, show_filer_on_start) =
        determine_startup_paths(&image_path, path_exists, is_container, can_load_directly);
    let bench_logger = if bench_enabled || log_enabled {
        let logger = BenchLogger::create()?;
        let bench_context = bench_path_context(&image_path);
        logger.log(
            "app.start",
            serde_json::json!({
                "image_path": image_path.display().to_string(),
                "config_path": config_path.as_ref().map(|path| path.display().to_string()),
                "path_exists": path_exists,
                "is_container": is_container,
                "can_load_directly": can_load_directly,
                "archive_prefers_low_io": archive_prefers_low_io(&image_path),
                "bench_path_context": bench_context,
                "bench_enabled": bench_enabled,
                "log_enabled": log_enabled,
                "bench_scenario": bench_scenario,
                "show_filer_on_start": show_filer_on_start,
                "startup_load_path": startup_load_path.as_ref().map(|path| path.display().to_string()),
                "log_path": logger.path().display().to_string(),
            }),
        );
        Some(logger)
    } else {
        None
    };
    set_global_bench_logger(bench_logger.clone());
    let image = blank_image();
    let rendered = image.clone();
    let title = format!("wml2viewer - {}", start_path.display());

    // ui::viewer::set_canvas_size(&str);
    // ui::menu::set_title(&str);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_icon(
                eframe::icon_data::from_png_bytes(APP_ICON_PNG)
                    .unwrap_or_else(|_| egui::IconData::default()),
            )
            .with_inner_size([320.0, 240.0])
            .with_min_inner_size([320.0, 240.0]),
        ..Default::default()
    };

    eframe::run_native(
        "wml2viewer",
        native_options,
        Box::new(move |cc| {
            apply_window_theme(&cc.egui_ctx, config.window.ui_theme);
            let _ = apply_resources(&cc.egui_ctx, &config.resources);
            let screen = cc.egui_ctx.input(|i| {
                i.viewport()
                    .monitor_size
                    .unwrap_or(egui::vec2(1280.0, 720.0))
            });

            let window_size = match config.window.size.clone() {
                WindowSize::Relative(ratio) => {
                    let ratio = ratio.clamp(0.1, 1.0);
                    egui::vec2(screen.x * ratio, screen.y * ratio)
                }
                WindowSize::Exact { width, height } => egui::vec2(width, height),
            };
            let window_size = egui::vec2(
                window_size.x.clamp(320.0, screen.x),
                window_size.y.clamp(240.0, screen.y),
            );

            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(window_size));

            match &config.window.start_position {
                WindowStartPosition::Center => {
                    let centered = egui::pos2(
                        ((screen.x - window_size.x) * 0.5).max(0.0),
                        ((screen.y - window_size.y) * 0.5).max(0.0),
                    );
                    cc.egui_ctx
                        .send_viewport_cmd(egui::ViewportCommand::OuterPosition(centered));
                }
                WindowStartPosition::Exact { x, y } => {
                    cc.egui_ctx
                        .send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                            *x, *y,
                        )));
                }
            }

            // Work around broken first-frame layout when the app starts in fullscreen.
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));

            Ok(Box::new(ViewerApp::new(
                cc,
                navigation_path.clone(),
                start_path,
                image,
                rendered,
                config,
                config_path.clone(),
                bench_logger.clone(),
                bench_enabled,
                bench_scenario.clone(),
                show_filer_on_start,
                startup_load_path.clone(),
            )))
        }),
    )?;

    Ok(())
}

fn determine_startup_paths(
    image_path: &Path,
    path_exists: bool,
    is_container: bool,
    can_load_directly: bool,
) -> (PathBuf, PathBuf, Option<PathBuf>, bool) {
    if path_exists || is_container || can_load_directly {
        (
            image_path.to_path_buf(),
            image_path.to_path_buf(),
            Some(image_path.to_path_buf()),
            false,
        )
    } else {
        (
            image_path.to_path_buf(),
            image_path.to_path_buf(),
            None,
            true,
        )
    }
}

fn apply_window_theme(ctx: &egui::Context, theme: WindowUiTheme) {
    match theme {
        WindowUiTheme::System => {}
        WindowUiTheme::Light => ctx.set_visuals(egui::Visuals::light()),
        WindowUiTheme::Dark => ctx.set_visuals(egui::Visuals::dark()),
    }
}

fn blank_image() -> LoadedImage {
    let canvas = Canvas::new(1, 1);
    LoadedImage {
        canvas,
        animation: Vec::new(),
        loop_count: None,
    }
}

fn bench_path_context(path: &Path) -> serde_json::Value {
    match bench_path_match(path) {
        Some(entry) => serde_json::json!({
            "class": entry.class_name,
            "configured_root": entry.raw_root,
            "matched": true,
        }),
        None => serde_json::json!({
            "class": "unclassified",
            "configured_root": serde_json::Value::Null,
            "matched": false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{bench_path_context, determine_startup_paths};
    use crate::path_classification::normalize_bench_path;
    use std::path::Path;

    #[test]
    fn normalize_bench_path_unifies_separator_and_case() {
        let normalized = normalize_bench_path(Path::new("F:/Comics/Series"));
        assert_eq!(normalized, "f:\\comics\\series");
    }

    #[test]
    fn bench_path_context_matches_datapath_roots() {
        let value = bench_path_context(Path::new("F:\\benchmark\\archive\\test.zip"));

        assert_eq!(value.get("matched").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(value.get("class").and_then(|v| v.as_str()), Some("ネットワーク"));
        assert_eq!(
            value.get("configured_root").and_then(|v| v.as_str()),
            Some("F:\\benchmark")
        );
    }

    #[test]
    fn startup_mode_directly_loads_virtual_zip_child() {
        let path = Path::new("F:\\comics\\sample.zip\\__zipv__\\00000000__001.jpg");

        let (_navigation_path, _start_path, startup_load_path, show_filer_on_start) =
            determine_startup_paths(path, false, false, true);

        assert_eq!(startup_load_path.as_deref(), Some(path));
        assert!(!show_filer_on_start);
    }

    #[test]
    fn startup_mode_shows_filer_for_missing_plain_file() {
        let path = Path::new("F:\\missing\\image.png");

        let (_navigation_path, _start_path, startup_load_path, show_filer_on_start) =
            determine_startup_paths(path, false, false, false);

        assert!(startup_load_path.is_none());
        assert!(show_filer_on_start);
    }
}
