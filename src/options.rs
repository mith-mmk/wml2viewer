/*!
! prelude options
*/

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub use crate::configs::resourses::{FontSizePreset, ResourceOptions};
pub use crate::dependent::plugins::PluginConfig;
pub use crate::ui::viewer::options::{
    BackgroundStyle, MangaSeparatorOptions, MangaSeparatorStyle, PaneSide, RenderOptions,
    RenderScaleMode, ViewerOptions, WindowOptions, WindowSize, WindowStartPosition, WindowUiTheme,
    ZoomOption,
};

#[derive(Clone, Default)]
pub struct AppConfig {
    pub viewer: ViewerOptions,
    pub window: WindowOptions,
    pub render: RenderOptions,
    pub resources: ResourceOptions,
    pub plugins: PluginConfig,
    pub storage: StorageOptions,
    pub input: InputOptions,
    pub navigation: NavigationOptions,
    pub runtime: RuntimeOptions,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewerAction {
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ZoomToggle,
    ToggleFullscreen,
    Reload,
    NextImage,
    PrevImage,
    FirstImage,
    LastImage,
    ToggleAnimation,
    ToggleGrayscale,
    ToggleMangaMode,
    ToggleSettings,
    ToggleFiler,
    ToggleSubfiler,
    SaveAs,
}

impl ViewerAction {
    pub const fn all() -> &'static [ViewerAction] {
        &[
            ViewerAction::ZoomIn,
            ViewerAction::ZoomOut,
            ViewerAction::ZoomReset,
            ViewerAction::ZoomToggle,
            ViewerAction::ToggleFullscreen,
            ViewerAction::Reload,
            ViewerAction::NextImage,
            ViewerAction::PrevImage,
            ViewerAction::FirstImage,
            ViewerAction::LastImage,
            ViewerAction::ToggleAnimation,
            ViewerAction::ToggleGrayscale,
            ViewerAction::ToggleMangaMode,
            ViewerAction::ToggleSettings,
            ViewerAction::ToggleFiler,
            ViewerAction::ToggleSubfiler,
            ViewerAction::SaveAs,
        ]
    }

