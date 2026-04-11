use crate::configs::config::{load_app_config, load_startup_path};
use crate::configs::resourses::apply_resources;
use crate::dependent::plugins::set_runtime_plugin_config;
use crate::drawers::canvas::Canvas;
use crate::drawers::image::LoadedImage;
use crate::filesystem::{is_browser_container, set_archive_zip_workaround};
use crate::options::*;
use crate::ui::menu::fileviewer::thumbnail::set_thumbnail_workaround;
use crate::ui::viewer::ViewerApp;
use eframe::egui::{self};
use std::error::Error;
use std::path::PathBuf;

const APP_ICON_PNG: &[u8] = include_bytes!("../resources/wml2viwer.png");

pub fn run(
    image_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let config = load_app_config(config_path.as_deref()).unwrap_or_default();
    set_runtime_plugin_config(config.plugins.clone());
    set_archive_zip_workaround(config.runtime.workaround.archive.zip.clone());
    set_thumbnail_workaround(config.runtime.workaround.thumbnail.clone());
    let image_path = image_path
        .unwrap_or(load_startup_path(config_path.as_deref()).unwrap_or(std::env::current_dir()?));
    let path_exists = image_path.exists();
    let is_container = is_browser_container(&image_path);
    let (navigation_path, start_path, startup_load_path, show_filer_on_start) = if path_exists {
        (
            image_path.clone(),
            image_path.clone(),
            Some(image_path.clone()),
            false,
        )
    } else if is_container {
        (
            image_path.clone(),
            image_path.clone(),
            Some(image_path.clone()),
            false,
        )
    } else {
        (image_path.clone(), image_path.clone(), None, true)
    };
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
                show_filer_on_start,
                startup_load_path.clone(),
            )))
        }),
    )?;

    Ok(())
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
