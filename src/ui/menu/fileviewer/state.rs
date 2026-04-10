use crate::dependent::ui_available_roots;
pub(crate) use crate::filesystem::{
    BrowserEntry as FilerEntry, BrowserNameSortMode as NameSortMode, BrowserScanOptions,
    BrowserSnapshotState as FilerSnapshotState, BrowserSortField as FilerSortField,
};
use crate::options::{ArchiveBrowseOption, NavigationSortOption};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum FilerViewMode {
    #[default]
    List,
    ThumbnailSmall,
    ThumbnailMedium,
    ThumbnailLarge,
    Detail,
}

#[derive(Debug)]
pub(crate) struct FilerState {
    pub(crate) snapshot: FilerSnapshotState,
    pub(crate) roots: Vec<PathBuf>,
    pub(crate) query_options_dirty: bool,
    pub(crate) view_mode: FilerViewMode,
    pub(crate) sort_field: FilerSortField,
    pub(crate) ascending: bool,
    pub(crate) separate_dirs: bool,
    pub(crate) archive_mode: ArchiveBrowseOption,
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
            snapshot: FilerSnapshotState::default(),
            roots: ui_available_roots(),
            query_options_dirty: true,
            view_mode: FilerViewMode::List,
            sort_field: FilerSortField::Name,
            ascending: true,
            separate_dirs: true,
            archive_mode: ArchiveBrowseOption::Folder,
            archive_as_container_in_sort: false,
            filter_text: String::new(),
            extension_filter: String::new(),
            name_sort_mode: NameSortMode::Os,
            url_input: String::new(),
            thumbnail_scale: 1.0,
        }
    }
}

impl FilerState {
    pub(crate) fn mark_query_options_dirty(&mut self) {
        self.query_options_dirty = true;
    }

    pub(crate) fn take_browser_scan_options(
        &mut self,
        navigation_sort: NavigationSortOption,
    ) -> Option<BrowserScanOptions> {
        if !self.query_options_dirty {
            return None;
        }
        self.query_options_dirty = false;
        Some(self.browser_scan_options(navigation_sort))
    }

    pub(crate) fn browser_scan_options(
        &self,
        navigation_sort: NavigationSortOption,
    ) -> BrowserScanOptions {
        BrowserScanOptions {
            navigation_sort,
            archive_mode: self.archive_mode,
            sort_field: self.sort_field,
            include_metadata: self.view_mode == FilerViewMode::Detail
                || self.sort_field != FilerSortField::Name,
            ascending: self.ascending,
            separate_dirs: self.separate_dirs,
            archive_as_container_in_sort: self.archive_as_container_in_sort,
            filter_text: self.filter_text.clone(),
            extension_filter: self.extension_filter.clone(),
            name_sort_mode: self.name_sort_mode,
            thumbnail_hint_count: self.thumbnail_hint_count(),
            thumbnail_hint_max_side: self.thumbnail_hint_max_side(),
        }
    }

    fn thumbnail_hint_count(&self) -> usize {
        match self.view_mode {
            FilerViewMode::List | FilerViewMode::Detail => 0,
            FilerViewMode::ThumbnailSmall => 24,
            FilerViewMode::ThumbnailMedium => 18,
            FilerViewMode::ThumbnailLarge => 12,
        }
    }

    fn thumbnail_hint_max_side(&self) -> u32 {
        let base = match self.view_mode {
            FilerViewMode::List | FilerViewMode::Detail => return 0,
            FilerViewMode::ThumbnailSmall => 72.0,
            FilerViewMode::ThumbnailMedium => 112.0,
            FilerViewMode::ThumbnailLarge => 160.0,
        };
        (base * self.thumbnail_scale.clamp(0.75, 2.5)).round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_scan_options_follow_filer_state() {
        let state = FilerState {
            sort_field: FilerSortField::Modified,
            ascending: false,
            separate_dirs: false,
            archive_mode: ArchiveBrowseOption::Archiver,
            archive_as_container_in_sort: true,
            filter_text: "cover".to_string(),
            extension_filter: "png".to_string(),
            name_sort_mode: NameSortMode::CaseInsensitive,
            ..Default::default()
        };

        let options = state.browser_scan_options(NavigationSortOption::Date);

        assert_eq!(options.navigation_sort, NavigationSortOption::Date);
        assert_eq!(options.archive_mode, ArchiveBrowseOption::Archiver);
        assert_eq!(options.sort_field, FilerSortField::Modified);
        assert!(options.include_metadata);
        assert!(!options.ascending);
        assert!(!options.separate_dirs);
        assert!(options.archive_as_container_in_sort);
        assert_eq!(options.filter_text, "cover");
        assert_eq!(options.extension_filter, "png");
        assert_eq!(options.name_sort_mode, NameSortMode::CaseInsensitive);
        assert_eq!(options.thumbnail_hint_count, 0);
        assert_eq!(options.thumbnail_hint_max_side, 0);
    }

    #[test]
    fn thumbnail_mode_enables_thumbnail_hints() {
        let state = FilerState {
            view_mode: FilerViewMode::ThumbnailMedium,
            thumbnail_scale: 1.5,
            ..Default::default()
        };

        let options = state.browser_scan_options(NavigationSortOption::OsName);

        assert_eq!(options.thumbnail_hint_count, 18);
        assert_eq!(options.thumbnail_hint_max_side, 168);
        assert!(!options.include_metadata);
    }
}
