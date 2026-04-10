use std::error::Error;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::dependent::default_config_dir;
use crate::dependent::plugins::PluginConfig;
use crate::drawers::affine::InterpolationAlgorithm;
use crate::options::{
    AppConfig, EndOfFolderOption, FontSizePreset, MangaSeparatorOptions, MangaSeparatorStyle,
    NavigationSortOption, PaneSide, RenderScaleMode, ResourceOptions, RuntimeOptions,
    StorageOptions, WindowUiTheme,
};
use crate::ui::viewer::options::{
    BackgroundStyle, RenderOptions, ViewerOptions, WindowOptions, WindowSize, WindowStartPosition,
    ZoomOption,
};

type ConfigResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
struct ConfigFile {
    viewer: ViewerConfigFile,
    window: WindowConfigFile,
    render: RenderConfigFile,
    resources: ResourceConfigFile,
    filesystem: FilesystemConfigFile,
    plugins: PluginConfig,
    storage: StorageConfigFile,
    navigation: NavigationConfigFile,
    runtime: RuntimeConfigFile,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct ViewerConfigFile {
    animation: bool,
    grayscale: bool,
    manga_mode: bool,
    manga_right_to_left: bool,
    manga_separator: MangaSeparatorConfigFile,
    background: BackgroundConfigFile,
}

impl Default for ViewerConfigFile {
    fn default() -> Self {
        Self {
            animation: true,
            grayscale: false,
            manga_mode: false,
            manga_right_to_left: true,
            manga_separator: MangaSeparatorConfigFile::default(),
            background: BackgroundConfigFile::Solid {
                rgba: [0, 0, 0, 255],
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct MangaSeparatorConfigFile {
    style: MangaSeparatorStyleConfigFile,
    color: [u8; 4],
    pixels: f32,
}

impl Default for MangaSeparatorConfigFile {
    fn default() -> Self {
        Self {
            style: MangaSeparatorStyleConfigFile::None,
            color: [24, 24, 24, 255],
            pixels: 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MangaSeparatorStyleConfigFile {
    None,
    Solid,
    Shadow,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BackgroundConfigFile {
    Solid {
        rgba: [u8; 4],
    },
    Tile {
        color1: [u8; 4],
        color2: [u8; 4],
        size: u32,
    },
}

impl Default for BackgroundConfigFile {
    fn default() -> Self {
        Self::Solid {
            rgba: [0, 0, 0, 255],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct WindowConfigFile {
    fullscreen: bool,
    size: WindowSizeConfigFile,
    start_position: WindowStartPositionConfigFile,
    remember_size: bool,
    remember_position: bool,
    ui_theme: WindowUiThemeConfigFile,
    pane_side: PaneSideConfigFile,
}

impl Default for WindowConfigFile {
    fn default() -> Self {
        Self {
            fullscreen: false,
            size: WindowSizeConfigFile::Relative(0.6),
            start_position: WindowStartPositionConfigFile::Center,
            remember_size: true,
            remember_position: true,
            ui_theme: WindowUiThemeConfigFile::Dark,
            pane_side: PaneSideConfigFile::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PaneSideConfigFile {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WindowUiThemeConfigFile {
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
enum WindowSizeConfigFile {
    Relative(f32),
    Exact { width: f32, height: f32 },
}

impl Default for WindowSizeConfigFile {
    fn default() -> Self {
        Self::Relative(0.6)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
enum WindowStartPositionConfigFile {
    Center,
    Exact { x: f32, y: f32 },
}

impl Default for WindowStartPositionConfigFile {
    fn default() -> Self {
        Self::Center
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct RenderConfigFile {
    scale_mode: RenderScaleModeConfigFile,
    zoom_option: ZoomOptionConfigFile,
    zoom_method: ZoomMethodConfigFile,
}

impl Default for RenderConfigFile {
    fn default() -> Self {
        Self {
            scale_mode: RenderScaleModeConfigFile::FastGpu,
            zoom_option: ZoomOptionConfigFile::FitScreen,
            zoom_method: ZoomMethodConfigFile::Bilinear,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RenderScaleModeConfigFile {
    FastGpu,
    PreciseCpu,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ZoomOptionConfigFile {
    None,
    FitWidth,
    FitHeight,
    FitScreen,
    FitScreenIncludeSmaller,
    FitScreenOnlySmaller,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ZoomMethodConfigFile {
    Nearest,
    Bilinear,
    Bicubic,
    Lanczos3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct ResourceConfigFile {
    locale: Option<String>,
    font_size: FontSizeConfigFile,
    font_paths: Vec<PathBuf>,
}

impl Default for ResourceConfigFile {
    fn default() -> Self {
        Self {
            locale: None,
            font_size: FontSizeConfigFile::Auto,
            font_paths: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FontSizeConfigFile {
    Auto,
    S,
    M,
    L,
    Ll,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct NavigationConfigFile {
    end_of_folder: EndOfFolderConfigFile,
    sort: NavigationSortConfigFile,
    archive: ArchiveBrowseConfigFile,
}

impl Default for NavigationConfigFile {
    fn default() -> Self {
        Self {
            end_of_folder: EndOfFolderConfigFile::Recursive,
            sort: NavigationSortConfigFile::OsName,
            archive: ArchiveBrowseConfigFile::Folder,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct StorageConfigFile {
    path_record: bool,
    path: Option<PathBuf>,
}

impl Default for StorageConfigFile {
    fn default() -> Self {
        Self {
            path_record: false,
            path: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum EndOfFolderConfigFile {
    Stop,
    Next,
    Loop,
    Recursive,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum NavigationSortConfigFile {
    OsName,
    Name,
    Date,
    Size,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
enum ArchiveBrowseConfigFile {
    Folder,
    Skip,
    Archiver,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct FilesystemConfigFile {
    thumbnail: ThumbnailConfigFile,
}

impl Default for FilesystemConfigFile {
    fn default() -> Self {
        Self {
            thumbnail: ThumbnailConfigFile::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct ThumbnailConfigFile {
    suppress_large_files: bool,
}

impl Default for ThumbnailConfigFile {
    fn default() -> Self {
        Self {
            suppress_large_files: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct RuntimeConfigFile {
    current_file: Option<PathBuf>,
    workaround: WorkaroundConfigFile,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct WorkaroundConfigFile {
    archive: ArchiveWorkaroundConfigFile,
}

impl Default for WorkaroundConfigFile {
    fn default() -> Self {
        Self {
            archive: ArchiveWorkaroundConfigFile::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct ArchiveWorkaroundConfigFile {
    zip: ZipWorkaroundConfigFile,
}

impl Default for ArchiveWorkaroundConfigFile {
    fn default() -> Self {
        Self {
            zip: ZipWorkaroundConfigFile::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct ZipWorkaroundConfigFile {
    threshold_mb: u64,
    local_cache: bool,
}

const LEGACY_ZIP_WORKAROUND_THRESHOLD_MB: u64 = 256;
const LEGACY_ZIP_WORKAROUND_LOCAL_CACHE: bool = false;

impl Default for ZipWorkaroundConfigFile {
    fn default() -> Self {
        Self {
            threshold_mb: 16,
            local_cache: true,
        }
    }
}

impl Default for RuntimeConfigFile {
    fn default() -> Self {
        Self {
            current_file: None,
            workaround: WorkaroundConfigFile::default(),
        }
    }
}

pub fn load_app_config(config_path: Option<&Path>) -> ConfigResult<AppConfig> {
    let Some(file) = load_config_file(config_path)? else {
        return Ok(AppConfig::default());
    };
    Ok(file.into())
}

pub fn load_startup_path(config_path: Option<&Path>) -> ConfigResult<PathBuf> {
    if let Some(file) = load_config_file(config_path)? {
        if let Some(path) = file.runtime.current_file {
            return Ok(path);
        }
    }

    Ok(std::env::current_dir()?)
}

pub fn save_app_config(
    config: &AppConfig,
    current_path: Option<&Path>,
    config_override: Option<&Path>,
) -> ConfigResult<()> {
    let path = resolve_config_path(config_override);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let text = toml::to_string_pretty(&ConfigFile::from_parts(config.clone(), current_path))?;
    fs::write(path, text)?;
    Ok(())
}

fn resolve_config_path(config_override: Option<&Path>) -> PathBuf {
    config_override
        .map(|path| path.to_path_buf())
        .or_else(|| default_config_dir().map(|dir| dir.join("config.toml")))
        .unwrap_or_else(|| PathBuf::from("wml2viewer.toml"))
}

fn load_config_file(config_override: Option<&Path>) -> ConfigResult<Option<ConfigFile>> {
    let path = resolve_config_path(config_override);
    if !path.exists() {
        return Ok(None);
    }

    let text = fs::read_to_string(path)?;
    let file: ConfigFile = toml::from_str(&text)?;
    Ok(Some(file))
}

impl From<ConfigFile> for AppConfig {
    fn from(value: ConfigFile) -> Self {
        let mut config = AppConfig::default();
        config.viewer = ViewerOptions {
            align: config.viewer.align,
            background: value.viewer.background.into(),
            fade: config.viewer.fade,
            animation: value.viewer.animation,
            grayscale: value.viewer.grayscale,
            manga_mode: value.viewer.manga_mode,
            manga_right_to_left: value.viewer.manga_right_to_left,
            manga_separator: value.viewer.manga_separator.into(),
        };
        config.window = value.window.into();
        config.render = value.render.into();
        config.resources = value.resources.into();
        config.plugins = value.plugins;
        config.storage = value.storage.into();
        config.navigation.end_of_folder = value.navigation.end_of_folder.into();
        config.navigation.sort = value.navigation.sort.into();
        config.navigation.archive = value.navigation.archive.into();
        config.runtime = value.runtime.into();
        config.runtime.workaround.thumbnail = value.filesystem.thumbnail.into();
        config
    }
}

impl From<AppConfig> for ConfigFile {
    fn from(value: AppConfig) -> Self {
        Self::from_parts(value, None)
    }
}

impl ConfigFile {
    fn from_parts(value: AppConfig, current_path: Option<&Path>) -> Self {
        let thumbnail = value.runtime.workaround.thumbnail.clone();
        Self {
            viewer: ViewerConfigFile {
                animation: value.viewer.animation,
                grayscale: value.viewer.grayscale,
                manga_mode: value.viewer.manga_mode,
                manga_right_to_left: value.viewer.manga_right_to_left,
                manga_separator: value.viewer.manga_separator.into(),
                background: value.viewer.background.into(),
            },
            window: value.window.into(),
            render: value.render.into(),
            resources: value.resources.into(),
            filesystem: FilesystemConfigFile {
                thumbnail: thumbnail.into(),
            },
            plugins: value.plugins,
            storage: value.storage.into(),
            navigation: NavigationConfigFile {
                end_of_folder: value.navigation.end_of_folder.into(),
                sort: value.navigation.sort.into(),
                archive: value.navigation.archive.into(),
            },
            runtime: RuntimeConfigFile {
                current_file: current_path.map(|path| path.to_path_buf()),
                workaround: value.runtime.into(),
            },
        }
    }
}

impl From<RuntimeConfigFile> for RuntimeOptions {
    fn from(value: RuntimeConfigFile) -> Self {
        Self {
            workaround: value.workaround.into(),
        }
    }
}

impl From<RuntimeOptions> for WorkaroundConfigFile {
    fn from(value: RuntimeOptions) -> Self {
        value.workaround.into()
    }
}

impl From<WorkaroundConfigFile> for crate::options::WorkaroundOptions {
    fn from(value: WorkaroundConfigFile) -> Self {
        Self {
            archive: value.archive.into(),
            thumbnail: crate::options::ThumbnailWorkaroundOptions::default(),
        }
    }
}

impl From<crate::options::WorkaroundOptions> for WorkaroundConfigFile {
    fn from(value: crate::options::WorkaroundOptions) -> Self {
        Self {
            archive: value.archive.into(),
        }
    }
}

impl From<ArchiveWorkaroundConfigFile> for crate::options::ArchiveWorkaroundOptions {
    fn from(value: ArchiveWorkaroundConfigFile) -> Self {
        Self {
            zip: value.zip.into(),
        }
    }
}

impl From<crate::options::ArchiveWorkaroundOptions> for ArchiveWorkaroundConfigFile {
    fn from(value: crate::options::ArchiveWorkaroundOptions) -> Self {
        Self {
            zip: value.zip.into(),
        }
    }
}

impl From<ZipWorkaroundConfigFile> for crate::options::ZipWorkaroundOptions {
    fn from(value: ZipWorkaroundConfigFile) -> Self {
        if value.threshold_mb == LEGACY_ZIP_WORKAROUND_THRESHOLD_MB
            && value.local_cache == LEGACY_ZIP_WORKAROUND_LOCAL_CACHE
        {
            return Self::default();
        }
        Self {
            threshold_mb: value.threshold_mb,
            local_cache: value.local_cache,
        }
    }
}

impl From<crate::options::ZipWorkaroundOptions> for ZipWorkaroundConfigFile {
    fn from(value: crate::options::ZipWorkaroundOptions) -> Self {
        Self {
            threshold_mb: value.threshold_mb,
            local_cache: value.local_cache,
        }
    }
}

impl From<ThumbnailConfigFile> for crate::options::ThumbnailWorkaroundOptions {
    fn from(value: ThumbnailConfigFile) -> Self {
        Self {
            suppress_large_files: value.suppress_large_files,
        }
    }
}

impl From<crate::options::ThumbnailWorkaroundOptions> for ThumbnailConfigFile {
    fn from(value: crate::options::ThumbnailWorkaroundOptions) -> Self {
        Self {
            suppress_large_files: value.suppress_large_files,
        }
    }
}

impl From<BackgroundConfigFile> for BackgroundStyle {
    fn from(value: BackgroundConfigFile) -> Self {
        match value {
            BackgroundConfigFile::Solid { rgba } => BackgroundStyle::Solid(rgba),
            BackgroundConfigFile::Tile {
                color1,
                color2,
                size,
            } => BackgroundStyle::Tile {
                color1,
                color2,
                size,
            },
        }
    }
}

impl From<BackgroundStyle> for BackgroundConfigFile {
    fn from(value: BackgroundStyle) -> Self {
        match value {
            BackgroundStyle::Solid(rgba) => Self::Solid { rgba },
            BackgroundStyle::Tile {
                color1,
                color2,
                size,
            } => Self::Tile {
                color1,
                color2,
                size,
            },
        }
    }
}

impl From<WindowConfigFile> for WindowOptions {
    fn from(value: WindowConfigFile) -> Self {
        Self {
            fullscreen: value.fullscreen,
            size: value.size.into(),
            start_position: value.start_position.into(),
            remember_size: value.remember_size,
            remember_position: value.remember_position,
            ui_theme: value.ui_theme.into(),
            pane_side: value.pane_side.into(),
        }
    }
}

impl From<WindowOptions> for WindowConfigFile {
    fn from(value: WindowOptions) -> Self {
        Self {
            fullscreen: value.fullscreen,
            size: value.size.into(),
            start_position: value.start_position.into(),
            remember_size: value.remember_size,
            remember_position: value.remember_position,
            ui_theme: value.ui_theme.into(),
            pane_side: value.pane_side.into(),
        }
    }
}

impl From<PaneSideConfigFile> for PaneSide {
    fn from(value: PaneSideConfigFile) -> Self {
        match value {
            PaneSideConfigFile::Left => PaneSide::Left,
            PaneSideConfigFile::Right => PaneSide::Right,
        }
    }
}

impl From<PaneSide> for PaneSideConfigFile {
    fn from(value: PaneSide) -> Self {
        match value {
            PaneSide::Left => Self::Left,
            PaneSide::Right => Self::Right,
        }
    }
}

impl From<WindowSizeConfigFile> for WindowSize {
    fn from(value: WindowSizeConfigFile) -> Self {
        match value {
            WindowSizeConfigFile::Relative(ratio) => WindowSize::Relative(ratio),
            WindowSizeConfigFile::Exact { width, height } => WindowSize::Exact { width, height },
        }
    }
}

impl From<WindowSize> for WindowSizeConfigFile {
    fn from(value: WindowSize) -> Self {
        match value {
            WindowSize::Relative(ratio) => Self::Relative(ratio),
            WindowSize::Exact { width, height } => Self::Exact { width, height },
        }
    }
}

impl From<WindowStartPositionConfigFile> for WindowStartPosition {
    fn from(value: WindowStartPositionConfigFile) -> Self {
        match value {
            WindowStartPositionConfigFile::Center => WindowStartPosition::Center,
            WindowStartPositionConfigFile::Exact { x, y } => WindowStartPosition::Exact { x, y },
        }
    }
}

impl From<WindowStartPosition> for WindowStartPositionConfigFile {
    fn from(value: WindowStartPosition) -> Self {
        match value {
            WindowStartPosition::Center => Self::Center,
            WindowStartPosition::Exact { x, y } => Self::Exact { x, y },
        }
    }
}

impl From<WindowUiThemeConfigFile> for WindowUiTheme {
    fn from(value: WindowUiThemeConfigFile) -> Self {
        match value {
            WindowUiThemeConfigFile::System => WindowUiTheme::System,
            WindowUiThemeConfigFile::Light => WindowUiTheme::Light,
            WindowUiThemeConfigFile::Dark => WindowUiTheme::Dark,
        }
    }
}

impl From<WindowUiTheme> for WindowUiThemeConfigFile {
    fn from(value: WindowUiTheme) -> Self {
        match value {
            WindowUiTheme::System => Self::System,
            WindowUiTheme::Light => Self::Light,
            WindowUiTheme::Dark => Self::Dark,
        }
    }
}

impl From<RenderConfigFile> for RenderOptions {
    fn from(value: RenderConfigFile) -> Self {
        Self {
            scale_mode: value.scale_mode.into(),
            zoom_option: value.zoom_option.into(),
            zoom_method: value.zoom_method.into(),
        }
    }
}

impl From<RenderOptions> for RenderConfigFile {
    fn from(value: RenderOptions) -> Self {
        Self {
            scale_mode: value.scale_mode.into(),
            zoom_option: value.zoom_option.into(),
            zoom_method: value.zoom_method.into(),
        }
    }
}

impl From<MangaSeparatorConfigFile> for MangaSeparatorOptions {
    fn from(value: MangaSeparatorConfigFile) -> Self {
        Self {
            style: value.style.into(),
            color: value.color,
            pixels: value.pixels,
        }
    }
}

impl From<MangaSeparatorOptions> for MangaSeparatorConfigFile {
    fn from(value: MangaSeparatorOptions) -> Self {
        Self {
            style: value.style.into(),
            color: value.color,
            pixels: value.pixels,
        }
    }
}

impl From<MangaSeparatorStyleConfigFile> for MangaSeparatorStyle {
    fn from(value: MangaSeparatorStyleConfigFile) -> Self {
        match value {
            MangaSeparatorStyleConfigFile::None => MangaSeparatorStyle::None,
            MangaSeparatorStyleConfigFile::Solid => MangaSeparatorStyle::Solid,
            MangaSeparatorStyleConfigFile::Shadow => MangaSeparatorStyle::Shadow,
        }
    }
}

impl From<MangaSeparatorStyle> for MangaSeparatorStyleConfigFile {
    fn from(value: MangaSeparatorStyle) -> Self {
        match value {
            MangaSeparatorStyle::None => Self::None,
            MangaSeparatorStyle::Solid => Self::Solid,
            MangaSeparatorStyle::Shadow => Self::Shadow,
        }
    }
}

impl From<ResourceConfigFile> for ResourceOptions {
    fn from(value: ResourceConfigFile) -> Self {
        Self {
            locale: value.locale.or_else(crate::dependent::system_locale),
            font_size: value.font_size.into(),
            font_paths: value.font_paths,
        }
    }
}

impl From<ResourceOptions> for ResourceConfigFile {
    fn from(value: ResourceOptions) -> Self {
        Self {
            locale: value.locale,
            font_size: value.font_size.into(),
            font_paths: value.font_paths,
        }
    }
}

impl From<StorageConfigFile> for StorageOptions {
    fn from(value: StorageConfigFile) -> Self {
        Self {
            path_record: value.path_record,
            path: value.path.or_else(crate::dependent::default_download_dir),
        }
    }
}

impl From<StorageOptions> for StorageConfigFile {
    fn from(value: StorageOptions) -> Self {
        Self {
            path_record: value.path_record,
            path: value.path,
        }
    }
}

impl From<FontSizeConfigFile> for FontSizePreset {
    fn from(value: FontSizeConfigFile) -> Self {
        match value {
            FontSizeConfigFile::Auto => FontSizePreset::Auto,
            FontSizeConfigFile::S => FontSizePreset::S,
            FontSizeConfigFile::M => FontSizePreset::M,
            FontSizeConfigFile::L => FontSizePreset::L,
            FontSizeConfigFile::Ll => FontSizePreset::LL,
        }
    }
}

impl From<FontSizePreset> for FontSizeConfigFile {
    fn from(value: FontSizePreset) -> Self {
        match value {
            FontSizePreset::Auto => Self::Auto,
            FontSizePreset::S => Self::S,
            FontSizePreset::M => Self::M,
            FontSizePreset::L => Self::L,
            FontSizePreset::LL => Self::Ll,
        }
    }
}

impl From<ZoomOptionConfigFile> for ZoomOption {
    fn from(value: ZoomOptionConfigFile) -> Self {
        match value {
            ZoomOptionConfigFile::None => ZoomOption::None,
            ZoomOptionConfigFile::FitWidth => ZoomOption::FitWidth,
            ZoomOptionConfigFile::FitHeight => ZoomOption::FitHeight,
            ZoomOptionConfigFile::FitScreen => ZoomOption::FitScreen,
            ZoomOptionConfigFile::FitScreenIncludeSmaller => ZoomOption::FitScreenIncludeSmaller,
            ZoomOptionConfigFile::FitScreenOnlySmaller => ZoomOption::FitScreenOnlySmaller,
        }
    }
}

impl From<ZoomOption> for ZoomOptionConfigFile {
    fn from(value: ZoomOption) -> Self {
        match value {
            ZoomOption::None => Self::None,
            ZoomOption::FitWidth => Self::FitWidth,
            ZoomOption::FitHeight => Self::FitHeight,
            ZoomOption::FitScreen => Self::FitScreen,
            ZoomOption::FitScreenIncludeSmaller => Self::FitScreenIncludeSmaller,
            ZoomOption::FitScreenOnlySmaller => Self::FitScreenOnlySmaller,
        }
    }
}

impl From<ZoomMethodConfigFile> for InterpolationAlgorithm {
    fn from(value: ZoomMethodConfigFile) -> Self {
        match value {
            ZoomMethodConfigFile::Nearest => InterpolationAlgorithm::NearestNeighber,
            ZoomMethodConfigFile::Bilinear => InterpolationAlgorithm::Bilinear,
            ZoomMethodConfigFile::Bicubic => InterpolationAlgorithm::BicubicAlpha(None),
            ZoomMethodConfigFile::Lanczos3 => InterpolationAlgorithm::Lanzcos3,
        }
    }
}

impl From<InterpolationAlgorithm> for ZoomMethodConfigFile {
    fn from(value: InterpolationAlgorithm) -> Self {
        match value {
            InterpolationAlgorithm::NearestNeighber => Self::Nearest,
            InterpolationAlgorithm::Bilinear => Self::Bilinear,
            InterpolationAlgorithm::Bicubic | InterpolationAlgorithm::BicubicAlpha(_) => {
                Self::Bicubic
            }
            InterpolationAlgorithm::Lanzcos3 | InterpolationAlgorithm::Lanzcos(_) => Self::Lanczos3,
        }
    }
}

impl From<RenderScaleModeConfigFile> for RenderScaleMode {
    fn from(value: RenderScaleModeConfigFile) -> Self {
        match value {
            RenderScaleModeConfigFile::FastGpu => RenderScaleMode::FastGpu,
            RenderScaleModeConfigFile::PreciseCpu => RenderScaleMode::PreciseCpu,
        }
    }
}

impl From<RenderScaleMode> for RenderScaleModeConfigFile {
    fn from(value: RenderScaleMode) -> Self {
        match value {
            RenderScaleMode::FastGpu => Self::FastGpu,
            RenderScaleMode::PreciseCpu => Self::PreciseCpu,
        }
    }
}

impl From<EndOfFolderConfigFile> for EndOfFolderOption {
    fn from(value: EndOfFolderConfigFile) -> Self {
        match value {
            EndOfFolderConfigFile::Stop => EndOfFolderOption::Stop,
            EndOfFolderConfigFile::Next => EndOfFolderOption::Next,
            EndOfFolderConfigFile::Loop => EndOfFolderOption::Loop,
            EndOfFolderConfigFile::Recursive => EndOfFolderOption::Recursive,
        }
    }
}

impl From<EndOfFolderOption> for EndOfFolderConfigFile {
    fn from(value: EndOfFolderOption) -> Self {
        match value {
            EndOfFolderOption::Stop => Self::Stop,
            EndOfFolderOption::Next => Self::Next,
            EndOfFolderOption::Loop => Self::Loop,
            EndOfFolderOption::Recursive => Self::Recursive,
        }
    }
}

impl From<NavigationSortConfigFile> for NavigationSortOption {
    fn from(value: NavigationSortConfigFile) -> Self {
        match value {
            NavigationSortConfigFile::OsName => NavigationSortOption::OsName,
            NavigationSortConfigFile::Name => NavigationSortOption::Name,
            NavigationSortConfigFile::Date => NavigationSortOption::Date,
            NavigationSortConfigFile::Size => NavigationSortOption::Size,
        }
    }
}

impl From<NavigationSortOption> for NavigationSortConfigFile {
    fn from(value: NavigationSortOption) -> Self {
        match value {
            NavigationSortOption::OsName => Self::OsName,
            NavigationSortOption::Name => Self::Name,
            NavigationSortOption::Date => Self::Date,
            NavigationSortOption::Size => Self::Size,
        }
    }
}

impl From<ArchiveBrowseConfigFile> for crate::options::ArchiveBrowseOption {
    fn from(value: ArchiveBrowseConfigFile) -> Self {
        match value {
            ArchiveBrowseConfigFile::Folder => Self::Folder,
            ArchiveBrowseConfigFile::Skip => Self::Skip,
            ArchiveBrowseConfigFile::Archiver => Self::Archiver,
        }
    }
}

impl From<crate::options::ArchiveBrowseOption> for ArchiveBrowseConfigFile {
    fn from(value: crate::options::ArchiveBrowseOption) -> Self {
        match value {
            crate::options::ArchiveBrowseOption::Folder => Self::Folder,
            crate::options::ArchiveBrowseOption::Skip => Self::Skip,
            crate::options::ArchiveBrowseOption::Archiver => Self::Archiver,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LEGACY_ZIP_WORKAROUND_LOCAL_CACHE, LEGACY_ZIP_WORKAROUND_THRESHOLD_MB,
        ZipWorkaroundConfigFile,
    };
    use crate::options::ZipWorkaroundOptions;

    #[test]
    fn zip_workaround_config_defaults_match_runtime_defaults() {
        let config = ZipWorkaroundConfigFile::default();
        let runtime = ZipWorkaroundOptions::default();
        assert_eq!(config.threshold_mb, runtime.threshold_mb);
        assert_eq!(config.local_cache, runtime.local_cache);
    }

    #[test]
    fn legacy_zip_workaround_defaults_are_migrated_on_load() {
        let runtime = ZipWorkaroundOptions::from(ZipWorkaroundConfigFile {
            threshold_mb: LEGACY_ZIP_WORKAROUND_THRESHOLD_MB,
            local_cache: LEGACY_ZIP_WORKAROUND_LOCAL_CACHE,
        });
        assert_eq!(
            runtime.threshold_mb,
            ZipWorkaroundOptions::default().threshold_mb
        );
        assert_eq!(
            runtime.local_cache,
            ZipWorkaroundOptions::default().local_cache
        );
    }
}
