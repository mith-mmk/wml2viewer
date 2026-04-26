pub(crate) mod dispatch;

use crate::options::ViewerAction;
use crate::ui::input::dispatch::collect_triggered_actions;
use crate::ui::viewer::FileActionDialogMode;
use crate::ui::viewer::ViewerApp;
use eframe::egui;
use std::time::Instant;

#[derive(Debug)]
enum PointerIntent {
    ToggleFit,
    NextImageAfterDelay,
    OpenContextMenu(egui::Pos2),
}

impl ViewerApp {
    pub(crate) fn handle_keyboard(&mut self, ctx: &egui::Context) {
        if self.show_left_menu {
            self.cancel_pending_single_click_navigation();
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.show_left_menu = false;
            }
            return;
        }

        if self.overlay.dialog.is_some() {
            self.cancel_pending_single_click_navigation();
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.overlay.dialog = None;
                self.suppress_next_pointer_intent = true;
            }
            return;
        }

        if self.file_action_dialog.open {
            self.cancel_pending_single_click_navigation();
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.file_action_dialog.open = false;
                self.file_action_dialog.mode = None;
                self.suppress_next_pointer_intent = true;
            }
            return;
        }

        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::S)) {
            self.open_save_dialog();
        }

        if ctx.input(|i| i.key_pressed(egui::Key::F1)) {
            self.open_help();
            return;
        }

        if self.show_settings || self.save_dialog.open {
            if !ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                if ctx.wants_keyboard_input() {
                    return;
                }
            }
        }

        if ctx.wants_keyboard_input() {
            return;
        }

        let triggered = collect_triggered_actions(ctx, &self.keymap);
        for action in triggered {
            if self.show_settings && !matches!(action, ViewerAction::ToggleSettings) {
                continue;
            }
            self.log_bench_state(
                "viewer.input_action",
                serde_json::json!({
                    "action": format!("{action:?}"),
                    "source": "keyboard",
                }),
            );
            self.apply_viewer_action(ctx, action);
        }
    }

    pub(crate) fn apply_viewer_action(&mut self, ctx: &egui::Context, action: ViewerAction) {
        match action {
            ViewerAction::ZoomIn => {
                let _ = self.set_zoom(self.zoom * 1.25);
            }
            ViewerAction::ZoomOut => {
                let _ = self.set_zoom(self.zoom / 1.25);
            }
            ViewerAction::ZoomReset => {
                let _ = self.set_zoom(1.0);
            }
            ViewerAction::ZoomToggle => {
                let _ = self.toggle_zoom();
            }
            ViewerAction::ToggleFullscreen => {
                let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                self.window_options.fullscreen = !fullscreen;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(!fullscreen));
            }
            ViewerAction::Reload => {
                if self.show_filer || self.show_subfiler {
                    self.refresh_current_filer_directory();
                } else {
                    let _ = self.reload_current();
                }
            }
            ViewerAction::NextImage => {
                let _ = self.next_image();
            }
            ViewerAction::PrevImage => {
                let _ = self.prev_image();
            }
            ViewerAction::FirstImage => {
                let _ = self.first_image();
            }
            ViewerAction::LastImage => {
                let _ = self.last_image();
            }
            ViewerAction::ToggleAnimation => {
                self.options.animation = !self.options.animation;
                self.current_frame = 0;
                self.last_frame_at = Instant::now();
                self.upload_current_frame();
            }
            ViewerAction::ToggleGrayscale => {
                self.options.grayscale = !self.options.grayscale;
                self.upload_current_frame();
                self.pending_fit_recalc = true;
            }
            ViewerAction::ToggleMangaMode => {
                self.options.manga_mode = !self.options.manga_mode;
                self.pending_fit_recalc = true;
            }
            ViewerAction::ToggleSettings => {
                if self.show_settings {
                    self.close_settings_dialog();
                } else {
                    self.open_settings_dialog();
                }
            }
            ViewerAction::ToggleFiler => {
                self.set_show_filer(!self.show_filer);
                self.pending_fit_recalc = true;
            }
            ViewerAction::ToggleSubfiler => {
                self.set_show_subfiler(!self.show_subfiler);
            }
            ViewerAction::SaveAs => {
                self.open_save_dialog();
            }
            ViewerAction::MoveFile
            | ViewerAction::CopyFile
            | ViewerAction::DeleteFile
            | ViewerAction::RenameFile => self.open_file_action_dialog(action),
            ViewerAction::SetMoveFolder1 => self.set_active_file_action_slot(action),
            ViewerAction::SetMoveFolder2 => self.set_active_file_action_slot(action),
            ViewerAction::SetCopyFolder1 => self.set_active_file_action_slot(action),
            ViewerAction::SetCopyFolder2 => self.set_active_file_action_slot(action),
        }
    }

    pub(crate) fn handle_pointer_input(&mut self, response: &egui::Response) -> bool {
        if self.suppress_next_pointer_intent {
            self.suppress_next_pointer_intent = false;
            self.cancel_pending_single_click_navigation();
            return true;
        }

        if self.pointer_input_blocked() {
            self.cancel_pending_single_click_navigation();
            return false;
        }

        if let Some(intent) = self.pointer_intent_from_response(response) {
            self.log_bench_state(
                "viewer.pointer_action",
                serde_json::json!({
                    "intent": format!("{intent:?}"),
                }),
            );
            self.perform_pointer_intent(response, intent);
            return true;
        }

        false
    }

    pub(crate) fn pointer_input_blocked(&self) -> bool {
        self.save_dialog.open || self.file_action_dialog.open || self.overlay.dialog.is_some()
    }

    pub(crate) fn response_has_pointer_intent(&self, response: &egui::Response) -> bool {
        self.pointer_intent_from_response(response).is_some()
    }

    fn pointer_intent_from_response(&self, response: &egui::Response) -> Option<PointerIntent> {
        if response.double_clicked_by(egui::PointerButton::Primary) {
            return Some(PointerIntent::ToggleFit);
        }

        if response.secondary_clicked() {
            let pos = response
                .interact_pointer_pos()
                .or_else(|| response.hover_pos())
                .unwrap_or(egui::pos2(32.0, 32.0));
            return Some(PointerIntent::OpenContextMenu(pos));
        }

        if response.clicked_by(egui::PointerButton::Primary) {
            return Some(PointerIntent::NextImageAfterDelay);
        }

        None
    }

    fn perform_pointer_intent(&mut self, _response: &egui::Response, intent: PointerIntent) {
        match intent {
            PointerIntent::ToggleFit => {
                self.cancel_pending_single_click_navigation();
                let _ = self.toggle_fit_zoom_mode();
            }
            PointerIntent::NextImageAfterDelay => {
                self.schedule_single_click_navigation();
            }
            PointerIntent::OpenContextMenu(pos) => {
                self.cancel_pending_single_click_navigation();
                self.left_menu_pos = pos;
                self.show_left_menu = true;
            }
        }
    }

    fn open_file_action_dialog(&mut self, action: ViewerAction) {
        let mode = match action {
            ViewerAction::MoveFile => FileActionDialogMode::Move,
            ViewerAction::CopyFile => FileActionDialogMode::Copy,
            ViewerAction::DeleteFile => FileActionDialogMode::Delete,
            ViewerAction::RenameFile => FileActionDialogMode::Rename,
            _ => return,
        };

        if !self.current_navigation_path.exists() || !self.current_navigation_path.is_file() {
            self.open_dialog_with_title_key(
                crate::ui::i18n::UiTextKey::AlertTitle,
                format!(
                    "{}: {}",
                    self.text(crate::ui::i18n::UiTextKey::CurrentTargetNotEditableFile),
                    self.current_navigation_path.display()
                ),
            );
            return;
        }

        self.file_action_dialog.mode = Some(mode);
        self.file_action_dialog.open = true;
        match mode {
            FileActionDialogMode::Move => {
                self.file_action_dialog.destination_path_input = self
                    .file_action
                    .active_move_folder()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
            }
            FileActionDialogMode::Copy => {
                self.file_action_dialog.destination_path_input = self
                    .file_action
                    .active_copy_folder()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default();
            }
            FileActionDialogMode::Rename => {
                self.file_action_dialog.rename_stem_input = self
                    .current_navigation_path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string();
                self.file_action_dialog.rename_extension = self
                    .current_navigation_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string();
            }
            FileActionDialogMode::Delete => {}
        }
    }

    fn set_active_file_action_slot(&mut self, action: ViewerAction) {
        match action {
            ViewerAction::SetMoveFolder1 => self.file_action.set_move_folder1(),
            ViewerAction::SetMoveFolder2 => self.file_action.set_move_folder2(),
            ViewerAction::SetCopyFolder1 => self.file_action.set_copy_folder1(),
            ViewerAction::SetCopyFolder2 => self.file_action.set_copy_folder2(),
            _ => return,
        }
        self.persist_config_async();
    }
}
