use crate::configs::config::save_app_config;
use crate::configs::resourses::{AppliedResources, apply_resources};
use crate::dependent::{default_download_dir, pick_save_directory};
use crate::drawers::canvas::Canvas;
use crate::drawers::image::{LoadedImage, SaveFormat, save_loaded_image};
use crate::filesystem::{
    FilesystemCommand, FilesystemResult, SharedBrowserWorkerState, SharedFilesystemCache,
    adjacent_entry, adjacent_entry_in_current_branch, adjacent_non_container_entry,
    archive_prefers_low_io, browser_directory_for_path, is_browser_container,
    navigation_branch_path, new_shared_browser_worker_state, new_shared_filesystem_cache,
    resolve_navigation_entry_path, set_archive_zip_workaround, spawn_browser_query_worker,
    spawn_filesystem_worker,
};
use crate::options::{
    AppConfig, EndOfFolderOption, KeyBinding, NavigationSortOption, PluginConfig, ResourceOptions,
    RuntimeOptions, ViewerAction,
};
use crate::ui::i18n::{UiTextKey, tr};
use crate::ui::menu::fileviewer::state::FilerState;
use crate::ui::menu::fileviewer::thumbnail::{
    ThumbnailCommand, ThumbnailResult, set_thumbnail_workaround, spawn_thumbnail_worker,
};
use crate::ui::render::{
    ActiveRenderRequest, RenderCommand, RenderResult, RenderWorkerPriority, aligned_offset,
    canvas_to_color_image, downscale_for_texture_limit, spawn_render_worker, worker_send_error,
};
use crate::ui::viewer::options::{
    RenderOptions, RenderScaleMode, ViewerOptions, WindowOptions, WindowStartPosition,
    WindowUiTheme,
};
use eframe::egui::{self, Pos2, TextureHandle, TextureOptions, vec2};
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
pub mod options;
mod state;
use options::ZoomOption;
pub(crate) use state::SettingsDraftState;
use state::{SaveDialogState, ViewerOverlayState};

const NAVIGATION_REPEAT_INTERVAL: Duration = Duration::from_millis(90);
const POINTER_SINGLE_CLICK_DELAY: Duration = Duration::from_millis(500);
const WAITING_CARD_DELAY: Duration = Duration::from_millis(180);

pub(crate) struct ViewerApp {
    pub(crate) current_navigation_path: PathBuf,
    pub(crate) current_path: PathBuf,
    pub(crate) source: LoadedImage,
    pub(crate) rendered: LoadedImage,
    pub(crate) default_texture: TextureHandle,
    pub(crate) prev_texture: Option<TextureHandle>,
    pub(crate) current_texture: TextureHandle,
    pub(crate) next_texture: Option<TextureHandle>,
    pub(crate) egui_ctx: egui::Context,

    pub(crate) zoom: f32,
    pub(crate) zoom_factor: f32,

    pub(crate) current_frame: usize,
    pub(crate) last_frame_at: Instant,
    pub(crate) completed_loops: u32,

    pub(crate) fit_zoom: f32,
    pub(crate) last_viewport_size: egui::Vec2,
    pub(crate) frame_counter: usize,
    pub(crate) startup_phase: StartupPhase,

    pub(crate) render_options: RenderOptions,
    pub(crate) options: ViewerOptions,
    pub(crate) window_options: WindowOptions,
    pub(crate) resources: ResourceOptions,
    pub(crate) plugins: PluginConfig,
    pub(crate) storage: crate::options::StorageOptions,
    pub(crate) runtime: RuntimeOptions,
    pub(crate) applied_locale: String,
    pub(crate) loaded_font_names: Vec<String>,
    pub(crate) resource_locale_input: String,
    pub(crate) resource_font_paths_input: String,
    pub(crate) keymap: HashMap<KeyBinding, ViewerAction>,
    pub(crate) end_of_folder: EndOfFolderOption,
    pub(crate) navigation_sort: NavigationSortOption,
    pub(crate) worker_tx: Sender<RenderCommand>,
    pub(crate) worker_rx: Receiver<RenderResult>,
    pub(crate) worker_join: Option<JoinHandle<()>>,
    pub(crate) next_request_id: u64,
    pub(crate) active_request: Option<ActiveRenderRequest>,
    pub(crate) pending_navigation_path: Option<PathBuf>,
    pub(crate) shared_filesystem_cache: Option<SharedFilesystemCache>,
    pub(crate) shared_browser_worker_state: Option<SharedBrowserWorkerState>,
    pub(crate) fs_tx: Option<Sender<FilesystemCommand>>,
    pub(crate) fs_rx: Option<Receiver<FilesystemResult>>,
    pub(crate) next_fs_request_id: u64,
    pub(crate) active_fs_request_id: Option<u64>,
    pub(crate) active_fs_input_request_id: Option<u64>,
    pub(crate) queued_navigation: Option<FilesystemCommand>,
    pub(crate) deferred_filesystem_init_path: Option<PathBuf>,
    pub(crate) filer_tx: Option<Sender<FilesystemCommand>>,
    pub(crate) filer_rx: Option<Receiver<FilesystemResult>>,
    pub(crate) next_filer_request_id: u64,
    pub(crate) thumbnail_tx: Option<Sender<ThumbnailCommand>>,
    pub(crate) thumbnail_rx: Option<Receiver<ThumbnailResult>>,
    pub(crate) next_thumbnail_request_id: u64,
    pub(crate) thumbnail_pending: HashMap<PathBuf, u32>,
    pub(crate) thumbnail_cache: HashMap<PathBuf, CachedThumbnail>,
    pub(crate) pending_filer_scroll_to: Option<PathBuf>,
    pub(crate) filer_needs_sync: bool,
    pub(crate) navigator_ready: bool,
    pub(crate) overlay: ViewerOverlayState,
    pub(crate) last_navigation_at: Option<Instant>,
    pub(crate) show_settings: bool,
    pub(crate) settings_draft: Option<SettingsDraftState>,
    pub(crate) show_restart_prompt: bool,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) max_texture_side: usize,
    pub(crate) texture_display_scale: f32,
    pub(crate) next_texture_display_scale: f32,
    pub(crate) current_texture_is_default: bool,
    pub(crate) pending_resize_after_load: bool,
    pub(crate) pending_resize_after_render: bool,
    pub(crate) pending_fit_recalc: bool,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) show_left_menu: bool,
    pub(crate) left_menu_pos: Pos2,
    pub(crate) save_dialog: SaveDialogState,
    pub(crate) show_filer: bool,
    pub(crate) show_subfiler: bool,
    pub(crate) filer: FilerState,
    pub(crate) susie64_search_paths_input: String,
    pub(crate) system_search_paths_input: String,
    pub(crate) ffmpeg_search_paths_input: String,
    pub(crate) startup_window_sync_frames: usize,
    pub(crate) deferred_filesystem_sync_frame: Option<usize>,
    pub(crate) empty_mode: bool,
    pub(crate) companion_tx: Sender<RenderCommand>,
    pub(crate) companion_rx: Receiver<RenderResult>,
    pub(crate) companion_join: Option<JoinHandle<()>>,
    pub(crate) companion_active_request: Option<ActiveRenderRequest>,
    pub(crate) manga_cached_navigation_path: Option<PathBuf>,
    pub(crate) manga_cached_spread_requested: bool,
    pub(crate) manga_cached_navigator_ready: bool,
    pub(crate) manga_cached_sort: NavigationSortOption,
    pub(crate) manga_cached_archive_mode: crate::options::ArchiveBrowseOption,
    pub(crate) desired_companion_navigation_path: Option<PathBuf>,
    pub(crate) next_manga_navigation_path: Option<PathBuf>,
    pub(crate) prev_manga_navigation_path: Option<PathBuf>,
    pub(crate) companion_navigation_path: Option<PathBuf>,
    pub(crate) companion_source: Option<LoadedImage>,
    pub(crate) companion_rendered: Option<LoadedImage>,
    pub(crate) companion_texture: Option<TextureHandle>,
    pub(crate) companion_texture_display_scale: f32,
    pub(crate) preload_tx: Sender<RenderCommand>,
    pub(crate) preload_rx: Receiver<RenderResult>,
    pub(crate) preload_join: Option<JoinHandle<()>>,
    pub(crate) next_preload_request_id: u64,
    pub(crate) active_preload_request_id: Option<u64>,
    pub(crate) pending_preload_navigation_path: Option<PathBuf>,
    pub(crate) preloaded_navigation_path: Option<PathBuf>,
    pub(crate) preloaded_load_path: Option<PathBuf>,
    pub(crate) preloaded_source: Option<LoadedImage>,
    pub(crate) preloaded_rendered: Option<LoadedImage>,
    pub(crate) preloaded_companion_navigation_path: Option<PathBuf>,
    pub(crate) preloaded_companion_source: Option<LoadedImage>,
    pub(crate) preloaded_companion_rendered: Option<LoadedImage>,
    pub(crate) preloaded_companion_texture: Option<TextureHandle>,
    pub(crate) preloaded_companion_texture_display_scale: f32,
    pub(crate) pending_primary_click_deadline: Option<Instant>,
}

