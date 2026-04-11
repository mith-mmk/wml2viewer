use crate::dependent::ui_available_roots;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Clone, Debug, Default)]
pub(crate) struct FilerMetadata {
    pub(crate) size: Option<u64>,
    pub(crate) modified: Option<SystemTime>,
}

#[derive(Clone, Debug)]
pub(crate) struct FilerEntry {
    pub(crate) path: PathBuf,
    pub(crate) label: String,
    pub(crate) is_container: bool,
    pub(crate) sort_as_container: bool,
    pub(crate) metadata: FilerMetadata,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum FilerViewMode {
    #[default]
    List,
    ThumbnailSmall,
    ThumbnailMedium,
    ThumbnailLarge,
    Detail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FilerSortField {
    Name,
    Modified,
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NameSortMode {
    Os,
    CaseSensitive,
    CaseInsensitive,
}

#[derive(Debug)]
pub(crate) struct FilerState {
    pub(crate) entries: Vec<FilerEntry>,
    pub(crate) directory: Option<PathBuf>,
    pub(crate) selected: Option<PathBuf>,
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) pending_request_id: Option<u64>,
    pub(crate) view_mode: FilerViewMode,
    pub(crate) sort_field: FilerSortField,
    pub(crate) ascending: bool,
    pub(crate) separate_dirs: bool,
    pub(crate) archive_as_container_in_sort: bool,
    pub(crate) filter_text: String,
    pub(crate) extension_filter: String,
    pub(crate) name_sort_mode: NameSortMode,
    pub(crate) url_input: String,
    pub(crate) thumbnail_scale: f32,
}

impl Default for FilerState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            directory: None,
            selected: None,
            roots: ui_available_roots(),
            pending_request_id: None,
            view_mode: FilerViewMode::List,
            sort_field: FilerSortField::Name,
            ascending: true,
            separate_dirs: true,
            archive_as_container_in_sort: false,
            filter_text: String::new(),
            extension_filter: String::new(),
            name_sort_mode: NameSortMode::Os,
            url_input: String::new(),
            thumbnail_scale: 1.0,
        }
    }
}