    pub const fn name(self) -> &'static str {
        match self {
            ViewerAction::ZoomIn => "ZoomIn",
            ViewerAction::ZoomOut => "ZoomOut",
            ViewerAction::ZoomReset => "ZoomReset",
            ViewerAction::ZoomToggle => "ZoomToggle",
            ViewerAction::ToggleFullscreen => "ToggleFullscreen",
            ViewerAction::Reload => "Reload",
            ViewerAction::NextImage => "NextImage",
            ViewerAction::PrevImage => "PrevImage",
            ViewerAction::FirstImage => "FirstImage",
            ViewerAction::LastImage => "LastImage",
            ViewerAction::ToggleAnimation => "ToggleAnimation",
            ViewerAction::ToggleGrayscale => "ToggleGrayscale",
            ViewerAction::ToggleMangaMode => "ToggleMangaMode",
            ViewerAction::ToggleSettings => "ToggleSettings",
            ViewerAction::ToggleFiler => "ToggleFiler",
            ViewerAction::ToggleSubfiler => "ToggleSubfiler",
            ViewerAction::SaveAs => "SaveAs",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::all()
            .iter()
            .copied()
            .find(|action| action.name().eq_ignore_ascii_case(name.trim()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl KeyBinding {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            shift: false,
            ctrl: false,
            alt: false,
        }
    }

    pub fn with_shift(mut self) -> Self {
        self.shift = true;
        self
    }
}

#[derive(Clone, Default)]
pub struct InputOptions {
    pub key_mapping: HashMap<KeyBinding, ViewerAction>,
    pub replace_default_keymap: bool,
}

impl InputOptions {
    pub fn merged_with_defaults(&self) -> HashMap<KeyBinding, ViewerAction> {
        let mut map = if self.replace_default_keymap {
            HashMap::new()
        } else {
            default_key_mapping()
        };
        for (binding, action) in &self.key_mapping {
            map.insert(binding.clone(), action.clone());
        }
        map
    }
}

pub fn default_key_mapping() -> HashMap<KeyBinding, ViewerAction> {
    let mut map = HashMap::new();
    map.insert(KeyBinding::new("Plus"), ViewerAction::ZoomIn);
    map.insert(KeyBinding::new("Minus"), ViewerAction::ZoomOut);
    map.insert(
        KeyBinding::new("Num0").with_shift(),
        ViewerAction::ZoomReset,
    );
    map.insert(KeyBinding::new("Enter"), ViewerAction::ToggleFullscreen);
    map.insert(KeyBinding::new("R").with_shift(), ViewerAction::Reload);
    map.insert(KeyBinding::new("F5"), ViewerAction::Reload);
    map.insert(KeyBinding::new("Space"), ViewerAction::NextImage);
    map.insert(KeyBinding::new("ArrowRight"), ViewerAction::NextImage);
    map.insert(
        KeyBinding::new("Space").with_shift(),
        ViewerAction::PrevImage,
    );
    map.insert(KeyBinding::new("ArrowLeft"), ViewerAction::PrevImage);
    map.insert(KeyBinding::new("Home"), ViewerAction::FirstImage);
    map.insert(KeyBinding::new("End"), ViewerAction::LastImage);
    map.insert(
        KeyBinding::new("G").with_shift(),
        ViewerAction::ToggleGrayscale,
    );
    map.insert(
        KeyBinding::new("C").with_shift(),
        ViewerAction::ToggleMangaMode,
    );
    map.insert(
        KeyBinding::new("V").with_shift(),
        ViewerAction::ToggleSubfiler,
    );
    map.insert(KeyBinding::new("F"), ViewerAction::ToggleFiler);
    map.insert(KeyBinding::new("P"), ViewerAction::ToggleSettings);
    map
}

#[cfg(test)]
mod tests {
    use super::{InputOptions, KeyBinding, ViewerAction, default_key_mapping};
    use std::collections::HashMap;

    #[test]
    fn default_key_mapping_includes_f5_reload() {
        let map = default_key_mapping();

        assert_eq!(map.get(&KeyBinding::new("F5")), Some(&ViewerAction::Reload));
    }

    #[test]
    fn replace_default_keymap_uses_only_custom_bindings() {
        let mut key_mapping = HashMap::new();
        key_mapping.insert(KeyBinding::new("Space"), ViewerAction::PrevImage);
        let options = InputOptions {
            key_mapping,
            replace_default_keymap: true,
        };

        let merged = options.merged_with_defaults();
        assert_eq!(merged.len(), 1);
        assert_eq!(
            merged.get(&KeyBinding::new("Space")),
            Some(&ViewerAction::PrevImage)
        );
        assert!(!merged.contains_key(&KeyBinding::new("ArrowRight")));
    }

    #[test]
    fn viewer_action_from_name_is_case_insensitive() {
        assert_eq!(
            ViewerAction::from_name("togglefiler"),
            Some(ViewerAction::ToggleFiler)
        );
        assert!(ViewerAction::from_name("unknown").is_none());
    }
}

#[derive(Clone, Default)]
pub struct StorageOptions {
    pub path_record: bool,
    pub path: Option<PathBuf>,
}

#[derive(Clone)]
pub struct RuntimeOptions {
    pub workaround: WorkaroundOptions,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            workaround: WorkaroundOptions::default(),
        }
    }
}

#[derive(Clone)]
pub struct WorkaroundOptions {
    pub archive: ArchiveWorkaroundOptions,
    pub thumbnail: ThumbnailWorkaroundOptions,
}

impl Default for WorkaroundOptions {
    fn default() -> Self {
        Self {
            archive: ArchiveWorkaroundOptions::default(),
            thumbnail: ThumbnailWorkaroundOptions::default(),
        }
    }
}

#[derive(Clone)]
pub struct ArchiveWorkaroundOptions {
    pub zip: ZipWorkaroundOptions,
}

impl Default for ArchiveWorkaroundOptions {
    fn default() -> Self {
        Self {
            zip: ZipWorkaroundOptions::default(),
        }
    }
}

#[derive(Clone)]
pub struct ZipWorkaroundOptions {
    pub threshold_mb: u64,
    pub local_cache: bool,
}

impl Default for ZipWorkaroundOptions {
    fn default() -> Self {
        Self {
            threshold_mb: 16,
            local_cache: true,
        }
    }
}

#[derive(Clone)]
pub struct ThumbnailWorkaroundOptions {
    pub suppress_large_files: bool,
}

impl Default for ThumbnailWorkaroundOptions {
    fn default() -> Self {
        Self {
            suppress_large_files: true,
        }
    }
}

#[derive(Clone)]
pub struct NavigationOptions {
    pub end_of_folder: EndOfFolderOption,
    pub sort: NavigationSortOption,
}

impl Default for NavigationOptions {
    fn default() -> Self {
        Self {
            end_of_folder: EndOfFolderOption::Recursive,
            sort: NavigationSortOption::OsName,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndOfFolderOption {
    Stop,
    Next,
    Loop,
    Recursive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavigationSortOption {
    OsName,
    Name,
    NameCaseSensitive,
    NameCaseInsensitive,
    Date,
    Size,
}
