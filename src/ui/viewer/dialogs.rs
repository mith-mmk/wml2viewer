use super::*;

impl ViewerApp {
    pub(super) fn color_image_from_canvas(&self, canvas: &Canvas) -> egui::ColorImage {
        let mut image = canvas_to_color_image(canvas);
        if self.options.grayscale {
            for pixel in &mut image.pixels {
                let luma = (0.299 * pixel.r() as f32
                    + 0.587 * pixel.g() as f32
                    + 0.114 * pixel.b() as f32)
                    .round()
                    .clamp(0.0, 255.0) as u8;
                *pixel = egui::Color32::from_rgba_unmultiplied(luma, luma, luma, pixel.a());
            }
        }
        image
    }

    pub(crate) fn open_save_dialog(&mut self) {
        self.save_dialog.open = true;
    }

    pub(super) fn poll_save_result(&mut self) {
        let Some(rx) = &self.save_dialog.result_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(message)) => {
                self.save_dialog.message = Some(message);
                self.save_dialog.in_progress = false;
                self.save_dialog.open = false;
                self.save_dialog.result_rx = None;
            }
            Ok(Err(message)) => {
                self.save_dialog.message = Some(message);
                self.save_dialog.in_progress = false;
                self.save_dialog.result_rx = None;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {
                self.save_dialog.message = Some("Save worker disconnected".to_string());
                self.save_dialog.in_progress = false;
                self.save_dialog.result_rx = None;
            }
        }
    }

    pub(super) fn save_dialog_ui(&mut self, ctx: &egui::Context) {
        if !self.save_dialog.open {
            return;
        }

        let mut open = self.save_dialog.open;
        let mut close_requested = false;
        egui::Window::new(self.text(UiTextKey::Save))
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(self.text(UiTextKey::Directory));
                    ui.label(
                        self.save_dialog
                            .output_dir
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| self.text(UiTextKey::NotSelected).to_string()),
                    );
                });
                if ui.button(self.text(UiTextKey::ChooseFolder)).clicked() {
                    self.save_dialog.output_dir =
                        pick_save_directory().or_else(default_download_dir);
                    if self.storage.path_record {
                        self.storage.path = self.save_dialog.output_dir.clone();
                        self.persist_config_async();
                    }
                }
                ui.horizontal(|ui| {
                    ui.label(self.text(UiTextKey::NameLabel));
                    ui.add_enabled_ui(!self.save_dialog.in_progress, |ui| {
                        ui.text_edit_singleline(&mut self.save_dialog.file_name);
                    });
                });
                ui.horizontal(|ui| {
                    ui.label(self.text(UiTextKey::Format));
                    ui.add_enabled_ui(!self.save_dialog.in_progress, |ui| {
                        egui::ComboBox::from_id_salt("save_format_dialog")
                            .selected_text(self.save_dialog.format.to_string())
                            .show_ui(ui, |ui| {
                                for format in SaveFormat::all() {
                                    ui.selectable_value(
                                        &mut self.save_dialog.format,
                                        format,
                                        format.to_string(),
                                    );
                                }
                            });
                    });
                });
                if self.save_dialog.in_progress {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        let dots = ".".repeat((self.frame_counter % 3) + 1);
                        ui.label(format!("Waiting{dots}"));
                    });
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            !self.save_dialog.in_progress,
                            egui::Button::new(self.text(UiTextKey::Save)),
                        )
                        .clicked()
                    {
                        self.save_current_as(self.save_dialog.format);
                    }
                    if ui.button(self.text(UiTextKey::Cancel)).clicked() {
                        close_requested = true;
                    }
                });
            });
        if close_requested {
            open = false;
        }
        self.save_dialog.open = open;
    }

    pub(super) fn execute_file_action_dialog(&mut self) {
        let Some(mode) = self.file_action_dialog.mode else {
            return;
        };
        let target = self.current_navigation_path.clone();
        let action = match mode {
            FileActionDialogMode::Move => ViewerAction::MoveFile,
            FileActionDialogMode::Copy => ViewerAction::CopyFile,
            FileActionDialogMode::Delete => ViewerAction::DeleteFile,
            FileActionDialogMode::Rename => ViewerAction::RenameFile,
        };

        let params = match mode {
            FileActionDialogMode::Move | FileActionDialogMode::Copy => {
                let destination = self.file_action_dialog.destination_path_input.trim();
                if destination.is_empty() {
                    self.open_dialog_with_title_key(
                        UiTextKey::AlertTitle,
                        self.text(UiTextKey::DestinationPathEmpty).to_string(),
                    );
                    return;
                }
                FunctionParams {
                    destination_path: Some(PathBuf::from(destination)),
                    rename_to: None,
                }
            }
            FileActionDialogMode::Delete => FunctionParams::default(),
            FileActionDialogMode::Rename => {
                let stem = self.file_action_dialog.rename_stem_input.trim();
                if stem.is_empty() {
                    self.open_dialog_with_title_key(
                        UiTextKey::AlertTitle,
                        self.text(UiTextKey::RenameTargetEmpty).to_string(),
                    );
                    return;
                }
                let rename_to = if self.file_action_dialog.rename_extension.is_empty() {
                    stem.to_string()
                } else {
                    format!("{}.{}", stem, self.file_action_dialog.rename_extension)
                };
                FunctionParams {
                    destination_path: None,
                    rename_to: Some(rename_to),
                }
            }
        };

        match call_fanction_for_action(&target, action, params.clone()) {
            Some(Ok(message)) => {
                if matches!(mode, FileActionDialogMode::Move | FileActionDialogMode::Copy) {
                    let destination =
                        PathBuf::from(self.file_action_dialog.destination_path_input.trim());
                    match mode {
                        FileActionDialogMode::Move => match self.file_action.active_move_slot {
                            crate::options::FileActionSlot::Folder1 => {
                                self.file_action.move_folder1 = Some(destination);
                            }
                            crate::options::FileActionSlot::Folder2 => {
                                self.file_action.move_folder2 = Some(destination);
                            }
                        },
                        FileActionDialogMode::Copy => match self.file_action.active_copy_slot {
                            crate::options::FileActionSlot::Folder1 => {
                                self.file_action.copy_folder1 = Some(destination);
                            }
                            crate::options::FileActionSlot::Folder2 => {
                                self.file_action.copy_folder2 = Some(destination);
                            }
                        },
                        _ => {}
                    }
                    self.persist_config_async();
                }
                self.file_action_dialog.open = false;
                self.file_action_dialog.mode = None;
                self.save_dialog.message = Some(message);

                match mode {
                    FileActionDialogMode::Copy => {
                        self.refresh_current_filer_directory();
                    }
                    FileActionDialogMode::Rename => {
                        let mut renamed_path = target.clone();
                        let stem = self.file_action_dialog.rename_stem_input.trim();
                        let file_name = if self.file_action_dialog.rename_extension.is_empty() {
                            stem.to_string()
                        } else {
                            format!("{}.{}", stem, self.file_action_dialog.rename_extension)
                        };
                        renamed_path.set_file_name(file_name);
                        let _ = self.request_load_path(renamed_path.clone());
                        self.refresh_current_filer_directory();
                    }
                    FileActionDialogMode::Move | FileActionDialogMode::Delete => {
                        let _ = self.next_image();
                        self.refresh_current_filer_directory();
                    }
                }
            }
            Some(Err(err)) => {
                self.open_dialog_with_title_key(UiTextKey::AlertTitle, err);
            }
            None => {
                self.open_dialog_with_title_key(
                    UiTextKey::AlertTitle,
                    self.text(UiTextKey::UnsupportedFilesystemAction).to_string(),
                );
            }
        }
    }

    pub(super) fn file_action_dialog_ui(&mut self, ctx: &egui::Context) {
        if !self.file_action_dialog.open {
            return;
        }

        let Some(mode) = self.file_action_dialog.mode else {
            self.file_action_dialog.open = false;
            return;
        };

        let mut open = self.file_action_dialog.open;
        let mut apply_requested = false;
        let mut close_requested = false;
        let title = match mode {
            FileActionDialogMode::Move => self.text(UiTextKey::MoveItem),
            FileActionDialogMode::Copy => self.text(UiTextKey::CopyItem),
            FileActionDialogMode::Delete => self.text(UiTextKey::DeleteItem),
            FileActionDialogMode::Rename => self.text(UiTextKey::RenameItem),
        };
        egui::Window::new(title)
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(format!(
                    "{}: {}",
                    self.text(UiTextKey::FileActionTarget),
                    self.current_navigation_path.display()
                ));
                match mode {
                    FileActionDialogMode::Move | FileActionDialogMode::Copy => {
                        ui.separator();
                        ui.label(self.text(UiTextKey::Directory));
                        ui.horizontal(|ui| {
                            ui.text_edit_singleline(
                                &mut self.file_action_dialog.destination_path_input,
                            );
                            if ui.button(self.text(UiTextKey::Browse)).clicked() {
                                if let Some(path) = pick_save_directory() {
                                    self.file_action_dialog.destination_path_input =
                                        path.to_string_lossy().into_owned();
                                }
                            }
                        });
                    }
                    FileActionDialogMode::Delete => {
                        ui.separator();
                        ui.label(self.text(UiTextKey::DeleteWithTrashWarning));
                        ui.label(self.text(UiTextKey::DeleteConfirmQuestion));
                    }
                    FileActionDialogMode::Rename => {
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(self.text(UiTextKey::NameLabel));
                            ui.text_edit_singleline(&mut self.file_action_dialog.rename_stem_input);
                            if !self.file_action_dialog.rename_extension.is_empty() {
                                ui.label(format!(".{}", self.file_action_dialog.rename_extension));
                            }
                        });
                        ui.label(self.text(UiTextKey::RenameExtensionFixed));
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(self.text(UiTextKey::Apply)).clicked() {
                        apply_requested = true;
                    }
                    if ui.button(self.text(UiTextKey::Cancel)).clicked() {
                        close_requested = true;
                    }
                });
            });

        if apply_requested {
            self.execute_file_action_dialog();
        }
        if close_requested {
            open = false;
            self.file_action_dialog.mode = None;
        }
        self.file_action_dialog.open = open;
    }

    pub(super) fn status_panel_ui(&mut self, ctx: &egui::Context) {
        let Some(message) = &self.save_dialog.message else {
            return;
        };

        egui::TopBottomPanel::bottom("status_overlay")
            .resizable(false)
            .exact_height(24.0)
            .show(ctx, |ui| {
                let text = ellipsize_end(message, 160);
                ui.horizontal(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(text).small());
                });
            });
    }

    pub(super) fn loading_overlay_ui(&mut self, ctx: &egui::Context) {
        let Some(message) = &self.overlay.loading_message else {
            return;
        };
        egui::TopBottomPanel::bottom("loading_overlay")
            .resizable(false)
            .exact_height(24.0)
            .show(ctx, |ui| {
                let text = ellipsize_end(message, 160);
                ui.horizontal(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(text).small());
                });
            });
    }

    pub(super) fn loading_card_ui(&self, ctx: &egui::Context) {
        if !self.current_texture_is_default {
            return;
        }
        if self.empty_mode {
            return;
        }
        if self.active_request.is_none() && self.active_fs_request_id.is_none() {
            return;
        }
        let Some(loading_started_at) = self.overlay.loading_started_at else {
            return;
        };
        let elapsed = loading_started_at.elapsed();
        if elapsed < WAITING_CARD_DELAY {
            ctx.request_repaint_after(WAITING_CARD_DELAY - elapsed);
            return;
        }

        egui::Area::new("viewer_waiting_card".into())
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::window(ui.style())
                    .corner_radius(12.0)
                    .show(ui, |ui| {
                        ui.set_min_width(220.0);
                        ui.vertical_centered(|ui| {
                            ui.add(egui::Spinner::new().size(22.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(loading_card_message(
                                    self.overlay.loading_message.as_deref(),
                                ))
                                .strong(),
                            );
                        });
                    });
            });
    }

    pub(super) fn alert_dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.overlay.dialog.clone() else {
            return;
        };

        let mut open = true;
        let mut close_requested = false;
        egui::Window::new(dialog.title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(dialog.message);
                if ui.button(self.text(UiTextKey::Close)).clicked() {
                    close_requested = true;
                }
            });
        if close_requested || !open {
            self.overlay.dialog = None;
            self.cancel_pending_single_click_navigation();
            self.suppress_next_pointer_intent = true;
        }
    }

}

