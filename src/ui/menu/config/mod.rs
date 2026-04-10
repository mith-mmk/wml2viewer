use crate::configs::resourses::{FontSizePreset, apply_resources};
use crate::dependent::plugins::{discover_plugin_modules, set_runtime_plugin_config};
use crate::dependent::{
    clean_system_integration, default_download_dir, pick_save_directory,
    register_system_file_associations, system_locale,
};
use crate::drawers::affine::InterpolationAlgorithm;
use crate::filesystem::set_archive_zip_workaround;
use crate::options::{AppConfig, EndOfFolderOption, NavigationOptions, PaneSide};
use crate::ui::i18n::UiTextKey;
use crate::ui::menu::fileviewer::thumbnail::set_thumbnail_workaround;
use crate::ui::render::interpolation_label;
use crate::ui::viewer::options::{
    BackgroundStyle, MangaSeparatorStyle, RenderScaleMode, WindowUiTheme, ZoomOption,
};
use crate::ui::viewer::{
    SettingsDraftState, SettingsTab, ViewerApp, build_settings_draft, join_search_paths,
    parse_search_paths,
};
use eframe::egui;

impl ViewerApp {
    pub(crate) fn settings_ui(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }

        if self.settings_draft.is_none() {
            self.reset_settings_draft_to_live();
        }
        let Some(mut draft_state) = self.settings_draft.take() else {
            return;
        };

        let initial_live_plugins = self.plugins.clone();
        let mut open = self.show_settings;
        let mut apply_requested = false;
        let mut reload_requested = false;
        let mut cancel_requested = false;