pub(crate) struct CachedThumbnail {
    pub(crate) texture: TextureHandle,
    pub(crate) max_side: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsTab {
    Viewer,
    Plugins,
    Resources,
    Render,
    Window,
    Navigation,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartupPhase {
    SingleViewer,
    Synchronizing,
    MultiViewer,
}

fn calc_fit_zoom(ctx_size: egui::Vec2, image_size: egui::Vec2, option: &ZoomOption) -> f32 {
    let image_width = image_size.x.max(1.0);
    let image_height = image_size.y.max(1.0);

    let canvas_width = ctx_size.x;
    let canvas_height = ctx_size.y;

    let zoom_w = canvas_width / image_width;
    let zoom_h = canvas_height / image_height;
    let fit = zoom_w.min(zoom_h);

    match option {
        ZoomOption::None => 1.0,
        ZoomOption::FitWidth => zoom_w.min(1.0),
        ZoomOption::FitHeight => zoom_h.min(1.0),
        ZoomOption::FitScreen => fit.min(1.0),
        ZoomOption::FitScreenIncludeSmaller => fit,
        ZoomOption::FitScreenOnlySmaller => fit.min(1.0),
    }
}

fn texture_options_for_scale_mode(
    scale_mode: RenderScaleMode,
    method: crate::drawers::affine::InterpolationAlgorithm,
) -> TextureOptions {
    match scale_mode {
        RenderScaleMode::FastGpu => match method {
            crate::drawers::affine::InterpolationAlgorithm::NearestNeighber => {
                TextureOptions::NEAREST
            }
            _ => TextureOptions::LINEAR,
        },
        RenderScaleMode::PreciseCpu => match method {
            crate::drawers::affine::InterpolationAlgorithm::NearestNeighber => {
                TextureOptions::NEAREST
            }
            _ => TextureOptions::LINEAR,
        },
    }
}

fn viewport_size_changed(current: egui::Vec2, previous: egui::Vec2) -> bool {
    if previous == egui::Vec2::ZERO {
        return true;
    }
    (current.x - previous.x).abs() > 1.0 || (current.y - previous.y).abs() > 1.0
}

fn default_save_file_name(path: &std::path::Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("image")
        .to_string()
}

fn blank_loaded_image() -> LoadedImage {
    LoadedImage {
        canvas: Canvas::new(1, 1),
        animation: Vec::new(),
        loop_count: None,
    }
}

fn loading_card_message(message: Option<&str>) -> String {
    match message {
        Some(message) if !message.trim().is_empty() => format!("Now Loading...\n{}", message),
        _ => "Now Loading...".to_string(),
    }
}

fn ellipsize_end(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    let head = chars
        .iter()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    format!("{head}...")
}

fn format_key_binding(binding: &KeyBinding) -> String {
    let mut parts = Vec::new();
    if binding.ctrl {
        parts.push("Ctrl");
    }
    if binding.shift {
        parts.push("Shift");
    }
    if binding.alt {
        parts.push("Alt");
    }
    parts.push(&binding.key);
    parts.join("+")
}

fn preloaded_navigation_matches(
    preloaded_navigation_path: Option<&std::path::Path>,
    requested_navigation_path: &std::path::Path,
) -> bool {
    preloaded_navigation_path == Some(requested_navigation_path)
}

#[cfg(test)]
fn same_navigation_branch(
    current_path: &std::path::Path,
    candidate_path: &std::path::Path,
) -> bool {
    navigation_branch_path(current_path) == navigation_branch_path(candidate_path)
}

fn adjacent_same_branch_navigation_target(
    current_path: &std::path::Path,
    navigation_sort: NavigationSortOption,
    archive_mode: crate::options::ArchiveBrowseOption,
    step: isize,
) -> Option<PathBuf> {
    let branch = navigation_branch_path(current_path)?;
    if !branch.is_dir() && is_browser_container(&branch) {
        return adjacent_entry_in_current_branch(current_path, navigation_sort, archive_mode, step)
            .filter(|candidate| navigation_branch_path(candidate) == Some(branch.clone()));
    }
    adjacent_non_container_entry(current_path, navigation_sort, archive_mode, step)
}

fn should_defer_filer_sync_for_navigation(
    navigation_path: &std::path::Path,
    current_load_path: Option<&std::path::Path>,
) -> bool {
    browser_directory_for_path(navigation_path, current_load_path)
        .filter(|dir| !dir.is_dir() && is_browser_container(dir))
        .is_some()
}

fn should_allow_preload_for_path(
    current_navigation_path: &std::path::Path,
    candidate_path: &std::path::Path,
) -> bool {
    if !archive_prefers_low_io(candidate_path) {
        return true;
    }
    navigation_branch_path(current_navigation_path) == navigation_branch_path(candidate_path)
}

fn should_defer_filer_request_while_loading(
    active_request: bool,
    active_fs_request: bool,
    active_fs_input_request: bool,
    companion_active_request: bool,
    preload_active_request: bool,
) -> bool {
    active_request
        || active_fs_request
        || active_fs_input_request
        || companion_active_request
        || preload_active_request
}

fn manga_companion_matches_preloaded(
    companion_path: &std::path::Path,
    preloaded_navigation_path: Option<&std::path::Path>,
) -> bool {
    preloaded_navigation_matches(preloaded_navigation_path, companion_path)
}

fn should_defer_preload_for_manga_low_io(
    manga_mode: bool,
    current_navigation_path: &std::path::Path,
    desired_companion: Option<&std::path::Path>,
    companion_ready: bool,
) -> bool {
    manga_mode
        && desired_companion.is_some()
        && archive_prefers_low_io(current_navigation_path)
        && !companion_ready
}

fn should_defer_thumbnail_io(
    current_navigation_path: &std::path::Path,
    active_request: bool,
    companion_active_request: bool,
    preload_active_request: bool,
) -> bool {
    archive_prefers_low_io(current_navigation_path)
        && (active_request || companion_active_request || preload_active_request)
}

pub(crate) fn join_search_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

pub(crate) fn parse_search_paths(input: &str) -> Vec<PathBuf> {
    input
        .split(';')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn locale_input_from_config(config: &AppConfig) -> String {
    config.resources.locale.clone().unwrap_or_default()
}

pub(crate) fn build_settings_draft(config: &AppConfig) -> SettingsDraftState {
    SettingsDraftState {
        config: config.clone(),
        resource_locale_input: locale_input_from_config(config),
        resource_font_paths_input: join_search_paths(&config.resources.font_paths),
        susie64_search_paths_input: join_search_paths(&config.plugins.susie64.search_path),
        ffmpeg_search_paths_input: join_search_paths(&config.plugins.ffmpeg.search_path),
    }
}

impl ViewerApp {
    pub(crate) fn new(
        cc: &eframe::CreationContext<'_>,
        navigation_path: PathBuf,
        path: PathBuf,
        source: LoadedImage,
        rendered: LoadedImage,
        config: AppConfig,
        config_path: Option<PathBuf>,
        show_filer_on_start: bool,
        startup_load_path: Option<PathBuf>,
    ) -> Self {
        let color_image = canvas_to_color_image(rendered.frame_canvas(0));

        let zoom = 1.0;
        let zoom_factor = 1.0;
        let texture_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("default")
            .to_owned();

        let default_texture = cc.egui_ctx.load_texture(
            texture_name,
            color_image,
            texture_options_for_scale_mode(config.render.scale_mode, config.render.zoom_method),
        );
        let AppliedResources {
            locale,
            loaded_fonts,
        } = apply_resources(&cc.egui_ctx, &config.resources);
        set_archive_zip_workaround(config.runtime.workaround.archive.zip.clone());
        set_thumbnail_workaround(config.runtime.workaround.thumbnail.clone());
        let (worker_tx, worker_rx, worker_join) =
            spawn_render_worker(source.clone(), RenderWorkerPriority::Primary);
        let (companion_tx, companion_rx, companion_join) =
            spawn_render_worker(source.clone(), RenderWorkerPriority::Companion);
        let (preload_tx, preload_rx, preload_join) =
            spawn_render_worker(source.clone(), RenderWorkerPriority::Preload);
        let resource_locale_input = config.resources.locale.clone().unwrap_or_default();
        let resource_font_paths_input = join_search_paths(&config.resources.font_paths);
        let defer_navigation_workers = !show_filer_on_start;
        let startup_phase = if defer_navigation_workers {
            StartupPhase::SingleViewer
        } else {
            StartupPhase::MultiViewer
        };

        let mut this = Self {
            current_navigation_path: navigation_path.clone(),
            current_path: path.clone(),
            source,
            rendered,
            default_texture: default_texture.clone(),
            prev_texture: None,
            current_texture: default_texture.clone(),
            next_texture: None,
            egui_ctx: cc.egui_ctx.clone(),

            zoom,
            zoom_factor,

            current_frame: 0,
            last_frame_at: Instant::now(),
            completed_loops: 0,

            fit_zoom: 1.0,
            last_viewport_size: egui::Vec2::ZERO,
            frame_counter: 0,
            startup_phase,

            render_options: config.render,
            options: config.viewer,
            window_options: config.window,
            resources: config.resources,
            plugins: config.plugins,
            storage: config.storage,
            runtime: config.runtime,
            applied_locale: locale,
            loaded_font_names: loaded_fonts,
            resource_locale_input,
            resource_font_paths_input,
            keymap: config.input.merged_with_defaults(),
            end_of_folder: config.navigation.end_of_folder,
            navigation_sort: config.navigation.sort,
            worker_tx,
            worker_rx,
            worker_join: Some(worker_join),
            next_request_id: 0,
            active_request: None,
            pending_navigation_path: None,
            shared_filesystem_cache: None,
            shared_browser_worker_state: None,
            fs_tx: None,
            fs_rx: None,
            next_fs_request_id: 0,
            active_fs_request_id: None,
            active_fs_input_request_id: None,
            queued_navigation: None,
            deferred_filesystem_init_path: None,
            filer_tx: None,
            filer_rx: None,
            next_filer_request_id: 0,
            thumbnail_tx: None,
            thumbnail_rx: None,
            next_thumbnail_request_id: 0,
            thumbnail_pending: HashMap::new(),
            thumbnail_cache: HashMap::new(),
            pending_filer_scroll_to: None,
            filer_needs_sync: true,
            navigator_ready: false,
            overlay: ViewerOverlayState::default(),
            last_navigation_at: None,
            show_settings: false,
            settings_draft: None,
            show_restart_prompt: false,
            settings_tab: SettingsTab::Viewer,
            max_texture_side: cc.egui_ctx.input(|i| i.max_texture_side),
            texture_display_scale: 1.0,
            next_texture_display_scale: 1.0,
            current_texture_is_default: true,
            pending_resize_after_load: false,
            pending_resize_after_render: false,
            pending_fit_recalc: false,
            config_path,
            show_left_menu: false,
            left_menu_pos: Pos2::ZERO,
            save_dialog: SaveDialogState {
                file_name: default_save_file_name(&path),
                ..SaveDialogState::default()
            },
            show_filer: show_filer_on_start,
            show_subfiler: false,
            filer: FilerState {
                archive_mode: config.navigation.archive,
                ..FilerState::default()
            },
            susie64_search_paths_input: String::new(),
            system_search_paths_input: String::new(),
            ffmpeg_search_paths_input: String::new(),
            startup_window_sync_frames: 0,
            deferred_filesystem_sync_frame: None,
            empty_mode: show_filer_on_start,
            companion_tx,
            companion_rx,
            companion_join: Some(companion_join),
            companion_active_request: None,
            manga_cached_navigation_path: None,
            manga_cached_spread_requested: false,
            manga_cached_navigator_ready: false,
            manga_cached_sort: config.navigation.sort,
            manga_cached_archive_mode: config.navigation.archive,
            desired_companion_navigation_path: None,
            next_manga_navigation_path: None,
            prev_manga_navigation_path: None,
            companion_navigation_path: None,
            companion_source: None,
            companion_rendered: None,
            companion_texture: None,
            companion_texture_display_scale: 1.0,
            preload_tx,
            preload_rx,
            preload_join: Some(preload_join),
            next_preload_request_id: 0,
            active_preload_request_id: None,
            pending_preload_navigation_path: None,
            preloaded_navigation_path: None,
            preloaded_load_path: None,
            preloaded_source: None,
            preloaded_rendered: None,
            preloaded_companion_navigation_path: None,
            preloaded_companion_source: None,
            preloaded_companion_rendered: None,
            preloaded_companion_texture: None,
            preloaded_companion_texture_display_scale: 1.0,
            pending_primary_click_deadline: None,
        };

        this.save_dialog.output_dir = this
            .storage
            .path
            .clone()
            .or_else(default_download_dir)
            .or_else(|| path.parent().map(|parent| parent.to_path_buf()));
        this.susie64_search_paths_input = join_search_paths(&this.plugins.susie64.search_path);
        this.system_search_paths_input = join_search_paths(&this.plugins.system.search_path);
        this.ffmpeg_search_paths_input = join_search_paths(&this.plugins.ffmpeg.search_path);
        this.apply_window_theme(&cc.egui_ctx);
        this.normalize_render_options();

        if !defer_navigation_workers {
            this.spawn_navigation_workers();
        }

        if let Some(path) = startup_load_path {
            this.deferred_filesystem_init_path = Some(navigation_path.clone());
            let _ = this.request_load_path(path);
        } else if !show_filer_on_start {
            this.deferred_filesystem_init_path = Some(navigation_path.clone());
            let _ = this.request_load_path(navigation_path.clone());
        } else {
            let _ = this.init_filesystem(navigation_path);
            if let Some(dir) = this.current_directory() {
                this.request_filer_directory(dir, Some(this.current_navigation_path.clone()));
            }
        }
        this
    }

    fn source_size(&self) -> egui::Vec2 {
        vec2(
            self.source.canvas.width() as f32,
            self.source.canvas.height() as f32,
        )
    }

    fn fit_target_size(&self) -> egui::Vec2 {
        if self.manga_spread_active() {
            if let Some(companion) = &self.companion_source {
                let separator = self.options.manga_separator.pixels.max(0.0);
                return vec2(
                    self.source.canvas.width() as f32 + companion.canvas.width() as f32 + separator,
                    self.source.canvas.height().max(companion.canvas.height()) as f32,
                );
            }
        }

        self.source_size()
    }

    fn paint_manga_separator(&self, ui: &mut egui::Ui, height: f32) {
        let width = self.options.manga_separator.pixels.max(0.0);
        if width <= 0.0 {
            return;
        }

        let (rect, _) = ui.allocate_exact_size(vec2(width, height.max(1.0)), egui::Sense::hover());
        match self.options.manga_separator.style {
            crate::ui::viewer::options::MangaSeparatorStyle::None => {}
            crate::ui::viewer::options::MangaSeparatorStyle::Solid => {
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(
                        self.options.manga_separator.color[0],
                        self.options.manga_separator.color[1],
                        self.options.manga_separator.color[2],
                        self.options.manga_separator.color[3],
                    ),
                );
            }
            crate::ui::viewer::options::MangaSeparatorStyle::Shadow => {
                let base = self.options.manga_separator.color;
                let steps = width.max(2.0) as usize;
                for step in 0..steps {
                    let t = (step as f32 + 0.5) / steps as f32;
                    let alpha = (1.0 - ((t - 0.5).abs() * 2.0)).max(0.0) * (base[3] as f32);
                    let x0 = rect.left() + (step as f32 / steps as f32) * rect.width();
                    let x1 = rect.left() + ((step + 1) as f32 / steps as f32) * rect.width();
                    let band = egui::Rect::from_min_max(
                        egui::pos2(x0, rect.top()),
                        egui::pos2(x1, rect.bottom()),
                    );
                    ui.painter().rect_filled(
                        band,
                        0.0,
                        egui::Color32::from_rgba_unmultiplied(
                            base[0],
                            base[1],
                            base[2],
                            alpha.round().clamp(0.0, 255.0) as u8,
                        ),
                    );
                }
            }
        }
    }

    pub(crate) fn text(&self, key: UiTextKey) -> &'static str {
        tr(&self.applied_locale, key)
    }

    pub(crate) fn apply_window_theme(&self, ctx: &egui::Context) {
        match self.window_options.ui_theme {
            WindowUiTheme::System => {}
            WindowUiTheme::Light => ctx.set_visuals(egui::Visuals::light()),
            WindowUiTheme::Dark => ctx.set_visuals(egui::Visuals::dark()),
        }
    }

    pub(crate) fn open_help(&self) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("help.html");
        let _ = std::fs::create_dir_all(path.parent().unwrap_or_else(|| std::path::Path::new(".")));
        let mut bindings = self
            .keymap
            .iter()
            .map(|(binding, action)| (format_key_binding(binding), format!("{action:?}")))
            .collect::<Vec<_>>();
        bindings.sort_by(|left, right| left.0.cmp(&right.0));

        let rows = bindings
            .into_iter()
            .map(|(binding, action)| format!("<tr><td>{binding}</td><td>{action}</td></tr>"))
            .collect::<Vec<_>>()
            .join("\n");
        let html = format!(
            r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>wml2viewer Help</title>
  <style>
    body {{ font-family: sans-serif; margin: 32px; line-height: 1.5; }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{ border: 1px solid #ccc; padding: 8px 10px; text-align: left; }}
    code {{ background: #f4f4f4; padding: 2px 4px; border-radius: 4px; }}
  </style>
</head>
<body>
  <h1>wml2viewer Help</h1>
  <h2>Key Bindings</h2>
  <table>
    <thead><tr><th>Key</th><th>Action</th></tr></thead>
    <tbody>{rows}</tbody>
  </table>
  <h2>Startup Options</h2>
  <ul>
    <li><code>wml2viewer [path]</code></li>
    <li><code>wml2viewer --config &lt;path&gt; [path]</code></li>
    <li><code>wml2viewer --config=&lt;path&gt; [path]</code></li>
    <li><code>wml2viewer --clean system</code></li>
    <li><code>wml2viewer --clean cache</code></li>
  </ul>
</body>
</html>"#
        );
        let _ = std::fs::write(&path, html);

        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", &path.display().to_string()])
            .spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&path).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
    }

    pub(crate) fn open_settings_dialog(&mut self) {
        if self.settings_draft.is_none() {
            self.settings_draft = Some(build_settings_draft(&self.current_config()));
        }
        self.show_settings = true;
    }

    pub(crate) fn close_settings_dialog(&mut self) {
        self.show_settings = false;
        self.settings_draft = None;
    }

    pub(crate) fn reset_settings_draft_to_live(&mut self) {
        self.settings_draft = Some(build_settings_draft(&self.current_config()));
    }

    pub(crate) fn apply_settings_draft(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.settings_draft.clone() else {
            return;
        };
        self.restore_config(draft.config, ctx);
        self.persist_config_async();
        self.settings_draft = Some(build_settings_draft(&self.current_config()));
    }

    pub(crate) fn normalize_render_options(&mut self) {
        if matches!(self.render_options.scale_mode, RenderScaleMode::FastGpu)
            && !matches!(
                self.render_options.zoom_method,
                crate::drawers::affine::InterpolationAlgorithm::NearestNeighber
                    | crate::drawers::affine::InterpolationAlgorithm::Bilinear
            )
        {
            self.render_options.zoom_method =
                crate::drawers::affine::InterpolationAlgorithm::Bilinear;
        }
    }

    pub(crate) fn schedule_single_click_navigation(&mut self) {
        self.pending_primary_click_deadline = Some(Instant::now() + POINTER_SINGLE_CLICK_DELAY);
    }

    pub(crate) fn cancel_pending_single_click_navigation(&mut self) {
        self.pending_primary_click_deadline = None;
    }

    fn poll_pending_pointer_actions(&mut self) {
        let Some(deadline) = self.pending_primary_click_deadline else {
            return;
        };
        if Instant::now() < deadline || self.pointer_input_blocked() {
            return;
        }
        self.pending_primary_click_deadline = None;
        let _ = self.next_image();
    }

    fn defer_initial_filesystem_sync(&mut self) {
        if self.deferred_filesystem_init_path.is_some() {
            self.startup_phase = StartupPhase::Synchronizing;
            self.deferred_filesystem_sync_frame = Some(self.frame_counter + 2);
        }
    }

    fn poll_deferred_filesystem_sync(&mut self) {
        let Some(target_frame) = self.deferred_filesystem_sync_frame else {
            return;
        };
        if self.frame_counter < target_frame || self.active_fs_request_id.is_some() {
            return;
        }
        self.deferred_filesystem_sync_frame = None;
        if let Some(sync_path) = self.deferred_filesystem_init_path.take() {
            let _ = self.init_filesystem(sync_path);
        }
    }

    fn texture_options(&self) -> TextureOptions {
        texture_options_for_scale_mode(
            self.render_options.scale_mode,
            self.render_options.zoom_method,
        )
    }

    fn current_draw_scale(&self) -> f32 {
        match self.render_options.scale_mode {
            RenderScaleMode::FastGpu => self.zoom.max(0.1),
            RenderScaleMode::PreciseCpu => 1.0,
        }
    }

    fn companion_draw_scale(&self) -> f32 {
        match self.render_options.scale_mode {
            RenderScaleMode::FastGpu => self.zoom.max(0.1),
            RenderScaleMode::PreciseCpu => 1.0,
        }
    }

    fn effective_zoom(&self) -> f32 {
        let base = if matches!(self.render_options.zoom_option, ZoomOption::None) {
            1.0
        } else {
            self.fit_zoom.max(0.1)
        };
        let factor = self.zoom_factor.clamp(0.1, 16.0);
        if matches!(self.render_options.zoom_option, ZoomOption::None) {
            factor
        } else {
            (base * factor).clamp(0.1, 16.0)
        }
    }

    fn sync_zoom(&mut self) -> Result<(), Box<dyn Error>> {
        let zoom = self.effective_zoom();
        if (zoom - self.zoom).abs() < f32::EPSILON {
            return Ok(());
        }
        self.zoom = zoom;
        self.invalidate_preload();
        self.request_resize_current()?;
        Ok(())
    }

    pub(crate) fn set_zoom(&mut self, zoom: f32) -> Result<(), Box<dyn Error>> {
        let zoom = zoom.clamp(0.1, 16.0);
        if matches!(self.render_options.zoom_option, ZoomOption::None) {
            self.zoom_factor = zoom;
        } else {
            let base = self.fit_zoom.max(0.1);
            self.zoom_factor = (zoom / base).clamp(0.1, 16.0);
        }
        self.sync_zoom()
    }

    pub(crate) fn toggle_zoom(&mut self) -> Result<(), Box<dyn Error>> {
        let target_zoom = if (self.zoom - 1.0).abs() < 0.01 {
            self.fit_zoom
        } else {
            1.0
        };
        self.set_zoom(target_zoom)
    }

    pub(crate) fn toggle_fit_zoom_mode(&mut self) -> Result<(), Box<dyn Error>> {
        if matches!(self.render_options.zoom_option, ZoomOption::None) {
            self.render_options.zoom_option = ZoomOption::FitScreen;
            self.zoom_factor = 1.0;
            self.pending_fit_recalc = true;
            Ok(())
        } else {
            self.render_options.zoom_option = ZoomOption::None;
            self.zoom_factor = 1.0;
            self.sync_zoom()
        }
    }

    fn animation_enabled(&self) -> bool {
        self.options.animation && self.rendered.is_animated()
    }

    fn current_canvas(&self) -> &Canvas {
        if self.animation_enabled() {
            self.rendered.frame_canvas(self.current_frame)
        } else {
            &self.rendered.canvas
        }
    }

    fn texture_name_for_path(&self, path: Option<&Path>) -> String {
        path.and_then(|value| value.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("image")
            .to_owned()
    }

    fn build_texture_from_canvas(
        &self,
        texture_name: &str,
        canvas: &Canvas,
    ) -> (TextureHandle, f32) {
        let (canvas, display_scale) = downscale_for_texture_limit(
            canvas,
            self.max_texture_side,
            self.render_options.zoom_method,
        );
        let image = self.color_image_from_canvas(&canvas);
        let texture =
            self.egui_ctx
                .load_texture(texture_name.to_owned(), image, self.texture_options());
        (texture, display_scale)
    }

    fn rebuild_current_texture(&mut self) {
        let texture_name = self.texture_name_for_path(Some(&self.current_path));
        let (texture, display_scale) =
            self.build_texture_from_canvas(&texture_name, self.current_canvas());
        self.current_texture = texture;
        self.texture_display_scale = display_scale;
        self.current_texture_is_default = false;
    }

    fn show_loading_texture(&mut self, reset_branch_cache: bool) {
        if !self.current_texture_is_default {
            self.prev_texture = Some(self.current_texture.clone());
        }
        if reset_branch_cache {
            self.prev_texture = None;
            self.next_texture = None;
            self.next_texture_display_scale = 1.0;
        }
        self.current_texture = self.default_texture.clone();
        self.current_texture_is_default = true;
        self.texture_display_scale = 1.0;
    }

    fn shutdown_render_worker(tx: &Sender<RenderCommand>, join: &mut Option<JoinHandle<()>>) {
        let _ = tx.send(RenderCommand::Shutdown);
        if let Some(handle) = join.take() {
            let _ = handle.join();
        }
    }

    pub(crate) fn upload_current_frame(&mut self) {
        let texture_name = self.texture_name_for_path(Some(&self.current_path));
        let (canvas, display_scale) = {
            let canvas = self.current_canvas();
            downscale_for_texture_limit(
                canvas,
                self.max_texture_side,
                self.render_options.zoom_method,
            )
        };
        let image = self.color_image_from_canvas(&canvas);
        self.texture_display_scale = display_scale;
        if self.current_texture_is_default {
            self.current_texture =
                self.egui_ctx
                    .load_texture(texture_name, image, self.texture_options());
            self.current_texture_is_default = false;
        } else {
            self.current_texture.set(image, self.texture_options());
        }
    }

    fn clear_current_image_display(&mut self) {
        let blank = blank_loaded_image();
        self.source = blank.clone();
        self.rendered = blank;
        self.current_frame = 0;
        self.completed_loops = 0;
        self.last_frame_at = Instant::now();
        self.texture_display_scale = 1.0;
        self.current_texture = self.default_texture.clone();
        self.current_texture_is_default = true;
    }

    fn current_viewport_size(&self) -> egui::Vec2 {
        if self.last_viewport_size != egui::Vec2::ZERO {
            self.last_viewport_size
        } else {
            self.egui_ctx.content_rect().size()
        }
    }

    fn maybe_defer_precise_display(
        &mut self,
        source_size: egui::Vec2,
        loaded_path: Option<&Path>,
    ) -> bool {
        if loaded_path.is_none() {
            return false;
        }
        if !matches!(self.render_options.scale_mode, RenderScaleMode::PreciseCpu) {
            return false;
        }
        if matches!(self.render_options.zoom_option, ZoomOption::None) {
            return false;
        }

        let viewport = self.current_viewport_size();
        if viewport == egui::Vec2::ZERO {
            return false;
        }

        let target_fit =
            calc_fit_zoom(viewport, source_size, &self.render_options.zoom_option).clamp(0.1, 16.0);
        let target_zoom = (target_fit * self.zoom_factor.clamp(0.1, 16.0)).clamp(0.1, 16.0);

        if (target_zoom - 1.0).abs() < 0.01 {
            self.fit_zoom = target_fit;
            self.zoom = target_zoom;
            self.pending_fit_recalc = false;
            return false;
        }

        self.fit_zoom = target_fit;
        self.zoom = target_zoom;
        self.pending_fit_recalc = false;
        self.overlay
            .set_loading_message(format!("Rendering {:.0}%", target_zoom * 100.0));
        true
    }

    fn update_window_title(&self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
            "wml2viewer - {}",
            self.current_path.display()
        )));
    }

    pub(crate) fn update_animation(&mut self, ctx: &egui::Context) {
        if !self.animation_enabled() {
            return;
        }

        let frame_delay = self.rendered.frame_delay_ms(self.current_frame).max(16);
        let elapsed = self.last_frame_at.elapsed();
        let delay = Duration::from_millis(frame_delay);

        if elapsed >= delay {
            if let Some(next_frame) = self.next_frame_index() {
                self.current_frame = next_frame;
                self.last_frame_at = Instant::now();
                self.upload_current_frame();
            }
        }

        let remaining = delay.saturating_sub(self.last_frame_at.elapsed());
        ctx.request_repaint_after(remaining.max(Duration::from_millis(16)));
    }

    pub(crate) fn next_frame_index(&mut self) -> Option<usize> {
        let frame_count = self.rendered.frame_count();
        if frame_count <= 1 {
            return None;
        }

        if self.current_frame + 1 < frame_count {
            return Some(self.current_frame + 1);
        }

        match self.source.loop_count {
            Some(loop_count) if loop_count > 0 && self.completed_loops + 1 >= loop_count => None,
            _ => {
                self.completed_loops += 1;
                Some(0)
            }
        }
    }

    pub(crate) fn reload_current(&mut self) -> Result<(), Box<dyn Error>> {
        self.request_load_path(self.current_navigation_path.clone())
    }

    pub(crate) fn current_directory(&self) -> Option<PathBuf> {
        browser_directory_for_path(&self.current_navigation_path, Some(&self.current_path))
    }

    pub(crate) fn request_filer_directory(&mut self, dir: PathBuf, selected: Option<PathBuf>) {
        if should_defer_filer_request_while_loading(
            self.active_request.is_some(),
            self.active_fs_request_id.is_some(),
            self.active_fs_input_request_id.is_some(),
            self.companion_active_request.is_some(),
            self.active_preload_request_id.is_some(),
        ) {
            self.filer_needs_sync = true;
            self.pending_filer_scroll_to = selected.clone();
            self.filer.snapshot.directory = Some(dir);
            self.filer.snapshot.selected = selected;
            return;
        }
        self.spawn_navigation_workers();
        let Some(filer_tx) = self.filer_tx.clone() else {
            return;
        };
        let request_id = self.alloc_filer_request_id();
        self.filer_needs_sync = false;
        self.filer.snapshot.begin_request(request_id);
        self.pending_filer_scroll_to = selected.clone();
        let _ = filer_tx.send(FilesystemCommand::OpenBrowserDirectory {
            request_id,
            dir,
            selected,
            options: self.filer.take_browser_scan_options(self.navigation_sort),
        });
    }

    fn sync_filer_directory_with_current_path(&mut self) {
        if let Some(rebased) = resolve_navigation_entry_path(&self.current_navigation_path) {
            if rebased != self.current_navigation_path {
                self.current_navigation_path = rebased.clone();
                self.set_filesystem_current(rebased);
            }
        }
        let previous_selected = self.filer.snapshot.selected.clone();
        if let Some((dir, selected)) = self.filer.snapshot.sync_with_navigation(
            &self.current_navigation_path,
            self.pending_navigation_path.as_deref(),
            Some(&self.current_path),
        ) {
            self.request_filer_directory(dir, selected);
        } else if self.filer.snapshot.selected != previous_selected {
            self.pending_filer_scroll_to = self.filer.snapshot.selected.clone();
        }
    }

    pub(crate) fn maybe_sync_visible_filer_with_current_path(&mut self) {
        if self.show_filer || self.show_subfiler {
            self.sync_filer_directory_with_current_path();
        } else {
            self.filer_needs_sync = true;
        }
    }

    fn poll_deferred_filer_sync(&mut self) {
        if !self.filer_needs_sync || !(self.show_filer || self.show_subfiler) {
            return;
        }
        if should_defer_filer_request_while_loading(
            self.active_request.is_some(),
            self.active_fs_request_id.is_some(),
            self.active_fs_input_request_id.is_some(),
            self.companion_active_request.is_some(),
            self.active_preload_request_id.is_some(),
        ) {
            return;
        }
        self.sync_filer_directory_with_current_path();
    }

    pub(crate) fn refresh_current_filer_directory(&mut self) {
        if let Some(dir) = self
            .filer
            .snapshot
            .directory
            .clone()
            .or_else(|| self.current_directory())
        {
            self.request_filer_directory(dir, self.filer.snapshot.selected.clone());
        }
    }

    pub(crate) fn set_filesystem_current(&mut self, path: PathBuf) {
        self.spawn_navigation_workers();
        let request_id = self.alloc_fs_request_id();
        if let Some(fs_tx) = &self.fs_tx {
            let _ = fs_tx.send(FilesystemCommand::SetCurrent { request_id, path });
        }
    }

    pub(crate) fn save_current_as(&mut self, format: SaveFormat) {
        if self.save_dialog.in_progress {
            return;
        }
        let Some(parent) = self
            .save_dialog
            .output_dir
            .clone()
            .or_else(|| self.storage.path.clone())
            .or_else(default_download_dir)
            .or_else(|| self.current_path.parent().map(|path| path.to_path_buf()))
        else {
            self.save_dialog.message = Some("Cannot determine save directory".to_string());
            return;
        };

        let file_name = self.save_dialog.file_name.trim();
        let stem = if file_name.is_empty() {
            default_save_file_name(&self.current_path)
        } else {
            file_name.to_string()
        };
        let output = parent.join(format!("{stem}.{}", format.extension()));
        let source = self.source.clone();
        let (tx, rx) = mpsc::channel();
        self.save_dialog.in_progress = true;
        self.save_dialog.result_rx = Some(rx);
        std::thread::spawn(move || {
            let result = save_loaded_image(&output, &source, format)
                .map(|_| format!("Saved {}", output.display()))
                .map_err(|err| format!("Save failed: {err}"));
            let _ = tx.send(result);
        });
    }

    pub(crate) fn persist_config_async(&self) {
        let config = self.current_config();
        let current_path = self.current_path.clone();
        let config_path = self.config_path.clone();
        std::thread::spawn(move || {
            let _ = save_app_config(&config, Some(&current_path), config_path.as_deref());
        });
    }

    fn color_image_from_canvas(&self, canvas: &Canvas) -> egui::ColorImage {
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

    fn poll_save_result(&mut self) {
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

    fn save_dialog_ui(&mut self, ctx: &egui::Context) {
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

    fn status_panel_ui(&mut self, ctx: &egui::Context) {
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

    fn loading_overlay_ui(&mut self, ctx: &egui::Context) {
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

    fn loading_card_ui(&self, ctx: &egui::Context) {
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

    fn alert_dialog_ui(&mut self, ctx: &egui::Context) {
        let Some(message) = self.overlay.alert_message.clone() else {
            return;
        };

        let mut open = true;
        let mut close_requested = false;
        egui::Window::new("Alert")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(message);
                if ui.button(self.text(UiTextKey::Close)).clicked() {
                    close_requested = true;
                }
            });
        if close_requested || !open {
            self.overlay.alert_message = None;
        }
    }

    fn is_current_portrait_page(&self) -> bool {
        self.source.canvas.height() >= self.source.canvas.width()
    }

    fn desired_manga_companion_path(&self) -> Option<PathBuf> {
        self.desired_companion_navigation_path.clone()
    }

    fn manga_companion_candidate_for_path(
        &self,
        navigation_path: &std::path::Path,
    ) -> Option<PathBuf> {
        if !self.options.manga_mode
            || !self.navigator_ready
            || self.empty_mode
            || self.last_viewport_size.x < self.last_viewport_size.y * 1.4
        {
            return None;
        }
        adjacent_same_branch_navigation_target(
            navigation_path,
            self.navigation_sort,
            self.filer.archive_mode,
            1,
        )
    }

    fn manga_spread_requested(&self) -> bool {
        self.options.manga_mode
            && !self.empty_mode
            && self.navigator_ready
            && self.last_viewport_size.x >= self.last_viewport_size.y * 1.4
            && self.is_current_portrait_page()
    }

    fn refresh_manga_targets(&mut self) -> bool {
        let spread_requested = self.manga_spread_requested();
        let unchanged = self.manga_cached_navigation_path.as_deref()
            == Some(self.current_navigation_path.as_path())
            && self.manga_cached_spread_requested == spread_requested
            && self.manga_cached_navigator_ready == self.navigator_ready
            && self.manga_cached_sort == self.navigation_sort
            && self.manga_cached_archive_mode == self.filer.archive_mode;
        if unchanged {
            return false;
        }

        self.manga_cached_navigation_path = Some(self.current_navigation_path.clone());
        self.manga_cached_spread_requested = spread_requested;
        self.manga_cached_navigator_ready = self.navigator_ready;
        self.manga_cached_sort = self.navigation_sort;
        self.manga_cached_archive_mode = self.filer.archive_mode;
        self.desired_companion_navigation_path = None;
        self.next_manga_navigation_path = None;
        self.prev_manga_navigation_path = None;

        if !spread_requested {
            return true;
        }

        let next_boundary = adjacent_same_branch_navigation_target(
            &self.current_navigation_path,
            self.navigation_sort,
            self.filer.archive_mode,
            1,
        );
        self.desired_companion_navigation_path = next_boundary.clone();
        self.next_manga_navigation_path = next_boundary.as_ref().and_then(|boundary| {
            adjacent_same_branch_navigation_target(
                boundary,
                self.navigation_sort,
                self.filer.archive_mode,
                1,
            )
            .or_else(|| Some(boundary.clone()))
        });

        self.prev_manga_navigation_path = adjacent_same_branch_navigation_target(
            &self.current_navigation_path,
            self.navigation_sort,
            self.filer.archive_mode,
            -1,
        )
        .and_then(|boundary| {
            adjacent_same_branch_navigation_target(
                &boundary,
                self.navigation_sort,
                self.filer.archive_mode,
                -1,
            )
            .or(Some(boundary))
        });
        true
    }

    fn clear_manga_companion(&mut self) {
        self.companion_navigation_path = None;
        self.companion_source = None;
        self.companion_rendered = None;
        self.companion_texture = None;
        self.companion_active_request = None;
        self.companion_texture_display_scale = 1.0;
    }

    fn apply_loaded_companion(
        &mut self,
        navigation_path: PathBuf,
        source: LoadedImage,
        rendered: LoadedImage,
    ) {
        let (canvas, display_scale) = downscale_for_texture_limit(
            rendered.frame_canvas(0),
            self.max_texture_side,
            self.render_options.zoom_method,
        );
        let image = self.color_image_from_canvas(&canvas);
        let texture = self
            .egui_ctx
            .load_texture("manga_companion", image, self.texture_options());
        self.companion_navigation_path = Some(navigation_path);
        self.companion_source = Some(source);
        self.companion_rendered = Some(rendered);
        self.companion_texture = Some(texture);
        self.companion_texture_display_scale = display_scale;
        self.companion_active_request = None;
        self.pending_fit_recalc |= !matches!(self.render_options.zoom_option, ZoomOption::None);
    }

    fn manga_spread_active(&self) -> bool {
        self.options.manga_mode
            && self.last_viewport_size.x >= self.last_viewport_size.y * 1.4
            && self.is_current_portrait_page()
            && self.companion_navigation_path.is_some()
            && self
                .companion_source
                .as_ref()
                .map(|image| image.canvas.height() >= image.canvas.width())
                .unwrap_or(false)
    }

    fn request_companion_load(&mut self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        let request_id = self.alloc_request_id();
        self.companion_active_request = Some(ActiveRenderRequest::Load(request_id));
        self.companion_navigation_path = Some(path.clone());
        self.companion_tx
            .send(RenderCommand::LoadPath {
                request_id,
                path,
                zoom: self.zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
            })
            .map_err(worker_send_error)?;
        Ok(())
    }

    fn request_companion_resize(&mut self) -> Result<(), Box<dyn Error>> {
        if self.companion_source.is_none() {
            return Ok(());
        }
        let request_id = self.alloc_request_id();
        self.companion_active_request = Some(ActiveRenderRequest::Resize(request_id));
        self.companion_tx
            .send(RenderCommand::ResizeCurrent {
                request_id,
                zoom: self.zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
            })
            .map_err(worker_send_error)?;
        Ok(())
    }

    fn try_apply_preloaded_to_companion(&mut self, path: &std::path::Path) -> bool {
        if !manga_companion_matches_preloaded(
            path,
            self.preloaded_companion_navigation_path.as_deref(),
        ) {
            return false;
        }
        let (Some(source), Some(rendered), Some(texture)) = (
            self.preloaded_companion_source.take(),
            self.preloaded_companion_rendered.take(),
            self.preloaded_companion_texture.take(),
        ) else {
            return false;
        };
        self.companion_navigation_path = Some(path.to_path_buf());
        self.companion_source = Some(source);
        self.companion_rendered = Some(rendered);
        self.companion_texture = Some(texture);
        self.companion_texture_display_scale = self.preloaded_companion_texture_display_scale;
        self.preloaded_companion_navigation_path = None;
        self.preloaded_companion_texture_display_scale = 1.0;
        self.companion_active_request = None;
        self.pending_fit_recalc |= !matches!(self.render_options.zoom_option, ZoomOption::None);
        true
    }

    fn sync_manga_companion(&mut self, ctx: &egui::Context) {
        if self.active_request.is_some() {
            return;
        }
        let desired = self.desired_manga_companion_path();
        if desired == self.companion_navigation_path && self.companion_rendered.is_some() {
            return;
        }

        if desired.is_none() {
            self.companion_navigation_path = None;
            self.companion_source = None;
            self.companion_rendered = None;
            self.companion_texture = None;
            self.companion_active_request = None;
            self.pending_fit_recalc |= !matches!(self.render_options.zoom_option, ZoomOption::None);
            return;
        }

        if self.try_apply_preloaded_to_companion(desired.as_deref().unwrap()) {
            ctx.request_repaint();
            return;
        }

        if self.companion_active_request.is_none() {
            let _ = self.request_companion_load(desired.unwrap());
            ctx.request_repaint();
        }
    }

    fn sync_manga_companion_if_targets_changed(&mut self, ctx: &egui::Context) {
        if self.refresh_manga_targets() {
            self.sync_manga_companion(ctx);
        }
    }

    fn manga_navigation_target(&mut self, forward: bool) -> Option<PathBuf> {
        self.refresh_manga_targets();
        if !self.navigator_ready || !self.manga_spread_active() {
            return None;
        }
        if forward {
            self.next_manga_navigation_path.clone()
        } else {
            self.prev_manga_navigation_path.clone()
        }
    }

    pub(crate) fn next_image(&mut self) -> Result<(), Box<dyn Error>> {
        self.cancel_pending_single_click_navigation();
        if !self.can_trigger_navigation() {
            return Ok(());
        }
        if let Some(target) = self.manga_navigation_target(true) {
            self.request_load_path(target)?;
            self.last_navigation_at = Some(Instant::now());
            return Ok(());
        }
        self.request_navigation(FilesystemCommand::Next {
            request_id: 0,
            policy: self.end_of_folder,
        })?;
        self.last_navigation_at = Some(Instant::now());
        Ok(())
    }

    pub(crate) fn prev_image(&mut self) -> Result<(), Box<dyn Error>> {
        self.cancel_pending_single_click_navigation();
        if !self.can_trigger_navigation() {
            return Ok(());
        }
        if let Some(target) = self.manga_navigation_target(false) {
            self.request_load_path(target)?;
            self.last_navigation_at = Some(Instant::now());
            return Ok(());
        }
        self.request_navigation(FilesystemCommand::Prev {
            request_id: 0,
            policy: self.end_of_folder,
        })?;
        self.last_navigation_at = Some(Instant::now());
        Ok(())
    }

    pub(crate) fn first_image(&mut self) -> Result<(), Box<dyn Error>> {
        self.cancel_pending_single_click_navigation();
        if !self.can_trigger_navigation() {
            return Ok(());
        }
        self.request_navigation(FilesystemCommand::First { request_id: 0 })?;
        self.last_navigation_at = Some(Instant::now());
        Ok(())
    }

    pub(crate) fn last_image(&mut self) -> Result<(), Box<dyn Error>> {
        self.cancel_pending_single_click_navigation();
        if !self.can_trigger_navigation() {
            return Ok(());
        }
        self.request_navigation(FilesystemCommand::Last { request_id: 0 })?;
        self.last_navigation_at = Some(Instant::now());
        Ok(())
    }

    fn can_trigger_navigation(&self) -> bool {
        self.last_navigation_at
            .map(|last| last.elapsed() >= NAVIGATION_REPEAT_INTERVAL)
            .unwrap_or(true)
    }

    pub(crate) fn request_load_path(&mut self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        self.request_load_target(path.clone(), path)
    }

    pub(crate) fn request_load_target(
        &mut self,
        navigation_path: PathBuf,
        load_request_path: PathBuf,
    ) -> Result<(), Box<dyn Error>> {
        let branch_changed = navigation_branch_path(&self.current_navigation_path)
            != navigation_branch_path(&navigation_path);
        let switching_image = self.current_navigation_path != navigation_path;
        if branch_changed {
            self.clear_manga_companion();
        }
        if self.try_take_preloaded(&navigation_path) {
            return Ok(());
        }
        self.invalidate_preload();
        if switching_image {
            self.zoom_factor = 1.0;
            self.zoom = 1.0;
        }
        if branch_changed {
            self.show_loading_texture(true); // フォルダ変わった時だけリセット
            self.clear_current_image_display();
        }
        //        self.show_loading_texture(branch_changed);
        //        self.clear_current_image_display();
        let request_id = self.alloc_request_id();
        self.active_request = Some(ActiveRenderRequest::Load(request_id));
        self.pending_navigation_path = Some(navigation_path.clone());
        if !should_defer_filer_sync_for_navigation(&navigation_path, Some(&load_request_path)) {
            self.maybe_sync_visible_filer_with_current_path();
        } else {
            self.filer_needs_sync = true;
        }
        self.pending_fit_recalc = !matches!(self.render_options.zoom_option, ZoomOption::None);
        self.overlay
            .set_loading_message(format!("Loading {}", navigation_path.display()));
        let load_zoom = if switching_image { 1.0 } else { self.zoom };
        if let Some(companion_path) = self.manga_companion_candidate_for_path(&navigation_path) {
            self.worker_tx
                .send(RenderCommand::LoadSpread {
                    request_id,
                    path: load_request_path,
                    companion_path,
                    zoom: load_zoom,
                    method: self.render_options.zoom_method,
                    scale_mode: self.render_options.scale_mode,
                })
                .map_err(worker_send_error)?;
        } else {
            self.worker_tx
                .send(RenderCommand::LoadPath {
                    request_id,
                    path: load_request_path,
                    zoom: load_zoom,
                    method: self.render_options.zoom_method,
                    scale_mode: self.render_options.scale_mode,
                })
                .map_err(worker_send_error)?;
        }
        Ok(())
    }

    pub(crate) fn request_resize_current(&mut self) -> Result<(), Box<dyn Error>> {
        if matches!(self.active_request, Some(ActiveRenderRequest::Load(_))) {
            self.pending_resize_after_load = true;
            return Ok(());
        }
        if matches!(self.active_request, Some(ActiveRenderRequest::Resize(_))) {
            self.pending_resize_after_render = true;
            return Ok(());
        }
        if matches!(self.render_options.scale_mode, RenderScaleMode::FastGpu) {
            self.rendered = self.source.clone();
            self.current_frame = self
                .current_frame
                .min(self.rendered.frame_count().saturating_sub(1));
            self.upload_current_frame();
            self.overlay.clear_loading_message();
            if self.companion_source.is_some() && self.companion_rendered.is_none() {
                if let Some(path) = self.companion_navigation_path.clone() {
                    let _ = self.request_companion_load(path);
                }
            }
            return Ok(());
        }
        self.invalidate_preload();
        let request_id = self.alloc_request_id();
        self.active_request = Some(ActiveRenderRequest::Resize(request_id));
        self.overlay
            .set_loading_message(format!("Rendering {:.0}%", self.zoom * 100.0));
        self.worker_tx
            .send(RenderCommand::ResizeCurrent {
                request_id,
                zoom: self.zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
            })
            .map_err(worker_send_error)?;
        if let Some(path) = self.companion_navigation_path.clone() {
            if self.companion_source.is_some() {
                let _ = self.request_companion_resize();
            } else {
                let _ = self.request_companion_load(path);
            }
        }
        Ok(())
    }

    fn alloc_request_id(&mut self) -> u64 {
        self.next_request_id += 1;
        self.next_request_id
    }

    fn alloc_fs_request_id(&mut self) -> u64 {
        self.next_fs_request_id += 1;
        self.next_fs_request_id
    }

    fn alloc_filer_request_id(&mut self) -> u64 {
        self.next_filer_request_id += 1;
        self.next_filer_request_id
    }

    fn alloc_thumbnail_request_id(&mut self) -> u64 {
        self.next_thumbnail_request_id += 1;
        self.next_thumbnail_request_id
    }

    fn alloc_preload_request_id(&mut self) -> u64 {
        self.next_preload_request_id += 1;
        self.next_preload_request_id
    }

    fn invalidate_preload(&mut self) {
        self.active_preload_request_id = None;
        self.pending_preload_navigation_path = None;
        self.preloaded_navigation_path = None;
        self.preloaded_load_path = None;
        self.preloaded_source = None;
        self.preloaded_rendered = None;
        self.preloaded_companion_navigation_path = None;
        self.preloaded_companion_source = None;
        self.preloaded_companion_rendered = None;
        self.preloaded_companion_texture = None;
        self.preloaded_companion_texture_display_scale = 1.0;
        self.next_texture = None;
        self.next_texture_display_scale = 1.0;
    }

    fn spawn_navigation_workers(&mut self) {
        let shared_cache = self
            .shared_filesystem_cache
            .get_or_insert_with(|| {
                new_shared_filesystem_cache(self.navigation_sort, self.filer.archive_mode)
            })
            .clone();
        let shared_browser_state = self
            .shared_browser_worker_state
            .get_or_insert_with(new_shared_browser_worker_state)
            .clone();
        if self.fs_tx.is_none() || self.fs_rx.is_none() {
            let (tx, rx) = spawn_filesystem_worker(
                self.navigation_sort,
                self.filer.archive_mode,
                shared_cache.clone(),
                shared_browser_state.clone(),
            );
            self.fs_tx = Some(tx);
            self.fs_rx = Some(rx);
        }
        if self.filer_tx.is_none() || self.filer_rx.is_none() {
            let (tx, rx) = spawn_browser_query_worker(shared_cache, shared_browser_state);
            self.filer_tx = Some(tx);
            self.filer_rx = Some(rx);
        }
        if self.thumbnail_tx.is_none() || self.thumbnail_rx.is_none() {
            let (tx, rx) = spawn_thumbnail_worker();
            self.thumbnail_tx = Some(tx);
            self.thumbnail_rx = Some(rx);
        }
    }

    fn init_filesystem(&mut self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        self.spawn_navigation_workers();
        self.deferred_filesystem_sync_frame = None;
        let Some(fs_tx) = self.fs_tx.clone() else {
            return Ok(());
        };
        let request_id = self.alloc_fs_request_id();
        self.active_fs_request_id = Some(request_id);
        self.overlay
            .set_loading_message(format!("Scanning {}", path.display()));
        fs_tx
            .send(FilesystemCommand::Init { request_id, path })
            .map_err(filesystem_send_error)?;
        Ok(())
    }

    pub(crate) fn request_source_input(&mut self, input: PathBuf) -> Result<(), Box<dyn Error>> {
        self.spawn_navigation_workers();
        let Some(fs_tx) = self.fs_tx.clone() else {
            return Ok(());
        };
        if let Some(active_request_id) = self.active_fs_input_request_id.take() {
            let _ = fs_tx.send(FilesystemCommand::CancelSourceInput {
                request_id: active_request_id,
            });
        }
        let request_id = self.alloc_fs_request_id();
        self.active_fs_input_request_id = Some(request_id);
        self.overlay
            .set_loading_message(format!("Opening {}", input.display()));
        fs_tx
            .send(FilesystemCommand::ResolveSourceInput { request_id, input })
            .map_err(filesystem_send_error)?;
        Ok(())
    }

    fn request_navigation(&mut self, mut command: FilesystemCommand) -> Result<(), Box<dyn Error>> {
        self.spawn_navigation_workers();
        if !self.navigator_ready {
            self.queued_navigation = Some(command);
            return Ok(());
        }
        if self.active_fs_request_id.is_some() {
            self.queued_navigation = Some(command);
            return Ok(());
        }
        let Some(fs_tx) = self.fs_tx.clone() else {
            self.queued_navigation = Some(command);
            return Ok(());
        };
        let request_id = self.alloc_fs_request_id();
        self.active_fs_request_id = Some(request_id);
        command = match command {
            FilesystemCommand::Init { path, .. } => FilesystemCommand::Init { request_id, path },
            FilesystemCommand::SetCurrent { path, .. } => {
                FilesystemCommand::SetCurrent { request_id, path }
            }
            FilesystemCommand::Next { policy, .. } => {
                FilesystemCommand::Next { request_id, policy }
            }
            FilesystemCommand::Prev { policy, .. } => {
                FilesystemCommand::Prev { request_id, policy }
            }
            FilesystemCommand::First { .. } => FilesystemCommand::First { request_id },
            FilesystemCommand::Last { .. } => FilesystemCommand::Last { request_id },
            FilesystemCommand::OpenBrowserDirectory {
                dir,
                selected,
                options,
                ..
            } => FilesystemCommand::OpenBrowserDirectory {
                request_id,
                dir,
                selected,
                options,
            },
            FilesystemCommand::ResolveSourceInput { input, .. } => {
                FilesystemCommand::ResolveSourceInput { request_id, input }
            }
            FilesystemCommand::CancelSourceInput { .. } => {
                FilesystemCommand::CancelSourceInput { request_id }
            }
        };
        self.overlay.set_loading_message("Scanning folder...");
        fs_tx.send(command).map_err(filesystem_send_error)?;
        Ok(())
    }

    fn apply_loaded_result(
        &mut self,
        path: Option<PathBuf>,
        source: LoadedImage,
        rendered: LoadedImage,
        companion: Option<(PathBuf, LoadedImage, LoadedImage)>,
    ) {
        let previous_navigation_path = self.current_navigation_path.clone();
        if let Some(pending_navigation_path) = self.pending_navigation_path.take() {
            self.current_navigation_path = if path
                .as_ref()
                .is_some_and(|_| is_browser_container(&pending_navigation_path))
            {
                resolve_navigation_entry_path(&pending_navigation_path)
                    .or_else(|| path.clone())
                    .unwrap_or(pending_navigation_path)
            } else {
                pending_navigation_path
            };
        }
        let loaded_path = path.clone();
        if let Some(path) = path {
            let request_id = self.alloc_fs_request_id();
            let folder_changed = navigation_branch_path(&previous_navigation_path)
                != navigation_branch_path(&self.current_navigation_path);
            self.current_path = path.clone();
            self.save_dialog.file_name = default_save_file_name(&path);
            if folder_changed {
                self.clear_manga_companion();
                self.prev_texture = None;
                self.next_texture = None;
                self.next_texture_display_scale = 1.0;
            }
            if let Some(fs_tx) = &self.fs_tx {
                let _ = fs_tx.send(FilesystemCommand::SetCurrent {
                    request_id,
                    path: self.current_navigation_path.clone(),
                });
            }
            self.maybe_sync_visible_filer_with_current_path();
        }
        self.source = source;
        self.rendered = rendered;
        self.pending_fit_recalc |= !matches!(self.render_options.zoom_option, ZoomOption::None);
        self.current_frame = self
            .current_frame
            .min(self.rendered.frame_count().saturating_sub(1));
        self.completed_loops = 0;
        self.last_frame_at = Instant::now();
        self.active_request = None;

        let source_size = vec2(
            self.source.canvas.width() as f32,
            self.source.canvas.height() as f32,
        );
        let defer_precise_display =
            self.maybe_defer_precise_display(source_size, loaded_path.as_deref());
        if defer_precise_display {
            let _ = self.request_resize_current();
        } else {
            self.rebuild_current_texture();
            if self.active_fs_request_id.is_none() {
                self.overlay.clear_loading_message();
            }
        }

        if !self.navigator_ready && self.active_fs_request_id.is_none() {
            if self.deferred_filesystem_init_path.is_some() {
                self.deferred_filesystem_init_path =
                    Some(loaded_path.unwrap_or_else(|| self.current_navigation_path.clone()));
                self.defer_initial_filesystem_sync();
            }
        }
        self.refresh_manga_targets();
        if let Some((navigation_path, source, rendered)) = companion {
            self.apply_loaded_companion(navigation_path, source, rendered);
        } else {
            let egui_ctx = self.egui_ctx.clone();
            self.sync_manga_companion(&egui_ctx);
        }
        self.schedule_preload();
        if self.pending_resize_after_load {
            self.pending_resize_after_load = false;
            let _ = self.request_resize_current();
        } else if self.pending_resize_after_render {
            self.pending_resize_after_render = false;
            let _ = self.request_resize_current();
        }
    }

    fn next_preload_candidate(&self) -> Option<PathBuf> {
        let step = if self.manga_spread_active() { 2 } else { 1 };
        adjacent_entry(
            &self.current_navigation_path,
            self.navigation_sort,
            self.filer.archive_mode,
            step,
        )
    }

    fn schedule_preload(&mut self) {
        if self.empty_mode || self.active_request.is_some() {
            return;
        }
        if !self.navigator_ready {
            return;
        }
        self.refresh_manga_targets();
        let desired_companion = self.desired_manga_companion_path();
        if should_defer_preload_for_manga_low_io(
            self.options.manga_mode,
            &self.current_navigation_path,
            desired_companion.as_deref(),
            self.companion_rendered.is_some(),
        ) {
            return;
        }
        let Some(path) = self.next_preload_candidate() else {
            return;
        };
        if !should_allow_preload_for_path(&self.current_navigation_path, &path) {
            return;
        }
        if desired_companion
            .as_ref()
            .is_some_and(|companion| companion == &path)
            && (self.companion_active_request.is_some() || self.companion_rendered.is_none())
        {
            return;
        }
        if self.preloaded_navigation_path.as_ref() == Some(&path)
            || self.pending_preload_navigation_path.as_ref() == Some(&path)
        {
            return;
        }
        let request_id = self.alloc_preload_request_id();
        self.active_preload_request_id = Some(request_id);
        self.pending_preload_navigation_path = Some(path.clone());
        if let Some(companion_path) = self.manga_companion_candidate_for_path(&path) {
            let _ = self.preload_tx.send(RenderCommand::LoadSpread {
                request_id,
                path,
                companion_path,
                zoom: self.zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
            });
        } else {
            let _ = self.preload_tx.send(RenderCommand::LoadPath {
                request_id,
                path,
                zoom: self.zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
            });
        }
    }

    fn try_take_preloaded(&mut self, path: &std::path::Path) -> bool {
        let matches_navigation =
            preloaded_navigation_matches(self.preloaded_navigation_path.as_deref(), path);
        if !matches_navigation {
            return false;
        }

        let source = self.preloaded_source.take();
        let rendered = self.preloaded_rendered.take();
        let load_path = self.preloaded_load_path.take();
        self.preloaded_navigation_path = None;
        self.pending_preload_navigation_path = None;
        if let (Some(source), Some(rendered)) = (source, rendered) {
            if let Some(texture) = self.next_texture.take() {
                self.current_texture = texture;
                self.current_texture_is_default = false;
                self.texture_display_scale = self.next_texture_display_scale;
            }
            self.pending_navigation_path = Some(path.to_path_buf());
            self.overlay.clear_loading_message();
            self.apply_loaded_result(load_path, source, rendered, None);
            return true;
        }
        false
    }

    fn respawn_render_worker(&mut self) {
        let (worker_tx, worker_rx, worker_join) =
            spawn_render_worker(self.source.clone(), RenderWorkerPriority::Primary);
        self.worker_tx = worker_tx;
        self.worker_rx = worker_rx;
        self.worker_join = Some(worker_join);
        self.active_request = None;
    }

    fn respawn_companion_worker(&mut self) {
        let seed = self
            .companion_source
            .clone()
            .unwrap_or_else(|| self.source.clone());
        let (tx, rx, join) = spawn_render_worker(seed, RenderWorkerPriority::Companion);
        self.companion_tx = tx;
        self.companion_rx = rx;
        self.companion_join = Some(join);
        self.companion_active_request = None;
    }

    fn respawn_preload_worker(&mut self) {
        let (tx, rx, join) =
            spawn_render_worker(self.source.clone(), RenderWorkerPriority::Preload);
        self.preload_tx = tx;
        self.preload_rx = rx;
        self.preload_join = Some(join);
        self.invalidate_preload();
    }

    pub(crate) fn respawn_filesystem_worker(&mut self) {
        let shared_cache = self
            .shared_filesystem_cache
            .get_or_insert_with(|| {
                new_shared_filesystem_cache(self.navigation_sort, self.filer.archive_mode)
            })
            .clone();
        let shared_browser_state = self
            .shared_browser_worker_state
            .get_or_insert_with(new_shared_browser_worker_state)
            .clone();
        let (tx, rx) = spawn_filesystem_worker(
            self.navigation_sort,
            self.filer.archive_mode,
            shared_cache,
            shared_browser_state,
        );
        self.fs_tx = Some(tx);
        self.fs_rx = Some(rx);
        self.navigator_ready = false;
        self.active_fs_request_id = None;
        self.active_fs_input_request_id = None;
        let _ = self.init_filesystem(self.current_navigation_path.clone());
    }

    fn respawn_filer_worker(&mut self) {
        let shared_cache = self
            .shared_filesystem_cache
            .get_or_insert_with(|| {
                new_shared_filesystem_cache(self.navigation_sort, self.filer.archive_mode)
            })
            .clone();
        let shared_browser_state = self
            .shared_browser_worker_state
            .get_or_insert_with(new_shared_browser_worker_state)
            .clone();
        let (tx, rx) = spawn_browser_query_worker(shared_cache, shared_browser_state);
        self.filer_tx = Some(tx);
        self.filer_rx = Some(rx);
        self.filer.snapshot.clear_pending_request();
        self.filer.mark_query_options_dirty();
        if let Some(dir) = self
            .filer
            .snapshot
            .directory
            .clone()
            .or_else(|| self.current_directory())
        {
            self.request_filer_directory(dir, self.filer.snapshot.selected.clone());
        }
    }

    fn respawn_thumbnail_worker(&mut self) {
        let (tx, rx) = spawn_thumbnail_worker();
        self.thumbnail_tx = Some(tx);
        self.thumbnail_rx = Some(rx);
        self.thumbnail_pending.clear();
    }

    fn poll_worker(&mut self) {
        loop {
            match self.worker_rx.try_recv() {
                Ok(RenderResult::Loaded {
                    request_id,
                    path,
                    source,
                    rendered,
                }) => {
                    let Some(active_request) = self.active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if !request_matches {
                        continue;
                    }
                    self.apply_loaded_result(path, source, rendered, None);
                }
                Ok(RenderResult::LoadedSpread {
                    request_id,
                    path,
                    source,
                    rendered,
                    companion,
                }) => {
                    let Some(active_request) = self.active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if !request_matches {
                        continue;
                    }
                    let companion = companion.map(|(_, source, rendered)| {
                        let navigation_base = self
                            .pending_navigation_path
                            .as_deref()
                            .unwrap_or(self.current_navigation_path.as_path());
                        let navigation_path = self
                            .manga_companion_candidate_for_path(navigation_base)
                            .or_else(|| self.desired_manga_companion_path())
                            .unwrap_or_else(|| path.clone());
                        (navigation_path, source, rendered)
                    });
                    self.apply_loaded_result(Some(path), source, rendered, companion);
                }
                Ok(RenderResult::Failed {
                    request_id,
                    path,
                    message,
                }) => {
                    let Some(active_request) = self.active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if !request_matches {
                        continue;
                    }
                    let failed_during_load = matches!(active_request, ActiveRenderRequest::Load(_));
                    self.pending_navigation_path = None;
                    let label = path
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .and_then(|name| name.to_str())
                        .unwrap_or("image");
                    self.save_dialog.message = Some(format!("Load failed: {label}: {message}"));
                    self.clear_current_image_display();
                    self.show_loading_texture(true);
                    self.overlay.clear_loading_message();
                    self.active_request = None;
                    if !self.navigator_ready && self.active_fs_request_id.is_none() {
                        if self.deferred_filesystem_init_path.is_some() {
                            self.deferred_filesystem_init_path =
                                Some(self.current_navigation_path.clone());
                            self.defer_initial_filesystem_sync();
                        }
                    }
                    if failed_during_load {
                        let _ = self.next_image();
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.overlay.alert_message = Some("render worker disconnected".to_string());
                    self.overlay.clear_loading_message();
                    self.respawn_render_worker();
                    if !self.empty_mode {
                        let _ = self.request_load_path(self.current_navigation_path.clone());
                    }
                    break;
                }
            }
        }
    }

    fn poll_preload_worker(&mut self) {
        loop {
            match self.preload_rx.try_recv() {
                Ok(RenderResult::Loaded {
                    request_id,
                    path,
                    source,
                    rendered,
                }) => {
                    if self.active_preload_request_id != Some(request_id) {
                        continue;
                    }
                    self.active_preload_request_id = None;
                    self.preloaded_navigation_path = self.pending_preload_navigation_path.take();
                    self.preloaded_load_path = path;
                    let texture_name =
                        self.texture_name_for_path(self.preloaded_load_path.as_deref());
                    let (texture, display_scale) =
                        self.build_texture_from_canvas(&texture_name, rendered.frame_canvas(0));
                    self.next_texture = Some(texture);
                    self.next_texture_display_scale = display_scale;
                    self.preloaded_source = Some(source);
                    self.preloaded_rendered = Some(rendered);
                }
                Ok(RenderResult::LoadedSpread {
                    request_id,
                    path,
                    source,
                    rendered,
                    companion,
                }) => {
                    if self.active_preload_request_id != Some(request_id) {
                        continue;
                    }
                    self.active_preload_request_id = None;
                    self.preloaded_navigation_path = self.pending_preload_navigation_path.take();
                    self.preloaded_load_path = Some(path);
                    let texture_name =
                        self.texture_name_for_path(self.preloaded_load_path.as_deref());
                    let (texture, display_scale) =
                        self.build_texture_from_canvas(&texture_name, rendered.frame_canvas(0));
                    self.next_texture = Some(texture);
                    self.next_texture_display_scale = display_scale;
                    self.preloaded_source = Some(source);
                    self.preloaded_rendered = Some(rendered);
                    if let Some((companion_path, companion_source, companion_rendered)) = companion
                    {
                        let texture_name = self.texture_name_for_path(Some(&companion_path));
                        let (texture, display_scale) = self.build_texture_from_canvas(
                            &texture_name,
                            companion_rendered.frame_canvas(0),
                        );
                        self.preloaded_companion_navigation_path = Some(companion_path);
                        self.preloaded_companion_source = Some(companion_source);
                        self.preloaded_companion_rendered = Some(companion_rendered);
                        self.preloaded_companion_texture = Some(texture);
                        self.preloaded_companion_texture_display_scale = display_scale;
                    } else {
                        self.preloaded_companion_navigation_path = None;
                        self.preloaded_companion_source = None;
                        self.preloaded_companion_rendered = None;
                        self.preloaded_companion_texture = None;
                        self.preloaded_companion_texture_display_scale = 1.0;
                    }
                }
                Ok(RenderResult::Failed { request_id, .. }) => {
                    if self.active_preload_request_id == Some(request_id) {
                        self.active_preload_request_id = None;
                        self.pending_preload_navigation_path = None;
                        self.preloaded_navigation_path = None;
                        self.preloaded_load_path = None;
                        self.preloaded_source = None;
                        self.preloaded_rendered = None;
                        self.preloaded_companion_navigation_path = None;
                        self.preloaded_companion_source = None;
                        self.preloaded_companion_rendered = None;
                        self.preloaded_companion_texture = None;
                        self.preloaded_companion_texture_display_scale = 1.0;
                        self.next_texture = None;
                        self.next_texture_display_scale = 1.0;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.respawn_preload_worker();
                    break;
                }
            }
        }
    }

    fn poll_companion_worker(&mut self) {
        loop {
            match self.companion_rx.try_recv() {
                Ok(RenderResult::Loaded {
                    request_id,
                    path,
                    source,
                    rendered,
                }) => {
                    let Some(active_request) = self.companion_active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if !request_matches {
                        continue;
                    }
                    let layout_changed = path.is_some()
                        || self
                            .companion_source
                            .as_ref()
                            .map(|image| {
                                image.canvas.width() != source.canvas.width()
                                    || image.canvas.height() != source.canvas.height()
                            })
                            .unwrap_or(true);

                    let (canvas, display_scale) = downscale_for_texture_limit(
                        rendered.frame_canvas(0),
                        self.max_texture_side,
                        self.render_options.zoom_method,
                    );
                    let image = self.color_image_from_canvas(&canvas);
                    let texture_options = self.texture_options();
                    let texture = if path.is_none() {
                        if let Some(texture) = &mut self.companion_texture {
                            texture.set(image, texture_options);
                            texture.clone()
                        } else {
                            self.egui_ctx
                                .load_texture("manga_companion", image, texture_options)
                        }
                    } else {
                        self.egui_ctx
                            .load_texture("manga_companion", image, texture_options)
                    };
                    self.companion_texture = Some(texture);
                    self.companion_source = Some(source);
                    self.companion_rendered = Some(rendered);
                    self.companion_texture_display_scale = display_scale;
                    if layout_changed {
                        self.pending_fit_recalc |=
                            !matches!(self.render_options.zoom_option, ZoomOption::None);
                    }
                    self.companion_active_request = None;
                    self.schedule_preload();
                }
                Ok(RenderResult::Failed { request_id, .. }) => {
                    let Some(active_request) = self.companion_active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if request_matches {
                        self.companion_source = None;
                        self.companion_rendered = None;
                        self.companion_texture = None;
                        self.companion_active_request = None;
                    }
                }
                Ok(RenderResult::LoadedSpread { .. }) => continue,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.companion_source = None;
                    self.companion_rendered = None;
                    self.companion_texture = None;
                    self.respawn_companion_worker();
                    if let Some(path) = self.desired_manga_companion_path() {
                        let _ = self.request_companion_load(path);
                    }
                    break;
                }
            }
        }
    }

    fn poll_filesystem(&mut self) {
        loop {
            let result = match self.fs_rx.as_ref() {
                Some(rx) => rx.try_recv(),
                None => return,
            };
            match result {
                Ok(FilesystemResult::NavigatorReady {
                    request_id,
                    navigation_path,
                    load_path,
                }) => {
                    if self.active_fs_request_id == Some(request_id) {
                        self.navigator_ready = true;
                        self.active_fs_request_id = None;
                        self.startup_phase = StartupPhase::MultiViewer;
                        match (navigation_path, load_path) {
                            (Some(navigation_path), Some(load_path)) => {
                                self.empty_mode = false;
                                if self.current_navigation_path != navigation_path
                                    || self.current_path != load_path
                                {
                                    let _ = self.request_load_target(navigation_path, load_path);
                                }
                            }
                            (Some(navigation_path), None) => {
                                self.current_navigation_path = navigation_path;
                            }
                            _ => {
                                self.empty_mode = true;
                                self.show_filer = true;
                                self.overlay
                                    .set_loading_message("No displayable file found");
                            }
                        }
                        if self.active_request.is_none() && !self.empty_mode {
                            self.overlay.clear_loading_message();
                        }
                    }
                }
                Ok(FilesystemResult::CurrentSet) => {}
                Ok(FilesystemResult::PathResolved {
                    request_id,
                    navigation_path,
                    load_path,
                }) => {
                    if self.active_fs_request_id == Some(request_id) {
                        self.empty_mode = false;
                        self.startup_phase = StartupPhase::MultiViewer;
                        if self.current_navigation_path != navigation_path
                            || self.current_path != load_path
                        {
                            let _ = self.request_load_target(navigation_path, load_path);
                        }
                        self.active_fs_request_id = None;
                    }
                }
                Ok(FilesystemResult::NoPath { request_id }) => {
                    if self.active_fs_request_id == Some(request_id) {
                        self.startup_phase = StartupPhase::MultiViewer;
                        self.overlay
                            .set_loading_message("No displayable file found");
                        self.show_filer = true;
                        self.active_fs_request_id = None;
                    }
                }
                Ok(FilesystemResult::InputPathResolved { request_id, path }) => {
                    if self.active_fs_input_request_id == Some(request_id) {
                        self.active_fs_input_request_id = None;
                        self.empty_mode = false;
                        self.pending_fit_recalc = true;
                        let _ = self.request_load_path(path);
                    }
                }
                Ok(FilesystemResult::InputPathFailed { request_id, input }) => {
                    if self.active_fs_input_request_id == Some(request_id) {
                        self.active_fs_input_request_id = None;
                        self.overlay
                            .set_loading_message(format!("Failed to open {}", input.display()));
                    }
                }
                Ok(FilesystemResult::InputPathCancelled { request_id, input }) => {
                    if self.active_fs_input_request_id == Some(request_id) {
                        self.active_fs_input_request_id = None;
                        self.overlay
                            .set_loading_message(format!("Cancelled opening {}", input.display()));
                    }
                }
                Ok(FilesystemResult::BrowserReset { .. })
                | Ok(FilesystemResult::BrowserAppend { .. })
                | Ok(FilesystemResult::ThumbnailHint { .. })
                | Ok(FilesystemResult::BrowserFinish { .. })
                | Ok(FilesystemResult::BrowserFailed { .. }) => {}
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.overlay
                        .set_loading_message("filesystem worker disconnected");
                    self.respawn_filesystem_worker();
                    break;
                }
            }
        }
        if self.active_fs_request_id.is_none() {
            if let Some(command) = self.queued_navigation.take() {
                let _ = self.request_navigation(command);
            }
        }
    }

    fn poll_filer_worker(&mut self) {
        loop {
            let result = match self.filer_rx.as_ref() {
                Some(rx) => rx.try_recv(),
                None => return,
            };
            match result {
                Ok(result) => {
                    if let FilesystemResult::ThumbnailHint {
                        request_id: _,
                        paths,
                        max_side,
                    } = &result
                    {
                        if should_defer_thumbnail_io(
                            &self.current_navigation_path,
                            self.active_request.is_some(),
                            self.companion_active_request.is_some(),
                            self.active_preload_request_id.is_some(),
                        ) {
                            continue;
                        }
                        self.queue_thumbnail_hints(paths, *max_side);
                        continue;
                    }
                    let previous_selected = self.filer.snapshot.selected.clone();
                    let applied = self.filer.snapshot.apply_query_result(
                        result,
                        &self.current_navigation_path,
                        self.pending_navigation_path.as_deref(),
                        Some(&self.current_path),
                    );
                    if applied && self.filer.snapshot.selected != previous_selected {
                        self.pending_filer_scroll_to = self.filer.snapshot.selected.clone();
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.respawn_filer_worker();
                    break;
                }
            }
        }
    }

    fn poll_thumbnail_worker(&mut self) {
        loop {
            let result = match self.thumbnail_rx.as_ref() {
                Some(rx) => rx.try_recv(),
                None => return,
            };
            match result {
                Ok(ThumbnailResult::Ready {
                    _request_id: _,
                    path,
                    max_side,
                    image,
                }) => {
                    self.thumbnail_pending.remove(&path);
                    let texture = self.egui_ctx.load_texture(
                        format!("thumb:{}", path.display()),
                        image,
                        TextureOptions::LINEAR,
                    );
                    self.thumbnail_cache
                        .insert(path, CachedThumbnail { texture, max_side });
                }
                Ok(ThumbnailResult::Failed {
                    _request_id: _,
                    path,
                    _max_side: _,
                    ..
                }) => {
                    self.thumbnail_pending.remove(&path);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.respawn_thumbnail_worker();
                    break;
                }
            }
        }
    }

    pub(crate) fn ensure_thumbnail(&mut self, path: &std::path::Path, max_side: u32) {
        if should_defer_thumbnail_io(
            &self.current_navigation_path,
            self.active_request.is_some(),
            self.companion_active_request.is_some(),
            self.active_preload_request_id.is_some(),
        ) {
            return;
        }
        self.spawn_navigation_workers();
        let Some(thumbnail_tx) = self.thumbnail_tx.clone() else {
            return;
        };
        if self
            .thumbnail_cache
            .get(path)
            .is_some_and(|cached| cached.max_side >= max_side)
        {
            return;
        }
        if self
            .thumbnail_pending
            .get(path)
            .is_some_and(|pending| *pending >= max_side)
        {
            return;
        }
        let request_id = self.alloc_thumbnail_request_id();
        let path = path.to_path_buf();
        self.thumbnail_pending.insert(path.clone(), max_side);
        let _ = thumbnail_tx.send(ThumbnailCommand::Generate {
            request_id,
            path,
            max_side,
        });
    }

    fn queue_thumbnail_hints(&mut self, paths: &[PathBuf], max_side: u32) {
        for path in paths {
            self.ensure_thumbnail(path, max_side);
        }
    }

    fn sync_window_state(&mut self, ctx: &egui::Context) {
        let viewport = ctx.input(|i| i.viewport().clone());
        self.startup_window_sync_frames += 1;

        if let Some(fullscreen) = viewport.fullscreen {
            self.window_options.fullscreen = fullscreen;
        }

        if self.window_options.fullscreen || self.startup_window_sync_frames < 20 {
            return;
        }

        if self.window_options.remember_size {
            if let Some(inner_rect) = viewport.inner_rect {
                self.window_options.size = crate::ui::viewer::options::WindowSize::Exact {
                    width: inner_rect.width(),
                    height: inner_rect.height(),
                };
            }
        }

        if self.window_options.remember_position {
            if let Some(outer_rect) = viewport.outer_rect {
                self.window_options.start_position = WindowStartPosition::Exact {
                    x: outer_rect.min.x,
                    y: outer_rect.min.y,
                };
            }
        }
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_window_state(ctx);
        self.update_window_title(ctx);
        self.poll_worker();
        self.poll_companion_worker();
        self.poll_preload_worker();
        self.poll_filesystem();
        self.poll_filer_worker();
        self.poll_thumbnail_worker();
        self.poll_save_result();
        self.poll_deferred_filer_sync();
        self.sync_manga_companion_if_targets_changed(ctx);
        self.handle_keyboard(ctx);
        self.poll_pending_pointer_actions();
        self.settings_ui(ctx);
        self.restart_prompt_ui(ctx);
        self.alert_dialog_ui(ctx);
        self.save_dialog_ui(ctx);
        self.left_click_menu_ui(ctx);
        self.filer_ui(ctx);
        self.subfiler_ui(ctx);
        self.status_panel_ui(ctx);

        let zoom_delta = ctx.input(|i| i.zoom_delta());

        if let Some(deadline) = self.pending_primary_click_deadline {
            let wait = deadline.saturating_duration_since(Instant::now());
            ctx.request_repaint_after(wait.min(POINTER_SINGLE_CLICK_DELAY));
        }

        if zoom_delta != 1.0 && !self.show_settings {
            let _ = self.set_zoom(self.zoom * zoom_delta);
        }

        self.frame_counter += 1;
        self.poll_deferred_filesystem_sync();
        self.update_animation(ctx);

        let panel = egui::CentralPanel::default().frame(egui::Frame::NONE);
        panel.show(ctx, |ui| {
            self.paint_background(ui, ui.max_rect());
            let display_rect = ui.max_rect();
            if self.active_request.is_some() || self.active_fs_request_id.is_some() {
                ctx.request_repaint_after(Duration::from_millis(16));
            }

            let viewport = ui.max_rect().size();
            let startup_viewport_settling =
                self.frame_counter < 8 && viewport_size_changed(viewport, self.last_viewport_size);

            if startup_viewport_settling {
                self.last_viewport_size = viewport;
                self.sync_manga_companion_if_targets_changed(ctx);
            } else if !self.empty_mode
                && (viewport_size_changed(viewport, self.last_viewport_size)
                    || self.pending_fit_recalc)
                && !matches!(self.render_options.zoom_option, ZoomOption::None)
            {
                self.last_viewport_size = viewport;
                self.pending_fit_recalc = false;
                self.sync_manga_companion_if_targets_changed(ctx);

                let new_zoom = calc_fit_zoom(
                    viewport,
                    self.fit_target_size(),
                    &self.render_options.zoom_option,
                );
                self.fit_zoom = new_zoom.clamp(0.1, 16.0);
                let _ = self.sync_zoom();
            }

            let draw_size = vec2(
                self.current_canvas().width() as f32 * self.current_draw_scale(),
                self.current_canvas().height() as f32 * self.current_draw_scale(),
            );
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let spread_active = self.manga_spread_active();
                    let companion = self
                        .companion_rendered
                        .as_ref()
                        .zip(self.companion_texture.as_ref());

                    let companion_draw_size = companion.map(|(companion_rendered, _)| {
                        vec2(
                            companion_rendered.canvas.width() as f32 * self.companion_draw_scale(),
                            companion_rendered.canvas.height() as f32 * self.companion_draw_scale(),
                        )
                    });
                    let total_draw_size = if spread_active {
                        if let Some(companion_draw_size) = companion_draw_size {
                            vec2(
                                draw_size.x + companion_draw_size.x,
                                draw_size.y.max(companion_draw_size.y),
                            )
                        } else {
                            draw_size
                        }
                    } else {
                        draw_size
                    };
                    let offset = aligned_offset(viewport, total_draw_size, self.options.align);

                    ui.add_space(offset.y.max(0.0));

                    let inner = ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(offset.x.max(0.0));
                        if spread_active {
                            if let Some((_, companion_texture)) = companion {
                                let companion_draw_size = companion_draw_size.unwrap_or(draw_size);
                                let draw_companion_first = self.options.manga_right_to_left;
                                if draw_companion_first {
                                    let first = ui.add(
                                        egui::Image::from_texture(companion_texture)
                                            .fit_to_exact_size(companion_draw_size)
                                            .sense(egui::Sense::click()),
                                    );
                                    self.paint_manga_separator(
                                        ui,
                                        draw_size.y.max(companion_draw_size.y),
                                    );
                                    ui.add(
                                        egui::Image::from_texture(&self.current_texture)
                                            .fit_to_exact_size(draw_size)
                                            .sense(egui::Sense::click()),
                                    );
                                    Some(first)
                                } else {
                                    let first = ui.add(
                                        egui::Image::from_texture(&self.current_texture)
                                            .fit_to_exact_size(draw_size)
                                            .sense(egui::Sense::click()),
                                    );
                                    self.paint_manga_separator(
                                        ui,
                                        draw_size.y.max(companion_draw_size.y),
                                    );
                                    ui.add(
                                        egui::Image::from_texture(companion_texture)
                                            .fit_to_exact_size(companion_draw_size)
                                            .sense(egui::Sense::click()),
                                    );
                                    Some(first)
                                }
                            } else {
                                Some(
                                    ui.add(
                                        egui::Image::from_texture(&self.current_texture)
                                            .fit_to_exact_size(draw_size)
                                            .sense(egui::Sense::click()),
                                    ),
                                )
                            }
                        } else {
                            Some(
                                ui.add(
                                    egui::Image::from_texture(&self.current_texture)
                                        .fit_to_exact_size(draw_size)
                                        .sense(egui::Sense::click()),
                                ),
                            )
                        }
                    });
                    let display_response = ui.interact(
                        display_rect,
                        ui.id().with("viewer_display_area"),
                        egui::Sense::click(),
                    );
                    if let Some(response) = inner.inner {
                        if !self.handle_pointer_input(&response)
                            && self.response_has_pointer_intent(&display_response)
                        {
                            let _ = self.handle_pointer_input(&display_response);
                        }
                    } else if self.response_has_pointer_intent(&display_response) {
                        let _ = self.handle_pointer_input(&display_response);
                    }

                    if self.empty_mode {
                        ui.add_space(8.0);
                        ui.label(format!(
                            "{} {}",
                            self.text(UiTextKey::NoDisplayableFileFound),
                            self.text(UiTextKey::OpenDirectoryOrFileFromFiler)
                        ));
                    }
                });
        });
        self.loading_overlay_ui(ctx);
        self.loading_card_ui(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        Self::shutdown_render_worker(&self.worker_tx, &mut self.worker_join);
        Self::shutdown_render_worker(&self.companion_tx, &mut self.companion_join);
        Self::shutdown_render_worker(&self.preload_tx, &mut self.preload_join);
        let _ = save_app_config(
            &self.current_config(),
            Some(&self.current_path),
            self.config_path.as_deref(),
        );
    }
}

fn filesystem_send_error(err: mpsc::SendError<FilesystemCommand>) -> Box<dyn Error> {
    Box::new(std::io::Error::other(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        adjacent_same_branch_navigation_target, manga_companion_matches_preloaded,
        preloaded_navigation_matches, same_navigation_branch, should_allow_preload_for_path,
        should_defer_filer_request_while_loading, should_defer_filer_sync_for_navigation,
        should_defer_preload_for_manga_low_io, should_defer_thumbnail_io,
    };
    use crate::filesystem::{build_zip_virtual_children, zip_index_is_available};
    use crate::options::{ArchiveBrowseOption, NavigationSortOption};
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::write::SimpleFileOptions;

    fn make_temp_dir() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("wml2viewer_viewer_{unique}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn make_zip_with_entries(path: &Path, names: &[&str]) {
        let file = fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for name in names {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            use std::io::Write;
            zip.write_all(b"data").unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn preloaded_navigation_match_requires_same_path() {
        assert!(preloaded_navigation_matches(
            Some(Path::new(r"F:\archive.zip#001.bmp")),
            Path::new(r"F:\archive.zip#001.bmp")
        ));
        assert!(!preloaded_navigation_matches(
            Some(Path::new(r"F:\archive.zip#002.bmp")),
            Path::new(r"F:\archive.zip#001.bmp")
        ));
    }

    #[test]
    fn same_navigation_branch_requires_same_container_or_directory() {
        assert!(same_navigation_branch(
            Path::new(r"F:\dir\001.png"),
            Path::new(r"F:\dir\002.png")
        ));
        assert!(!same_navigation_branch(
            Path::new(r"F:\dir\001.png"),
            Path::new(r"F:\other\002.png")
        ));
    }

    #[test]
    fn filer_sync_is_deferred_for_archive_virtual_navigation() {
        assert!(should_defer_filer_sync_for_navigation(
            Path::new(r"F:\archive.zip\__zipv__\00000000__001.png"),
            Some(Path::new(r"F:\archive.zip"))
        ));
        assert!(!should_defer_filer_sync_for_navigation(
            Path::new(r"F:\dir\001.png"),
            Some(Path::new(r"F:\dir\001.png"))
        ));
    }

    #[test]
    fn adjacent_same_branch_navigation_target_works_inside_zip() {
        let dir = make_temp_dir();
        let archive = dir.join("pages.zip");
        make_zip_with_entries(&archive, &["001.png", "002.png"]);
        let children = build_zip_virtual_children(&archive);
        assert!(!zip_index_is_available(&archive));

        let next = adjacent_same_branch_navigation_target(
            &children[0],
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Folder,
            1,
        );

        assert_eq!(next, Some(children[1].clone()));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn should_allow_preload_for_same_zip_branch_even_when_low_io() {
        let current = Path::new(r"F:\archive.zip\__zipv__\00000000__001.png");
        let next = Path::new(r"F:\archive.zip\__zipv__\00000001__002.png");
        assert!(should_allow_preload_for_path(current, next));
    }

    #[test]
    fn filer_request_is_deferred_while_loading_or_navigation_is_active() {
        assert!(should_defer_filer_request_while_loading(
            true, false, false, false, false
        ));
        assert!(should_defer_filer_request_while_loading(
            false, true, false, false, false
        ));
        assert!(should_defer_filer_request_while_loading(
            false, false, true, false, false
        ));
        assert!(should_defer_filer_request_while_loading(
            false, false, false, true, false
        ));
        assert!(should_defer_filer_request_while_loading(
            false, false, false, false, true
        ));
        assert!(!should_defer_filer_request_while_loading(
            false, false, false, false, false
        ));
    }

    #[test]
    fn manga_companion_can_reuse_matching_preload() {
        let companion = Path::new(r"F:\archive.zip\__zipv__\00000001__002.png");
        let preloaded = Some(Path::new(r"F:\archive.zip\__zipv__\00000001__002.png"));
        assert!(manga_companion_matches_preloaded(companion, preloaded));
        assert!(!manga_companion_matches_preloaded(
            companion,
            Some(Path::new(r"F:\archive.zip\__zipv__\00000002__003.png"))
        ));
    }

    #[test]
    fn low_io_manga_defers_preload_until_companion_is_ready() {
        let current = Path::new(r"F:\archive.zip\__zipv__\00000000__001.png");
        let companion = Some(Path::new(r"F:\archive.zip\__zipv__\00000001__002.png"));
        assert!(should_defer_preload_for_manga_low_io(
            true, current, companion, false
        ));
        assert!(!should_defer_preload_for_manga_low_io(
            true, current, companion, true
        ));
        assert!(!should_defer_preload_for_manga_low_io(
            false, current, companion, false
        ));
    }

    #[test]
    fn low_io_archive_defers_thumbnail_work_while_rendering() {
        let current = Path::new(r"F:\archive.zip\__zipv__\00000000__001.png");
        assert!(should_defer_thumbnail_io(current, true, false, false));
        assert!(should_defer_thumbnail_io(current, false, true, false));
        assert!(should_defer_thumbnail_io(current, false, false, true));
        assert!(!should_defer_thumbnail_io(current, false, false, false));
        assert!(!should_defer_thumbnail_io(
            Path::new(r"F:\dir\001.png"),
            true,
            false,
            false
        ));
    }
}
