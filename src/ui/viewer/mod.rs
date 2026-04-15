use crate::benchlog::BenchLogger;
use crate::configs::config::save_app_config;
use crate::configs::resourses::{AppliedResources, apply_resources};
use crate::dependent::{default_download_dir, pick_save_directory};
use crate::drawers::canvas::Canvas;
use crate::drawers::image::{LoadedImage, SaveFormat, save_loaded_image};
use crate::filesystem::{
    FilesystemCommand, FilesystemResult, adjacent_entry, archive_prefers_low_io,
    is_browser_container, navigation_branch_path, resolve_navigation_entry_path, resolve_start_path,
    set_archive_zip_workaround, spawn_filesystem_worker,
};
use crate::options::{
    AppConfig, EndOfFolderOption, KeyBinding, NavigationSortOption, PluginConfig, ResourceOptions,
    RuntimeOptions, ViewerAction,
};
use crate::ui::i18n::{UiTextKey, tr};
use crate::ui::menu::fileviewer::state::{
    FilerSortField, FilerState, FilerUserRequest, NameSortMode,
};
use crate::ui::menu::fileviewer::thumbnail::{
    ThumbnailCommand, ThumbnailResult, set_thumbnail_workaround, spawn_thumbnail_worker,
};
use crate::ui::menu::fileviewer::worker::{FilerCommand, FilerResult, spawn_filer_worker};
use crate::ui::render::{
    ActiveRenderRequest, LoadedRenderPage, RenderCommand, RenderLoadMetrics, RenderResult, aligned_offset,
    canvas_to_color_image, downscale_for_texture_limit, spawn_render_worker, worker_send_error,
};
use crate::ui::viewer::options::{
    RenderOptions, RenderScaleMode, ViewerOptions, WindowOptions, WindowStartPosition,
    WindowUiTheme,
};
use eframe::egui::{self, Pos2, TextureHandle, TextureOptions, vec2};
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
pub mod options;
mod state;
use options::ZoomOption;
pub(crate) use state::SettingsDraftState;
use state::{SaveDialogState, ViewerOverlayState};

const NAVIGATION_REPEAT_INTERVAL: Duration = Duration::from_millis(180);
const POINTER_SINGLE_CLICK_DELAY: Duration = Duration::from_millis(500);
const WAITING_CARD_DELAY: Duration = Duration::from_millis(180);
const PRELOAD_CACHE_CAPACITY: usize = 2;
const ZIP_TO_ZIP_RANDOM_WALK_ROUNDS: usize = 8;

pub(crate) struct ViewerApp {
    pub(crate) current_navigation_path: PathBuf,
    pub(crate) current_path: PathBuf,
    pub(crate) source: LoadedImage,
    pub(crate) rendered: LoadedImage,
    pub(crate) default_texture: TextureHandle,
    pub(crate) prev_texture: Option<TextureHandle>,
    pub(crate) current_texture: TextureHandle,
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
    pending_viewer_navigation: Option<PendingViewerNavigation>,
    pub(crate) fs_tx: Option<Sender<FilesystemCommand>>,
    pub(crate) fs_rx: Option<Receiver<FilesystemResult>>,
    pub(crate) next_fs_request_id: u64,
    pub(crate) active_fs_request_id: Option<u64>,
    pub(crate) queued_filesystem_init_path: Option<PathBuf>,
    pub(crate) queued_navigation: Option<FilesystemCommand>,
    pub(crate) deferred_filesystem_init_path: Option<PathBuf>,
    pub(crate) filer_tx: Option<Sender<FilerCommand>>,
    pub(crate) filer_rx: Option<Receiver<FilerResult>>,
    pub(crate) next_filer_request_id: u64,
    pub(crate) thumbnail_tx: Option<Sender<ThumbnailCommand>>,
    pub(crate) thumbnail_rx: Option<Receiver<ThumbnailResult>>,
    pub(crate) next_thumbnail_request_id: u64,
    pub(crate) thumbnail_pending: HashSet<PathBuf>,
    pub(crate) thumbnail_cache: HashMap<PathBuf, TextureHandle>,
    pub(crate) navigator_ready: bool,
    pub(crate) overlay: ViewerOverlayState,
    pub(crate) last_navigation_at: Option<Instant>,
    pub(crate) show_settings: bool,
    pub(crate) settings_draft: Option<SettingsDraftState>,
    pub(crate) show_restart_prompt: bool,
    pub(crate) settings_tab: SettingsTab,
    pub(crate) max_texture_side: usize,
    pub(crate) texture_display_scale: f32,
    pub(crate) current_texture_is_default: bool,
    pub(crate) pending_resize_after_load: bool,
    pub(crate) pending_resize_after_render: bool,
    pub(crate) pending_fit_recalc: bool,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) bench_logger: Option<BenchLogger>,
    pub(crate) show_left_menu: bool,
    pub(crate) left_menu_pos: Pos2,
    pub(crate) save_dialog: SaveDialogState,
    pub(crate) show_filer: bool,
    pub(crate) show_subfiler: bool,
    pub(crate) filer: FilerState,
    pub(crate) pending_filer_focus_path: Option<PathBuf>,
    pub(crate) pending_subfiler_focus_path: Option<PathBuf>,
    last_filer_snapshot_signature: Option<(PathBuf, u64)>,
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
    pub(crate) companion_navigation_path: Option<PathBuf>,
    companion_display: Option<DisplayedPageState>,
    pub(crate) preload_tx: Sender<RenderCommand>,
    pub(crate) preload_rx: Receiver<RenderResult>,
    pub(crate) preload_join: Option<JoinHandle<()>>,
    pub(crate) next_preload_request_id: u64,
    pub(crate) active_preload_request_id: Option<u64>,
    pub(crate) pending_preload_navigation_path: Option<PathBuf>,
    preload_cache: VecDeque<PreloadedEntry>,
    pub(crate) pending_primary_click_deadline: Option<Instant>,
    pub(crate) bench_initial_load_logged: bool,
    pub(crate) bench_startup_sync_logged: bool,
    bench_automation: Option<BenchAutomationState>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingViewerNavigation {
    Next,
    Prev,
    First,
    Last,
}

