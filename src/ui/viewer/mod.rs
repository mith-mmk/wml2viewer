use crate::benchlog::BenchLogger;
use crate::configs::config::save_app_config;
use crate::configs::resourses::{AppliedResources, apply_resources};
use crate::dependent::{default_download_dir, default_temp_dir, pick_save_directory};
use crate::drawers::canvas::Canvas;
use crate::drawers::image::{LoadedImage, SaveFormat, save_loaded_image};
use crate::filesystem::function::{FunctionParams, call_fanction_for_action};
use crate::filesystem::{
    FilesystemCommand, FilesystemResult, RecursiveOrder, adjacent_entry, archive_prefers_low_io,
    is_browser_container, navigation_branch_path, resolve_end_path, resolve_navigation_entry_path,
    resolve_start_path, set_archive_zip_workaround, spawn_filesystem_worker,
};
use crate::options::{
    AppConfig, EndOfFolderOption, FileActionOptions, FilesystemOptions, FolderRefreshMode,
    InputOptions, KeyBinding, NavigationSortOption, PluginConfig, ResourceOptions, RuntimeOptions,
    TransitionEffect, ViewerAction,
};
use crate::ui::i18n::{UiTextKey, tr};
use crate::ui::input::dispatch::canonical_key_binding_name;
use crate::ui::menu::fileviewer::CascadeMenuState;
use crate::ui::menu::fileviewer::state::{
    FilerEntry, FilerSortField, FilerState, FilerUserRequest, NameSortMode,
};
use crate::ui::menu::fileviewer::thumbnail::{
    ThumbnailCommand, ThumbnailResult, set_thumbnail_workaround, spawn_thumbnail_worker,
};
use crate::ui::menu::fileviewer::worker::{FilerCommand, FilerResult, spawn_filer_worker};
use crate::ui::render::{
    ActiveRenderRequest, LoadedRenderPage, PreparedTexture, RenderCommand, RenderLoadMetrics,
    RenderResult, aligned_offset, canvas_to_color_image, downscale_for_texture_limit,
    spawn_render_worker, worker_send_error,
};
use crate::ui::viewer::options::{
    RenderOptions, RenderScaleMode, ViewerOptions, WindowOptions, WindowStartPosition,
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
use std::time::{Duration, Instant, SystemTime};
mod dialogs;
mod navigation;
pub mod options;
mod state;
mod workers;
use options::ZoomOption;
pub(crate) use state::FileActionDialogMode;
pub(crate) use state::KeyMappingRowDraft;
pub(crate) use state::SettingsDraftState;
use state::{FileActionDialogState, OverlayDialogState, SaveDialogState, ViewerOverlayState};

const NAVIGATION_REPEAT_INTERVAL: Duration = Duration::from_millis(180);
const POINTER_SINGLE_CLICK_DELAY: Duration = Duration::from_millis(500);
const WAITING_CARD_DELAY: Duration = Duration::from_millis(180);
const STARTUP_LAYOUT_SETTLE_FRAMES: usize = 8;
const STARTUP_LAYOUT_REPAINT_INTERVAL: Duration = Duration::from_millis(16);
const AUTO_FOLDER_REFRESH_INTERVAL: Duration = Duration::from_millis(1000);
const PRELOAD_CACHE_CAPACITY: usize = 2;
const TRANSITION_FRAME_SAMPLE_CAPACITY: usize = 4096;
const ZIP_TO_ZIP_RANDOM_WALK_ROUNDS: usize = 8;
const RENDER_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const HELP_HTML_TEMPLATE: &str = include_str!("../../../resources/help.html");
const HELP_KEY_BINDINGS_ROWS_TOKEN: &str = "{{KEY_BINDINGS_ROWS}}";

pub(crate) struct ViewerApp {
    pub(crate) current_navigation_path: PathBuf,
    pub(crate) current_path: PathBuf,
    pub(crate) source: LoadedImage,
    pub(crate) rendered: LoadedImage,
    pub(crate) default_texture: TextureHandle,
    pub(crate) prev_texture: Option<TextureHandle>,
    pub(crate) current_texture: TextureHandle,
    pending_transition_previous: Option<TransitionPreviousState>,
    active_transition: Option<ImageTransitionState>,
    last_drawn_scene: Option<TransitionScene>,
    pub(crate) egui_ctx: egui::Context,
    system_visuals: egui::Visuals,

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
    pub(crate) filesystem_options: FilesystemOptions,
    pub(crate) storage: crate::options::StorageOptions,
    pub(crate) runtime: RuntimeOptions,
    pub(crate) file_action: FileActionOptions,
    pub(crate) applied_locale: String,
    pub(crate) loaded_font_names: Vec<String>,
    pub(crate) resource_locale_input: String,
    pub(crate) resource_font_paths_input: String,
    pub(crate) keymap: HashMap<KeyBinding, ViewerAction>,
    pub(crate) input_options: InputOptions,
    pub(crate) end_of_folder: EndOfFolderOption,
    pub(crate) navigation_sort: NavigationSortOption,
    pub(crate) recursive_order: RecursiveOrder,
    pub(crate) worker_tx: Sender<RenderCommand>,
    pub(crate) worker_rx: Receiver<RenderResult>,
    pub(crate) worker_join: Option<JoinHandle<()>>,
    pub(crate) next_request_id: u64,
    pub(crate) active_request: Option<ActiveRenderRequest>,
    active_request_started_at: Option<Instant>,
    pub(crate) pending_navigation_path: Option<PathBuf>,
    pending_viewer_navigation: Option<PendingViewerNavigation>,
    pub(crate) fs_tx: Option<Sender<FilesystemCommand>>,
    pub(crate) fs_rx: Option<Receiver<FilesystemResult>>,
    pub(crate) next_fs_request_id: u64,
    pub(crate) active_fs_request_id: Option<u64>,
    pub(crate) queued_filesystem_init_path: Option<PathBuf>,
    pub(crate) queued_navigation: Option<QueuedNavigation>,
    active_navigation_transition_direction: Option<ImageTransitionDirection>,
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
    pub(crate) cascade_menu: CascadeMenuState,
    pub(crate) suppress_next_pointer_intent: bool,
    pub(crate) left_menu_pos: Pos2,
    pub(crate) save_dialog: SaveDialogState,
    pub(crate) file_action_dialog: FileActionDialogState,
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
    manga_companion_lookup: Option<MangaCompanionLookup>,
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
    last_auto_refresh_at: Instant,
    pub(crate) last_auto_refresh_signature: Option<(PathBuf, u64)>,
    auto_refresh_tx: Sender<AutoRefreshResult>,
    auto_refresh_rx: Receiver<AutoRefreshResult>,
    auto_refresh_pending: Option<(u64, PathBuf)>,
    next_auto_refresh_id: u64,
    auto_refresh_dirty: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SettingsTab {
    Viewer,
    Input,
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
    Command(QueuedNavigation),
}

#[derive(Clone, Debug)]
pub(crate) struct QueuedNavigation {
    command: FilesystemCommand,
    transition_direction: Option<ImageTransitionDirection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageTransitionDirection {
    Forward,
    Backward,
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
struct TransitionPreviousState {
    scene: TransitionScene,
    direction: Option<ImageTransitionDirection>,
}

#[derive(Clone)]
struct TransitionImageLayer {
    texture: TextureHandle,
    rect: egui::Rect,
}

#[derive(Clone)]
struct TransitionSeparatorLayer {
    rect: egui::Rect,
    options: crate::ui::viewer::options::MangaSeparatorOptions,
}

#[derive(Clone)]
struct TransitionScene {
    viewport: egui::Rect,
    images: Vec<TransitionImageLayer>,
    separator: Option<TransitionSeparatorLayer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MangaCompanionLookupKey {
    navigation_path: PathBuf,
    sort: NavigationSortOption,
    direction: isize,
    branch_path: Option<PathBuf>,
    branch_modified: Option<SystemTime>,
    branch_len: Option<u64>,
}

struct MangaCompanionLookup {
    key: MangaCompanionLookupKey,
    path: Option<PathBuf>,
}

struct AutoRefreshResult {
    request_id: u64,
    directory: PathBuf,
    signature: Option<u64>,
}

struct ImageTransitionState {
    effect: TransitionEffect,
    prepared_at: Instant,
    started_at: Option<Instant>,
    duration: Duration,
    previous: TransitionPreviousState,
    frames: TransitionFrameRecorder,
    display_period: TransitionDisplayPeriod,
    cache_record_ms: Option<f64>,
}

#[derive(Clone, Copy)]
struct TransitionDisplayPeriod {
    period: Duration,
    source: &'static str,
    refresh_hz: Option<u32>,
    #[cfg(windows)]
    monitor_key: Option<isize>,
}

impl Default for TransitionDisplayPeriod {
    fn default() -> Self {
        Self {
            period: Duration::from_nanos(16_666_667),
            source: "assumed_60_hz",
            refresh_hz: None,
            #[cfg(windows)]
            monitor_key: None,
        }
    }
}

#[derive(Clone, Copy)]
struct TransitionFrameSample {
    gap: Duration,
    expected_frame_period: Duration,
}

struct TransitionFrameRecorder {
    last_frame_nr: Option<u64>,
    last_draw_at: Option<Instant>,
    draw_count: u32,
    samples: Vec<TransitionFrameSample>,
    total_gap_count: u64,
    gaps_over_two_frame_periods: u64,
    max_gap: Option<Duration>,
    reservoir_state: u64,
}

impl Default for TransitionFrameRecorder {
    fn default() -> Self {
        Self {
            last_frame_nr: None,
            last_draw_at: None,
            draw_count: 0,
            samples: Vec::new(),
            total_gap_count: 0,
            gaps_over_two_frame_periods: 0,
            max_gap: None,
            reservoir_state: 0x5eed_cafe_d15c_a11e,
        }
    }
}

impl TransitionFrameRecorder {
    fn record(&mut self, frame_nr: u64, now: Instant, expected_frame_period: Duration) {
        if self.last_frame_nr == Some(frame_nr) {
            return;
        }
        if let Some(last_draw_at) = self.last_draw_at {
            let sample = TransitionFrameSample {
                gap: now.duration_since(last_draw_at),
                expected_frame_period,
            };
            self.total_gap_count += 1;
            self.max_gap = Some(self.max_gap.unwrap_or(Duration::ZERO).max(sample.gap));
            if sample.gap > expected_frame_period.saturating_mul(2) {
                self.gaps_over_two_frame_periods += 1;
            }
            if self.samples.len() < TRANSITION_FRAME_SAMPLE_CAPACITY {
                self.samples.push(sample);
            } else {
                self.reservoir_state = self
                    .reservoir_state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                let index = self.reservoir_state % self.total_gap_count;
                if index < TRANSITION_FRAME_SAMPLE_CAPACITY as u64 {
                    self.samples[index as usize] = sample;
                }
            }
        }
        self.last_frame_nr = Some(frame_nr);
        self.last_draw_at = Some(now);
        self.draw_count += 1;
    }
}

struct TransitionFrameStats {
    p50_gap: Option<Duration>,
    p95_gap: Option<Duration>,
    p99_gap: Option<Duration>,
    max_gap: Option<Duration>,
    gaps_over_two_frame_periods: usize,
}

fn transition_frame_stats(samples: &[TransitionFrameSample]) -> TransitionFrameStats {
    let mut gaps = samples.iter().map(|sample| sample.gap).collect::<Vec<_>>();
    gaps.sort_unstable();
    let percentile = |percent: usize| {
        let rank = (gaps.len() * percent).div_ceil(100);
        rank.checked_sub(1)
            .and_then(|index| gaps.get(index).copied())
    };
    TransitionFrameStats {
        p50_gap: percentile(50),
        p95_gap: percentile(95),
        p99_gap: percentile(99),
        max_gap: gaps.last().copied(),
        gaps_over_two_frame_periods: samples
            .iter()
            .filter(|sample| sample.gap > sample.expected_frame_period.saturating_mul(2))
            .count(),
    }
}

#[cfg(windows)]
fn transition_display_period_for_window(
    frame: &eframe::Frame,
    previous: TransitionDisplayPeriod,
) -> TransitionDisplayPeriod {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Gdi::{
        DEVMODEW, DM_DISPLAYFREQUENCY, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW,
        GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFOEXW, MonitorFromWindow,
    };

    let Ok(handle) = frame.window_handle() else {
        return previous;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return previous;
    };
    // The borrowed window handle remains valid for this call on the UI thread.
    let monitor = unsafe {
        MonitorFromWindow(
            handle.hwnd.get() as windows_sys::Win32::Foundation::HWND,
            MONITOR_DEFAULTTONEAREST,
        )
    };
    if monitor.is_null() || previous.monitor_key == Some(monitor as isize) {
        return previous;
    }

    let mut result = TransitionDisplayPeriod {
        monitor_key: Some(monitor as isize),
        ..TransitionDisplayPeriod::default()
    };
    // MONITORINFOEXW starts with MONITORINFO. cbSize requests the device name too.
    let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) } == 0 {
        return result;
    }
    let mut mode: DEVMODEW = unsafe { std::mem::zeroed() };
    mode.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
    if unsafe { EnumDisplaySettingsW(info.szDevice.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode) }
        != 0
        && mode.dmFields & DM_DISPLAYFREQUENCY != 0
        && mode.dmDisplayFrequency > 1
    {
        result.period = Duration::from_secs_f64(1.0 / f64::from(mode.dmDisplayFrequency));
        result.source = "windows_current_display_mode";
        result.refresh_hz = Some(mode.dmDisplayFrequency);
    }
    result
}

#[derive(Clone)]
struct DisplayedPageState {
    source: LoadedImage,
    rendered: LoadedImage,
    texture: Option<TextureHandle>,
    texture_display_scale: f32,
    prepared_texture: Option<PreparedTexture>,
}

#[derive(Clone)]
struct PreloadedEntry {
    navigation_path: PathBuf,
    load_path: Option<PathBuf>,
    zoom: f32,
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

fn queue_navigation_command(
    slot: &mut Option<QueuedNavigation>,
    command: FilesystemCommand,
    transition_direction: Option<ImageTransitionDirection>,
) {
    *slot = Some(QueuedNavigation {
        command,
        transition_direction,
    });
}

fn take_next_queued_filesystem_work(
    queued_filesystem_init_path: &mut Option<PathBuf>,
    queued_navigation: &mut Option<QueuedNavigation>,
) -> Option<PendingFilesystemWork> {
    if let Some(path) = queued_filesystem_init_path.take() {
        Some(PendingFilesystemWork::Init(path))
    } else {
        queued_navigation.take().map(PendingFilesystemWork::Command)
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

fn startup_layout_is_settling(
    frame_counter: usize,
    current: egui::Vec2,
    previous: egui::Vec2,
) -> bool {
    frame_counter < STARTUP_LAYOUT_SETTLE_FRAMES && viewport_size_changed(current, previous)
}

fn should_recalculate_fit_layout(
    empty_mode: bool,
    current: egui::Vec2,
    previous: egui::Vec2,
    pending_fit_recalc: bool,
    zoom_option: &ZoomOption,
) -> bool {
    !empty_mode
        && !matches!(zoom_option, ZoomOption::None)
        && (viewport_size_changed(current, previous) || pending_fit_recalc)
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

fn waiting_card_should_show(active_render_request: bool, active_filesystem_request: bool) -> bool {
    active_render_request || active_filesystem_request
}

fn should_defer_filer_scan(show_filer: bool, show_subfiler: bool) -> bool {
    !show_filer && !show_subfiler
}

fn resolved_navigation_path_for_load(
    pending_navigation_path: PathBuf,
    loaded_path: Option<&Path>,
) -> PathBuf {
    if pending_navigation_path.is_dir() {
        return loaded_path
            .map(Path::to_path_buf)
            .unwrap_or(pending_navigation_path);
    }
    if loaded_path.is_some() && is_browser_container(&pending_navigation_path) {
        return resolve_navigation_entry_path(&pending_navigation_path)
            .or_else(|| loaded_path.map(Path::to_path_buf))
            .unwrap_or(pending_navigation_path);
    }
    pending_navigation_path
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

pub(crate) fn format_key_binding(binding: &KeyBinding) -> String {
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

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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

fn optional_path_to_string(path: Option<&PathBuf>) -> String {
    path.map(|value| value.display().to_string())
        .unwrap_or_default()
}

pub(crate) fn build_settings_draft(config: &AppConfig) -> SettingsDraftState {
    let effective_keymap = config.input.merged_with_defaults();
    SettingsDraftState {
        config: config.clone(),
        resource_locale_input: locale_input_from_config(config),
        resource_font_paths_input: join_search_paths(&config.resources.font_paths),
        susie64_search_paths_input: join_search_paths(&config.plugins.susie64.search_path),
        ffmpeg_search_paths_input: join_search_paths(&config.plugins.ffmpeg.search_path),
        move_folder1_input: optional_path_to_string(config.file_action.move_folder1.as_ref()),
        move_folder2_input: optional_path_to_string(config.file_action.move_folder2.as_ref()),
        copy_folder1_input: optional_path_to_string(config.file_action.copy_folder1.as_ref()),
        copy_folder2_input: optional_path_to_string(config.file_action.copy_folder2.as_ref()),
        key_mapping_rows: key_mapping_rows_from_map(&effective_keymap),
        key_mapping_error: None,
    }
}

pub(crate) fn key_mapping_rows_from_map(
    keymap: &HashMap<KeyBinding, ViewerAction>,
) -> Vec<KeyMappingRowDraft> {
    let mut rows = keymap
        .iter()
        .map(|(binding, action)| KeyMappingRowDraft {
            binding: canonical_key_binding(binding),
            action: *action,
        })
        .collect::<Vec<_>>();
    rows.sort_by(|lhs, rhs| {
        lhs.action
            .name()
            .cmp(rhs.action.name())
            .then(lhs.binding.key.cmp(&rhs.binding.key))
            .then(lhs.binding.ctrl.cmp(&rhs.binding.ctrl))
            .then(lhs.binding.shift.cmp(&rhs.binding.shift))
            .then(lhs.binding.alt.cmp(&rhs.binding.alt))
    });
    rows
}

fn canonical_key_binding(binding: &KeyBinding) -> KeyBinding {
    KeyBinding {
        key: canonical_key_binding_name(&binding.key),
        ctrl: binding.ctrl,
        alt: binding.alt,
        shift: binding.shift,
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
        let (auto_refresh_tx, auto_refresh_rx) = mpsc::channel();
        let resource_locale_input = config.resources.locale.clone().unwrap_or_default();
        let resource_font_paths_input = join_search_paths(&config.resources.font_paths);
        let defer_navigation_workers = !show_filer_on_start;
        let startup_phase = if defer_navigation_workers {
            StartupPhase::SingleViewer
        } else {
            StartupPhase::MultiViewer
        };
        let (bench_scenario_name, bench_actions) = bench_automation_plan(bench_scenario.as_deref());

        let input_options = config.input.clone();
        let keymap = input_options.merged_with_defaults();

        let mut this = Self {
            current_navigation_path: navigation_path.clone(),
            current_path: path.clone(),
            source,
            rendered,
            default_texture: default_texture.clone(),
            prev_texture: None,
            current_texture: default_texture.clone(),
            pending_transition_previous: None,
            active_transition: None,
            last_drawn_scene: None,
            egui_ctx: cc.egui_ctx.clone(),
            system_visuals: cc.egui_ctx.style().visuals.clone(),

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
            filesystem_options: config.filesystem,
            storage: config.storage,
            runtime: config.runtime,
            file_action: config.file_action,
            applied_locale: locale,
            loaded_font_names: loaded_fonts,
            resource_locale_input,
            resource_font_paths_input,
            keymap,
            input_options,
            end_of_folder: config.navigation.end_of_folder,
            navigation_sort: config.navigation.sort,
            recursive_order: RecursiveOrder::default(),
            worker_tx,
            worker_rx,
            worker_join: Some(worker_join),
            next_request_id: 0,
            active_request: None,
            active_request_started_at: None,
            pending_navigation_path: None,
            pending_viewer_navigation: None,
            fs_tx: None,
            fs_rx: None,
            next_fs_request_id: 0,
            active_fs_request_id: None,
            queued_filesystem_init_path: None,
            queued_navigation: None,
            active_navigation_transition_direction: None,
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
            cascade_menu: CascadeMenuState::default(),
            suppress_next_pointer_intent: false,
            left_menu_pos: Pos2::ZERO,
            save_dialog: SaveDialogState {
                file_name: default_save_file_name(&path),
                ..SaveDialogState::default()
            },
            file_action_dialog: FileActionDialogState::default(),
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
            manga_companion_lookup: None,
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
            last_auto_refresh_at: Instant::now(),
            last_auto_refresh_signature: None,
            auto_refresh_tx,
            auto_refresh_rx,
            auto_refresh_pending: None,
            next_auto_refresh_id: 0,
            auto_refresh_dirty: None,
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

    fn paint_manga_separator(
        &self,
        ui: &mut egui::Ui,
        height: f32,
    ) -> Option<TransitionSeparatorLayer> {
        let width = self.options.manga_separator.pixels.max(0.0);
        if width <= 0.0 {
            return None;
        }

        let (rect, _) = ui.allocate_exact_size(vec2(width, height.max(1.0)), egui::Sense::hover());
        let layer = TransitionSeparatorLayer {
            rect,
            options: self.options.manga_separator.clone(),
        };
        if self.active_transition.is_none() {
            paint_transition_separator(ui.painter(), &layer, egui::Vec2::ZERO, 1.0);
        }
        Some(layer)
    }

    fn add_transition_image_widget(
        &self,
        ui: &mut egui::Ui,
        texture: &TextureHandle,
        size: egui::Vec2,
    ) -> egui::Response {
        if self.active_transition.is_some() {
            ui.allocate_exact_size(size, egui::Sense::click_and_drag())
                .1
        } else {
            ui.add(
                egui::Image::from_texture(texture)
                    .fit_to_exact_size(size)
                    .sense(egui::Sense::click_and_drag()),
            )
        }
    }

    pub(crate) fn text(&self, key: UiTextKey) -> &'static str {
        tr(&self.applied_locale, key)
    }

    pub(crate) fn apply_window_theme(&self, ctx: &egui::Context) {
        crate::ui::theme::apply_window_theme(
            ctx,
            self.window_options.ui_theme,
            &self.system_visuals,
        );
    }

    pub(crate) fn open_help(&self) {
        let mut bindings = self
            .keymap
            .iter()
            .map(|(binding, action)| (format_key_binding(binding), format!("{action:?}")))
            .collect::<Vec<_>>();
        bindings.sort_by(|left, right| left.0.cmp(&right.0));

        let rows = bindings
            .into_iter()
            .map(|(binding, action)| {
                format!(
                    "<tr><td>{}</td><td>{}</td></tr>",
                    escape_html(&binding),
                    escape_html(&action)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let html = HELP_HTML_TEMPLATE.replace(HELP_KEY_BINDINGS_ROWS_TOKEN, &rows);
        let temp_root = default_temp_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("wml2viewer");
        let _ = std::fs::create_dir_all(&temp_root);
        let path = temp_root.join(format!(
            "help-{}.html",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
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

    fn poll_auto_folder_refresh(&mut self) {
        let auto_enabled = matches!(
            self.filesystem_options.folder_refresh,
            FolderRefreshMode::Auto
        );
        let filer_visible = self.show_filer || self.show_subfiler;
        let observed_directory = if filer_visible {
            self.filer
                .directory
                .clone()
                .or_else(|| self.current_directory())
        } else {
            self.current_directory()
        };
        if self
            .auto_refresh_pending
            .as_ref()
            .is_some_and(|(_, directory)| observed_directory.as_ref() != Some(directory))
        {
            self.auto_refresh_pending = None;
            self.last_auto_refresh_signature = None;
            self.last_auto_refresh_at = Instant::now() - AUTO_FOLDER_REFRESH_INTERVAL;
        }
        while let Ok(result) = self.auto_refresh_rx.try_recv() {
            if self.auto_refresh_pending.as_ref()
                != Some(&(result.request_id, result.directory.clone()))
            {
                continue;
            }
            self.auto_refresh_pending = None;
            if !auto_enabled || observed_directory.as_ref() != Some(&result.directory) {
                continue;
            }
            let Some(signature) = result.signature else {
                self.last_auto_refresh_signature = None;
                continue;
            };
            let previous = self
                .last_auto_refresh_signature
                .replace((result.directory.clone(), signature));
            if previous
                .as_ref()
                .is_none_or(|(previous_dir, _)| previous_dir != &result.directory)
            {
                self.manga_companion_lookup = None;
            }
            let changed = previous
                .as_ref()
                .is_some_and(|(previous_dir, previous_signature)| {
                    previous_dir == &result.directory && *previous_signature != signature
                });
            if !changed {
                continue;
            }
            self.manga_companion_lookup = None;
            self.log_bench_state(
                "viewer.filer.auto_refresh_detected",
                serde_json::json!({"directory": result.directory.display().to_string()}),
            );
            if filer_visible {
                if self.filer.pending_request_id.is_some() {
                    self.auto_refresh_dirty = Some(result.directory);
                } else {
                    self.refresh_current_filer_directory();
                }
            } else {
                self.filer.entries.clear();
                self.last_filer_snapshot_signature = None;
            }
        }
        if !auto_enabled {
            self.last_auto_refresh_signature = None;
            self.auto_refresh_dirty = None;
            self.auto_refresh_pending = None;
            return;
        }
        if let Some(dirty_dir) = self.auto_refresh_dirty.take() {
            if filer_visible && observed_directory.as_ref() == Some(&dirty_dir) {
                if self.filer.pending_request_id.is_some() {
                    self.auto_refresh_dirty = Some(dirty_dir);
                } else {
                    self.refresh_current_filer_directory();
                }
            }
        }
        self.egui_ctx
            .request_repaint_after(AUTO_FOLDER_REFRESH_INTERVAL);
        if (filer_visible && self.filer.pending_request_id.is_some())
            || self.auto_refresh_pending.is_some()
            || self.last_auto_refresh_at.elapsed() < AUTO_FOLDER_REFRESH_INTERVAL
        {
            return;
        }
        self.last_auto_refresh_at = Instant::now();
        let Some(dir) = observed_directory else {
            self.last_auto_refresh_signature = None;
            return;
        };
        self.next_auto_refresh_id = self.next_auto_refresh_id.wrapping_add(1).max(1);
        let request_id = self.next_auto_refresh_id;
        self.auto_refresh_pending = Some((request_id, dir.clone()));
        let tx = self.auto_refresh_tx.clone();
        let ctx = self.egui_ctx.clone();
        std::thread::spawn(move || {
            let signature = folder_refresh_signature(&dir);
            let _ = tx.send(AutoRefreshResult {
                request_id,
                directory: dir,
                signature,
            });
            ctx.request_repaint();
        });
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
        // FastGpu changes only the GPU draw size. The source texture is unchanged.
        if matches!(self.render_options.scale_mode, RenderScaleMode::PreciseCpu) {
            self.request_resize_current()?;
        }
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

    fn transition_effect_enabled(&self) -> bool {
        self.options.animation
            && !matches!(self.options.transition.effect, TransitionEffect::None)
            && !self.current_texture_is_default
    }

    pub(super) fn cancel_image_transition(&mut self, reason: &'static str) {
        let Some(state) = self.active_transition.take() else {
            return;
        };
        self.log_bench_state(
            "viewer.transition.cancelled",
            serde_json::json!({
                "effect": format!("{:?}", state.effect),
                "reason": reason,
                "elapsed_ms": state.started_at.map(|started| started.elapsed().as_secs_f64() * 1000.0),
                "duration_ms": state.duration.as_millis(),
                "draw_count": state.frames.draw_count,
            }),
        );
    }

    fn prepare_image_transition(
        &mut self,
        switching_image: bool,
        branch_changed: bool,
        direction: Option<ImageTransitionDirection>,
    ) {
        self.pending_transition_previous = None;
        self.cancel_image_transition("new_image_request");
        if !switching_image || branch_changed || !self.transition_effect_enabled() {
            return;
        }
        let Some(scene) = self.last_drawn_scene.clone() else {
            return;
        };
        if !scene
            .images
            .iter()
            .any(|image| image.texture.id() == self.current_texture.id())
        {
            return;
        }
        self.pending_transition_previous = Some(TransitionPreviousState { scene, direction });
    }

    fn start_image_transition(&mut self) {
        let Some(previous) = self.pending_transition_previous.take() else {
            return;
        };
        if matches!(self.options.transition.effect, TransitionEffect::None)
            || self.current_texture_is_default
        {
            return;
        }
        let effect =
            transition_effect_for_direction(self.options.transition.effect, previous.direction);
        self.active_transition = Some(ImageTransitionState {
            effect,
            prepared_at: Instant::now(),
            started_at: None,
            duration: Duration::from_millis(self.options.transition.duration_ms.max(1)),
            previous,
            frames: TransitionFrameRecorder::default(),
            display_period: TransitionDisplayPeriod::default(),
            cache_record_ms: None,
        });
    }

    fn draw_transition_scene(&mut self, ui: &mut egui::Ui, current: &TransitionScene) {
        let Some(state) = self.active_transition.as_mut() else {
            return;
        };
        if state.previous.scene.viewport != current.viewport {
            self.cancel_image_transition("viewport_changed");
            paint_transition_scene(ui.painter(), current, egui::Vec2::ZERO, 1.0);
            return;
        }
        let now = Instant::now();
        state.frames.record(
            ui.ctx().cumulative_frame_nr(),
            now,
            state.display_period.period,
        );
        let progress = transition_progress(&mut state.started_at, now, state.duration);
        if progress >= 1.0 {
            let state = self.active_transition.take().expect("transition is active");
            let stats = transition_frame_stats(&state.frames.samples);
            let gap_ms = |gap: Option<Duration>| gap.map(|gap| gap.as_secs_f64() * 1000.0);
            let sampled_all_gaps =
                state.frames.total_gap_count == state.frames.samples.len() as u64;
            let (max_gap, gaps_over_two_frame_periods) = if sampled_all_gaps {
                (stats.max_gap, stats.gaps_over_two_frame_periods as u64)
            } else {
                (
                    state.frames.max_gap,
                    state.frames.gaps_over_two_frame_periods,
                )
            };
            self.log_bench_state(
                "viewer.transition.completed",
                serde_json::json!({
                    "effect": format!("{:?}", state.effect),
                    "duration_ms": state.duration.as_millis(),
                    "actual_elapsed_ms": state.started_at.map(|started| now.duration_since(started).as_secs_f64() * 1000.0),
                    "prepare_to_first_draw_ms": state.started_at.map(|started| started.duration_since(state.prepared_at).as_secs_f64() * 1000.0),
                    "cache_record_ms": state.cache_record_ms,
                    "draw_count": state.frames.draw_count,
                    "frame_gap_sample_count": state.frames.samples.len(),
                    "frame_gap_total_count": state.frames.total_gap_count,
                    "frame_gap_p50_ms": gap_ms(stats.p50_gap),
                    "frame_gap_p95_ms": gap_ms(stats.p95_gap),
                    "frame_gap_p99_ms": gap_ms(stats.p99_gap),
                    "max_draw_gap_ms": gap_ms(max_gap).unwrap_or(0.0),
                    "gaps_over_two_frame_periods": gaps_over_two_frame_periods,
                    "frame_period_ms": state.display_period.period.as_secs_f64() * 1000.0,
                    "frame_period_source": state.display_period.source,
                    "refresh_hz": state.display_period.refresh_hz,
                    "percentile_method": if sampled_all_gaps {
                        "nearest_rank"
                    } else {
                        "nearest_rank_reservoir"
                    },
                }),
            );
            self.schedule_preload();
            if self
                .preload_cache
                .iter()
                .any(|entry| entry.display.texture.is_none())
            {
                ui.ctx().request_repaint();
            }
            paint_transition_scene(ui.painter(), current, egui::Vec2::ZERO, 1.0);
            return;
        }

        let viewport = current.viewport;
        let previous = &state.previous.scene;
        match state.effect {
            TransitionEffect::None => {
                paint_transition_scene(ui.painter(), current, egui::Vec2::ZERO, 1.0);
            }
            TransitionEffect::Fade => {
                paint_transition_scene(ui.painter(), previous, egui::Vec2::ZERO, 1.0 - progress);
                paint_transition_scene(ui.painter(), current, egui::Vec2::ZERO, progress);
            }
            TransitionEffect::SlideRightToLeft
            | TransitionEffect::SlideLeftToRight
            | TransitionEffect::SlideTopToBottom
            | TransitionEffect::SlideBottomToTop => {
                let offset = slide_transition_offset(state.effect, viewport.size(), progress);
                let old_clip = slide_layer_clip(viewport, offset.previous);
                let new_clip = slide_layer_clip(viewport, offset.current);
                paint_transition_scene(
                    &ui.painter().with_clip_rect(old_clip),
                    previous,
                    offset.previous,
                    1.0,
                );
                paint_transition_scene(
                    &ui.painter().with_clip_rect(new_clip),
                    current,
                    offset.current,
                    1.0,
                );
            }
            TransitionEffect::SpiralWipeIn | TransitionEffect::SpiralWipeOut => {
                paint_transition_scene(ui.painter(), previous, egui::Vec2::ZERO, 1.0);
                let reveal = spiral_reveal_rect(viewport, progress);
                paint_transition_scene(
                    &ui.painter().with_clip_rect(reveal),
                    current,
                    egui::Vec2::ZERO,
                    1.0,
                );
                if matches!(state.effect, TransitionEffect::SpiralWipeOut) {
                    paint_transition_scene(
                        ui.painter(),
                        previous,
                        egui::Vec2::ZERO,
                        1.0 - progress,
                    );
                }
            }
        }
        ui.ctx().request_repaint();
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
        let started_at = Instant::now();
        let (canvas, display_scale) = downscale_for_texture_limit(
            canvas,
            self.max_texture_side,
            self.render_options.zoom_method,
        );
        let downscale_done_at = Instant::now();
        let image = self.color_image_from_canvas(&canvas);
        let conversion_done_at = Instant::now();
        let texture =
            self.egui_ctx
                .load_texture(texture_name.to_owned(), image, self.texture_options());
        self.log_bench_state(
            "viewer.texture.build",
            serde_json::json!({
                "downscale_ms": downscale_done_at.duration_since(started_at).as_secs_f64() * 1000.0,
                "color_conversion_ms": conversion_done_at.duration_since(downscale_done_at).as_secs_f64() * 1000.0,
                "texture_registration_ms": conversion_done_at.elapsed().as_secs_f64() * 1000.0,
                "dimensions": [canvas.width(), canvas.height()],
            }),
        );
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

    fn texture_from_prepared(
        &self,
        texture_name: &str,
        mut prepared: PreparedTexture,
    ) -> Option<(TextureHandle, f32)> {
        if prepared.max_texture_side != self.max_texture_side
            || prepared.method != self.render_options.zoom_method
        {
            return None;
        }
        if self.options.grayscale {
            Self::apply_grayscale_to_color_image(std::sync::Arc::make_mut(&mut prepared.image));
        }
        let started = Instant::now();
        let texture = self.egui_ctx.load_texture(
            texture_name,
            egui::ImageData::Color(prepared.image),
            self.texture_options(),
        );
        self.log_bench_state(
            "viewer.texture.prepared",
            serde_json::json!({
                "worker_downscale_ms": prepared.downscale_ms,
                "worker_color_conversion_ms": prepared.color_conversion_ms,
                "ui_registration_ms": started.elapsed().as_secs_f64() * 1000.0,
            }),
        );
        Some((texture, prepared.display_scale))
    }

    fn register_prepared_texture(&mut self, prepared: PreparedTexture) -> bool {
        if self.current_frame != 0 {
            return false;
        }
        let texture_name = self.texture_name_for_path(Some(&self.current_path));
        let Some((texture, display_scale)) = self.texture_from_prepared(&texture_name, prepared)
        else {
            return false;
        };
        self.current_texture = texture;
        self.texture_display_scale = display_scale;
        self.current_texture_is_default = false;
        true
    }

    fn show_loading_texture(&mut self, reset_branch_cache: bool) {
        if !self.current_texture_is_default {
            self.prev_texture = Some(self.current_texture.clone());
        }
        if reset_branch_cache {
            self.prev_texture = None;
        }
        self.cancel_image_transition("loading_placeholder");
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
        self.pending_transition_previous = None;
        self.cancel_image_transition("clear_display");
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
        if self.current_navigation_path.is_dir() {
            return Some(self.current_navigation_path.clone());
        }
        if let Some(parent) = self.current_navigation_path.parent() {
            let marker = parent.file_name().and_then(|name| name.to_str());
            if matches!(marker, Some("__wmlv__" | "__zipv__" | "__lhav__")) {
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
        if self.filer.directory.as_ref() != Some(&dir) {
            self.filer.entries.clear();
        }
        self.filer.directory = Some(dir.clone());
        self.filer.selected = selected.clone();
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

    pub(crate) fn cancel_filer_scan(&mut self) {
        if should_clear_filer_request_on_hide(self.filer.pending_user_request.as_ref()) {
            self.filer.pending_user_request = None;
        }
        if self.filer.pending_request_id.take().is_none() {
            return;
        }
        self.filer.entries.clear();
        self.last_filer_snapshot_signature = None;
        let request_id = self.alloc_filer_request_id();
        if let Some(filer_tx) = &self.filer_tx {
            let _ = filer_tx.send(FilerCommand::Cancel { request_id });
        }
    }

    pub(crate) fn browse_filer_directory(&mut self, dir: PathBuf) {
        self.filer.pending_user_request = Some(FilerUserRequest::BrowseDirectory {
            directory: dir.clone(),
        });
        self.filer.committed_browse_directory = None;
        self.request_filer_directory(dir, None);
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
            Some(FilerUserRequest::Refresh {
                directory: refresh_dir,
                selected,
            }) if refresh_dir == directory => {
                return selected.clone();
            }
            Some(FilerUserRequest::BrowseDirectory {
                directory: browse_dir,
            }) if browse_dir == directory => {
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

    fn sync_filer_selected_with_current_when_aligned(&mut self) {
        if !should_sync_filer_selected_with_current(
            self.filer.pending_user_request.as_ref(),
            self.filer.directory.as_deref(),
            self.current_directory().as_deref(),
        ) {
            return;
        }
        if let Some(dir) = self.filer.directory.as_deref() {
            let next_selected = self
                .selected_path_for_filer_directory(dir, Some(self.current_navigation_path.clone()));
            if self.filer.selected != next_selected {
                self.filer.selected = next_selected.clone();
                if self.show_filer {
                    self.pending_filer_focus_path = next_selected;
                }
            }
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
        if let Some(committed_browse_directory) = self
            .filer
            .committed_browse_directory
            .as_ref()
            .filter(|browse_dir| *browse_dir != &dir)
            .cloned()
        {
            let filer_already_aligned = should_clear_stale_committed_browse_when_filer_aligned(
                self.filer.directory.as_deref(),
                &dir,
                self.filer.pending_user_request.as_ref(),
            );
            if filer_already_aligned
                || should_clear_stale_committed_browse_for_viewer_navigation(
                    self.show_filer,
                    self.filer.pending_user_request.as_ref(),
                )
            {
                self.log_bench_state(
                    "viewer.filer.sync_with_current_path.cleared_stale_committed_browse",
                    serde_json::json!({
                        "directory": dir.display().to_string(),
                        "committed_browse_directory": committed_browse_directory.display().to_string(),
                        "filer_already_aligned": filer_already_aligned,
                    }),
                );
                self.filer.committed_browse_directory = None;
            } else {
                self.log_bench_state(
                    "viewer.filer.sync_with_current_path.skipped_committed_browse",
                    serde_json::json!({
                        "directory": dir.display().to_string(),
                        "committed_browse_directory": committed_browse_directory.display().to_string(),
                    }),
                );
                return;
            }
        }
        if should_defer_filer_scan(self.show_filer, self.show_subfiler) {
            if self.filer.directory.as_ref() != Some(&dir) {
                self.filer.entries.clear();
                self.last_filer_snapshot_signature = None;
            }
            self.filer.directory = Some(dir);
            self.filer.selected = selected;
            self.pending_filer_focus_path = None;
            self.cancel_filer_scan();
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
        self.auto_refresh_dirty = None;
        self.manga_companion_lookup = None;
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

    pub(crate) fn accept_filer_selection(&mut self, navigation_path: &Path) {
        self.active_fs_request_id = None;
        self.active_navigation_transition_direction = None;
        self.queued_filesystem_init_path = None;
        self.queued_navigation = None;
        self.pending_viewer_navigation = None;
        self.cancel_filer_scan();
        if self.navigator_ready {
            self.set_filesystem_current(navigation_path.to_path_buf());
        } else {
            let _ = self.init_filesystem(navigation_path.to_path_buf());
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

    pub(crate) fn open_dialog(&mut self, title: String, message: String) {
        self.overlay.dialog = Some(OverlayDialogState { title, message });
    }

    pub(crate) fn open_dialog_with_title_key(&mut self, title: UiTextKey, message: String) {
        self.open_dialog(self.text(title).to_string(), message);
    }

    fn is_current_portrait_page(&self) -> bool {
        self.source.canvas.height() >= self.source.canvas.width()
    }

    fn desired_manga_companion_path(&mut self) -> Option<PathBuf> {
        if !self.options.manga_mode
            || self.empty_mode
            || !self.navigator_ready
            || !self.is_current_portrait_page()
        {
            return None;
        }
        let navigation_path = self.current_navigation_path.clone();
        self.desired_manga_companion_path_for_navigation(&navigation_path)
    }

    fn desired_manga_companion_path_for_navigation(
        &mut self,
        navigation_path: &Path,
    ) -> Option<PathBuf> {
        if !self.options.manga_mode {
            return None;
        }
        let branch_path = navigation_branch_path(navigation_path);
        let branch_metadata = branch_path
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok());
        let sort = self.navigation_sort;
        let direction = self.navigation_direction_sign();
        let key = MangaCompanionLookupKey {
            navigation_path: navigation_path.to_path_buf(),
            sort,
            direction,
            branch_path,
            branch_modified: branch_metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok()),
            branch_len: branch_metadata.as_ref().map(|metadata| metadata.len()),
        };
        cached_spread_companion_path(&mut self.manga_companion_lookup, key, || {
            spread_companion_path_for_navigation(navigation_path, sort, direction, true)
        })
    }

    fn clear_manga_companion(&mut self) {
        self.companion_navigation_path = None;
        self.companion_display = None;
        self.companion_active_request = None;
    }

    fn visible_companion_source(&self) -> Option<&LoadedImage> {
        self.companion_navigation_path.as_ref().and(
            self.companion_display
                .as_ref()
                .map(|display| &display.source),
        )
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
            self.apply_companion_loaded(entry.load_path, entry.display);
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
                max_texture_side: self.max_texture_side,
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
                source: self.companion_display.as_ref().unwrap().source.clone(),
                zoom: self.zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
                max_texture_side: self.max_texture_side,
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
        if !self.navigator_ready
            || !self.manga_spread_active()
            || self.end_of_folder == EndOfFolderOption::Recursive
        {
            return None;
        }
        let direction = self.navigation_direction_sign();

        let boundary_target = adjacent_entry(
            &self.current_navigation_path,
            self.navigation_sort,
            if forward { direction } else { -direction },
        )?;
        let current_branch = navigation_branch_path(&self.current_navigation_path);
        let boundary_branch = navigation_branch_path(&boundary_target);
        if current_branch != boundary_branch {
            return Some(boundary_target);
        }

        let step = if forward {
            2 * direction
        } else {
            -2 * direction
        };
        adjacent_entry(&self.current_navigation_path, self.navigation_sort, step)
    }

    fn navigation_direction_sign(&self) -> isize {
        if self.filer.ascending { 1 } else { -1 }
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
                    self.request_load_path_with_transition_direction(
                        target,
                        Some(ImageTransitionDirection::Forward),
                    )
                } else {
                    let command = if self.filer.ascending
                        || self.end_of_folder == EndOfFolderOption::Recursive
                    {
                        FilesystemCommand::Next {
                            request_id: 0,
                            policy: self.end_of_folder,
                        }
                    } else {
                        FilesystemCommand::Prev {
                            request_id: 0,
                            policy: self.end_of_folder,
                        }
                    };
                    self.request_navigation(command, Some(ImageTransitionDirection::Forward))
                }
            }
            PendingViewerNavigation::Prev => {
                if let Some(target) = self.manga_navigation_target(false) {
                    self.request_load_path_with_transition_direction(
                        target,
                        Some(ImageTransitionDirection::Backward),
                    )
                } else {
                    let command = if self.filer.ascending
                        || self.end_of_folder == EndOfFolderOption::Recursive
                    {
                        FilesystemCommand::Prev {
                            request_id: 0,
                            policy: self.end_of_folder,
                        }
                    } else {
                        FilesystemCommand::Next {
                            request_id: 0,
                            policy: self.end_of_folder,
                        }
                    };
                    self.request_navigation(command, Some(ImageTransitionDirection::Backward))
                }
            }
            PendingViewerNavigation::First => {
                if self.should_apply_edge_noop(PendingViewerNavigation::First)
                    && self.navigation_edge_reached(PendingViewerNavigation::First)
                {
                    return;
                }
                if let Some((target, is_container)) =
                    self.filer_edge_navigation_target(PendingViewerNavigation::First)
                {
                    if should_skip_edge_navigation_for_same_target(
                        &self.current_navigation_path,
                        &target,
                        PendingViewerNavigation::First,
                    ) {
                        Ok(())
                    } else {
                        self.request_filer_edge_target_navigation(
                            target,
                            is_container,
                            PendingViewerNavigation::First,
                        )
                    }
                } else {
                    let command = if self.filer.ascending {
                        FilesystemCommand::First { request_id: 0 }
                    } else {
                        FilesystemCommand::Last { request_id: 0 }
                    };
                    self.request_navigation(command, None)
                }
            }
            PendingViewerNavigation::Last => {
                if self.should_apply_edge_noop(PendingViewerNavigation::Last)
                    && self.navigation_edge_reached(PendingViewerNavigation::Last)
                {
                    return;
                }
                if let Some((target, is_container)) =
                    self.filer_edge_navigation_target(PendingViewerNavigation::Last)
                {
                    if should_skip_edge_navigation_for_same_target(
                        &self.current_navigation_path,
                        &target,
                        PendingViewerNavigation::Last,
                    ) {
                        Ok(())
                    } else {
                        self.request_filer_edge_target_navigation(
                            target,
                            is_container,
                            PendingViewerNavigation::Last,
                        )
                    }
                } else {
                    let command = if self.filer.ascending {
                        FilesystemCommand::Last { request_id: 0 }
                    } else {
                        FilesystemCommand::First { request_id: 0 }
                    };
                    self.request_navigation(command, None)
                }
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
            && self.active_transition.is_none()
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

    fn filer_edge_navigation_target(
        &self,
        navigation: PendingViewerNavigation,
    ) -> Option<(PathBuf, bool)> {
        if !self.show_filer {
            return None;
        }
        let targets = self
            .filer
            .entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.is_container))
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return None;
        }
        let use_front = match navigation {
            PendingViewerNavigation::First => self.filer.ascending,
            PendingViewerNavigation::Last => !self.filer.ascending,
            PendingViewerNavigation::Next | PendingViewerNavigation::Prev => return None,
        };
        if use_front {
            targets.first().cloned()
        } else {
            targets.last().cloned()
        }
    }

    fn navigation_edge_reached(&self, navigation: PendingViewerNavigation) -> bool {
        let direction = self.navigation_direction_sign();
        let step = match navigation {
            PendingViewerNavigation::First => -direction,
            PendingViewerNavigation::Last => direction,
            PendingViewerNavigation::Next | PendingViewerNavigation::Prev => return false,
        };
        adjacent_entry(&self.current_navigation_path, self.navigation_sort, step).is_none()
    }

    fn should_apply_edge_noop(&self, navigation: PendingViewerNavigation) -> bool {
        should_apply_edge_noop(
            navigation,
            self.show_filer,
            self.filer.directory.as_deref(),
            self.current_directory().as_deref(),
        )
    }

    fn bench_random_file_entry(
        &mut self,
    ) -> Option<crate::ui::menu::fileviewer::state::FilerEntry> {
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

    fn bench_container_entry_by_path(
        &self,
        path: &Path,
    ) -> Option<crate::ui::menu::fileviewer::state::FilerEntry> {
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
                        self.request_filer_directory(
                            dir,
                            Some(self.current_navigation_path.clone()),
                        );
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
                let Some(parent) = directory.and_then(|dir| dir.parent().map(Path::to_path_buf))
                else {
                    return false;
                };
                self.browse_filer_directory(parent);
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
        self.request_load_target_with_transition_direction(navigation_path, load_request_path, None)
    }

    pub(crate) fn request_load_target_with_transition_direction(
        &mut self,
        navigation_path: PathBuf,
        load_request_path: PathBuf,
        transition_direction: Option<ImageTransitionDirection>,
    ) -> Result<(), Box<dyn Error>> {
        let branch_changed = navigation_branch_path(&self.current_navigation_path)
            != navigation_branch_path(&navigation_path);
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
                "transition_direction": transition_direction.map(|direction| format!("{direction:?}")),
            }),
        );
        self.prepare_image_transition(switching_image, branch_changed, transition_direction);
        if self.try_take_preloaded(&navigation_path) {
            self.start_image_transition();
            self.log_bench_state(
                "viewer.request_load_target.preloaded_hit",
                serde_json::json!({
                    "navigation_path": navigation_path.display().to_string(),
                }),
            );
            return Ok(());
        }
        if branch_changed {
            self.show_loading_texture(true); // フォルダ変わった時だけリセット
            self.clear_current_image_display();
        }
        //        self.show_loading_texture(branch_changed);
        //        self.clear_current_image_display();
        let request_id = self.alloc_request_id();
        self.active_request = Some(ActiveRenderRequest::Load(request_id));
        self.active_request_started_at = Some(Instant::now());
        self.pending_navigation_path = Some(navigation_path.clone());
        if switching_image {
            self.pending_fit_recalc = false;
        }
        self.overlay
            .set_loading_message(format!("Loading {}", navigation_path.display()));
        let load_zoom = if switching_image { 1.0 } else { self.zoom };
        // Folder/branch switch can trigger expensive synchronous adjacent lookup.
        // Prioritize primary image load to avoid UI stalls; companion will be synced afterward.
        let spread_companion_path = if branch_changed {
            None
        } else {
            self.desired_manga_companion_path_for_navigation(&navigation_path)
        };
        self.worker_tx
            .send(RenderCommand::LoadPath {
                request_id,
                path: load_request_path,
                companion_path: spread_companion_path.clone(),
                zoom: load_zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
                max_texture_side: self.max_texture_side,
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
        self.invalidate_preload();
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
        let request_id = self.alloc_request_id();
        self.active_request = Some(ActiveRenderRequest::Resize(request_id));
        self.active_request_started_at = Some(Instant::now());
        self.overlay
            .set_loading_message(format!("Rendering {:.0}%", self.zoom * 100.0));
        self.worker_tx
            .send(RenderCommand::ResizeCurrent {
                request_id,
                source: self.source.clone(),
                zoom: self.zoom,
                method: self.render_options.zoom_method,
                scale_mode: self.render_options.scale_mode,
                max_texture_side: self.max_texture_side,
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
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_window_state(ctx);
        self.update_window_title(ctx);
        self.poll_worker();
        self.poll_render_request_timeout();
        self.poll_companion_worker();
        self.poll_preload_worker();
        self.poll_filesystem();
        self.poll_filer_worker();
        self.poll_auto_folder_refresh();
        self.poll_thumbnail_worker();
        self.poll_save_result();
        self.sync_manga_companion(ctx);
        self.handle_keyboard(ctx);
        self.poll_pending_pointer_actions();
        self.settings_ui(ctx);
        self.restart_prompt_ui(ctx);
        self.alert_dialog_ui(ctx);
        self.save_dialog_ui(ctx);
        self.file_action_dialog_ui(ctx);
        self.left_click_menu_ui(ctx);
        self.run_bench_automation(ctx);
        if !self.transition_effect_enabled() {
            self.cancel_image_transition("effect_disabled");
        }
        self.filer_ui(ctx);
        self.subfiler_ui(ctx);
        self.status_panel_ui(ctx);
        #[cfg(windows)]
        if let Some(transition) = self.active_transition.as_mut() {
            transition.display_period =
                transition_display_period_for_window(_frame, transition.display_period);
        }

        let zoom_delta = if self.input_options.touch.pinch_zoom {
            ctx.input(|i| i.zoom_delta())
        } else {
            1.0
        };

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
                startup_layout_is_settling(self.frame_counter, viewport, self.last_viewport_size);

            if startup_viewport_settling {
                self.last_viewport_size = viewport;
                self.pending_fit_recalc |= !self.empty_mode
                    && !matches!(self.render_options.zoom_option, ZoomOption::None);
                ctx.request_repaint_after(STARTUP_LAYOUT_REPAINT_INTERVAL);
            } else if should_recalculate_fit_layout(
                self.empty_mode,
                viewport,
                self.last_viewport_size,
                self.pending_fit_recalc,
                &self.render_options.zoom_option,
            ) {
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
            let mut current_scene = TransitionScene {
                viewport: display_rect,
                images: Vec::with_capacity(2),
                separator: None,
            };
            egui::ScrollArea::both()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let spread_active = self.manga_spread_active();
                    let response = if spread_active {
                        let companion = self.visible_companion();
                        let companion_draw_size = companion.map(|(companion_rendered, _)| {
                            vec2(
                                companion_rendered.canvas.width() as f32
                                    * self.companion_draw_scale(),
                                companion_rendered.canvas.height() as f32
                                    * self.companion_draw_scale(),
                            )
                        });
                        let total_draw_size = if let Some(companion_draw_size) = companion_draw_size
                        {
                            vec2(
                                draw_size.x
                                    + companion_draw_size.x
                                    + self.options.manga_separator.pixels.max(0.0),
                                draw_size.y.max(companion_draw_size.y),
                            )
                        } else {
                            draw_size
                        };
                        let offset = aligned_offset(viewport, total_draw_size, self.options.align);
                        ui.add_space(offset.y.max(0.0));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.add_space(offset.x.max(0.0));
                            if let Some((_, companion_texture)) = companion {
                                let companion_draw_size = companion_draw_size.unwrap_or(draw_size);
                                let (first_texture, first_size, second_texture, second_size) =
                                    if self.options.manga_right_to_left {
                                        (
                                            companion_texture,
                                            companion_draw_size,
                                            &self.current_texture,
                                            draw_size,
                                        )
                                    } else {
                                        (
                                            &self.current_texture,
                                            draw_size,
                                            companion_texture,
                                            companion_draw_size,
                                        )
                                    };
                                let first =
                                    self.add_transition_image_widget(ui, first_texture, first_size);
                                current_scene.images.push(TransitionImageLayer {
                                    texture: first_texture.clone(),
                                    rect: first.rect,
                                });
                                current_scene.separator = self.paint_manga_separator(
                                    ui,
                                    draw_size.y.max(companion_draw_size.y),
                                );
                                let second = self.add_transition_image_widget(
                                    ui,
                                    second_texture,
                                    second_size,
                                );
                                current_scene.images.push(TransitionImageLayer {
                                    texture: second_texture.clone(),
                                    rect: second.rect,
                                });
                                Some(first)
                            } else {
                                let first = self.add_transition_image_widget(
                                    ui,
                                    &self.current_texture,
                                    draw_size,
                                );
                                current_scene.images.push(TransitionImageLayer {
                                    texture: self.current_texture.clone(),
                                    rect: first.rect,
                                });
                                Some(first)
                            }
                        })
                        .inner
                    } else {
                        let offset = aligned_offset(viewport, draw_size, self.options.align);
                        ui.add_space(offset.y.max(0.0));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.add_space(offset.x.max(0.0));
                            let first = self.add_transition_image_widget(
                                ui,
                                &self.current_texture,
                                draw_size,
                            );
                            current_scene.images.push(TransitionImageLayer {
                                texture: self.current_texture.clone(),
                                rect: first.rect,
                            });
                            Some(first)
                        })
                        .inner
                    };
                    let display_response = ui.interact(
                        display_rect,
                        ui.id().with("viewer_display_area"),
                        egui::Sense::click_and_drag(),
                    );
                    if let Some(response) = response {
                        if !self.handle_pointer_input(&response)
                            && self.response_has_pointer_intent(&display_response)
                        {
                            let _ = self.handle_pointer_input(&display_response);
                        }
                    } else if self.response_has_pointer_intent(&display_response) {
                        let _ = self.handle_pointer_input(&display_response);
                    }

                    if self.empty_mode && self.active_fs_request_id.is_none() {
                        ui.add_space(8.0);
                        ui.label(format!(
                            "{} {}",
                            self.text(UiTextKey::NoDisplayableFileFound),
                            self.text(UiTextKey::OpenDirectoryOrFileFromFiler)
                        ));
                    }
                });
            if self.active_transition.is_some() {
                self.draw_transition_scene(ui, &current_scene);
            }
            self.last_drawn_scene = Some(current_scene);
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
    failed_navigation_path.is_some_and(|path| path == current_navigation_path)
}

fn should_clear_filer_select_request_for_current(
    pending_user_request: Option<&FilerUserRequest>,
    current_navigation_path: &Path,
) -> bool {
    matches!(pending_user_request, Some(FilerUserRequest::SelectFile { navigation_path }) if is_same_navigation_target(navigation_path, current_navigation_path))
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

fn should_clear_stale_committed_browse_for_viewer_navigation(
    show_filer: bool,
    pending_user_request: Option<&FilerUserRequest>,
) -> bool {
    !show_filer && pending_user_request.is_none()
}

fn should_clear_stale_committed_browse_when_filer_aligned(
    filer_directory: Option<&Path>,
    current_directory: &Path,
    pending_user_request: Option<&FilerUserRequest>,
) -> bool {
    filer_directory == Some(current_directory) && pending_user_request.is_none()
}

fn should_clear_filer_request_on_hide(pending_user_request: Option<&FilerUserRequest>) -> bool {
    matches!(
        pending_user_request,
        Some(FilerUserRequest::BrowseDirectory { .. } | FilerUserRequest::Refresh { .. })
    )
}

fn should_handoff_filer_control_to_viewer_navigation(
    pending_user_request: Option<&FilerUserRequest>,
    committed_browse_directory: Option<&Path>,
) -> bool {
    pending_user_request.is_none() && committed_browse_directory.is_some()
}

fn should_cancel_filer_request_for_viewer_navigation(
    pending_user_request: Option<&FilerUserRequest>,
) -> bool {
    matches!(
        pending_user_request,
        Some(FilerUserRequest::BrowseDirectory { .. } | FilerUserRequest::Refresh { .. })
    )
}

fn should_sync_filer_selected_with_current(
    pending_user_request: Option<&FilerUserRequest>,
    filer_directory: Option<&Path>,
    current_directory: Option<&Path>,
) -> bool {
    pending_user_request.is_none()
        && filer_directory.is_some()
        && filer_directory == current_directory
}

fn should_skip_edge_navigation_for_same_target(
    current: &Path,
    target: &Path,
    navigation: PendingViewerNavigation,
) -> bool {
    if !is_browser_container(target) {
        return is_same_navigation_target(current, target);
    }
    let edge_target = match navigation {
        PendingViewerNavigation::First => resolve_start_path(target),
        PendingViewerNavigation::Last => resolve_end_path(target),
        PendingViewerNavigation::Next | PendingViewerNavigation::Prev => None,
    };
    edge_target
        .as_deref()
        .map(|edge| is_same_navigation_target(current, edge))
        .unwrap_or(false)
}

fn should_apply_edge_noop(
    navigation: PendingViewerNavigation,
    show_filer: bool,
    filer_directory: Option<&Path>,
    current_directory: Option<&Path>,
) -> bool {
    matches!(
        navigation,
        PendingViewerNavigation::First | PendingViewerNavigation::Last
    ) && (!show_filer || filer_directory == current_directory)
}

fn should_reinitialize_filesystem_from_filer_snapshot(
    current_navigation_path: &Path,
    current_directory: Option<&Path>,
    filer_directory: Option<&Path>,
    filer_entries: &[FilerEntry],
    filer_selected: Option<&Path>,
) -> bool {
    if current_directory != filer_directory {
        return false;
    }
    let current_exists = filer_entries
        .iter()
        .any(|entry| is_same_navigation_target(&entry.path, current_navigation_path));
    if !current_exists {
        return true;
    }
    filer_selected
        .map(|selected| !is_same_navigation_target(selected, current_navigation_path))
        .unwrap_or(false)
}

fn is_same_navigation_target(lhs: &Path, rhs: &Path) -> bool {
    if lhs == rhs {
        return true;
    }
    let lhs_rebased = resolve_navigation_entry_path(lhs);
    let rhs_rebased = resolve_navigation_entry_path(rhs);
    match (lhs_rebased, rhs_rebased) {
        (Some(lhs), Some(rhs)) => lhs == rhs,
        _ => false,
    }
}

fn navigation_sort_for_filer(
    filer_sort_field: FilerSortField,
    name_sort_mode: NameSortMode,
) -> NavigationSortOption {
    match filer_sort_field {
        FilerSortField::Name => match name_sort_mode {
            NameSortMode::Os => NavigationSortOption::OsName,
            NameSortMode::CaseSensitive => NavigationSortOption::NameCaseSensitive,
            NameSortMode::CaseInsensitive => NavigationSortOption::NameCaseInsensitive,
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
    navigation_direction_sign: isize,
    manga_mode: bool,
) -> Option<PathBuf> {
    if !manga_mode {
        return None;
    }
    let companion = adjacent_entry(navigation_path, navigation_sort, navigation_direction_sign)?;
    let current_branch = navigation_branch_path(navigation_path);
    let companion_branch = navigation_branch_path(&companion);
    (current_branch == companion_branch).then_some(companion)
}

fn cached_spread_companion_path(
    cache: &mut Option<MangaCompanionLookup>,
    key: MangaCompanionLookupKey,
    resolve: impl FnOnce() -> Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(cached) = cache.as_ref().filter(|cached| cached.key == key) {
        return cached.path.clone();
    }
    let path = resolve();
    *cache = Some(MangaCompanionLookup {
        key,
        path: path.clone(),
    });
    path
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

fn folder_refresh_signature(path: &Path) -> Option<u64> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    let metadata = std::fs::metadata(path).ok()?;
    metadata.is_dir().hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
        .hash(&mut hasher);

    if metadata.is_dir() {
        let mut rows = std::fs::read_dir(path)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| {
                let entry_path = entry.path();
                let metadata = entry.metadata().ok();
                let modified = metadata
                    .as_ref()
                    .and_then(|metadata| metadata.modified().ok())
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos())
                    .unwrap_or_default();
                (
                    entry_path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    metadata
                        .as_ref()
                        .map(|metadata| metadata.is_dir())
                        .unwrap_or(false),
                    metadata
                        .as_ref()
                        .map(|metadata| metadata.len())
                        .unwrap_or(0),
                    modified,
                )
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| left.0.cmp(&right.0));
        rows.hash(&mut hasher);
    }

    Some(hasher.finish())
}

fn transition_progress(started_at: &mut Option<Instant>, now: Instant, duration: Duration) -> f32 {
    let started_at = started_at.get_or_insert(now);
    if duration.is_zero() {
        return 1.0;
    }
    (now.duration_since(*started_at).as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
}

fn slide_layer_clip(viewport: egui::Rect, offset: egui::Vec2) -> egui::Rect {
    viewport.intersect(viewport.translate(offset))
}

fn paint_transition_scene(
    painter: &egui::Painter,
    scene: &TransitionScene,
    offset: egui::Vec2,
    opacity: f32,
) {
    if opacity <= 0.0 {
        return;
    }
    for image in &scene.images {
        paint_texture(
            painter,
            image.texture.id(),
            image.rect.translate(offset),
            opacity,
        );
    }
    if let Some(separator) = &scene.separator {
        paint_transition_separator(painter, separator, offset, opacity);
    }
}

fn paint_transition_separator(
    painter: &egui::Painter,
    separator: &TransitionSeparatorLayer,
    offset: egui::Vec2,
    opacity: f32,
) {
    use crate::ui::viewer::options::MangaSeparatorStyle;
    let rect = separator.rect.translate(offset);
    let base = separator.options.color;
    match separator.options.style {
        MangaSeparatorStyle::None => {}
        MangaSeparatorStyle::Solid => {
            painter.rect_filled(
                rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(
                    base[0],
                    base[1],
                    base[2],
                    (base[3] as f32 * opacity).round() as u8,
                ),
            );
        }
        MangaSeparatorStyle::Shadow => {
            let steps = rect.width().max(2.0) as usize;
            for step in 0..steps {
                let t = (step as f32 + 0.5) / steps as f32;
                let alpha = (1.0 - ((t - 0.5).abs() * 2.0)).max(0.0) * base[3] as f32 * opacity;
                let x0 = rect.left() + (step as f32 / steps as f32) * rect.width();
                let x1 = rect.left() + ((step + 1) as f32 / steps as f32) * rect.width();
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(x0, rect.top()),
                        egui::pos2(x1, rect.bottom()),
                    ),
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

fn paint_texture(
    painter: &egui::Painter,
    texture_id: egui::TextureId,
    rect: egui::Rect,
    opacity: f32,
) {
    let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
    painter.image(
        texture_id,
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::from_white_alpha(alpha),
    );
}

struct SlideTransitionOffset {
    previous: egui::Vec2,
    current: egui::Vec2,
}

fn slide_transition_offset(
    effect: TransitionEffect,
    size: egui::Vec2,
    progress: f32,
) -> SlideTransitionOffset {
    match effect {
        TransitionEffect::SlideRightToLeft => SlideTransitionOffset {
            previous: vec2(-size.x * progress, 0.0),
            current: vec2(size.x * (1.0 - progress), 0.0),
        },
        TransitionEffect::SlideLeftToRight => SlideTransitionOffset {
            previous: vec2(size.x * progress, 0.0),
            current: vec2(-size.x * (1.0 - progress), 0.0),
        },
        TransitionEffect::SlideTopToBottom => SlideTransitionOffset {
            previous: vec2(0.0, size.y * progress),
            current: vec2(0.0, -size.y * (1.0 - progress)),
        },
        TransitionEffect::SlideBottomToTop => SlideTransitionOffset {
            previous: vec2(0.0, -size.y * progress),
            current: vec2(0.0, size.y * (1.0 - progress)),
        },
        _ => SlideTransitionOffset {
            previous: egui::Vec2::ZERO,
            current: egui::Vec2::ZERO,
        },
    }
}

fn transition_effect_for_direction(
    effect: TransitionEffect,
    direction: Option<ImageTransitionDirection>,
) -> TransitionEffect {
    if !matches!(direction, Some(ImageTransitionDirection::Backward)) {
        return effect;
    }

    match effect {
        TransitionEffect::SlideRightToLeft => TransitionEffect::SlideLeftToRight,
        TransitionEffect::SlideLeftToRight => TransitionEffect::SlideRightToLeft,
        TransitionEffect::SlideTopToBottom => TransitionEffect::SlideBottomToTop,
        TransitionEffect::SlideBottomToTop => TransitionEffect::SlideTopToBottom,
        TransitionEffect::None
        | TransitionEffect::Fade
        | TransitionEffect::SpiralWipeIn
        | TransitionEffect::SpiralWipeOut => effect,
    }
}

fn spiral_reveal_rect(rect: egui::Rect, progress: f32) -> egui::Rect {
    let eased = (progress.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2).sin();
    let phase = progress.clamp(0.0, 1.0) * std::f32::consts::TAU * 3.0;
    let ripple = (phase.sin() * 0.06) * (1.0 - eased);
    let width_ratio = (eased + ripple).clamp(0.0, 1.0);
    let height_ratio = (eased - ripple).clamp(0.0, 1.0);
    egui::Rect::from_center_size(
        rect.center(),
        vec2(
            rect.width() * width_ratio.max(0.01),
            rect.height() * height_ratio.max(0.01),
        ),
    )
}

#[cfg(test)]
#[path = "../../../tests/support/ui_viewer.rs"]
mod tests;
