//! Viewer option models derived from `SPEC.md`.

use crate::drawers::affine::InterpolationAlgorithm;
use crate::drawers::image::ImageAlign;

#[derive(Clone)]
pub struct ViewerOptions {
    pub align: ImageAlign,
    pub background: BackgroundStyle,
    pub fade: bool,
    pub animation: bool,
    pub grayscale: bool,
    pub manga_mode: bool,
    pub manga_right_to_left: bool,
    pub manga_separator: MangaSeparatorOptions,
}

impl Default for ViewerOptions {
    fn default() -> Self {
        Self {
            align: ImageAlign::Center,
            background: BackgroundStyle::Solid([0, 0, 0, 255]),
            fade: false,
            animation: true,
            grayscale: false,
            manga_mode: false,
            manga_right_to_left: true,
            manga_separator: MangaSeparatorOptions::default(),
        }
    }
}

#[derive(Clone)]
pub struct MangaSeparatorOptions {
    pub style: MangaSeparatorStyle,
    pub color: [u8; 4],
    pub pixels: f32,
}

impl Default for MangaSeparatorOptions {
    fn default() -> Self {
        Self {
            style: MangaSeparatorStyle::None,
            color: [24, 24, 24, 255],
            pixels: 2.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MangaSeparatorStyle {
    None,
    Solid,
    Shadow,
}

#[derive(Clone)]
pub enum BackgroundStyle {
    Solid([u8; 4]),
    Tile {
        color1: [u8; 4],
        color2: [u8; 4],
        size: u32,
    },
}

#[derive(Clone)]
pub struct RenderOptions {
    pub scale_mode: RenderScaleMode,
    pub zoom_option: ZoomOption,
    pub zoom_method: InterpolationAlgorithm,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            scale_mode: RenderScaleMode::FastGpu,
            zoom_option: ZoomOption::FitScreen,
            zoom_method: InterpolationAlgorithm::Bilinear,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderScaleMode {
    FastGpu,
    PreciseCpu,
}

#[derive(Clone)]
pub struct WindowOptions {
    pub fullscreen: bool,
    pub size: WindowSize,
    pub start_position: WindowStartPosition,
    pub remember_size: bool,
    pub remember_position: bool,
    pub ui_theme: WindowUiTheme,
    pub pane_side: PaneSide,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            fullscreen: false,
            size: WindowSize::Relative(0.6),
            start_position: WindowStartPosition::Center,
            remember_size: true,
            remember_position: true,
            ui_theme: WindowUiTheme::Dark,
            pane_side: PaneSide::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaneSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowUiTheme {
    System,
    Light,
    Dark,
}

#[derive(Clone)]
pub enum WindowSize {
    Relative(f32),
    Exact { width: f32, height: f32 },
}

#[derive(Clone)]
pub enum WindowStartPosition {
    Center,
    Exact { x: f32, y: f32 },
}

#[derive(Clone, PartialEq, Eq)]
pub enum ZoomOption {
    None,
    FitWidth,
    FitHeight,
    FitScreen,
    FitScreenIncludeSmaller,
    FitScreenOnlySmaller,
}