enum PendingFilesystemWork {
    Init(PathBuf),
    Command(FilesystemCommand),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchAction {
    Reload,
    Next,
    Prev,
    ToggleMangaOn,
    ToggleMangaOff,
    RefreshFiler,
    EnsureCurrentDirectoryInFiler,
    OpenSubfiler,
    BrowseParentDirectory,
    BrowseFirstContainer,
    BrowseSiblingContainer,
    BrowseRandomContainer,
    SelectNeighborFromFiler,
    SelectRandomFileFromFiler,
}

struct BenchAutomationState {
    scenario_name: String,
    actions: Vec<BenchAction>,
    next_index: usize,
    next_action_at: Instant,
    random_state: u64,
}

#[derive(Clone)]
struct DisplayedPageState {
    source: LoadedImage,
    rendered: LoadedImage,
    texture: Option<TextureHandle>,
    texture_display_scale: f32,
}

#[derive(Clone)]
struct PreloadedEntry {
    navigation_path: PathBuf,
    load_path: Option<PathBuf>,
    display: DisplayedPageState,
}

fn remember_preloaded_entry_in_cache(cache: &mut VecDeque<PreloadedEntry>, entry: PreloadedEntry) {
    if let Some(index) = cache
        .iter()
        .position(|cached| cached.navigation_path == entry.navigation_path)
    {
        cache.remove(index);
    }
    cache.push_front(entry);
    while cache.len() > PRELOAD_CACHE_CAPACITY {
        cache.pop_back();
    }
}

fn should_prioritize_companion_preload(
    desired_companion: Option<&Path>,
    companion_navigation_path: Option<&Path>,
    companion_ready: bool,
) -> bool {
    match desired_companion {
        Some(desired_companion) => {
            companion_navigation_path != Some(desired_companion) || !companion_ready
        }
        None => false,
    }
}

fn zip_to_zip_random_walk_actions(rounds: usize) -> Vec<BenchAction> {
    let mut actions = Vec::with_capacity(rounds * 10);
    for _ in 0..rounds {
        actions.push(BenchAction::BrowseParentDirectory);
        actions.push(BenchAction::BrowseRandomContainer);
        actions.push(BenchAction::SelectRandomFileFromFiler);
        actions.push(BenchAction::Next);
        actions.push(BenchAction::Prev);
        actions.push(BenchAction::SelectRandomFileFromFiler);
        actions.push(BenchAction::Next);
        actions.push(BenchAction::SelectRandomFileFromFiler);
        actions.push(BenchAction::Prev);
        actions.push(BenchAction::RefreshFiler);
    }
    actions
}

fn bench_automation_plan(name: Option<&str>) -> (&'static str, Vec<BenchAction>) {
    match name {
        Some("zip_to_zip_random") => (
            "zip_to_zip_random",
            zip_to_zip_random_walk_actions(ZIP_TO_ZIP_RANDOM_WALK_ROUNDS),
        ),
        Some("zip_to_zip") => (
            "zip_to_zip",
            vec![
                BenchAction::BrowseParentDirectory,
                BenchAction::BrowseSiblingContainer,
                BenchAction::RefreshFiler,
                BenchAction::BrowseParentDirectory,
                BenchAction::BrowseSiblingContainer,
            ],
        ),
        Some("filer_refresh_race") => (
            "filer_refresh_race",
            vec![
                BenchAction::EnsureCurrentDirectoryInFiler,
                BenchAction::BrowseParentDirectory,
                BenchAction::BrowseFirstContainer,
                BenchAction::RefreshFiler,
                BenchAction::EnsureCurrentDirectoryInFiler,
                BenchAction::OpenSubfiler,
                BenchAction::SelectNeighborFromFiler,
            ],
        ),
        Some("zip_subfiler") => (
            "zip_subfiler",
            vec![
                BenchAction::EnsureCurrentDirectoryInFiler,
                BenchAction::OpenSubfiler,
                BenchAction::SelectNeighborFromFiler,
                BenchAction::RefreshFiler,
            ],
        ),
        _ => (
            "default",
            vec![
                BenchAction::Reload,
                BenchAction::Next,
                BenchAction::Prev,
                BenchAction::ToggleMangaOn,
                BenchAction::Next,
                BenchAction::ToggleMangaOff,
            ],
        ),
    }
}

fn should_clear_filer_user_request_after_snapshot(request: Option<&FilerUserRequest>) -> bool {
    matches!(request, Some(FilerUserRequest::Refresh { .. }))
}

fn should_reinitialize_filesystem_after_load(previous: &Path, current: &Path) -> bool {
    navigation_branch_path(previous) != navigation_branch_path(current)
}

fn queue_navigation_command(slot: &mut Option<FilesystemCommand>, command: FilesystemCommand) {
    *slot = Some(command);
}

fn take_next_queued_filesystem_work(
    queued_filesystem_init_path: &mut Option<PathBuf>,
    queued_navigation: &mut Option<FilesystemCommand>,
) -> Option<PendingFilesystemWork> {
    if let Some(path) = queued_filesystem_init_path.take() {
        Some(PendingFilesystemWork::Init(path))
    } else {
        queued_navigation
            .take()
            .map(PendingFilesystemWork::Command)
    }
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
    let head = chars.iter().take(max_chars.saturating_sub(3)).collect::<String>();
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
    fn bench_metrics_payload(metrics: &RenderLoadMetrics) -> serde_json::Value {
        serde_json::json!({
            "resolved_path": metrics.resolved_path.as_ref().map(|path| path.display().to_string()),
            "used_virtual_bytes": metrics.used_virtual_bytes,
            "decoded_from_bytes": metrics.decoded_from_bytes,
            "source_bytes_len": metrics.source_bytes_len,
            "resolve_ms": metrics.resolve_ms,
            "read_ms": metrics.read_ms,
            "decode_ms": metrics.decode_ms,
            "resize_ms": metrics.resize_ms,
        })
    }

    pub(crate) fn new(
        cc: &eframe::CreationContext<'_>,
        navigation_path: PathBuf,
        path: PathBuf,
        source: LoadedImage,
        rendered: LoadedImage,
        config: AppConfig,
        config_path: Option<PathBuf>,
        bench_logger: Option<BenchLogger>,
        bench_enabled: bool,
        bench_scenario: Option<String>,
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
        let (worker_tx, worker_rx, worker_join) = spawn_render_worker(source.clone());
        let (companion_tx, companion_rx, companion_join) = spawn_render_worker(source.clone());
        let (preload_tx, preload_rx, preload_join) = spawn_render_worker(source.clone());
        let resource_locale_input = config.resources.locale.clone().unwrap_or_default();
        let resource_font_paths_input = join_search_paths(&config.resources.font_paths);
        let defer_navigation_workers = !show_filer_on_start;
        let startup_phase = if defer_navigation_workers {
            StartupPhase::SingleViewer
        } else {
            StartupPhase::MultiViewer
        };
        let (bench_scenario_name, bench_actions) = bench_automation_plan(bench_scenario.as_deref());

        let mut this = Self {
            current_navigation_path: navigation_path.clone(),
            current_path: path.clone(),
            source,
            rendered,
            default_texture: default_texture.clone(),
            prev_texture: None,
            current_texture: default_texture.clone(),
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
            pending_viewer_navigation: None,
            fs_tx: None,
            fs_rx: None,
            next_fs_request_id: 0,
            active_fs_request_id: None,
            queued_filesystem_init_path: None,
            queued_navigation: None,
            deferred_filesystem_init_path: None,
            filer_tx: None,
            filer_rx: None,
            next_filer_request_id: 0,
            thumbnail_tx: None,
            thumbnail_rx: None,
            next_thumbnail_request_id: 0,
            thumbnail_pending: HashSet::new(),
            thumbnail_cache: HashMap::new(),
            navigator_ready: false,
            overlay: ViewerOverlayState::default(),
            last_navigation_at: None,
            show_settings: false,
            settings_draft: None,
            show_restart_prompt: false,
            settings_tab: SettingsTab::Viewer,
            max_texture_side: cc.egui_ctx.input(|i| i.max_texture_side),
            texture_display_scale: 1.0,
            current_texture_is_default: true,
            pending_resize_after_load: false,
            pending_resize_after_render: false,
            pending_fit_recalc: false,
            config_path,
            bench_logger,
            show_left_menu: false,
            left_menu_pos: Pos2::ZERO,
            save_dialog: SaveDialogState {
                file_name: default_save_file_name(&path),
                ..SaveDialogState::default()
            },
            show_filer: show_filer_on_start,
            show_subfiler: false,
            filer: FilerState::default(),
            pending_filer_focus_path: None,
            pending_subfiler_focus_path: None,
            last_filer_snapshot_signature: None,
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
            companion_navigation_path: None,
            companion_display: None,
            preload_tx,
            preload_rx,
            preload_join: Some(preload_join),
            next_preload_request_id: 0,
            active_preload_request_id: None,
            pending_preload_navigation_path: None,
            preload_cache: VecDeque::new(),
            pending_primary_click_deadline: None,
            bench_initial_load_logged: false,
            bench_startup_sync_logged: false,
            bench_automation: bench_enabled.then_some(BenchAutomationState {
                scenario_name: bench_scenario_name.to_string(),
                actions: bench_actions,
                next_index: 0,
                next_action_at: Instant::now() + Duration::from_millis(250),
                random_state: 0x5eed_cafe_d15c_a11e,
            }),
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
            if let Some(companion) = self.visible_companion_source() {
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
    <li><code>wml2viewer --clean system</code> (planned)</li>
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
            self.render_options.zoom_method = crate::drawers::affine::InterpolationAlgorithm::Bilinear;
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
            self.log_bench_state(
                "viewer.startup_sync.deferred",
                serde_json::json!({
                    "target_frame": self.deferred_filesystem_sync_frame,
                }),
            );
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
        texture_options_for_scale_mode(self.render_options.scale_mode, self.render_options.zoom_method)
    }

    fn current_draw_scale(&self) -> f32 {
        match self.render_options.scale_mode {
            RenderScaleMode::FastGpu => self.zoom.max(0.1),
            RenderScaleMode::PreciseCpu => 1.0,
        }
    }

    fn companion_draw_scale(&self) -> f32 {
        self.current_draw_scale()
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
        let texture = self
            .egui_ctx
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
        }
        self.current_texture = self.default_texture.clone();
        self.current_texture_is_default = true;
        self.texture_display_scale = 1.0;
    }

    fn shutdown_render_worker(
        tx: &Sender<RenderCommand>,
        join: &mut Option<JoinHandle<()>>,
    ) {
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

        let target_fit = calc_fit_zoom(
            viewport,
            source_size,
            &self.render_options.zoom_option,
        )
        .clamp(0.1, 16.0);
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
        if self.current_navigation_path.is_dir() {
            return Some(self.current_navigation_path.clone());
        }
        if let Some(parent) = self.current_navigation_path.parent() {
            let marker = parent.file_name().and_then(|name| name.to_str());
            if matches!(marker, Some("__wmlv__" | "__zipv__")) {
                return parent.parent().map(|path| path.to_path_buf());
            }
            return Some(parent.to_path_buf());
        }
        self.current_path.parent().map(|path| path.to_path_buf())
    }

    pub(crate) fn request_filer_directory(&mut self, dir: PathBuf, selected: Option<PathBuf>) {
        self.spawn_navigation_workers();
        let Some(filer_tx) = self.filer_tx.clone() else {
            return;
        };
        let request_id = self.alloc_filer_request_id();
        self.filer.pending_request_id = Some(request_id);
        self.log_bench_state(
            "viewer.filer.request_directory",
            serde_json::json!({
                "request_id": request_id,
                "directory": dir.display().to_string(),
                "selected": selected.as_ref().map(|path| path.display().to_string()),
            }),
        );
        let _ = filer_tx.send(FilerCommand::OpenDirectory {
            request_id,
            dir,
            sort: self.navigation_sort,
            selected,
            sort_field: self.filer.sort_field,
            ascending: self.filer.ascending,
            separate_dirs: self.filer.separate_dirs,
            archive_as_container_in_sort: self.filer.archive_as_container_in_sort,
            filter_text: self.filer.filter_text.clone(),
            extension_filter: self.filer.extension_filter.clone(),
            name_sort_mode: self.filer.name_sort_mode,
        });
    }

    fn filer_selected_for_directory(
        &self,
        directory: &std::path::Path,
        fallback: Option<PathBuf>,
    ) -> Option<PathBuf> {
        match &self.filer.pending_user_request {
            Some(FilerUserRequest::SelectFile { navigation_path }) => {
                if navigation_path.parent() == Some(directory) {
                    return Some(navigation_path.clone());
                }
            }
            Some(FilerUserRequest::Refresh { directory: refresh_dir, selected })
                if refresh_dir == directory =>
            {
                return selected.clone();
            }
            Some(FilerUserRequest::BrowseDirectory { directory: browse_dir })
                if browse_dir == directory =>
            {
                return fallback;
            }
            _ => {}
        }
        self.selected_path_for_filer_directory(directory, fallback)
    }

    fn clear_committed_filer_user_request(&mut self) {
        let should_clear = should_clear_filer_select_request_for_current(
            self.filer.pending_user_request.as_ref(),
            &self.current_navigation_path,
        );
        if should_clear {
            self.filer.pending_user_request = None;
            self.filer.committed_browse_directory = None;
        }
    }

    fn sync_filer_directory_with_current_path(&mut self) {
        let Some(dir) = self.current_directory() else {
            return;
        };
        let mut rebased_navigation_path = None;
        if let Some(rebased) = resolve_navigation_entry_path(&self.current_navigation_path) {
            if rebased != self.current_navigation_path {
                self.current_navigation_path = rebased.clone();
                self.set_filesystem_current(rebased);
                rebased_navigation_path = Some(self.current_navigation_path.clone());
            }
        }
        let selected = Some(self.current_navigation_path.clone());
        self.log_bench_state(
            "viewer.filer.sync_with_current_path",
            serde_json::json!({
                "directory": dir.display().to_string(),
                "selected": selected.as_ref().map(|path| path.display().to_string()),
                "same_directory": self.filer.directory.as_ref() == Some(&dir),
                "entries_empty": self.filer.entries.is_empty(),
                "had_pending_request": self.filer.pending_request_id.is_some(),
                "pending_user_request": self.filer.pending_user_request.as_ref().map(|request| format!("{request:?}")),
                "committed_browse_directory": self.filer.committed_browse_directory.as_ref().map(|path| path.display().to_string()),
                "rebased_navigation_path": rebased_navigation_path.as_ref().map(|path| path.display().to_string()),
            }),
        );
        if self.filer.pending_user_request.is_some() {
            self.log_bench_state(
                "viewer.filer.sync_with_current_path.skipped_pending_user_request",
                serde_json::json!({
                    "directory": dir.display().to_string(),
                }),
            );
            return;
        }
        if self
            .filer
            .committed_browse_directory
            .as_ref()
            .is_some_and(|browse_dir| browse_dir != &dir)
        {
            self.log_bench_state(
                "viewer.filer.sync_with_current_path.skipped_committed_browse",
                serde_json::json!({
                    "directory": dir.display().to_string(),
                    "committed_browse_directory": self.filer.committed_browse_directory.as_ref().map(|path| path.display().to_string()),
                }),
            );
            return;
        }
        if self.filer.directory.as_ref() == Some(&dir) {
            self.filer.selected = selected.clone();
            self.pending_filer_focus_path = selected.clone();
            if self.filer.entries.is_empty() && self.filer.pending_request_id.is_none() {
                self.request_filer_directory(dir, selected);
            }
        } else {
            self.pending_filer_focus_path = selected.clone();
            self.request_filer_directory(dir, selected);
        }
    }

    fn selected_path_for_filer_directory(
        &self,
        directory: &std::path::Path,
        fallback: Option<PathBuf>,
    ) -> Option<PathBuf> {
        if self.current_directory().as_deref() == Some(directory) {
            resolve_navigation_entry_path(&self.current_navigation_path)
                .or_else(|| Some(self.current_navigation_path.clone()))
        } else {
            fallback
        }
    }

    pub(crate) fn refresh_current_filer_directory(&mut self) {
        if let Some(dir) = self
            .filer
            .directory
            .clone()
            .or_else(|| self.current_directory())
        {
            self.filer.pending_user_request = Some(FilerUserRequest::Refresh {
                directory: dir.clone(),
                selected: self.filer.selected.clone(),
            });
            self.log_bench_state(
                "viewer.filer.refresh_requested",
                serde_json::json!({
                    "directory": dir.display().to_string(),
                    "selected": self.filer.selected.as_ref().map(|path| path.display().to_string()),
                }),
            );
            self.request_filer_directory(dir, self.filer.selected.clone());
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
        if !self.options.manga_mode
            || self.empty_mode
            || !self.navigator_ready
            || !self.is_current_portrait_page()
        {
            return None;
        }
        spread_companion_path_for_navigation(
            &self.current_navigation_path,
            self.navigation_sort,
            self.options.manga_mode,
        )
    }

    fn desired_manga_companion_path_for_navigation(&self, navigation_path: &Path) -> Option<PathBuf> {
        spread_companion_path_for_navigation(navigation_path, self.navigation_sort, self.options.manga_mode)
    }

    fn clear_manga_companion(&mut self) {
        self.companion_navigation_path = None;
        self.companion_display = None;
        self.companion_active_request = None;
    }

    fn visible_companion_source(&self) -> Option<&LoadedImage> {
        self.companion_navigation_path
            .as_ref()
            .and(self.companion_display.as_ref().map(|display| &display.source))
    }

    fn visible_companion(&self) -> Option<(&LoadedImage, &TextureHandle)> {
        self.companion_navigation_path
            .as_ref()
            .and(self.companion_display.as_ref().and_then(|display| {
                display
                    .texture
                    .as_ref()
                    .map(|texture| (&display.rendered, texture))
            }))
    }

    fn manga_spread_active(&self) -> bool {
        self.options.manga_mode
            && self.last_viewport_size.x >= self.last_viewport_size.y * 1.4
            && self.is_current_portrait_page()
            && self
                .visible_companion_source()
                .map(|image| image.canvas.height() >= image.canvas.width())
                .unwrap_or(false)
    }

    fn request_companion_load(&mut self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        if let Some(entry) = self.cached_preloaded_entry(&path) {
            self.log_bench_state(
                "viewer.request_companion_load.preloaded_hit",
                serde_json::json!({
                    "path": path.display().to_string(),
                    "load_path": entry.load_path.as_ref().map(|load_path| load_path.display().to_string()),
                }),
            );
            self.companion_navigation_path = Some(path);
            self.apply_companion_loaded(
                entry.load_path,
                entry.display,
            );
            return Ok(());
        }
        let request_id = self.alloc_request_id();
        self.companion_active_request = Some(ActiveRenderRequest::Load(request_id));
        self.companion_navigation_path = Some(path.clone());
        self.companion_tx
            .send(RenderCommand::LoadPath {
                request_id,
                path,
                companion_path: None,
                zoom: self.zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
            })
            .map_err(worker_send_error)?;
        Ok(())
    }

    fn request_companion_resize(&mut self) -> Result<(), Box<dyn Error>> {
        if self.companion_display.is_none() {
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

    fn sync_manga_companion(&mut self, ctx: &egui::Context) {
        if should_defer_companion_sync_during_primary_load(self.active_request) {
            return;
        }
        let desired = self.desired_manga_companion_path();
        if desired == self.companion_navigation_path && self.visible_companion().is_some() {
            return;
        }

        if desired.is_none() {
            self.clear_manga_companion();
            self.pending_fit_recalc |= !matches!(self.render_options.zoom_option, ZoomOption::None);
            return;
        }

        let desired = desired.unwrap();
        if let Some(entry) = self.cached_preloaded_entry(&desired) {
            self.companion_navigation_path = Some(desired);
            self.apply_companion_loaded(entry.load_path, entry.display);
            ctx.request_repaint();
            return;
        }

        if self.companion_active_request.is_none() {
            let _ = self.request_companion_load(desired);
            ctx.request_repaint();
        }
    }

    fn manga_navigation_target(&self, forward: bool) -> Option<PathBuf> {
        if !self.navigator_ready || !self.manga_spread_active() {
            return None;
        }

        let boundary_target = adjacent_entry(
            &self.current_navigation_path,
            self.navigation_sort,
            if forward { 1 } else { -1 },
        )?;
        let current_branch = navigation_branch_path(&self.current_navigation_path);
        let boundary_branch = navigation_branch_path(&boundary_target);
        if current_branch != boundary_branch {
            return Some(boundary_target);
        }

        let step = if forward { 2 } else { -2 };
        adjacent_entry(&self.current_navigation_path, self.navigation_sort, step)
    }

    pub(crate) fn next_image(&mut self) -> Result<(), Box<dyn Error>> {
        self.cancel_pending_single_click_navigation();
        if !self.can_trigger_navigation() {
            return Ok(());
        }
        if self.navigation_blocked_by_active_load() {
            self.queue_viewer_navigation(PendingViewerNavigation::Next);
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
        if self.navigation_blocked_by_active_load() {
            self.queue_viewer_navigation(PendingViewerNavigation::Prev);
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
        if self.navigation_blocked_by_active_load() {
            self.queue_viewer_navigation(PendingViewerNavigation::First);
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
        if self.navigation_blocked_by_active_load() {
            self.queue_viewer_navigation(PendingViewerNavigation::Last);
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

    pub(crate) fn set_show_subfiler(&mut self, show: bool) {
        self.show_subfiler = show;
        if show {
            self.pending_subfiler_focus_path = Some(self.current_navigation_path.clone());
        } else {
            self.pending_subfiler_focus_path = None;
        }
    }

    pub(crate) fn sync_navigation_sort_with_filer_sort(&mut self) {
        let desired = navigation_sort_for_filer(self.filer.sort_field, self.filer.name_sort_mode);
        if self.navigation_sort == desired {
            return;
        }
        self.navigation_sort = desired;
        self.log_bench_state(
            "viewer.navigation_sort.synced_from_filer",
            serde_json::json!({
                "navigation_sort": format!("{:?}", self.navigation_sort),
                "filer_sort_field": format!("{:?}", self.filer.sort_field),
                "filer_name_sort_mode": format!("{:?}", self.filer.name_sort_mode),
            }),
        );
        self.respawn_filesystem_worker();
    }

    pub(crate) fn log_bench_state(&self, event: &str, payload: serde_json::Value) {
        let Some(logger) = &self.bench_logger else {
            return;
        };
        logger.log(
            event,
            serde_json::json!({
                "state": {
                    "current_navigation_path": self.current_navigation_path.display().to_string(),
                    "current_path": self.current_path.display().to_string(),
                    "pending_navigation_path": self.pending_navigation_path.as_ref().map(|path| path.display().to_string()),
                    "pending_viewer_navigation": self.pending_viewer_navigation.map(|nav| format!("{nav:?}")),
                    "navigator_ready": self.navigator_ready,
                    "active_request": format!("{:?}", self.active_request),
                    "active_fs_request_id": self.active_fs_request_id,
                    "queued_filesystem_init_path": self.queued_filesystem_init_path.as_ref().map(|path| path.display().to_string()),
                    "queued_navigation": self.queued_navigation.as_ref().map(|command| format!("{command:?}")),
                    "startup_phase": format!("{:?}", self.startup_phase),
                    "show_filer": self.show_filer,
                    "show_subfiler": self.show_subfiler,
                    "empty_mode": self.empty_mode,
                    "filer_directory": self.filer.directory.as_ref().map(|path| path.display().to_string()),
                    "filer_selected": self.filer.selected.as_ref().map(|path| path.display().to_string()),
                    "filer_pending_request_id": self.filer.pending_request_id,
                    "filer_pending_user_request": self.filer.pending_user_request.as_ref().map(|request| format!("{request:?}")),
                    "filer_committed_browse_directory": self.filer.committed_browse_directory.as_ref().map(|path| path.display().to_string()),
                    "last_filer_snapshot_signature": self.last_filer_snapshot_signature.as_ref().map(|(directory, signature)| serde_json::json!({
                        "directory": directory.display().to_string(),
                        "signature": signature,
                    })),
                    "pending_filer_focus_path": self.pending_filer_focus_path.as_ref().map(|path| path.display().to_string()),
                    "pending_subfiler_focus_path": self.pending_subfiler_focus_path.as_ref().map(|path| path.display().to_string()),
                    "active_preload_request_id": self.active_preload_request_id,
                    "pending_preload_navigation_path": self.pending_preload_navigation_path.as_ref().map(|path| path.display().to_string()),
                    "preload_cache_navigation_paths": self.preload_cache.iter().map(|entry| entry.navigation_path.display().to_string()).collect::<Vec<_>>(),
                },
                "event_payload": payload,
            }),
        );
    }

    fn log_bench_startup_sync_once(&mut self, reason: &str) {
        if self.bench_startup_sync_logged {
            return;
        }
        self.bench_startup_sync_logged = true;
        self.log_bench_state(
            "viewer.startup_sync.completed",
            serde_json::json!({
                "reason": reason,
                "frame_counter": self.frame_counter,
            }),
        );
    }

    fn navigation_blocked_by_active_load(&self) -> bool {
        matches!(self.active_request, Some(ActiveRenderRequest::Load(_)))
    }

    fn queue_viewer_navigation(&mut self, navigation: PendingViewerNavigation) {
        self.pending_viewer_navigation = Some(navigation);
        self.log_bench_state(
            "viewer.navigation.queued_during_load",
            serde_json::json!({
                "navigation": format!("{navigation:?}"),
            }),
        );
    }

    fn flush_pending_viewer_navigation(&mut self) {
        if self.navigation_blocked_by_active_load() {
            return;
        }
        let Some(navigation) = self.pending_viewer_navigation.take() else {
            return;
        };
        self.log_bench_state(
            "viewer.navigation.flushed_after_load",
            serde_json::json!({
                "navigation": format!("{navigation:?}"),
            }),
        );
        self.cancel_pending_single_click_navigation();
        let result = match navigation {
            PendingViewerNavigation::Next => {
                if let Some(target) = self.manga_navigation_target(true) {
                    self.request_load_path(target)
                } else {
                    self.request_navigation(FilesystemCommand::Next {
                        request_id: 0,
                        policy: self.end_of_folder,
                    })
                }
            }
            PendingViewerNavigation::Prev => {
                if let Some(target) = self.manga_navigation_target(false) {
                    self.request_load_path(target)
                } else {
                    self.request_navigation(FilesystemCommand::Prev {
                        request_id: 0,
                        policy: self.end_of_folder,
                    })
                }
            }
            PendingViewerNavigation::First => {
                self.request_navigation(FilesystemCommand::First { request_id: 0 })
            }
            PendingViewerNavigation::Last => {
                self.request_navigation(FilesystemCommand::Last { request_id: 0 })
            }
        };
        if result.is_ok() {
            self.last_navigation_at = Some(Instant::now());
        }
    }

    fn bench_automation_ready(&self) -> bool {
        self.navigator_ready
            && self.active_request.is_none()
            && self.active_fs_request_id.is_none()
            && self.companion_active_request.is_none()
            && self.active_preload_request_id.is_none()
            && self.filer.pending_request_id.is_none()
            && !self.empty_mode
    }

    fn advance_bench_automation(&mut self, delay_ms: u64) {
        if let Some(state) = &mut self.bench_automation {
            state.next_index += 1;
            state.next_action_at = Instant::now() + Duration::from_millis(delay_ms);
        }
    }

    fn defer_bench_automation(&mut self, delay_ms: u64) {
        if let Some(state) = &mut self.bench_automation {
            state.next_action_at = Instant::now() + Duration::from_millis(delay_ms);
        }
    }

    fn bench_neighbor_entry_path(&self) -> Option<PathBuf> {
        self.filer
            .entries
            .iter()
            .filter(|entry| !entry.is_container)
            .find(|entry| entry.path != self.current_navigation_path)
            .map(|entry| entry.path.clone())
            .or_else(|| {
                self.filer
                    .entries
                    .iter()
                    .find(|entry| !entry.is_container)
                    .map(|entry| entry.path.clone())
            })
    }

    fn bench_random_file_entry(&mut self) -> Option<crate::ui::menu::fileviewer::state::FilerEntry> {
        let entries = self
            .filer
            .entries
            .iter()
            .filter(|entry| !entry.is_container)
            .cloned()
            .collect::<Vec<_>>();
        let index = self.next_bench_random_index(entries.len())?;
        entries.get(index).cloned()
    }

    fn bench_random_container_entry(
        &mut self,
    ) -> Option<crate::ui::menu::fileviewer::state::FilerEntry> {
        let entries = self
            .filer
            .entries
            .iter()
            .filter(|entry| entry.is_container)
            .cloned()
            .collect::<Vec<_>>();
        let index = self.next_bench_random_index(entries.len())?;
        entries.get(index).cloned()
    }

    fn bench_sibling_container_path(&self) -> Option<PathBuf> {
        let current_branch = navigation_branch_path(&self.current_navigation_path);
        let containers = self
            .filer
            .entries
            .iter()
            .filter(|entry| entry.is_container)
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let current_index = containers
            .iter()
            .position(|path| current_branch.as_ref() == Some(path));

        current_index
            .and_then(|index| containers.get(index + 1).cloned())
            .or_else(|| {
                current_index
                    .and_then(|index| index.checked_sub(1))
                    .and_then(|index| containers.get(index).cloned())
            })
            .or_else(|| {
                containers
                    .iter()
                    .find(|path| current_branch.as_ref() != Some(*path))
                    .cloned()
            })
    }

    fn bench_container_entry_by_path(&self, path: &Path) -> Option<crate::ui::menu::fileviewer::state::FilerEntry> {
        self.filer
            .entries
            .iter()
            .find(|entry| entry.is_container && entry.path == path)
            .cloned()
    }

    fn next_bench_random_index(&mut self, upper_bound: usize) -> Option<usize> {
        if upper_bound == 0 {
            return None;
        }
        let state = self.bench_automation.as_mut()?;
        state.random_state = state
            .random_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        Some(((state.random_state >> 32) as usize) % upper_bound)
    }

    fn bench_random_container_path(&mut self) -> Option<PathBuf> {
        let current_branch = navigation_branch_path(&self.current_navigation_path);
        let containers = self
            .filer
            .entries
            .iter()
            .filter(|entry| entry.is_container)
            .map(|entry| entry.path.clone())
            .filter(|path| current_branch.as_ref() != Some(path))
            .collect::<Vec<_>>();
        let index = self.next_bench_random_index(containers.len())?;
        containers.get(index).cloned()
    }

    fn run_bench_action(&mut self, action: BenchAction) -> bool {
        match action {
            BenchAction::Reload => {
                let _ = self.reload_current();
                true
            }
            BenchAction::Next => {
                let _ = self.next_image();
                true
            }
            BenchAction::Prev => {
                let _ = self.prev_image();
                true
            }
            BenchAction::ToggleMangaOn => {
                self.options.manga_mode = true;
                self.pending_fit_recalc = true;
                true
            }
            BenchAction::ToggleMangaOff => {
                self.options.manga_mode = false;
                self.pending_fit_recalc = true;
                true
            }
            BenchAction::RefreshFiler => {
                self.refresh_current_filer_directory();
                true
            }
            BenchAction::EnsureCurrentDirectoryInFiler => {
                let Some(dir) = self.current_directory() else {
                    return false;
                };
                self.filer.committed_browse_directory = None;
                let selected = Some(self.current_navigation_path.clone());
                if self.filer.directory.as_ref() == Some(&dir) && !self.filer.entries.is_empty() {
                    self.filer.selected = selected;
                } else {
                    self.request_filer_directory(dir, selected);
                }
                true
            }
            BenchAction::OpenSubfiler => {
                self.set_show_subfiler(true);
                if let Some(dir) = self.current_directory() {
                    if self.filer.directory.as_ref() != Some(&dir) {
                        self.request_filer_directory(dir, Some(self.current_navigation_path.clone()));
                    }
                }
                true
            }
            BenchAction::BrowseParentDirectory => {
                let directory = self
                    .filer
                    .directory
                    .clone()
                    .or_else(|| self.current_directory());
                let Some(parent) = directory.and_then(|dir| dir.parent().map(Path::to_path_buf)) else {
                    return false;
                };
                self.request_filer_directory(parent, None);
                true
            }
            BenchAction::BrowseFirstContainer => {
                let Some(path) = self
                    .filer
                    .entries
                    .iter()
                    .find(|entry| entry.is_container)
                    .map(|entry| entry.path.clone())
                else {
                    return false;
                };
                let Some(entry) = self.bench_container_entry_by_path(&path) else {
                    return false;
                };
                self.bench_activate_filer_entry(entry);
                true
            }
            BenchAction::BrowseSiblingContainer => {
                let Some(path) = self.bench_sibling_container_path() else {
                    return false;
                };
                let Some(entry) = self.bench_container_entry_by_path(&path) else {
                    return false;
                };
                self.bench_activate_filer_entry(entry);
                true
            }
            BenchAction::BrowseRandomContainer => {
                let Some(path) = self.bench_random_container_path() else {
                    return false;
                };
                let Some(entry) = self.bench_container_entry_by_path(&path) else {
                    return false;
                };
                self.bench_activate_filer_entry(entry);
                true
            }
            BenchAction::SelectNeighborFromFiler => {
                let Some(path) = self.bench_neighbor_entry_path() else {
                    return false;
                };
                let load_path = resolve_start_path(&path).unwrap_or_else(|| path.clone());
                self.filer.selected = Some(path.clone());
                self.empty_mode = false;
                self.show_filer = false;
                self.pending_fit_recalc = true;
                if self.show_subfiler {
                    self.pending_subfiler_focus_path = Some(path.clone());
                }
                let _ = self.request_load_target(path, load_path);
                true
            }
            BenchAction::SelectRandomFileFromFiler => {
                let Some(entry) = self
                    .bench_random_file_entry()
                    .or_else(|| self.bench_random_container_entry())
                else {
                    return false;
                };
                self.bench_activate_filer_entry(entry);
                true
            }
        }
    }

    fn run_bench_automation(&mut self, ctx: &egui::Context) {
        let Some(state) = self.bench_automation.as_ref() else {
            return;
        };
        let next_action_at = state.next_action_at;
        if Instant::now() < next_action_at {
            ctx.request_repaint_after(next_action_at.saturating_duration_since(Instant::now()));
            return;
        }

        let scenario_name = state.scenario_name.clone();
        let next_index = state.next_index;
        let Some(action) = state.actions.get(next_index).copied() else {
            if self.bench_automation_ready() {
                self.log_bench_state(
                    "viewer.bench_automation.completed",
                    serde_json::json!({
                        "frame_counter": self.frame_counter,
                        "scenario": scenario_name,
                    }),
                );
                self.log_bench_state(
                    "viewer.bench_automation.closing",
                    serde_json::json!({
                        "frame_counter": self.frame_counter,
                        "scenario": scenario_name,
                    }),
                );
                self.bench_automation = None;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                self.defer_bench_automation(100);
            }
            return;
        };

        if !self.bench_automation_ready() {
            self.defer_bench_automation(100);
            return;
        }

        self.log_bench_state(
            "viewer.bench_automation.action",
            serde_json::json!({
                "action": format!("{action:?}"),
                "scenario": scenario_name,
                "index": next_index,
            }),
        );

        if self.run_bench_action(action) {
            self.advance_bench_automation(500);
        } else {
            self.log_bench_state(
                "viewer.bench_automation.action_skipped",
                serde_json::json!({
                    "action": format!("{action:?}"),
                    "scenario": scenario_name,
                    "index": next_index,
                }),
            );
            self.advance_bench_automation(150);
        }
    }

    pub(crate) fn request_load_target(
        &mut self,
        navigation_path: PathBuf,
        load_request_path: PathBuf,
    ) -> Result<(), Box<dyn Error>> {
        let branch_changed =
            navigation_branch_path(&self.current_navigation_path) != navigation_branch_path(&navigation_path);
        let switching_image = self.current_navigation_path != navigation_path;
        if branch_changed {
            self.clear_manga_companion();
        }
        self.log_bench_state(
            "viewer.request_load_target",
            serde_json::json!({
                "navigation_path": navigation_path.display().to_string(),
                "load_request_path": load_request_path.display().to_string(),
                "branch_changed": branch_changed,
                "switching_image": switching_image,
            }),
        );
        if self.try_take_preloaded(&navigation_path) {
            self.log_bench_state(
                "viewer.request_load_target.preloaded_hit",
                serde_json::json!({
                    "navigation_path": navigation_path.display().to_string(),
                }),
            );
            return Ok(());
        }
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
        self.pending_fit_recalc = !matches!(self.render_options.zoom_option, ZoomOption::None);
        self.overlay
            .set_loading_message(format!("Loading {}", navigation_path.display()));
        let load_zoom = if switching_image { 1.0 } else { self.zoom };
        let spread_companion_path = self.desired_manga_companion_path_for_navigation(&navigation_path);
        self.worker_tx
            .send(RenderCommand::LoadPath {
                request_id,
                path: load_request_path,
                companion_path: spread_companion_path.clone(),
                zoom: load_zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
            })
            .map_err(worker_send_error)?;
        self.log_bench_state(
            "viewer.request_load_target.spread_plan",
            serde_json::json!({
                "navigation_path": navigation_path.display().to_string(),
                "companion_path": spread_companion_path.as_ref().map(|path| path.display().to_string()),
            }),
        );
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
            if self.companion_display.is_some() {
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
            if self.companion_display.is_some() {
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
        self.preload_cache.clear();
    }

    fn preload_cache_contains(&self, path: &Path) -> bool {
        self.preload_cache
            .iter()
            .any(|entry| entry.navigation_path == path)
    }

    fn cached_preloaded_entry(&self, path: &Path) -> Option<PreloadedEntry> {
        self.preload_cache
            .iter()
            .find(|entry| entry.navigation_path == path)
            .cloned()
    }

    fn remember_preloaded_entry(&mut self, entry: PreloadedEntry) {
        remember_preloaded_entry_in_cache(&mut self.preload_cache, entry);
    }

    fn remember_loaded_page_in_cache(
        &mut self,
        navigation_path: &Path,
        load_path: Option<&Path>,
        source: &LoadedImage,
        rendered: &LoadedImage,
    ) {
        self.remember_preloaded_entry(PreloadedEntry {
            navigation_path: navigation_path.to_path_buf(),
            load_path: load_path.map(Path::to_path_buf),
            display: DisplayedPageState {
                source: source.clone(),
                rendered: rendered.clone(),
                texture: (!self.current_texture_is_default).then(|| self.current_texture.clone()),
                texture_display_scale: self.texture_display_scale,
            },
        });
    }

    fn take_preloaded_entry(&mut self, path: &Path) -> Option<PreloadedEntry> {
        let index = self
            .preload_cache
            .iter()
            .position(|cached| cached.navigation_path == path)?;
        self.preload_cache.remove(index)
    }

    fn apply_companion_loaded(
        &mut self,
        path: Option<PathBuf>,
        display: DisplayedPageState,
    ) {
        let previous_companion = self.companion_display.clone();
        let layout_changed = path.is_some()
            || previous_companion
                .as_ref()
                .map(|current| {
                    current.source.canvas.width() != display.source.canvas.width()
                        || current.source.canvas.height() != display.source.canvas.height()
                })
                .unwrap_or(true);

        let mut display = display;
        let texture = if let Some(texture) = display.texture.clone() {
            texture
        } else {
            let (canvas, display_scale) = downscale_for_texture_limit(
                display.rendered.frame_canvas(0),
                self.max_texture_side,
                self.render_options.zoom_method,
            );
            let image = self.color_image_from_canvas(&canvas);
            let texture_options = self.texture_options();
            display.texture_display_scale = display_scale;
            if path.is_none() {
                if let Some(existing) = &mut previous_companion.clone() {
                    if let Some(texture) = &mut existing.texture {
                        texture.set(image, texture_options);
                        texture.clone()
                    } else {
                        self.egui_ctx
                            .load_texture("manga_companion", image, texture_options)
                    }
                } else {
                    self.egui_ctx
                        .load_texture("manga_companion", image, texture_options)
                }
            } else {
                self.egui_ctx
                    .load_texture("manga_companion", image, texture_options)
            }
        };

        display.texture = Some(texture);
        if display.texture_display_scale <= 0.0 {
            display.texture_display_scale = 1.0;
        }

        self.companion_display = Some(display);
        if layout_changed {
            self.pending_fit_recalc |= !matches!(self.render_options.zoom_option, ZoomOption::None);
        }
        self.companion_active_request = None;
    }

    fn apply_spread_companion_result(&mut self, companion: Option<LoadedRenderPage>) {
        let desired = self.desired_manga_companion_path_for_navigation(&self.current_navigation_path);
        match companion {
            Some(companion) if desired.as_ref() == Some(&companion.path) => {
                self.companion_navigation_path = Some(companion.path.clone());
                self.apply_companion_loaded(
                    Some(companion.path),
                    DisplayedPageState {
                        source: companion.source,
                        rendered: companion.rendered,
                        texture: None,
                        texture_display_scale: 1.0,
                    },
                );
            }
            _ => {
                self.clear_manga_companion();
            }
        }
    }

    fn spawn_navigation_workers(&mut self) {
        if self.fs_tx.is_none() || self.fs_rx.is_none() {
            let (tx, rx) = spawn_filesystem_worker(self.navigation_sort);
            self.fs_tx = Some(tx);
            self.fs_rx = Some(rx);
        }
        if self.filer_tx.is_none() || self.filer_rx.is_none() {
            let (tx, rx) = spawn_filer_worker();
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
        if should_queue_filesystem_init(self.active_fs_request_id) {
            self.log_bench_state(
                "viewer.init_filesystem.queued_busy",
                serde_json::json!({
                    "path": path.display().to_string(),
                    "active_fs_request_id": self.active_fs_request_id,
                }),
            );
            queue_filesystem_init_path(&mut self.queued_filesystem_init_path, path);
            return Ok(());
        }
        let Some(fs_tx) = self.fs_tx.clone() else {
            return Ok(());
        };
        let request_id = self.alloc_fs_request_id();
        self.active_fs_request_id = Some(request_id);
        self.log_bench_state(
            "viewer.init_filesystem",
            serde_json::json!({
                "request_id": request_id,
                "path": path.display().to_string(),
            }),
        );
        self.overlay
            .set_loading_message(format!("Scanning {}", path.display()));
        fs_tx
            .send(FilesystemCommand::Init { request_id, path })
            .map_err(filesystem_send_error)?;
        Ok(())
    }

    fn request_navigation(&mut self, mut command: FilesystemCommand) -> Result<(), Box<dyn Error>> {
        self.spawn_navigation_workers();
        if !self.navigator_ready {
            self.log_bench_state(
                "viewer.request_navigation.queued_not_ready",
                serde_json::json!({
                    "command": format!("{command:?}"),
                }),
            );
            queue_navigation_command(&mut self.queued_navigation, command);
            return Ok(());
        }
        if self.active_fs_request_id.is_some() {
            self.log_bench_state(
                "viewer.request_navigation.queued_busy",
                serde_json::json!({
                    "command": format!("{command:?}"),
                }),
            );
            queue_navigation_command(&mut self.queued_navigation, command);
            return Ok(());
        }
        let Some(fs_tx) = self.fs_tx.clone() else {
            self.log_bench_state(
                "viewer.request_navigation.queued_no_worker",
                serde_json::json!({
                    "command": format!("{command:?}"),
                }),
            );
            queue_navigation_command(&mut self.queued_navigation, command);
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
        };
        self.log_bench_state(
            "viewer.request_navigation.sent",
            serde_json::json!({
                "request_id": request_id,
                "command": format!("{command:?}"),
            }),
        );
        self.overlay.set_loading_message("Scanning folder...");
        fs_tx.send(command).map_err(filesystem_send_error)?;
        Ok(())
    }

    fn apply_loaded_result(
        &mut self,
        path: Option<PathBuf>,
        source: LoadedImage,
        rendered: LoadedImage,
        companion: Option<LoadedRenderPage>,
    ) {
        self.log_bench_state(
            "viewer.apply_loaded_result.begin",
            serde_json::json!({
                "loaded_path": path.as_ref().map(|path| path.display().to_string()),
                "source_size": [source.canvas.width(), source.canvas.height()],
                "rendered_size": [rendered.canvas.width(), rendered.canvas.height()],
            }),
        );
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
            let folder_changed = should_reinitialize_filesystem_after_load(
                &previous_navigation_path,
                &self.current_navigation_path,
            );
            let committed_filer_select = should_cancel_filesystem_request_for_filer_select(
                self.filer.pending_user_request.as_ref(),
                &self.current_navigation_path,
                self.active_fs_request_id,
            );
            self.current_path = path.clone();
            self.save_dialog.file_name = default_save_file_name(&path);
            if folder_changed {
                self.clear_manga_companion();
                self.prev_texture = None;
            }
            if committed_filer_select {
                self.log_bench_state(
                    "viewer.filesystem.restarted_for_filer_select",
                    serde_json::json!({
                        "active_fs_request_id": self.active_fs_request_id,
                        "navigation_path": self.current_navigation_path.display().to_string(),
                    }),
                );
                self.navigator_ready = false;
                self.queued_navigation = None;
                self.respawn_filesystem_worker();
                let _ = self.init_filesystem(self.current_navigation_path.clone());
            } else if folder_changed {
                self.navigator_ready = false;
                self.queued_navigation = None;
                let _ = self.init_filesystem(self.current_navigation_path.clone());
            } else if let Some(fs_tx) = self.fs_tx.clone() {
                let request_id = self.alloc_fs_request_id();
                let _ = fs_tx.send(FilesystemCommand::SetCurrent {
                    request_id,
                    path: self.current_navigation_path.clone(),
                });
            }
            if self.show_subfiler {
                self.pending_subfiler_focus_path = Some(self.current_navigation_path.clone());
            }
            if should_clear_stale_filer_refresh_request(
                self.filer.pending_user_request.as_ref(),
                self.current_directory().as_deref(),
            ) {
                self.log_bench_state(
                    "viewer.filer.refresh_request_cleared_after_branch_change",
                    serde_json::json!({
                        "current_directory": self.current_directory().map(|path| path.display().to_string()),
                    }),
                );
                self.filer.pending_user_request = None;
            }
            self.clear_committed_filer_user_request();
            self.sync_filer_directory_with_current_path();
            self.apply_spread_companion_result(companion);
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

        let source_size = vec2(self.source.canvas.width() as f32, self.source.canvas.height() as f32);
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
        let cache_navigation_path = self.current_navigation_path.clone();
        let cache_source = self.source.clone();
        let cache_rendered = self.rendered.clone();
        self.remember_loaded_page_in_cache(
            &cache_navigation_path,
            loaded_path.as_deref(),
            &cache_source,
            &cache_rendered,
        );
        if !self.navigator_ready && self.active_fs_request_id.is_none() {
            if self.deferred_filesystem_init_path.is_some() {
                self.deferred_filesystem_init_path =
                    Some(
                        loaded_path
                            .clone()
                            .unwrap_or_else(|| self.current_navigation_path.clone()),
                    );
                self.defer_initial_filesystem_sync();
            }
        }
        self.schedule_preload();
        if !self.bench_initial_load_logged {
            self.bench_initial_load_logged = true;
            self.log_bench_state(
                "viewer.initial_load.completed",
                serde_json::json!({
                    "loaded_path": loaded_path.as_ref().map(|path| path.display().to_string()),
                    "frame_counter": self.frame_counter,
                    "startup_phase": format!("{:?}", self.startup_phase),
                }),
            );
        }
        self.log_bench_state(
            "viewer.apply_loaded_result.end",
            serde_json::json!({
                "loaded_path": loaded_path.as_ref().map(|path| path.display().to_string()),
            }),
        );
        if self.pending_resize_after_load {
            self.pending_resize_after_load = false;
            let _ = self.request_resize_current();
        } else if self.pending_resize_after_render {
            self.pending_resize_after_render = false;
            let _ = self.request_resize_current();
        }
        self.flush_pending_viewer_navigation();
    }

    fn next_preload_candidate(&self) -> Option<PathBuf> {
        if let Some(companion) = self.desired_manga_companion_path() {
            let companion_ready = self.visible_companion().is_some();
            if should_prioritize_companion_preload(
                Some(companion.as_path()),
                self.companion_navigation_path.as_deref(),
                companion_ready,
            ) {
                return Some(companion);
            }
        }
        let step = if self.manga_spread_active() { 2 } else { 1 };
        adjacent_entry(&self.current_navigation_path, self.navigation_sort, step)
    }

    fn schedule_preload(&mut self) {
        if self.empty_mode || self.active_request.is_some() {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_busy",
                serde_json::json!({}),
            );
            return;
        }
        if !self.navigator_ready {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_not_ready",
                serde_json::json!({}),
            );
            return;
        }
        if archive_prefers_low_io(&self.current_navigation_path) {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_low_io_current",
                serde_json::json!({
                    "path": self.current_navigation_path.display().to_string(),
                }),
            );
            return;
        }
        let Some(path) = self.next_preload_candidate() else {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_no_candidate",
                serde_json::json!({}),
            );
            return;
        };
        if archive_prefers_low_io(&path) {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_low_io_candidate",
                serde_json::json!({
                    "path": path.display().to_string(),
                }),
            );
            return;
        }
        if self.preload_cache_contains(&path)
            || self.pending_preload_navigation_path.as_ref() == Some(&path)
        {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_duplicate",
                serde_json::json!({
                    "path": path.display().to_string(),
                }),
            );
            return;
        }
        let request_id = self.alloc_preload_request_id();
        self.active_preload_request_id = Some(request_id);
        self.pending_preload_navigation_path = Some(path.clone());
        self.log_bench_state(
            "viewer.schedule_preload.sent",
            serde_json::json!({
                "request_id": request_id,
                "path": path.display().to_string(),
            }),
        );
        let _ = self.preload_tx.send(RenderCommand::LoadPath {
            request_id,
            path,
            companion_path: None,
            zoom: self.zoom,
            method: self.render_options.zoom_method,
            scale_mode: self.render_options.scale_mode,
        });
    }

    fn try_take_preloaded(&mut self, path: &std::path::Path) -> bool {
        let Some(entry) = self.take_preloaded_entry(path) else {
            return false;
        };

        self.log_bench_state(
            "viewer.try_take_preloaded.hit",
            serde_json::json!({
                "path": path.display().to_string(),
                "load_path": entry.load_path.as_ref().map(|path| path.display().to_string()),
            }),
        );
        if let Some(texture) = entry.display.texture {
            self.current_texture = texture;
            self.current_texture_is_default = false;
            self.texture_display_scale = entry.display.texture_display_scale;
        }
        self.pending_navigation_path = Some(path.to_path_buf());
        self.overlay.clear_loading_message();
        self.apply_loaded_result(entry.load_path, entry.display.source, entry.display.rendered, None);
        true
    }

    fn respawn_render_worker(&mut self) {
        let (worker_tx, worker_rx, worker_join) = spawn_render_worker(self.source.clone());
        self.worker_tx = worker_tx;
        self.worker_rx = worker_rx;
        self.worker_join = Some(worker_join);
        self.active_request = None;
    }

    fn respawn_companion_worker(&mut self) {
        let seed = self
            .companion_display
            .as_ref()
            .map(|display| display.source.clone())
            .unwrap_or_else(|| self.source.clone());
        let (tx, rx, join) = spawn_render_worker(seed);
        self.companion_tx = tx;
        self.companion_rx = rx;
        self.companion_join = Some(join);
        self.companion_active_request = None;
    }

    fn respawn_preload_worker(&mut self) {
        let (tx, rx, join) = spawn_render_worker(self.source.clone());
        self.preload_tx = tx;
        self.preload_rx = rx;
        self.preload_join = Some(join);
        self.invalidate_preload();
    }

    pub(crate) fn respawn_filesystem_worker(&mut self) {
        let (tx, rx) = spawn_filesystem_worker(self.navigation_sort);
        self.fs_tx = Some(tx);
        self.fs_rx = Some(rx);
        self.navigator_ready = false;
        self.active_fs_request_id = None;
        let _ = self.init_filesystem(self.current_navigation_path.clone());
    }

    fn respawn_filer_worker(&mut self) {
        let (tx, rx) = spawn_filer_worker();
        self.filer_tx = Some(tx);
        self.filer_rx = Some(rx);
        self.filer.pending_request_id = None;
        if let Some(dir) = self
            .filer
            .directory
            .clone()
            .or_else(|| self.current_directory())
        {
            self.request_filer_directory(dir, self.filer.selected.clone());
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
                    companion,
                    metrics,
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
                    self.log_bench_state(
                        "viewer.poll_worker.loaded",
                        serde_json::json!({
                            "request_id": request_id,
                            "path": path.as_ref().map(|path| path.display().to_string()),
                            "companion_path": companion.as_ref().map(|page| page.path.display().to_string()),
                            "metrics": Self::bench_metrics_payload(&metrics),
                        }),
                    );
                    self.apply_loaded_result(path, source, rendered, companion);
                }
                Ok(RenderResult::Failed {
                    request_id,
                    path,
                    message,
                    metrics,
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
                    self.log_bench_state(
                        "viewer.poll_worker.failed",
                        serde_json::json!({
                            "request_id": request_id,
                            "path": path.as_ref().map(|path| path.display().to_string()),
                            "message": message,
                            "metrics": Self::bench_metrics_payload(&metrics),
                        }),
                    );
                    let failed_during_load =
                        matches!(active_request, ActiveRenderRequest::Load(_));
                    let failed_navigation_path = self.pending_navigation_path.take();
                    let should_advance = failed_during_load
                        && should_advance_after_load_failure(
                            &self.current_navigation_path,
                            failed_navigation_path.as_deref(),
                        );
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
                    if matches!(
                        self.filer.pending_user_request,
                        Some(FilerUserRequest::SelectFile { .. })
                    ) {
                        self.log_bench_state(
                            "viewer.filer.select_request_cleared_after_load_failure",
                            serde_json::json!({
                                "failed_navigation_path": failed_navigation_path
                                    .as_ref()
                                    .map(|path| path.display().to_string()),
                            }),
                        );
                        self.filer.pending_user_request = None;
                    }
                    self.flush_pending_viewer_navigation();
                    if !self.navigator_ready && self.active_fs_request_id.is_none() {
                        if self.deferred_filesystem_init_path.is_some() {
                            self.deferred_filesystem_init_path =
                                Some(self.current_navigation_path.clone());
                            self.defer_initial_filesystem_sync();
                        }
                    }
                    if should_advance {
                        let _ = self.next_image();
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.log_bench_state(
                        "viewer.poll_worker.disconnected",
                        serde_json::json!({}),
                    );
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
                    companion: _,
                    metrics,
                }) => {
                    if self.active_preload_request_id != Some(request_id) {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_preload_worker.loaded",
                        serde_json::json!({
                            "request_id": request_id,
                            "path": path.as_ref().map(|path| path.display().to_string()),
                            "navigation_path": self.pending_preload_navigation_path.as_ref().map(|path| path.display().to_string()),
                            "metrics": Self::bench_metrics_payload(&metrics),
                        }),
                    );
                    self.active_preload_request_id = None;
                    let Some(navigation_path) = self.pending_preload_navigation_path.take() else {
                        continue;
                    };
                    let texture_name = self.texture_name_for_path(path.as_deref());
                    let (texture, display_scale) =
                        self.build_texture_from_canvas(&texture_name, rendered.frame_canvas(0));
                    self.remember_preloaded_entry(PreloadedEntry {
                        navigation_path,
                        load_path: path,
                        display: DisplayedPageState {
                            source,
                            rendered,
                            texture: Some(texture),
                            texture_display_scale: display_scale,
                        },
                    });
                }
                Ok(RenderResult::Failed {
                    request_id,
                    metrics,
                    ..
                }) => {
                    if self.active_preload_request_id == Some(request_id) {
                        self.log_bench_state(
                            "viewer.poll_preload_worker.failed",
                            serde_json::json!({
                                "request_id": request_id,
                                "navigation_path": self.pending_preload_navigation_path.as_ref().map(|path| path.display().to_string()),
                                "metrics": Self::bench_metrics_payload(&metrics),
                            }),
                        );
                        self.active_preload_request_id = None;
                        self.pending_preload_navigation_path = None;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.log_bench_state(
                        "viewer.poll_preload_worker.disconnected",
                        serde_json::json!({}),
                    );
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
                    companion: _,
                    metrics,
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
                    self.log_bench_state(
                        "viewer.poll_companion_worker.loaded",
                        serde_json::json!({
                            "request_id": request_id,
                            "path": path.as_ref().map(|path| path.display().to_string()),
                            "companion_navigation_path": self.companion_navigation_path.as_ref().map(|path| path.display().to_string()),
                            "metrics": Self::bench_metrics_payload(&metrics),
                        }),
                    );
                    let (canvas, display_scale) = downscale_for_texture_limit(
                        rendered.frame_canvas(0),
                        self.max_texture_side,
                        self.render_options.zoom_method,
                    );
                    let image = self.color_image_from_canvas(&canvas);
                    let texture_options = self.texture_options();
                    let texture = if path.is_none() {
                        if let Some(texture) = self
                            .companion_display
                            .as_mut()
                            .and_then(|display| display.texture.as_mut())
                        {
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
                    if let Some(navigation_path) = self.companion_navigation_path.clone() {
                        self.remember_preloaded_entry(PreloadedEntry {
                            navigation_path,
                            load_path: path.clone(),
                            display: DisplayedPageState {
                                source: source.clone(),
                                rendered: rendered.clone(),
                                texture: Some(texture.clone()),
                                texture_display_scale: display_scale,
                            },
                        });
                    }
                    self.apply_companion_loaded(path, DisplayedPageState {
                        source,
                        rendered,
                        texture: Some(texture),
                        texture_display_scale: display_scale,
                    });
                }
                Ok(RenderResult::Failed {
                    request_id,
                    metrics,
                    ..
                }) => {
                    let Some(active_request) = self.companion_active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if request_matches {
                        self.log_bench_state(
                            "viewer.poll_companion_worker.failed",
                            serde_json::json!({
                                "request_id": request_id,
                                "companion_navigation_path": self.companion_navigation_path.as_ref().map(|path| path.display().to_string()),
                                "metrics": Self::bench_metrics_payload(&metrics),
                            }),
                        );
                        self.clear_manga_companion();
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.log_bench_state(
                        "viewer.poll_companion_worker.disconnected",
                        serde_json::json!({}),
                    );
                    self.clear_manga_companion();
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
                        self.log_bench_state(
                            "viewer.poll_filesystem.navigator_ready",
                            serde_json::json!({
                                "request_id": request_id,
                                "navigation_path": navigation_path.as_ref().map(|path| path.display().to_string()),
                                "load_path": load_path.as_ref().map(|path| path.display().to_string()),
                            }),
                        );
                        self.navigator_ready = true;
                        self.active_fs_request_id = None;
                        self.startup_phase = StartupPhase::MultiViewer;
                        self.log_bench_startup_sync_once("navigator_ready");
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
                        self.log_bench_state(
                            "viewer.poll_filesystem.path_resolved",
                            serde_json::json!({
                                "request_id": request_id,
                                "navigation_path": navigation_path.display().to_string(),
                                "load_path": load_path.display().to_string(),
                            }),
                        );
                        self.empty_mode = false;
                        self.startup_phase = StartupPhase::MultiViewer;
                        self.log_bench_startup_sync_once("path_resolved");
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
                        self.log_bench_state(
                            "viewer.poll_filesystem.no_path",
                            serde_json::json!({
                                "request_id": request_id,
                            }),
                        );
                        self.startup_phase = StartupPhase::MultiViewer;
                        self.log_bench_startup_sync_once("no_path");
                        self.overlay
                            .set_loading_message("No displayable file found");
                        self.show_filer = true;
                        self.active_fs_request_id = None;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.log_bench_state(
                        "viewer.poll_filesystem.disconnected",
                        serde_json::json!({}),
                    );
                    self.overlay
                        .set_loading_message("filesystem worker disconnected");
                    self.respawn_filesystem_worker();
                    break;
                }
            }
        }
        if self.active_fs_request_id.is_none() {
            match take_next_queued_filesystem_work(
                &mut self.queued_filesystem_init_path,
                &mut self.queued_navigation,
            ) {
                Some(PendingFilesystemWork::Init(path)) => {
                    let _ = self.init_filesystem(path);
                }
                Some(PendingFilesystemWork::Command(command)) => {
                    let _ = self.request_navigation(command);
                }
                None => {}
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
                Ok(FilerResult::Reset {
                    request_id,
                    directory,
                    selected,
                }) => {
                    if self.filer.pending_request_id != Some(request_id) {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_filer_worker.reset",
                        serde_json::json!({
                            "request_id": request_id,
                            "directory": directory.display().to_string(),
                            "selected": selected.as_ref().map(|path| path.display().to_string()),
                        }),
                    );
                    self.filer.directory = Some(directory);
                    self.filer.entries.clear();
                    self.filer.selected = self.filer_selected_for_directory(
                        self.filer.directory.as_deref().unwrap(),
                        selected,
                    );
                }
                Ok(FilerResult::Append {
                    request_id,
                    entries,
                }) => {
                    if self.filer.pending_request_id != Some(request_id) {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_filer_worker.append",
                        serde_json::json!({
                            "request_id": request_id,
                            "entry_count": entries.len(),
                        }),
                    );
                    self.filer.entries.extend(entries);
                }
                Ok(FilerResult::Snapshot {
                    request_id,
                    directory,
                    entries,
                    selected,
                }) => {
                    if self.filer.pending_request_id != Some(request_id) {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_filer_worker.snapshot",
                        serde_json::json!({
                            "request_id": request_id,
                            "directory": directory.display().to_string(),
                            "entry_count": entries.len(),
                            "selected": selected.as_ref().map(|path| path.display().to_string()),
                        }),
                    );
                    if matches!(
                        self.filer.pending_user_request.as_ref(),
                        Some(FilerUserRequest::BrowseDirectory { directory: browse_dir }) if browse_dir == &directory
                    ) {
                        self.filer.committed_browse_directory = Some(directory.clone());
                        self.log_bench_state(
                            "viewer.filer.browse_committed",
                            serde_json::json!({
                                "request_id": request_id,
                                "directory": directory.display().to_string(),
                            }),
                        );
                        self.filer.pending_user_request = None;
                    }
                    self.filer.pending_request_id = None;
                    self.filer.directory = Some(directory);
                    self.filer.entries = entries;
                    let snapshot_signature = filer_entries_signature(&self.filer.entries);
                    let snapshot_changed_in_same_directory = filer_snapshot_changed_in_same_directory(
                        self.last_filer_snapshot_signature
                            .as_ref()
                            .map(|(directory, signature)| (directory.as_path(), *signature)),
                        self.filer.directory.as_deref().unwrap(),
                        snapshot_signature,
                    );
                    self.last_filer_snapshot_signature = Some((
                        self.filer.directory.as_deref().unwrap().to_path_buf(),
                        snapshot_signature,
                    ));
                    self.filer.selected = self.filer_selected_for_directory(
                        self.filer.directory.as_deref().unwrap(),
                        selected,
                    );
                    if snapshot_changed_in_same_directory
                        && self.navigator_ready
                        && self.active_fs_request_id.is_none()
                        && self.current_directory().as_deref() == self.filer.directory.as_deref()
                    {
                        self.log_bench_state(
                            "viewer.filesystem.reinit_from_filer_snapshot",
                            serde_json::json!({
                                "directory": self.filer.directory.as_ref().map(|path| path.display().to_string()),
                                "entry_count": self.filer.entries.len(),
                                "current_navigation_path": self.current_navigation_path.display().to_string(),
                            }),
                        );
                        self.navigator_ready = false;
                        self.queued_navigation = None;
                        let _ = self.init_filesystem(self.current_navigation_path.clone());
                    }
                    if should_clear_filer_user_request_after_snapshot(
                        self.filer.pending_user_request.as_ref(),
                    ) {
                        self.filer.pending_user_request = None;
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
                    image,
                }) => {
                    self.thumbnail_pending.remove(&path);
                    let texture = self.egui_ctx.load_texture(
                        format!("thumb:{}", path.display()),
                        image,
                        TextureOptions::LINEAR,
                    );
                    self.thumbnail_cache.insert(path, texture);
                }
                Ok(ThumbnailResult::Failed {
                    _request_id: _,
                    path,
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
        self.spawn_navigation_workers();
        let Some(thumbnail_tx) = self.thumbnail_tx.clone() else {
            return;
        };
        if self.thumbnail_cache.contains_key(path) || self.thumbnail_pending.contains(path) {
            return;
        }
        let request_id = self.alloc_thumbnail_request_id();
        let path = path.to_path_buf();
        self.thumbnail_pending.insert(path.clone());
        let _ = thumbnail_tx.send(ThumbnailCommand::Generate {
            request_id,
            path,
            max_side,
        });
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
        self.sync_manga_companion(ctx);
        self.handle_keyboard(ctx);
        self.poll_pending_pointer_actions();
        self.settings_ui(ctx);
        self.restart_prompt_ui(ctx);
        self.alert_dialog_ui(ctx);
        self.save_dialog_ui(ctx);
        self.left_click_menu_ui(ctx);
        self.run_bench_automation(ctx);
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
            } else if !self.empty_mode
                && (viewport_size_changed(viewport, self.last_viewport_size)
                    || self.pending_fit_recalc)
                && !matches!(self.render_options.zoom_option, ZoomOption::None)
            {
                self.last_viewport_size = viewport;
                self.pending_fit_recalc = false;

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
                    let companion = self.visible_companion();

                    let companion_draw_size = companion.map(|(companion_rendered, _)| {
                        vec2(
                            companion_rendered.canvas.width() as f32 * self.companion_draw_scale(),
                            companion_rendered.canvas.height() as f32
                                * self.companion_draw_scale(),
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

fn should_advance_after_load_failure(
    current_navigation_path: &Path,
    failed_navigation_path: Option<&Path>,
) -> bool {
    failed_navigation_path
        .is_some_and(|path| path == current_navigation_path)
}

fn should_clear_filer_select_request_for_current(
    pending_user_request: Option<&FilerUserRequest>,
    current_navigation_path: &Path,
) -> bool {
    matches!(
        pending_user_request,
        Some(FilerUserRequest::SelectFile { navigation_path })
            if navigation_path == current_navigation_path
    )
}

fn should_clear_stale_filer_refresh_request(
    pending_user_request: Option<&FilerUserRequest>,
    current_directory: Option<&Path>,
) -> bool {
    matches!(
        (pending_user_request, current_directory),
        (
            Some(FilerUserRequest::Refresh { directory, .. }),
            Some(current_directory),
        ) if directory != current_directory
    )
}

fn navigation_sort_for_filer(
    filer_sort_field: FilerSortField,
    name_sort_mode: NameSortMode,
) -> NavigationSortOption {
    match filer_sort_field {
        FilerSortField::Name => match name_sort_mode {
            NameSortMode::Os => NavigationSortOption::OsName,
            NameSortMode::CaseSensitive | NameSortMode::CaseInsensitive => {
                NavigationSortOption::Name
            }
        },
        FilerSortField::Modified => NavigationSortOption::Date,
        FilerSortField::Size => NavigationSortOption::Size,
    }
}

fn should_queue_filesystem_init(active_fs_request_id: Option<u64>) -> bool {
    active_fs_request_id.is_some()
}

fn queue_filesystem_init_path(slot: &mut Option<PathBuf>, path: PathBuf) {
    *slot = Some(path);
}

fn should_defer_companion_sync_during_primary_load(
    active_request: Option<ActiveRenderRequest>,
) -> bool {
    matches!(active_request, Some(ActiveRenderRequest::Load(_)))
}

fn spread_companion_path_for_navigation(
    navigation_path: &Path,
    navigation_sort: NavigationSortOption,
    manga_mode: bool,
) -> Option<PathBuf> {
    if !manga_mode {
        return None;
    }
    let companion = adjacent_entry(navigation_path, navigation_sort, 1)?;
    let current_branch = navigation_branch_path(navigation_path);
    let companion_branch = navigation_branch_path(&companion);
    (current_branch == companion_branch).then_some(companion)
}

fn should_cancel_filesystem_request_for_filer_select(
    pending_user_request: Option<&FilerUserRequest>,
    current_navigation_path: &Path,
    active_fs_request_id: Option<u64>,
) -> bool {
    active_fs_request_id.is_some()
        && matches!(
            pending_user_request,
            Some(FilerUserRequest::SelectFile { navigation_path })
                if navigation_path == current_navigation_path
        )
}

fn filer_entries_signature(entries: &[crate::ui::menu::fileviewer::state::FilerEntry]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    entries.len().hash(&mut hasher);
    for entry in entries {
        entry.path.hash(&mut hasher);
        entry.is_container.hash(&mut hasher);
    }
    hasher.finish()
}

fn filer_snapshot_changed_in_same_directory(
    previous: Option<(&Path, u64)>,
    snapshot_directory: &Path,
    snapshot_signature: u64,
) -> bool {
    matches!(
        previous,
        Some((directory, signature))
            if directory == snapshot_directory && signature != snapshot_signature
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawers::canvas::Canvas;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn dummy_loaded_image(width: u32, height: u32) -> LoadedImage {
        LoadedImage {
            canvas: Canvas::new(width, height),
            animation: Vec::new(),
            loop_count: None,
        }
    }

    fn dummy_preloaded_entry(path: &str) -> PreloadedEntry {
        PreloadedEntry {
            navigation_path: PathBuf::from(path),
            load_path: Some(PathBuf::from(path)),
            display: DisplayedPageState {
                source: dummy_loaded_image(4, 4),
                rendered: dummy_loaded_image(4, 4),
                texture: None,
                texture_display_scale: 1.0,
            },
        }
    }

    #[test]
    fn remember_preloaded_entry_in_cache_keeps_two_most_recent_entries() {
        let mut cache = VecDeque::new();
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("a"));
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("b"));
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("c"));

        let paths = cache
            .iter()
            .map(|entry| entry.navigation_path.clone())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec![PathBuf::from("c"), PathBuf::from("b")]);
    }

    #[test]
    fn remember_preloaded_entry_in_cache_refreshes_existing_entry_recency() {
        let mut cache = VecDeque::new();
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("a"));
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("b"));
        remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("a"));

        let paths = cache
            .iter()
            .map(|entry| entry.navigation_path.clone())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec![PathBuf::from("a"), PathBuf::from("b")]);
    }

    #[test]
    fn should_prioritize_companion_preload_until_visible_companion_is_ready() {
        let desired = Path::new("companion");

        assert!(should_prioritize_companion_preload(
            Some(desired),
            None,
            false,
        ));
        assert!(should_prioritize_companion_preload(
            Some(desired),
            Some(desired),
            false,
        ));
        assert!(!should_prioritize_companion_preload(
            Some(desired),
            Some(desired),
            true,
        ));
        assert!(!should_prioritize_companion_preload(None, None, false));
    }

    #[test]
    fn snapshot_only_clears_refresh_user_request() {
        assert!(should_clear_filer_user_request_after_snapshot(Some(
            &FilerUserRequest::Refresh {
                directory: PathBuf::from("dir"),
                selected: None,
            },
        )));
        assert!(!should_clear_filer_user_request_after_snapshot(Some(
            &FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir"),
            },
        )));
        assert!(!should_clear_filer_user_request_after_snapshot(Some(
            &FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir\\file"),
            },
        )));
    }

    #[test]
    fn zip_to_zip_bench_plan_is_available() {
        let (name, actions) = bench_automation_plan(Some("zip_to_zip"));

        assert_eq!(name, "zip_to_zip");
        assert!(actions.contains(&BenchAction::BrowseSiblingContainer));
    }

    #[test]
    fn zip_to_zip_random_bench_plan_is_available() {
        let (name, actions) = bench_automation_plan(Some("zip_to_zip_random"));

        assert_eq!(name, "zip_to_zip_random");
        assert!(actions.contains(&BenchAction::BrowseRandomContainer));
        assert!(actions.contains(&BenchAction::SelectRandomFileFromFiler));
        assert!(actions.contains(&BenchAction::Next));
        assert!(actions.contains(&BenchAction::Prev));
        assert_eq!(
            actions
                .iter()
                .filter(|action| **action == BenchAction::BrowseRandomContainer)
                .count(),
            ZIP_TO_ZIP_RANDOM_WALK_ROUNDS,
        );
        assert_eq!(
            actions
                .iter()
                .filter(|action| **action == BenchAction::RefreshFiler)
                .count(),
            ZIP_TO_ZIP_RANDOM_WALK_ROUNDS,
        );
    }

    #[test]
    fn snapshot_does_not_clear_browse_user_request_directly() {
        assert!(!should_clear_filer_user_request_after_snapshot(Some(
            &FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir"),
            },
        )));
    }

    #[test]
    fn branch_change_requires_filesystem_reinit_after_load() {
        assert!(should_reinitialize_filesystem_after_load(
            Path::new("a.zip\\__zipv__\\0001.jpg"),
            Path::new("b.zip\\__zipv__\\0001.jpg"),
        ));
        assert!(!should_reinitialize_filesystem_after_load(
            Path::new("a.zip\\__zipv__\\0001.jpg"),
            Path::new("a.zip\\__zipv__\\0002.jpg"),
        ));
    }

    #[test]
    fn load_failure_only_auto_advances_when_current_image_failed() {
        assert!(should_advance_after_load_failure(
            Path::new("dir\\current.png"),
            Some(Path::new("dir\\current.png")),
        ));
        assert!(!should_advance_after_load_failure(
            Path::new("dir\\current.png"),
            Some(Path::new("dir\\other.png")),
        ));
        assert!(!should_advance_after_load_failure(
            Path::new("dir\\current.png"),
            None,
        ));
    }

    #[test]
    fn clears_matching_filer_select_request_for_current_path() {
        assert!(should_clear_filer_select_request_for_current(
            Some(&FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir\\current.png"),
            }),
            Path::new("dir\\current.png"),
        ));
        assert!(!should_clear_filer_select_request_for_current(
            Some(&FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir\\other.png"),
            }),
            Path::new("dir\\current.png"),
        ));
        assert!(!should_clear_filer_select_request_for_current(
            Some(&FilerUserRequest::BrowseDirectory {
                directory: PathBuf::from("dir"),
            }),
            Path::new("dir\\current.png"),
        ));
    }

    #[test]
    fn clears_stale_filer_refresh_request_after_directory_change() {
        assert!(should_clear_stale_filer_refresh_request(
            Some(&FilerUserRequest::Refresh {
                directory: PathBuf::from("dir-a"),
                selected: Some(PathBuf::from("dir-a\\current.png")),
            }),
            Some(Path::new("dir-b")),
        ));
        assert!(!should_clear_stale_filer_refresh_request(
            Some(&FilerUserRequest::Refresh {
                directory: PathBuf::from("dir-a"),
                selected: Some(PathBuf::from("dir-a\\current.png")),
            }),
            Some(Path::new("dir-a")),
        ));
        assert!(!should_clear_stale_filer_refresh_request(
            Some(&FilerUserRequest::SelectFile {
                navigation_path: PathBuf::from("dir-a\\current.png"),
            }),
            Some(Path::new("dir-b")),
        ));
        assert!(!should_clear_stale_filer_refresh_request(
            Some(&FilerUserRequest::Refresh {
                directory: PathBuf::from("dir-a"),
                selected: None,
            }),
            None,
        ));
    }

    #[test]
    fn maps_filer_sort_to_navigation_sort() {
        assert_eq!(
            navigation_sort_for_filer(FilerSortField::Name, NameSortMode::Os),
            NavigationSortOption::OsName,
        );
        assert_eq!(
            navigation_sort_for_filer(FilerSortField::Name, NameSortMode::CaseSensitive),
            NavigationSortOption::Name,
        );
        assert_eq!(
            navigation_sort_for_filer(FilerSortField::Modified, NameSortMode::Os),
            NavigationSortOption::Date,
        );
        assert_eq!(
            navigation_sort_for_filer(FilerSortField::Size, NameSortMode::CaseInsensitive),
            NavigationSortOption::Size,
        );
    }

    #[test]
    fn queues_filesystem_init_when_request_is_already_active() {
        assert!(should_queue_filesystem_init(Some(1)));
        assert!(!should_queue_filesystem_init(None));
    }

    #[test]
    fn queued_filesystem_init_is_not_overwritten_by_navigation_queue() {
        let mut queued_init = None;
        queue_filesystem_init_path(&mut queued_init, PathBuf::from("dir-a"));
        let mut queued_navigation = None;
        queue_navigation_command(&mut queued_navigation, FilesystemCommand::Next {
            request_id: 0,
            policy: EndOfFolderOption::Recursive,
        });
        queue_navigation_command(&mut queued_navigation, FilesystemCommand::Prev {
            request_id: 0,
            policy: EndOfFolderOption::Recursive,
        });

        assert_eq!(queued_init, Some(PathBuf::from("dir-a")));
        assert!(matches!(
            queued_navigation,
            Some(FilesystemCommand::Prev {
                policy: EndOfFolderOption::Recursive,
                ..
            })
        ));
    }

    #[test]
    fn queued_filesystem_work_prioritizes_init_before_navigation() {
        let mut queued_init = Some(PathBuf::from("dir-a"));
        let mut queued_navigation = Some(FilesystemCommand::Next {
            request_id: 0,
            policy: EndOfFolderOption::Recursive,
        });

        let first = take_next_queued_filesystem_work(&mut queued_init, &mut queued_navigation);
        let second = take_next_queued_filesystem_work(&mut queued_init, &mut queued_navigation);

        assert!(matches!(
            first,
            Some(PendingFilesystemWork::Init(path)) if path == PathBuf::from("dir-a")
        ));
        assert!(matches!(
            second,
            Some(PendingFilesystemWork::Command(FilesystemCommand::Next {
                policy: EndOfFolderOption::Recursive,
                ..
            }))
        ));
        assert!(queued_init.is_none());
        assert!(queued_navigation.is_none());
    }

    #[test]
    fn defers_companion_sync_while_primary_load_is_active() {
        assert!(should_defer_companion_sync_during_primary_load(Some(
            ActiveRenderRequest::Load(7),
        )));
        assert!(!should_defer_companion_sync_during_primary_load(Some(
            ActiveRenderRequest::Resize(7),
        )));
        assert!(!should_defer_companion_sync_during_primary_load(None));
    }


    #[test]
    fn cancels_busy_filesystem_request_for_matching_filer_select() {
        let pending = FilerUserRequest::SelectFile {
            navigation_path: PathBuf::from("dir\\current.png"),
        };

        assert!(should_cancel_filesystem_request_for_filer_select(
            Some(&pending),
            Path::new("dir\\current.png"),
            Some(7),
        ));
        assert!(!should_cancel_filesystem_request_for_filer_select(
            Some(&pending),
            Path::new("dir\\other.png"),
            Some(7),
        ));
        assert!(!should_cancel_filesystem_request_for_filer_select(
            Some(&pending),
            Path::new("dir\\current.png"),
            None,
        ));
    }

    #[test]
    fn detects_filer_snapshot_change_in_same_directory_only() {
        assert!(!filer_snapshot_changed_in_same_directory(
            None,
            Path::new("dir-a"),
            10
        ));
        assert!(!filer_snapshot_changed_in_same_directory(
            Some((Path::new("dir-a"), 10)),
            Path::new("dir-a"),
            10,
        ));
        assert!(filer_snapshot_changed_in_same_directory(
            Some((Path::new("dir-a"), 10)),
            Path::new("dir-a"),
            11,
        ));
        assert!(!filer_snapshot_changed_in_same_directory(
            Some((Path::new("dir-a"), 10)),
            Path::new("dir-b"),
            10,
        ));
    }

    #[test]
    fn spread_companion_path_for_navigation_uses_same_branch_neighbor() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("wml2viewer-spread-{unique}"));
        let first = root.join("001.png");
        let second = root.join("002.png");
        fs::create_dir_all(&root).unwrap();
        fs::write(&first, []).unwrap();
        fs::write(&second, []).unwrap();

        let companion = spread_companion_path_for_navigation(
            &first,
            NavigationSortOption::Name,
            true,
        );

        assert_eq!(companion.as_deref(), Some(second.as_path()));

        let _ = fs::remove_dir_all(root);
    }
}
