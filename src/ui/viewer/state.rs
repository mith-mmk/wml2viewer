use crate::drawers::image::SaveFormat;
use crate::options::AppConfig;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Instant;

pub(crate) struct ViewerOverlayState {
    pub(crate) loading_message: Option<String>,
    pub(crate) alert_message: Option<String>,
    pub(crate) loading_started_at: Option<Instant>,
}

impl Default for ViewerOverlayState {
    fn default() -> Self {
        Self {
            loading_message: None,
            alert_message: None,
            loading_started_at: None,
        }
    }
}

impl ViewerOverlayState {
    pub(crate) fn set_loading_message(&mut self, message: impl Into<String>) {
        if self.loading_message.is_none() {
            self.loading_started_at = Some(Instant::now());
        }
        self.loading_message = Some(message.into());
    }

    pub(crate) fn clear_loading_message(&mut self) {
        self.loading_message = None;
        self.loading_started_at = None;
    }
}

pub(crate) struct SaveDialogState {
    pub(crate) format: SaveFormat,
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) file_name: String,
    pub(crate) message: Option<String>,
    pub(crate) open: bool,
    pub(crate) in_progress: bool,
    pub(crate) result_rx: Option<Receiver<Result<String, String>>>,
}

impl Default for SaveDialogState {
    fn default() -> Self {
        Self {
            format: SaveFormat::Png,
            output_dir: None,
            file_name: String::new(),
            message: None,
            open: false,
            in_progress: false,
            result_rx: None,
        }
    }
}

#[derive(Clone)]
pub(crate) struct SettingsDraftState {
    pub(crate) config: AppConfig,
    pub(crate) resource_locale_input: String,
    pub(crate) resource_font_paths_input: String,
    pub(crate) susie64_search_paths_input: String,
    pub(crate) ffmpeg_search_paths_input: String,
}