        egui::Window::new(self.text(UiTextKey::Settings))
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| {
                self.settings_tab_strip(ui);
                ui.separator();

                match self.settings_tab {
                    SettingsTab::Viewer => self.settings_viewer_tab(ui, &mut draft_state),
                    SettingsTab::Plugins => self.settings_plugins_tab(ui, &mut draft_state),
                    SettingsTab::Resources => self.settings_resources_tab(ui, &mut draft_state),
                    SettingsTab::Render => self.settings_render_tab(ui, &mut draft_state),
                    SettingsTab::Window => self.settings_window_tab(ui, &mut draft_state),
                    SettingsTab::Navigation => self.settings_navigation_tab(ui, &mut draft_state),
                    SettingsTab::System => self.settings_system_tab(ui),
                }

                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(self.text(UiTextKey::Apply)).clicked() {
                        apply_requested = true;
                    }
                    if ui.button(self.text(UiTextKey::Cancel)).clicked() {
                        cancel_requested = true;
                    }
                    if ui.button(self.text(UiTextKey::Undo)).clicked() {
                        draft_state = build_settings_draft(&self.current_config());
                    }
                    if ui.button(self.text(UiTextKey::Reset)).clicked() {
                        draft_state = build_settings_draft(&AppConfig::default());
                    }
                    if ui.button(self.text(UiTextKey::ReloadCurrent)).clicked() {
                        reload_requested = true;
                    }
                    if ui.button(self.text(UiTextKey::Help)).clicked() {
                        self.open_help();
                    }
                });
            });

        if cancel_requested {
            open = false;
        }
        if reload_requested {
            let _ = self.reload_current();
        }
        if apply_requested {
            self.settings_draft = Some(draft_state.clone());
            let previous = self.current_config();
            self.apply_settings_draft(ctx);
            self.finish_settings_apply(ctx, previous, initial_live_plugins);
            open = false;
        } else if open {
            self.settings_draft = Some(draft_state);
        }

        if !open {
            self.close_settings_dialog();
        } else {
            self.show_settings = true;
        }
    }

    fn settings_tab_strip(&mut self, ui: &mut egui::Ui) {
        let viewer_text = self.text(UiTextKey::Viewer);
        let render_text = self.text(UiTextKey::Render);
        let window_text = self.text(UiTextKey::Window);
        let navigation_text = self.text(UiTextKey::Navigation);
        let plugins_text = self.text(UiTextKey::Plugins);
        let resources_text = self.text(UiTextKey::Resources);
        let system_text = self.text(UiTextKey::System);
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Viewer, viewer_text);
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Render, render_text);
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Window, window_text);
            ui.selectable_value(
                &mut self.settings_tab,
                SettingsTab::Navigation,
                navigation_text,
            );
            ui.selectable_value(&mut self.settings_tab, SettingsTab::Plugins, plugins_text);
            ui.selectable_value(
                &mut self.settings_tab,
                SettingsTab::Resources,
                resources_text,
            );
            ui.selectable_value(&mut self.settings_tab, SettingsTab::System, system_text);
        });
    }

    fn settings_viewer_tab(&mut self, ui: &mut egui::Ui, draft_state: &mut SettingsDraftState) {
        let draft = &mut draft_state.config;
        ui.group(|ui| {
            ui.checkbox(&mut draft.viewer.animation, self.text(UiTextKey::Animation));
            ui.checkbox(&mut draft.viewer.grayscale, self.text(UiTextKey::Grayscale));
            ui.checkbox(
                &mut draft.viewer.manga_mode,
                self.text(UiTextKey::MangaMode),
            );
            ui.checkbox(
                &mut draft.viewer.manga_right_to_left,
                self.text(UiTextKey::MangaRightToLeft),
            );
            ui.separator();
            ui.label(self.text(UiTextKey::Separator));
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::SeparatorStyle));
                egui::ComboBox::from_id_salt("manga_separator_style")
                    .selected_text(match draft.viewer.manga_separator.style {
                        MangaSeparatorStyle::None => self.text(UiTextKey::None),
                        MangaSeparatorStyle::Solid => self.text(UiTextKey::Solid),
                        MangaSeparatorStyle::Shadow => self.text(UiTextKey::Shadow),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.viewer.manga_separator.style,
                            MangaSeparatorStyle::None,
                            self.text(UiTextKey::None),
                        );
                        ui.selectable_value(
                            &mut draft.viewer.manga_separator.style,
                            MangaSeparatorStyle::Solid,
                            self.text(UiTextKey::Solid),
                        );
                        ui.selectable_value(
                            &mut draft.viewer.manga_separator.style,
                            MangaSeparatorStyle::Shadow,
                            self.text(UiTextKey::Shadow),
                        );
                    });
            });
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::SeparatorPixels));
                ui.add(
                    egui::DragValue::new(&mut draft.viewer.manga_separator.pixels)
                        .range(0.0..=64.0)
                        .speed(0.25),
                );
            });
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::SeparatorColor));
                ui.color_edit_button_srgba_unmultiplied(&mut draft.viewer.manga_separator.color);
            });
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::Background));
                if ui.button(self.text(UiTextKey::Black)).clicked() {
                    draft.viewer.background = BackgroundStyle::Solid([0, 0, 0, 255]);
                }
                if ui.button(self.text(UiTextKey::Gray)).clicked() {
                    draft.viewer.background = BackgroundStyle::Solid([48, 48, 48, 255]);
                }
                if ui.button(self.text(UiTextKey::Tile)).clicked() {
                    draft.viewer.background = BackgroundStyle::Tile {
                        color1: [32, 32, 32, 255],
                        color2: [80, 80, 80, 255],
                        size: 16,
                    };
                }
            });
        });
    }

    fn settings_plugins_tab(&mut self, ui: &mut egui::Ui, draft_state: &mut SettingsDraftState) {
        let draft = &mut draft_state.config;
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label("internal priority");
                ui.add(
                    egui::DragValue::new(&mut draft.plugins.internal_priority)
                        .range(-1000..=1000)
                        .speed(10.0),
                );
            });
            ui.separator();
            ui.heading("susie64");
            ui.checkbox(
                &mut draft.plugins.susie64.enable,
                self.text(UiTextKey::Enable),
            );
            ui.horizontal(|ui| {
                ui.label("priority");
                ui.add(
                    egui::DragValue::new(&mut draft.plugins.susie64.priority)
                        .range(-1000..=1000)
                        .speed(10.0),
                );
            });
            ui.label(self.text(UiTextKey::SearchPath));
            if ui
                .text_edit_singleline(&mut draft_state.susie64_search_paths_input)
                .changed()
            {
                draft.plugins.susie64.search_path =
                    parse_search_paths(&draft_state.susie64_search_paths_input);
            }
            if ui.button(self.text(UiTextKey::Browse)).clicked() {
                if let Some(path) = pick_save_directory() {
                    draft.plugins.susie64.search_path = vec![path];
                    draft_state.susie64_search_paths_input =
                        join_search_paths(&draft.plugins.susie64.search_path);
                }
            }
            if ui.button(self.text(UiTextKey::LoadModules)).clicked() {
                draft.plugins.susie64.modules =
                    discover_plugin_modules("susie64", &draft.plugins.susie64);
            }
            ui.label(format!(
                "{}: {}",
                self.text(UiTextKey::Modules),
                draft.plugins.susie64.modules.len()
            ));

            ui.separator();
            ui.heading("system");
            ui.checkbox(
                &mut draft.plugins.system.enable,
                self.text(UiTextKey::Enable),
            );
            ui.horizontal(|ui| {
                ui.label("priority");
                ui.add(
                    egui::DragValue::new(&mut draft.plugins.system.priority)
                        .range(-1000..=1000)
                        .speed(10.0),
                );
            });
            ui.label(self.text(UiTextKey::SearchPathOsApi));
            ui.label(format!(
                "{}: {}",
                self.text(UiTextKey::Modules),
                draft.plugins.system.modules.len()
            ));

            ui.separator();
            ui.heading("ffmpeg");
            ui.checkbox(
                &mut draft.plugins.ffmpeg.enable,
                self.text(UiTextKey::Enable),
            );
            ui.horizontal(|ui| {
                ui.label("priority");
                ui.add(
                    egui::DragValue::new(&mut draft.plugins.ffmpeg.priority)
                        .range(-1000..=1000)
                        .speed(10.0),
                );
            });
            ui.label(self.text(UiTextKey::SearchPath));
            if ui
                .text_edit_singleline(&mut draft_state.ffmpeg_search_paths_input)
                .changed()
            {
                draft.plugins.ffmpeg.search_path =
                    parse_search_paths(&draft_state.ffmpeg_search_paths_input);
            }
            if ui.button(self.text(UiTextKey::Browse)).clicked() {
                if let Some(path) = pick_save_directory() {
                    draft.plugins.ffmpeg.search_path = vec![path];
                    draft_state.ffmpeg_search_paths_input =
                        join_search_paths(&draft.plugins.ffmpeg.search_path);
                }
            }
            if ui.button(self.text(UiTextKey::LoadModules)).clicked() {
                draft.plugins.ffmpeg.modules =
                    discover_plugin_modules("ffmpeg", &draft.plugins.ffmpeg);
            }
            ui.label(format!(
                "{}: {}",
                self.text(UiTextKey::Modules),
                draft.plugins.ffmpeg.modules.len()
            ));
        });
    }

    fn settings_resources_tab(&mut self, ui: &mut egui::Ui, draft_state: &mut SettingsDraftState) {
        let draft = &mut draft_state.config;
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::Locale));
                ui.text_edit_singleline(&mut draft_state.resource_locale_input);
                if ui.button(self.text(UiTextKey::Auto)).clicked() {
                    draft_state.resource_locale_input = system_locale().unwrap_or_default();
                }
            });
            let trimmed = draft_state.resource_locale_input.trim();
            draft.resources.locale = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            };

            if !self.loaded_font_names.is_empty() {
                ui.label(format!(
                    "{}: {}",
                    self.text(UiTextKey::Fonts),
                    self.loaded_font_names.join(", ")
                ));
            }
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::FontSize));
                egui::ComboBox::from_id_salt("font_size")
                    .selected_text(font_size_label(draft.resources.font_size))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.resources.font_size,
                            FontSizePreset::Auto,
                            self.text(UiTextKey::Auto),
                        );
                        ui.selectable_value(&mut draft.resources.font_size, FontSizePreset::S, "S");
                        ui.selectable_value(&mut draft.resources.font_size, FontSizePreset::M, "M");
                        ui.selectable_value(&mut draft.resources.font_size, FontSizePreset::L, "L");
                        ui.selectable_value(
                            &mut draft.resources.font_size,
                            FontSizePreset::LL,
                            "LL",
                        );
                    });
            });
            ui.label(self.text(UiTextKey::SearchPath));
            if ui
                .text_edit_singleline(&mut draft_state.resource_font_paths_input)
                .changed()
            {
                draft.resources.font_paths =
                    parse_search_paths(&draft_state.resource_font_paths_input);
            }
            ui.separator();
            ui.label(self.text(UiTextKey::Workaround));
            ui.horizontal(|ui| {
                ui.label(format!("{} ZIP", self.text(UiTextKey::Archive)));
                ui.label(self.text(UiTextKey::ThresholdMb));
                ui.add(
                    egui::DragValue::new(&mut draft.runtime.workaround.archive.zip.threshold_mb)
                        .range(16..=16_384)
                        .speed(8.0),
                );
                ui.checkbox(
                    &mut draft.runtime.workaround.archive.zip.local_cache,
                    self.text(UiTextKey::LocalCache),
                );
            });
            ui.checkbox(
                &mut draft.runtime.workaround.thumbnail.suppress_large_files,
                self.text(UiTextKey::ThumbnailSuppression),
            );
        });
    }

    fn settings_render_tab(&mut self, ui: &mut egui::Ui, draft_state: &mut SettingsDraftState) {
        let draft = &mut draft_state.config;
        normalize_draft_render_options(&mut draft.render);
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::ZoomMode));
                egui::ComboBox::from_id_salt("zoom_option")
                    .selected_text(zoom_option_label(
                        &self.applied_locale,
                        &draft.render.zoom_option,
                    ))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.render.zoom_option,
                            ZoomOption::None,
                            self.text(UiTextKey::None),
                        );
                        ui.selectable_value(
                            &mut draft.render.zoom_option,
                            ZoomOption::FitWidth,
                            self.text(UiTextKey::FitWidth),
                        );
                        ui.selectable_value(
                            &mut draft.render.zoom_option,
                            ZoomOption::FitHeight,
                            self.text(UiTextKey::FitHeight),
                        );
                        ui.selectable_value(
                            &mut draft.render.zoom_option,
                            ZoomOption::FitScreen,
                            self.text(UiTextKey::FitScreen),
                        );
                        ui.selectable_value(
                            &mut draft.render.zoom_option,
                            ZoomOption::FitScreenIncludeSmaller,
                            self.text(UiTextKey::FitScreenIncludeSmaller),
                        );
                        ui.selectable_value(
                            &mut draft.render.zoom_option,
                            ZoomOption::FitScreenOnlySmaller,
                            self.text(UiTextKey::FitScreenOnlySmaller),
                        );
                    });
            });
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::ScaleMode));
                egui::ComboBox::from_id_salt("scale_mode")
                    .selected_text(match draft.render.scale_mode {
                        RenderScaleMode::FastGpu => self.text(UiTextKey::FastGpu),
                        RenderScaleMode::PreciseCpu => self.text(UiTextKey::PreciseCpu),
                    })
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(
                                &mut draft.render.scale_mode,
                                RenderScaleMode::FastGpu,
                                self.text(UiTextKey::FastGpu),
                            )
                            .changed()
                        {
                            normalize_draft_render_options(&mut draft.render);
                        }
                        ui.selectable_value(
                            &mut draft.render.scale_mode,
                            RenderScaleMode::PreciseCpu,
                            self.text(UiTextKey::PreciseCpu),
                        );
                    });
            });
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::Resize));
                egui::ComboBox::from_id_salt("zoom_method")
                    .selected_text(interpolation_label(draft.render.zoom_method))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.render.zoom_method,
                            InterpolationAlgorithm::NearestNeighber,
                            self.text(UiTextKey::Nearest),
                        );
                        ui.selectable_value(
                            &mut draft.render.zoom_method,
                            InterpolationAlgorithm::Bilinear,
                            self.text(UiTextKey::Bilinear),
                        );
                        if matches!(draft.render.scale_mode, RenderScaleMode::PreciseCpu) {
                            ui.selectable_value(
                                &mut draft.render.zoom_method,
                                InterpolationAlgorithm::BicubicAlpha(None),
                                self.text(UiTextKey::Bicubic),
                            );
                            ui.selectable_value(
                                &mut draft.render.zoom_method,
                                InterpolationAlgorithm::Lanzcos3,
                                self.text(UiTextKey::Lanczos3),
                            );
                        }
                    });
            });
        });
    }

    fn settings_window_tab(&mut self, ui: &mut egui::Ui, draft_state: &mut SettingsDraftState) {
        let draft = &mut draft_state.config;
        ui.group(|ui| {
            ui.checkbox(
                &mut draft.window.fullscreen,
                self.text(UiTextKey::Fullscreen),
            );
            ui.checkbox(
                &mut draft.window.remember_size,
                self.text(UiTextKey::RememberSize),
            );
            ui.checkbox(
                &mut draft.window.remember_position,
                self.text(UiTextKey::RememberPosition),
            );
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::Theme));
                egui::ComboBox::from_id_salt("window_theme")
                    .selected_text(match draft.window.ui_theme {
                        WindowUiTheme::System => self.text(UiTextKey::System),
                        WindowUiTheme::Light => self.text(UiTextKey::Light),
                        WindowUiTheme::Dark => self.text(UiTextKey::Dark),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.window.ui_theme,
                            WindowUiTheme::System,
                            self.text(UiTextKey::System),
                        );
                        ui.selectable_value(
                            &mut draft.window.ui_theme,
                            WindowUiTheme::Light,
                            self.text(UiTextKey::Light),
                        );
                        ui.selectable_value(
                            &mut draft.window.ui_theme,
                            WindowUiTheme::Dark,
                            self.text(UiTextKey::Dark),
                        );
                    });
            });
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::PaneSide));
                egui::ComboBox::from_id_salt("pane_side")
                    .selected_text(match draft.window.pane_side {
                        PaneSide::Left => self.text(UiTextKey::Left),
                        PaneSide::Right => self.text(UiTextKey::Right),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.window.pane_side,
                            PaneSide::Left,
                            self.text(UiTextKey::Left),
                        );
                        ui.selectable_value(
                            &mut draft.window.pane_side,
                            PaneSide::Right,
                            self.text(UiTextKey::Right),
                        );
                    });
            });
            match &mut draft.window.size {
                crate::ui::viewer::options::WindowSize::Relative(ratio) => {
                    ui.label(self.text(UiTextKey::WindowSizeRelative));
                    ui.add(egui::Slider::new(ratio, 0.2..=1.0).text("ratio"));
                    if ui.button(self.text(UiTextKey::UseExactSize)).clicked() {
                        draft.window.size = crate::ui::viewer::options::WindowSize::Exact {
                            width: self.last_viewport_size.x.max(320.0),
                            height: self.last_viewport_size.y.max(240.0),
                        };
                    }
                }
                crate::ui::viewer::options::WindowSize::Exact { width, height } => {
                    ui.label(self.text(UiTextKey::WindowSizeExact));
                    ui.add(egui::DragValue::new(width).speed(1.0).prefix("W "));
                    ui.add(egui::DragValue::new(height).speed(1.0).prefix("H "));
                    if ui.button(self.text(UiTextKey::UseRelativeSize)).clicked() {
                        draft.window.size = crate::ui::viewer::options::WindowSize::Relative(0.8);
                    }
                }
            }
        });
    }

    fn settings_navigation_tab(&mut self, ui: &mut egui::Ui, draft_state: &mut SettingsDraftState) {
        let draft = &mut draft_state.config;
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(self.text(UiTextKey::EndOfFolder));
                egui::ComboBox::from_id_salt("end_of_folder")
                    .selected_text(end_of_folder_label(
                        &self.applied_locale,
                        draft.navigation.end_of_folder,
                    ))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut draft.navigation.end_of_folder,
                            EndOfFolderOption::Stop,
                            self.text(UiTextKey::Stop),
                        );
                        ui.selectable_value(
                            &mut draft.navigation.end_of_folder,
                            EndOfFolderOption::Loop,
                            self.text(UiTextKey::Loop),
                        );
                        ui.selectable_value(
                            &mut draft.navigation.end_of_folder,
                            EndOfFolderOption::Next,
                            self.text(UiTextKey::Next),
                        );
                        ui.selectable_value(
                            &mut draft.navigation.end_of_folder,
                            EndOfFolderOption::Recursive,
                            self.text(UiTextKey::Recursive),
                        );
                    });
            });
            let remember_changed = ui
                .checkbox(
                    &mut draft.storage.path_record,
                    self.text(UiTextKey::RememberSavePath),
                )
                .changed();
            if remember_changed && draft.storage.path_record && draft.storage.path.is_none() {
                draft.storage.path = self
                    .save_dialog
                    .output_dir
                    .clone()
                    .or_else(default_download_dir)
                    .or_else(|| self.current_path.parent().map(|path| path.to_path_buf()));
            }
        });
    }

    fn settings_system_tab(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(format!(
                "{}: {}",
                self.text(UiTextKey::ProgramName),
                crate::get_prograname()
            ));
            ui.label(format!(
                "{}: {}",
                self.text(UiTextKey::Version),
                crate::get_version()
            ));
            ui.label(format!(
                "{}: {}",
                self.text(UiTextKey::Author),
                crate::get_auther()
            ));
            ui.label(format!(
                "{}: {}",
                self.text(UiTextKey::Copyright),
                crate::get_copyright()
            ));
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                if ui.button(self.text(UiTextKey::RegisterSystem)).clicked() {
                    match std::env::current_exe()
                        .ok()
                        .and_then(|exe| register_system_file_associations(&exe).ok())
                    {
                        Some(()) => {
                            self.overlay.alert_message =
                                Some(self.text(UiTextKey::RegisteredFileAssociations).to_string());
                        }
                        None => {
                            self.overlay.alert_message =
                                Some(self.text(UiTextKey::FailedFileAssociations).to_string());
                        }
                    }
                }
                if ui.button(self.text(UiTextKey::CleanSystem)).clicked() {
                    match clean_system_integration() {
                        Ok(()) => {
                            self.overlay.alert_message =
                                Some(self.text(UiTextKey::CleanedSystemIntegration).to_string());
                        }
                        Err(err) => {
                            self.overlay.alert_message = Some(err.to_string());
                        }
                    }
                }
            });
        });
    }

    fn finish_settings_apply(
        &mut self,
        ctx: &egui::Context,
        previous: AppConfig,
        initial_live_plugins: crate::dependent::plugins::PluginConfig,
    ) {
        if self.window_options.ui_theme != previous.window.ui_theme {
            self.apply_window_theme(ctx);
        }
        if self.window_options.fullscreen != previous.window.fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                self.window_options.fullscreen,
            ));
        }
        if self.window_options.pane_side != previous.window.pane_side {
            self.refresh_current_filer_directory();
        }
        if self.navigation_sort != previous.navigation.sort {
            self.respawn_filesystem_worker();
            self.refresh_current_filer_directory();
        }
        if self.render_options.zoom_option != previous.render.zoom_option {
            self.pending_fit_recalc = true;
        }
        if self.render_options.scale_mode != previous.render.scale_mode
            || self.render_options.zoom_method != previous.render.zoom_method
        {
            let _ = self.request_resize_current();
        }
        if self.options.grayscale != previous.viewer.grayscale {
            self.upload_current_frame();
        }
        if self.resources.locale != previous.resources.locale
            || self.resources.font_size != previous.resources.font_size
            || self.resources.font_paths != previous.resources.font_paths
        {
            let applied = apply_resources(ctx, &self.resources);
            self.applied_locale = applied.locale;
            self.loaded_font_names = applied.loaded_fonts;
        }
        if self.runtime.workaround.archive.zip.threshold_mb
            != previous.runtime.workaround.archive.zip.threshold_mb
            || self.runtime.workaround.archive.zip.local_cache
                != previous.runtime.workaround.archive.zip.local_cache
        {
            set_archive_zip_workaround(self.runtime.workaround.archive.zip.clone());
        }
        if self.runtime.workaround.thumbnail.suppress_large_files
            != previous.runtime.workaround.thumbnail.suppress_large_files
        {
            set_thumbnail_workaround(self.runtime.workaround.thumbnail.clone());
        }
        if self.plugins != previous.plugins {
            set_runtime_plugin_config(self.plugins.clone());
        }
        if self.plugins != initial_live_plugins {
            self.show_restart_prompt = true;
        }
    }

    pub(crate) fn restore_config(&mut self, config: AppConfig, ctx: &egui::Context) {
        self.options = config.viewer;
        self.window_options = config.window;
        self.render_options = config.render;
        self.resources = config.resources;
        self.plugins = config.plugins;
        self.storage = config.storage;
        self.runtime = config.runtime;
        self.keymap = config.input.merged_with_defaults();
        self.end_of_folder = config.navigation.end_of_folder;
        self.navigation_sort = config.navigation.sort;
        self.normalize_render_options();
        self.save_dialog.output_dir = self
            .storage
            .path
            .clone()
            .or_else(default_download_dir)
            .or_else(|| self.current_path.parent().map(|path| path.to_path_buf()));
        self.susie64_search_paths_input = join_search_paths(&self.plugins.susie64.search_path);
        self.system_search_paths_input = join_search_paths(&self.plugins.system.search_path);
        self.ffmpeg_search_paths_input = join_search_paths(&self.plugins.ffmpeg.search_path);
        self.resource_locale_input = self.resources.locale.clone().unwrap_or_default();
        self.resource_font_paths_input = join_search_paths(&self.resources.font_paths);
        let _ = ctx;
        self.pending_fit_recalc = true;
    }

    pub(crate) fn restart_prompt_ui(&mut self, ctx: &egui::Context) {
        if !self.show_restart_prompt {
            return;
        }

        let mut open = self.show_restart_prompt;
        let mut close_requested = false;
        egui::Window::new(self.text(UiTextKey::RestartRecommended))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(self.text(UiTextKey::RestartToApplyPluginChanges));
                if ui.button(self.text(UiTextKey::Close)).clicked() {
                    close_requested = true;
                }
            });
        if close_requested {
            open = false;
        }
        self.show_restart_prompt = open;
    }

    pub(crate) fn current_config(&self) -> AppConfig {
        AppConfig {
            viewer: self.options.clone(),
            window: self.window_options.clone(),
            render: self.render_options.clone(),
            plugins: self.plugins.clone(),
            storage: self.storage.clone(),
            runtime: self.runtime.clone(),
            input: Default::default(),
            resources: self.resources.clone(),
            navigation: NavigationOptions {
                end_of_folder: self.end_of_folder,
                sort: self.navigation_sort,
                archive: self.filer.archive_mode,
            },
        }
    }
}

fn end_of_folder_label(locale: &str, option: EndOfFolderOption) -> &'static str {
    match option {
        EndOfFolderOption::Stop => crate::ui::i18n::tr(locale, UiTextKey::Stop),
        EndOfFolderOption::Next => crate::ui::i18n::tr(locale, UiTextKey::Next),
        EndOfFolderOption::Loop => crate::ui::i18n::tr(locale, UiTextKey::Loop),
        EndOfFolderOption::Recursive => crate::ui::i18n::tr(locale, UiTextKey::Recursive),
    }
}

fn zoom_option_label(locale: &str, option: &ZoomOption) -> &'static str {
    match option {
        ZoomOption::None => crate::ui::i18n::tr(locale, UiTextKey::None),
        ZoomOption::FitWidth => crate::ui::i18n::tr(locale, UiTextKey::FitWidth),
        ZoomOption::FitHeight => crate::ui::i18n::tr(locale, UiTextKey::FitHeight),
        ZoomOption::FitScreen => crate::ui::i18n::tr(locale, UiTextKey::FitScreen),
        ZoomOption::FitScreenIncludeSmaller => {
            crate::ui::i18n::tr(locale, UiTextKey::FitScreenIncludeSmaller)
        }
        ZoomOption::FitScreenOnlySmaller => {
            crate::ui::i18n::tr(locale, UiTextKey::FitScreenOnlySmaller)
        }
    }
}

fn font_size_label(option: FontSizePreset) -> &'static str {
    match option {
        FontSizePreset::Auto => "Auto",
        FontSizePreset::S => "S",
        FontSizePreset::M => "M",
        FontSizePreset::L => "L",
        FontSizePreset::LL => "LL",
    }
}

fn normalize_draft_render_options(render: &mut crate::ui::viewer::options::RenderOptions) {
    if matches!(render.scale_mode, RenderScaleMode::FastGpu)
        && !matches!(
            render.zoom_method,
            InterpolationAlgorithm::NearestNeighber | InterpolationAlgorithm::Bilinear
        )
    {
        render.zoom_method = InterpolationAlgorithm::Bilinear;
    }
}
