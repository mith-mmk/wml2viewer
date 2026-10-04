mod icons;
pub(crate) mod state;
pub(crate) mod thumbnail;
pub(crate) mod worker;

use crate::dependent::{download_http_url, normalize_locale_tag};
use crate::drawers::image::SaveFormat;
use crate::filesystem::resolve_start_path;
use crate::ui::i18n::UiTextKey;
use crate::ui::menu::fileviewer::icons::{SvgIcon, paint_svg_icon};
use crate::ui::menu::fileviewer::state::{
    FilerEntry, FilerSortField, FilerUserRequest, FilerViewMode, NameSortMode,
};
use crate::ui::menu::style;
use crate::ui::viewer::ViewerApp;
use crate::ui::viewer::options::PaneSide;
use chrono::{DateTime, Local};
use eframe::egui;
use std::time::SystemTime;
use wml2::draw::image_from_file;
use wml2::metadata::exif::parse_exif;
use wml2::metadata::{DataMap, Metadata};
use wml2::tiff::header::{DataPack, TiffHeader};
use wml2::tiff::tags::{gps_mapper, tag_mapper};

const FILER_REGULAR_MIN_WIDTH: f32 = 240.0;
const FILER_COMPACT_MIN_WIDTH: f32 = 144.0;
const FILER_VIEWER_RESERVED_WIDTH: f32 = 280.0;
const FILER_REGULAR_MAX_WIDTH: f32 = 420.0;
const SUBFILER_REGULAR_MIN_HEIGHT: f32 = 88.0;
const SUBFILER_COMPACT_MIN_HEIGHT: f32 = 72.0;
const SUBFILER_DEFAULT_HEIGHT: f32 = 110.0;
const CASCADE_MENU_WIDTH: f32 = 240.0;
const CASCADE_MENU_GAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CascadeMenuSection {
    File,
    SaveImage,
    View,
    Zoom,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CascadeMenuState {
    pub(crate) levels: Vec<CascadeMenuSection>,
    pub(crate) focused_item: usize,
}

impl CascadeMenuState {
    pub(crate) fn close(&mut self) {
        self.levels.clear();
        self.focused_item = 0;
    }

    fn is_open(&self, section: CascadeMenuSection) -> bool {
        self.levels.contains(&section)
    }

    fn open_root(&mut self, section: CascadeMenuSection) {
        self.levels.clear();
        self.levels.push(section);
        self.focused_item = 0;
    }

    fn toggle_zoom(&mut self) {
        if self.levels.as_slice() == [CascadeMenuSection::View, CascadeMenuSection::Zoom] {
            self.levels.pop();
        } else {
            self.levels = vec![CascadeMenuSection::View, CascadeMenuSection::Zoom];
        }
        self.focused_item = 0;
    }

    fn move_focus(&mut self, row_count: usize, forward: bool) {
        if forward {
            self.focused_item = (self.focused_item + 1) % row_count;
        } else {
            self.focused_item = (self.focused_item + row_count - 1) % row_count;
        }
    }

    fn close_current_level(&mut self) -> bool {
        let closed = self.levels.pop().is_some();
        if closed {
            self.focused_item = 0;
        }
        closed
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PanelSizeRange {
    default: f32,
    min: f32,
    max: f32,
}

impl ViewerApp {
    pub(crate) fn dismiss_cascade_menu(&mut self) {
        self.cascade_menu.close();
        self.show_left_menu = false;
        self.suppress_next_pointer_intent = true;
    }

    pub(crate) fn handle_cascade_menu_keyboard(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.dismiss_cascade_menu();
            return;
        }

        let row_count = self.cascade_menu_row_count();
        if ctx.input(|input| input.key_pressed(egui::Key::ArrowUp)) {
            self.cascade_menu.move_focus(row_count, false);
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::ArrowDown)) {
            self.cascade_menu.move_focus(row_count, true);
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::ArrowLeft)) {
            if !self.cascade_menu.close_current_level() {
                self.dismiss_cascade_menu();
            }
            return;
        }
        if ctx.input(|input| input.key_pressed(egui::Key::ArrowRight)) {
            match self.cascade_menu.levels.as_slice() {
                [] => match self.cascade_menu.focused_item {
                    0 => self.cascade_menu.open_root(CascadeMenuSection::File),
                    1 => self.cascade_menu.open_root(CascadeMenuSection::SaveImage),
                    2 => self.cascade_menu.open_root(CascadeMenuSection::View),
                    _ => {}
                },
                [CascadeMenuSection::View] if self.cascade_menu.focused_item == 0 => {
                    self.cascade_menu.toggle_zoom();
                }
                _ => {}
            }
            return;
        }
        if !ctx.input(|input| input.key_pressed(egui::Key::Enter)) {
            return;
        }

        match self.cascade_menu.levels.as_slice() {
            [] => match self.cascade_menu.focused_item {
                0 => self.cascade_menu.open_root(CascadeMenuSection::File),
                1 => self.cascade_menu.open_root(CascadeMenuSection::SaveImage),
                2 => self.cascade_menu.open_root(CascadeMenuSection::View),
                3 => {
                    self.open_dialog_with_title_key(
                        UiTextKey::MenuInfoSection,
                        self.current_image_info_text(),
                    );
                    self.dismiss_cascade_menu();
                }
                4 => {
                    self.apply_viewer_action(ctx, crate::options::ViewerAction::ToggleSettings);
                    self.dismiss_cascade_menu();
                }
                5 => {
                    self.open_dialog_with_title_key(UiTextKey::MenuAboutSection, self.about_text());
                    self.dismiss_cascade_menu();
                }
                _ => {}
            },
            [CascadeMenuSection::File] => {
                match self.cascade_menu.focused_item {
                    0 => {
                        let _ = self.reload_current();
                    }
                    1 => self.apply_viewer_action(ctx, crate::options::ViewerAction::MoveFile),
                    2 => self.apply_viewer_action(ctx, crate::options::ViewerAction::CopyFile),
                    3 => self.apply_viewer_action(ctx, crate::options::ViewerAction::RenameFile),
                    4 => self.apply_viewer_action(ctx, crate::options::ViewerAction::DeleteFile),
                    _ => return,
                }
                self.dismiss_cascade_menu();
            }
            [CascadeMenuSection::SaveImage] => {
                if let Some(format) = SaveFormat::all()
                    .get(self.cascade_menu.focused_item)
                    .cloned()
                {
                    self.save_dialog.format = format;
                    self.open_save_dialog();
                    self.dismiss_cascade_menu();
                }
            }
            [CascadeMenuSection::View] => match self.cascade_menu.focused_item {
                0 => self.cascade_menu.toggle_zoom(),
                1 => {
                    self.apply_viewer_action(ctx, crate::options::ViewerAction::ToggleMangaMode);
                    self.dismiss_cascade_menu();
                }
                _ => {}
            },
            [CascadeMenuSection::View, CascadeMenuSection::Zoom] => {
                match self.cascade_menu.focused_item {
                    0 => self.apply_viewer_action(ctx, crate::options::ViewerAction::ZoomIn),
                    1 => self.apply_viewer_action(ctx, crate::options::ViewerAction::ZoomOut),
                    2 => self.apply_viewer_action(ctx, crate::options::ViewerAction::ZoomReset),
                    preset => {
                        if let Some(scale) = [50_u32, 75, 100, 125, 150, 200].get(preset - 3) {
                            let _ = self.set_zoom(*scale as f32 / 100.0);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn cascade_menu_row_count(&self) -> usize {
        match self.cascade_menu.levels.as_slice() {
            [] => 6,
            [CascadeMenuSection::File] => 5,
            [CascadeMenuSection::SaveImage] => SaveFormat::all().len(),
            [CascadeMenuSection::View] => 2,
            [CascadeMenuSection::View, CascadeMenuSection::Zoom] => 9,
            _ => 1,
        }
    }

    fn cascade_menu_item_focused(&self, section: Option<CascadeMenuSection>, index: usize) -> bool {
        match section {
            Some(section) => {
                self.cascade_menu.levels.last() == Some(&section)
                    && self.cascade_menu.focused_item == index
            }
            None => self.cascade_menu.levels.is_empty() && self.cascade_menu.focused_item == index,
        }
    }

    pub(crate) fn left_click_menu_ui(&mut self, ctx: &egui::Context) {
        if !self.show_left_menu {
            return;
        }
        self.cancel_pending_single_click_navigation();

        let content_rect = ctx.content_rect();
        let root_pos = clamp_popup_position(self.left_menu_pos, content_rect, cascade_menu_size(6));
        let mut close_requested = false;
        let mut menu_rects = Vec::new();
        let root = egui::Area::new("context_menu_root".into())
            .order(egui::Order::Foreground)
            .fixed_pos(root_pos)
            .show(ctx, |ui| {
                style::menu_frame(ctx).show(ui, |ui| {
                    ui.set_width(CASCADE_MENU_WIDTH);
                    if style::menu_row(
                        ui,
                        self.text(UiTextKey::MenuFileSection),
                        Some("›"),
                        self.cascade_menu.is_open(CascadeMenuSection::File)
                            || self.cascade_menu_item_focused(None, 0),
                    )
                    .clicked()
                    {
                        self.cascade_menu.open_root(CascadeMenuSection::File);
                    }
                    if style::menu_row(
                        ui,
                        self.text(UiTextKey::MenuImageSection),
                        Some("›"),
                        self.cascade_menu.is_open(CascadeMenuSection::SaveImage)
                            || self.cascade_menu_item_focused(None, 1),
                    )
                    .clicked()
                    {
                        self.cascade_menu.open_root(CascadeMenuSection::SaveImage);
                    }
                    if style::menu_row(
                        ui,
                        self.text(UiTextKey::MenuViewSection),
                        Some("›"),
                        self.cascade_menu.is_open(CascadeMenuSection::View)
                            || self.cascade_menu_item_focused(None, 2),
                    )
                    .clicked()
                    {
                        self.cascade_menu.open_root(CascadeMenuSection::View);
                    }
                    if style::menu_row(
                        ui,
                        self.text(UiTextKey::ImageInformation),
                        None,
                        self.cascade_menu_item_focused(None, 3),
                    )
                    .clicked()
                    {
                        self.open_dialog_with_title_key(
                            UiTextKey::MenuInfoSection,
                            self.current_image_info_text(),
                        );
                        close_requested = true;
                    }
                    if style::menu_row(
                        ui,
                        self.text(UiTextKey::ToggleSettings),
                        None,
                        self.cascade_menu_item_focused(None, 4),
                    )
                    .clicked()
                    {
                        self.apply_viewer_action(ctx, crate::options::ViewerAction::ToggleSettings);
                        close_requested = true;
                    }
                    if style::menu_row(
                        ui,
                        self.text(UiTextKey::MenuAboutSection),
                        None,
                        self.cascade_menu_item_focused(None, 5),
                    )
                    .clicked()
                    {
                        self.open_dialog_with_title_key(
                            UiTextKey::MenuAboutSection,
                            self.about_text(),
                        );
                        close_requested = true;
                    }
                })
            });
        menu_rects.push(root.response.rect);

        let mut child_rect = None;
        match self.cascade_menu.levels.first().copied() {
            Some(CascadeMenuSection::File) => {
                let area = egui::Area::new("context_menu_file".into())
                    .order(egui::Order::Foreground)
                    .fixed_pos(cascade_child_position(root.response.rect, content_rect, 5))
                    .show(ctx, |ui| {
                        style::menu_frame(ctx).show(ui, |ui| {
                            ui.set_width(CASCADE_MENU_WIDTH);
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::ReloadCurrent),
                                None,
                                self.cascade_menu_item_focused(Some(CascadeMenuSection::File), 0),
                            )
                            .clicked()
                            {
                                let _ = self.reload_current();
                                close_requested = true;
                            }
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::MoveItem),
                                None,
                                self.cascade_menu_item_focused(Some(CascadeMenuSection::File), 1),
                            )
                            .clicked()
                            {
                                self.apply_viewer_action(
                                    ctx,
                                    crate::options::ViewerAction::MoveFile,
                                );
                                close_requested = true;
                            }
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::CopyItem),
                                None,
                                self.cascade_menu_item_focused(Some(CascadeMenuSection::File), 2),
                            )
                            .clicked()
                            {
                                self.apply_viewer_action(
                                    ctx,
                                    crate::options::ViewerAction::CopyFile,
                                );
                                close_requested = true;
                            }
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::RenameItem),
                                None,
                                self.cascade_menu_item_focused(Some(CascadeMenuSection::File), 3),
                            )
                            .clicked()
                            {
                                self.apply_viewer_action(
                                    ctx,
                                    crate::options::ViewerAction::RenameFile,
                                );
                                close_requested = true;
                            }
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::DeleteItem),
                                None,
                                self.cascade_menu_item_focused(Some(CascadeMenuSection::File), 4),
                            )
                            .clicked()
                            {
                                self.apply_viewer_action(
                                    ctx,
                                    crate::options::ViewerAction::DeleteFile,
                                );
                                close_requested = true;
                            }
                        })
                    });
                child_rect = Some(area.response.rect);
            }
            Some(CascadeMenuSection::SaveImage) => {
                let formats = SaveFormat::all();
                let area = egui::Area::new("context_menu_save".into())
                    .order(egui::Order::Foreground)
                    .fixed_pos(cascade_child_position(
                        root.response.rect,
                        content_rect,
                        formats.len(),
                    ))
                    .show(ctx, |ui| {
                        style::menu_frame(ctx).show(ui, |ui| {
                            ui.set_width(CASCADE_MENU_WIDTH);
                            for (index, format) in formats.into_iter().enumerate() {
                                if style::menu_row(
                                    ui,
                                    &format.to_string(),
                                    None,
                                    self.cascade_menu_item_focused(
                                        Some(CascadeMenuSection::SaveImage),
                                        index,
                                    ),
                                )
                                .clicked()
                                {
                                    self.save_dialog.format = format;
                                    self.open_save_dialog();
                                    close_requested = true;
                                }
                            }
                        })
                    });
                child_rect = Some(area.response.rect);
            }
            Some(CascadeMenuSection::View) => {
                let area = egui::Area::new("context_menu_view".into())
                    .order(egui::Order::Foreground)
                    .fixed_pos(cascade_child_position(root.response.rect, content_rect, 2))
                    .show(ctx, |ui| {
                        style::menu_frame(ctx).show(ui, |ui| {
                            ui.set_width(CASCADE_MENU_WIDTH);
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::ZoomPresetAction),
                                Some("›"),
                                self.cascade_menu.is_open(CascadeMenuSection::Zoom)
                                    || self.cascade_menu_item_focused(
                                        Some(CascadeMenuSection::View),
                                        0,
                                    ),
                            )
                            .clicked()
                            {
                                self.cascade_menu.toggle_zoom();
                            }
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::ToggleManga),
                                None,
                                self.options.manga_mode
                                    || self.cascade_menu_item_focused(
                                        Some(CascadeMenuSection::View),
                                        1,
                                    ),
                            )
                            .clicked()
                            {
                                self.apply_viewer_action(
                                    ctx,
                                    crate::options::ViewerAction::ToggleMangaMode,
                                );
                                close_requested = true;
                            }
                        })
                    });
                child_rect = Some(area.response.rect);
            }
            _ => {}
        }
        if let Some(rect) = child_rect {
            menu_rects.push(rect);
        }

        if self.cascade_menu.levels.as_slice()
            == [CascadeMenuSection::View, CascadeMenuSection::Zoom]
        {
            if let Some(parent) = child_rect {
                let area = egui::Area::new("context_menu_zoom".into())
                    .order(egui::Order::Foreground)
                    .fixed_pos(cascade_child_position(parent, content_rect, 9))
                    .show(ctx, |ui| {
                        style::menu_frame(ctx).show(ui, |ui| {
                            ui.set_width(CASCADE_MENU_WIDTH);
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::ZoomInAction),
                                None,
                                self.cascade_menu_item_focused(Some(CascadeMenuSection::Zoom), 0),
                            )
                            .clicked()
                            {
                                self.apply_viewer_action(ctx, crate::options::ViewerAction::ZoomIn);
                            }
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::ZoomOutAction),
                                None,
                                self.cascade_menu_item_focused(Some(CascadeMenuSection::Zoom), 1),
                            )
                            .clicked()
                            {
                                self.apply_viewer_action(
                                    ctx,
                                    crate::options::ViewerAction::ZoomOut,
                                );
                            }
                            if style::menu_row(
                                ui,
                                self.text(UiTextKey::ZoomResetAction),
                                None,
                                (self.zoom - 1.0).abs() < f32::EPSILON
                                    || self.cascade_menu_item_focused(
                                        Some(CascadeMenuSection::Zoom),
                                        2,
                                    ),
                            )
                            .clicked()
                            {
                                self.apply_viewer_action(
                                    ctx,
                                    crate::options::ViewerAction::ZoomReset,
                                );
                            }
                            for (index, scale) in
                                [50_u32, 75, 100, 125, 150, 200].into_iter().enumerate()
                            {
                                let zoom = scale as f32 / 100.0;
                                let current = (self.zoom - zoom).abs() < 0.01;
                                let selected = current
                                    || self.cascade_menu_item_focused(
                                        Some(CascadeMenuSection::Zoom),
                                        index + 3,
                                    );
                                if style::menu_row(
                                    ui,
                                    &format!("{scale}%"),
                                    current.then_some("✓"),
                                    selected,
                                )
                                .clicked()
                                {
                                    let _ = self.set_zoom(zoom);
                                }
                            }
                        })
                    });
                menu_rects.push(area.response.rect);
            }
        }

        let pointer_clicked_outside = ctx.input(|i| {
            i.pointer.any_click()
                && i.pointer
                    .interact_pos()
                    .is_some_and(|pos| !menu_rects.iter().any(|rect| rect.contains(pos)))
        });
        close_requested |= pointer_clicked_outside;
        if close_requested {
            self.dismiss_cascade_menu();
        }
    }

    fn current_image_info_text(&self) -> String {
        let mut lines = Vec::new();
        lines.push(self.text(UiTextKey::ImageInformation).to_string());
        lines.push(String::new());
        lines.push("[Basic]".to_string());
        lines.push(format!(
            "Path: {}",
            normalize_backslash_display(&self.current_path.to_string_lossy())
        ));
        lines.push(format!("Format: {}", file_format_label(&self.current_path)));
        lines.push(format!(
            "Resolution: {} x {}",
            self.source.canvas.width(),
            self.source.canvas.height()
        ));
        lines.push(format!("Frames: {}", self.source.frame_count()));
        if let Ok(meta) = std::fs::metadata(&self.current_path) {
            lines.push(String::new());
            lines.push("[File]".to_string());
            lines.push(format!("Size: {}", format_human_size(meta.len())));
            if let Ok(created) = meta.created() {
                lines.push(format!(
                    "Created: {}",
                    format_system_time(created, &self.applied_locale)
                ));
            }
            if let Ok(modified) = meta.modified() {
                lines.push(format!(
                    "Modified: {}",
                    format_system_time(modified, &self.applied_locale)
                ));
            }
        }
        lines.push(String::new());
        lines.push("[Color]".to_string());
        lines.push("Color Profile: (not available)".to_string());

        append_exif_sections(&mut lines, &self.current_path);
        lines.join("\n")
    }

    fn about_text(&self) -> String {
        format!(
            "{}\n{}: {}\n{}: {}\n{}: {}",
            crate::get_program_name(),
            self.text(UiTextKey::Version),
            crate::get_version(),
            self.text(UiTextKey::Author),
            crate::get_author(),
            self.text(UiTextKey::Copyright),
            crate::get_copyright(),
        )
    }

    pub(crate) fn filer_ui(&mut self, ctx: &egui::Context) {
        if !self.show_filer {
            return;
        }

        let content = ctx.content_rect();
        let preferred_width = match self.filer.view_mode {
            FilerViewMode::ThumbnailLarge => 420.0,
            FilerViewMode::ThumbnailMedium => 360.0,
            _ => 300.0,
        };
        let width_range = filer_width_range(content, preferred_width);

        let panel = match self.window_options.pane_side {
            PaneSide::Left => egui::SidePanel::left("filer_panel"),
            PaneSide::Right => egui::SidePanel::right("filer_panel"),
        };

        panel
            .resizable(true)
            .default_width(width_range.default)
            .min_width(width_range.min)
            .max_width(width_range.max)
            .show(ctx, |ui| {
                let mut refresh_requested = false;
                let list_text = self.text(UiTextKey::List);
                let thumb_small_text = self.text(UiTextKey::ThumbnailSmall);
                let thumb_medium_text = self.text(UiTextKey::ThumbnailMedium);
                let thumb_large_text = self.text(UiTextKey::ThumbnailLarge);
                let detail_text = self.text(UiTextKey::Detail);
                let sort_text = self.text(UiTextKey::Sort);
                let name_text = self.text(UiTextKey::Name);
                let name_sort_order_text = self.text(UiTextKey::NameSortOrder);
                let date_text = self.text(UiTextKey::Date);
                let size_text = self.text(UiTextKey::Size);
                let asc_text = self.text(UiTextKey::Asc);
                let desc_text = self.text(UiTextKey::Desc);
                let separate_text = self.text(UiTextKey::Separate);
                let os_text = self.text(UiTextKey::Os);
                let case_text = self.text(UiTextKey::Case);
                let no_case_text = self.text(UiTextKey::NoCase);
                let filter_text = self.text(UiTextKey::Filter);
                let extension_text = self.text(UiTextKey::Extension);
                let url_text = self.text(UiTextKey::Url);
                let open_url_text = self.text(UiTextKey::OpenUrl);
                let up_text = self.text(UiTextKey::Up);
                let icon_color = ui.visuals().text_color();
                ui.heading(self.text(UiTextKey::Filer));
                ui.horizontal_wrapped(|ui| {
                    if icon_toolbar_button(
                        ui,
                        SvgIcon::ThumbnailGrid,
                        self.filer.view_mode == FilerViewMode::List,
                        list_text,
                        icon_color,
                    ) {
                        self.filer.view_mode = FilerViewMode::List;
                        refresh_requested = true;
                    }
                    if icon_toolbar_button(
                        ui,
                        SvgIcon::ThumbnailSmall,
                        self.filer.view_mode == FilerViewMode::ThumbnailSmall,
                        thumb_small_text,
                        icon_color,
                    ) {
                        self.filer.view_mode = FilerViewMode::ThumbnailSmall;
                        refresh_requested = true;
                    }
                    if icon_toolbar_button(
                        ui,
                        SvgIcon::ThumbnailMedium,
                        self.filer.view_mode == FilerViewMode::ThumbnailMedium,
                        thumb_medium_text,
                        icon_color,
                    ) {
                        self.filer.view_mode = FilerViewMode::ThumbnailMedium;
                        refresh_requested = true;
                    }
                    if icon_toolbar_button(
                        ui,
                        SvgIcon::ThumbnailLarge,
                        self.filer.view_mode == FilerViewMode::ThumbnailLarge,
                        thumb_large_text,
                        icon_color,
                    ) {
                        self.filer.view_mode = FilerViewMode::ThumbnailLarge;
                        refresh_requested = true;
                    }
                    if icon_toolbar_button(
                        ui,
                        SvgIcon::Detail,
                        self.filer.view_mode == FilerViewMode::Detail,
                        detail_text,
                        icon_color,
                    ) {
                        self.filer.view_mode = FilerViewMode::Detail;
                        refresh_requested = true;
                    }
                    if matches!(
                        self.filer.view_mode,
                        FilerViewMode::ThumbnailSmall
                            | FilerViewMode::ThumbnailMedium
                            | FilerViewMode::ThumbnailLarge
                    ) {
                        ui.add(
                            egui::Slider::new(&mut self.filer.thumbnail_scale, 0.75..=2.5)
                                .show_value(false)
                                .text("thumb"),
                        );
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(sort_text);
                    if icon_toolbar_button(
                        ui,
                        SvgIcon::Sort,
                        self.filer.sort_field == FilerSortField::Name,
                        name_text,
                        icon_color,
                    ) {
                        self.filer.sort_field = FilerSortField::Name;
                        refresh_requested = true;
                    }
                    if icon_toolbar_button(
                        ui,
                        SvgIcon::SortByDate,
                        self.filer.sort_field == FilerSortField::Modified,
                        date_text,
                        icon_color,
                    ) {
                        self.filer.sort_field = FilerSortField::Modified;
                        refresh_requested = true;
                    }
                    if icon_toolbar_button(
                        ui,
                        SvgIcon::SortBySize,
                        self.filer.sort_field == FilerSortField::Size,
                        size_text,
                        icon_color,
                    ) {
                        self.filer.sort_field = FilerSortField::Size;
                        refresh_requested = true;
                    }
                    ui.add_space(12.0);
                    if icon_toolbar_button(
                        ui,
                        if self.filer.ascending {
                            SvgIcon::SortAsc
                        } else {
                            SvgIcon::SortDesc
                        },
                        false,
                        if self.filer.ascending {
                            asc_text
                        } else {
                            desc_text
                        },
                        icon_color,
                    ) {
                        self.filer.ascending = !self.filer.ascending;
                        refresh_requested = true;
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if simple_toolbar_button(ui, separate_text, self.filer.separate_dirs) {
                        self.filer.separate_dirs = !self.filer.separate_dirs;
                        refresh_requested = true;
                    }
                    ui.label(name_sort_order_text);
                    let selected_name_sort_text = match self.filer.name_sort_mode {
                        NameSortMode::Os => os_text,
                        NameSortMode::CaseSensitive => case_text,
                        NameSortMode::CaseInsensitive => no_case_text,
                    };
                    egui::ComboBox::from_id_salt("filer_name_sort_mode")
                        .selected_text(selected_name_sort_text)
                        .show_ui(ui, |ui| {
                            refresh_requested |= ui
                                .selectable_value(
                                    &mut self.filer.name_sort_mode,
                                    NameSortMode::Os,
                                    os_text,
                                )
                                .changed();
                            refresh_requested |= ui
                                .selectable_value(
                                    &mut self.filer.name_sort_mode,
                                    NameSortMode::CaseSensitive,
                                    case_text,
                                )
                                .changed();
                            refresh_requested |= ui
                                .selectable_value(
                                    &mut self.filer.name_sort_mode,
                                    NameSortMode::CaseInsensitive,
                                    no_case_text,
                                )
                                .changed();
                        });
                });
                ui.horizontal(|ui| {
                    let _ =
                        icon_toolbar_button(ui, SvgIcon::Filter, false, filter_text, icon_color);
                    refresh_requested |= ui
                        .text_edit_singleline(&mut self.filer.filter_text)
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label(extension_text);
                    refresh_requested |= ui
                        .text_edit_singleline(&mut self.filer.extension_filter)
                        .changed();
                });
                ui.horizontal(|ui| {
                    ui.label(url_text);
                    ui.text_edit_singleline(&mut self.filer.url_input);
                    if ui.button(open_url_text).clicked() {
                        if let Some(path) = download_http_url(&self.filer.url_input) {
                            self.empty_mode = false;
                            self.pending_fit_recalc = true;
                            let _ = self.request_load_path(path);
                        }
                    }
                });
                let current_root = self
                    .filer
                    .directory
                    .as_ref()
                    .and_then(|dir| self.filer.roots.iter().find(|root| dir.starts_with(root)))
                    .cloned()
                    .or_else(|| self.filer.roots.first().cloned());
                egui::ComboBox::from_id_salt("filer_roots")
                    .selected_text(
                        current_root
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "(root)".to_string()),
                    )
                    .show_ui(ui, |ui| {
                        for root in self.filer.roots.clone() {
                            if ui
                                .selectable_label(
                                    current_root.as_ref() == Some(&root),
                                    root.display().to_string(),
                                )
                                .clicked()
                            {
                                self.browse_filer_directory(root);
                            }
                        }
                    });
                if let Some(dir) = &self.filer.directory {
                    ui.label(dir.display().to_string());
                    if let Some(parent) = dir.parent() {
                        if icon_toolbar_button(ui, SvgIcon::Up, false, up_text, icon_color) {
                            self.browse_filer_directory(parent.to_path_buf());
                        }
                    }
                }
                ui.separator();
                if refresh_requested {
                    self.sync_navigation_sort_with_filer_sort();
                    self.refresh_current_filer_directory();
                }
                if self.filer.pending_request_id.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(self.text(UiTextKey::Loading));
                    });
                }
                let panel_width = ui.available_width();
                let focus_target = self
                    .filer
                    .pending_request_id
                    .is_none()
                    .then(|| self.pending_filer_focus_path.clone())
                    .flatten();
                let mut focus_consumed = false;
                if matches!(
                    self.filer.view_mode,
                    FilerViewMode::List | FilerViewMode::Detail
                ) {
                    let row_height = ui.spacing().interact_size.y;
                    let row_stride = row_height + ui.spacing().item_spacing.y;
                    let focus_index = focus_target.as_ref().and_then(|path| {
                        self.filer
                            .entries
                            .iter()
                            .position(|entry| &entry.path == path)
                    });
                    let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
                    if let Some(index) = focus_index {
                        scroll = scroll.vertical_scroll_offset(index as f32 * row_stride);
                    }
                    scroll.show_rows(ui, row_height, self.filer.entries.len(), |ui, range| {
                        ui.set_min_width(panel_width.max(160.0));
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        let visible_entries = self.filer.entries[range].to_vec();
                        for entry in visible_entries {
                            ui.push_id(entry.path.clone(), |ui| {
                                self.filer_entry_row(
                                    ui,
                                    entry,
                                    focus_target.as_ref(),
                                    &mut focus_consumed,
                                );
                            });
                        }
                    });
                } else {
                    let item_width = match self.filer.view_mode {
                        FilerViewMode::ThumbnailSmall => 72.0,
                        FilerViewMode::ThumbnailMedium => 112.0,
                        FilerViewMode::ThumbnailLarge => 160.0,
                        _ => 96.0,
                    } * self.filer.thumbnail_scale;
                    let spacing = ui.spacing().item_spacing;
                    let columns = ((panel_width.max(item_width) + spacing.x)
                        / (item_width.max(1.0) + spacing.x))
                        .floor()
                        .max(1.0) as usize;
                    let row_height = item_width + 56.0;
                    let row_count = self.filer.entries.len().div_ceil(columns);
                    let focus_index = focus_target.as_ref().and_then(|path| {
                        self.filer
                            .entries
                            .iter()
                            .position(|entry| &entry.path == path)
                    });
                    let mut scroll = egui::ScrollArea::vertical().auto_shrink([false, false]);
                    if let Some(index) = focus_index {
                        scroll = scroll.vertical_scroll_offset(
                            (index / columns) as f32 * (row_height + spacing.y),
                        );
                    }
                    scroll.show_rows(ui, row_height, row_count, |ui, rows| {
                        ui.set_min_width(panel_width.max(160.0));
                        // Activation can clear the live listing. Snapshot only the visible tiles.
                        let start = (rows.start * columns).min(self.filer.entries.len());
                        let end = (rows.end * columns).min(self.filer.entries.len());
                        let visible_entries = self.filer.entries[start..end].to_vec();
                        for row in visible_entries.chunks(columns) {
                            ui.horizontal(|ui| {
                                for entry in row.iter().cloned() {
                                    ui.push_id(entry.path.clone(), |ui| {
                                        self.filer_thumbnail_tile(
                                            ui,
                                            entry,
                                            item_width,
                                            focus_target.as_ref(),
                                            &mut focus_consumed,
                                        );
                                    });
                                }
                            });
                        }
                    });
                }
                if focus_consumed {
                    self.pending_filer_focus_path = None;
                }
            });
    }

    fn filer_entry_row(
        &mut self,
        ui: &mut egui::Ui,
        entry: FilerEntry,
        focus_target: Option<&std::path::PathBuf>,
        focus_consumed: &mut bool,
    ) {
        let selected = self.filer.selected.as_ref() == Some(&entry.path)
            || self.current_navigation_path == entry.path;
        let text = if self.filer.view_mode == FilerViewMode::Detail {
            let modified = entry
                .metadata
                .modified
                .map(|value| format_system_time(value, &self.applied_locale))
                .unwrap_or_else(|| "-".to_string());
            let size = entry
                .metadata
                .size
                .map(format_human_size)
                .unwrap_or_else(|| "-".to_string());
            format!(
                "{} {}    {}    {}",
                if entry.is_container { "[DIR]" } else { "    " },
                entry.label,
                modified,
                size
            )
        } else {
            entry.label.clone()
        };
        let response = ui.selectable_label(selected, text);
        if !*focus_consumed && focus_target == Some(&entry.path) {
            ui.scroll_to_rect(response.rect, Some(egui::Align::Center));
            *focus_consumed = true;
        }
        if let Some(size) = entry.metadata.size {
            let modified = entry
                .metadata
                .modified
                .map(|value| format!("\n{}", format_system_time(value, &self.applied_locale)))
                .unwrap_or_default();
            response
                .clone()
                .on_hover_text(format!("{size} bytes{modified}"));
        }
        if response.clicked() {
            self.activate_filer_entry(entry);
        }
    }

    fn filer_thumbnail_tile(
        &mut self,
        ui: &mut egui::Ui,
        entry: FilerEntry,
        item_width: f32,
        focus_target: Option<&std::path::PathBuf>,
        focus_consumed: &mut bool,
    ) {
        let entry_label = entry.label.clone();
        let selected = self.filer.selected.as_ref() == Some(&entry.path)
            || self.current_navigation_path == entry.path;
        ui.allocate_ui_with_layout(
            egui::vec2(item_width, item_width + 56.0),
            egui::Layout::top_down(egui::Align::Center),
            |ui| {
                let thumb_side = (item_width - 16.0).max(48.0);
                let thumb_size = egui::vec2(thumb_side, thumb_side);
                let response = if entry.is_container {
                    let icon_side = thumb_side * 0.58;
                    let (rect, response) = ui.allocate_exact_size(thumb_size, egui::Sense::click());
                    if selected {
                        ui.painter().rect_stroke(
                            rect.expand(2.0),
                            8.0,
                            egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                            egui::StrokeKind::Outside,
                        );
                    }
                    paint_svg_icon(
                        ui.painter(),
                        egui::Rect::from_center_size(
                            rect.center(),
                            egui::vec2(icon_side, icon_side),
                        ),
                        if entry.path.is_dir() {
                            SvgIcon::Folder
                        } else {
                            SvgIcon::Archive
                        },
                        ui.visuals().text_color(),
                    );
                    response.on_hover_text(self.text(UiTextKey::FolderArchive))
                } else {
                    self.ensure_thumbnail(&entry.path, thumb_size.x.max(32.0) as u32);
                    if let Some(texture) = self.thumbnail_cache.get(&entry.path) {
                        let response = ui.add(
                            egui::Image::from_texture(texture)
                                .fit_to_exact_size(thumb_size)
                                .sense(egui::Sense::click()),
                        );
                        if selected {
                            ui.painter().rect_stroke(
                                response.rect.expand(2.0),
                                8.0,
                                egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                                egui::StrokeKind::Outside,
                            );
                        }
                        response
                    } else {
                        let response = ui.add_sized(
                            thumb_size,
                            egui::Label::new(self.text(UiTextKey::Loading))
                                .sense(egui::Sense::click()),
                        );
                        if selected {
                            ui.painter().rect_stroke(
                                response.rect.expand(2.0),
                                8.0,
                                egui::Stroke::new(2.0, ui.visuals().selection.stroke.color),
                                egui::StrokeKind::Outside,
                            );
                        }
                        response
                    }
                };
                if response.clicked() {
                    self.activate_filer_entry(entry.clone());
                }
                if !*focus_consumed && focus_target == Some(&entry.path) {
                    ui.scroll_to_rect(response.rect, Some(egui::Align::Center));
                    *focus_consumed = true;
                }
                let label_height = if item_width >= 180.0 { 48.0 } else { 40.0 };
                let label = thumbnail_label(&entry_label, item_width);
                ui.add_sized(
                    [item_width - 8.0, label_height],
                    egui::Label::new(
                        egui::RichText::new(label)
                            .small()
                            .color(ui.visuals().text_color()),
                    )
                    .wrap(),
                );
            },
        );
    }

    pub(crate) fn subfiler_ui(&mut self, ctx: &egui::Context) {
        if !self.show_subfiler {
            return;
        }
        let Some(current_dir) = self.current_directory() else {
            return;
        };
        if self.filer.directory.as_ref() != Some(&current_dir) {
            return;
        }

        let height_range = subfiler_height_range(ctx.content_rect());
        egui::TopBottomPanel::bottom("subfiler_panel")
            .resizable(true)
            .default_height(height_range.default)
            .min_height(height_range.min)
            .max_height(height_range.max)
            .show(ctx, |ui| {
                let mut close_requested = false;
                let focus_target = self
                    .filer
                    .pending_request_id
                    .is_none()
                    .then(|| self.pending_subfiler_focus_path.clone())
                    .flatten();
                let mut focus_consumed = false;
                ui.horizontal(|ui| {
                    ui.label(self.text(UiTextKey::Subfiler));
                    ui.label(if self.options.manga_right_to_left {
                        self.text(UiTextKey::RightToLeft)
                    } else {
                        self.text(UiTextKey::LeftToRight)
                    });
                    if ui.button(self.text(UiTextKey::Close)).clicked() {
                        close_requested = true;
                    }
                });
                let mut file_indices = self
                    .filer
                    .entries
                    .iter()
                    .enumerate()
                    .filter_map(|(index, entry)| (!entry.is_container).then_some(index))
                    .collect::<Vec<_>>();
                if self.options.manga_right_to_left {
                    file_indices.reverse();
                }
                let tile_width = 112.0;
                let stride = tile_width + ui.spacing().item_spacing.x;
                let focus_index = focus_target.as_ref().and_then(|path| {
                    file_indices
                        .iter()
                        .position(|&index| &self.filer.entries[index].path == path)
                });
                let mut scroll = egui::ScrollArea::horizontal();
                if let Some(index) = focus_index {
                    let offset = index as f32 * stride;
                    scroll = scroll.horizontal_scroll_offset(if self.options.manga_right_to_left {
                        (offset - ui.available_width() + stride).max(0.0)
                    } else {
                        offset
                    });
                }
                scroll.show_viewport(ui, |ui, viewport| {
                    ui.set_min_width(file_indices.len() as f32 * stride);
                    let start = ((viewport.min.x / stride).floor() as usize)
                        .saturating_sub(1)
                        .min(file_indices.len());
                    let end = ((viewport.max.x / stride).ceil() as usize + 1)
                        .min(file_indices.len())
                        .max(start);
                    let visible_entries = file_indices[start..end]
                        .iter()
                        .map(|&index| self.filer.entries[index].clone())
                        .collect::<Vec<_>>();
                    ui.horizontal(|ui| {
                        if start > 0 {
                            ui.add_space(start as f32 * stride - ui.spacing().item_spacing.x);
                        }
                        for entry in visible_entries {
                            self.ensure_thumbnail(&entry.path, 72);
                            let selected = self.current_navigation_path == entry.path;
                            ui.push_id(&entry.path, |ui| {
                                ui.allocate_ui_with_layout(
                                    egui::vec2(tile_width, 96.0),
                                    egui::Layout::top_down(egui::Align::Center),
                                    |ui| {
                                        let mut frame = egui::Frame::group(ui.style());
                                        if selected {
                                            frame.stroke = egui::Stroke::new(
                                                2.0,
                                                ui.visuals().selection.stroke.color,
                                            );
                                        }
                                        frame.show(ui, |ui| {
                                            let response = if let Some(texture) =
                                                self.thumbnail_cache.get(&entry.path)
                                            {
                                                ui.add(egui::Button::image(
                                                    egui::Image::from_texture(texture)
                                                        .fit_to_exact_size(egui::vec2(72.0, 72.0)),
                                                ))
                                            } else {
                                                ui.button("...")
                                            };
                                            if focus_target.as_ref() == Some(&entry.path) {
                                                ui.scroll_to_rect(
                                                    response.rect,
                                                    Some(if self.options.manga_right_to_left {
                                                        egui::Align::Max
                                                    } else {
                                                        egui::Align::Min
                                                    }),
                                                );
                                                focus_consumed = true;
                                            }
                                            if response.clicked() {
                                                self.activate_filer_entry(entry.clone());
                                            }
                                        });
                                    },
                                );
                            });
                        }
                    });
                });
                if close_requested {
                    self.set_show_subfiler(false);
                } else if focus_consumed {
                    self.pending_subfiler_focus_path = None;
                }
            });
    }

    fn activate_filer_entry(&mut self, entry: FilerEntry) {
        if entry.is_container {
            self.log_bench_state(
                "viewer.filer.entry_activated",
                serde_json::json!({
                    "kind": "container",
                    "path": entry.path.display().to_string(),
                }),
            );
            self.browse_filer_directory(entry.path);
            return;
        }
        let navigation_path = entry.path.clone();
        let load_path =
            resolve_start_path(&navigation_path).unwrap_or_else(|| navigation_path.clone());
        self.log_bench_state(
            "viewer.filer.entry_activated",
            serde_json::json!({
                "kind": "file",
                "navigation_path": navigation_path.display().to_string(),
                "load_path": load_path.display().to_string(),
            }),
        );
        self.filer.pending_user_request = Some(FilerUserRequest::SelectFile {
            navigation_path: navigation_path.clone(),
        });
        self.accept_filer_selection(&navigation_path);
        self.filer.committed_browse_directory = None;
        self.filer.selected = Some(navigation_path.clone());
        self.empty_mode = false;
        self.set_show_filer(false);
        self.pending_fit_recalc = true;
        if self.show_subfiler {
            self.pending_subfiler_focus_path = Some(navigation_path.clone());
        }
        let _ = self.request_load_target(navigation_path, load_path);
    }

    pub(crate) fn bench_activate_filer_entry(&mut self, entry: FilerEntry) {
        self.activate_filer_entry(entry);
    }
}

fn filer_width_range(content: egui::Rect, preferred: f32) -> PanelSizeRange {
    let width = content.width().max(1.0);
    let compact = width < 520.0;
    let min = if compact {
        (width * 0.5).clamp(FILER_COMPACT_MIN_WIDTH, FILER_REGULAR_MIN_WIDTH)
    } else {
        FILER_REGULAR_MIN_WIDTH
    }
    .min(width);
    let viewer_reserved = if compact {
        (width * 0.5).max(120.0)
    } else {
        FILER_VIEWER_RESERVED_WIDTH
    };
    let design_max = if content.width() >= content.height() * 1.5 {
        (content.width() * 0.5).max(FILER_VIEWER_RESERVED_WIDTH)
    } else {
        FILER_REGULAR_MAX_WIDTH
    };
    let absolute_max = (width - viewer_reserved).max(min);
    let max = design_max.min(absolute_max).max(min);
    PanelSizeRange {
        default: preferred.clamp(min, max),
        min,
        max,
    }
}

fn subfiler_height_range(content: egui::Rect) -> PanelSizeRange {
    let height = content.height().max(1.0);
    let compact = height < 360.0;
    let min = if compact {
        SUBFILER_COMPACT_MIN_HEIGHT.min(height)
    } else {
        SUBFILER_REGULAR_MIN_HEIGHT
    };
    let viewer_reserved = if compact {
        (height * 0.55).max(120.0)
    } else {
        240.0
    };
    let design_max = (height * 0.4).clamp(SUBFILER_DEFAULT_HEIGHT, 180.0);
    let absolute_max = (height - viewer_reserved).max(min);
    let max = design_max.min(absolute_max).max(min);
    PanelSizeRange {
        default: SUBFILER_DEFAULT_HEIGHT.clamp(min, max),
        min,
        max,
    }
}

fn clamp_popup_position(
    position: egui::Pos2,
    content: egui::Rect,
    estimated_size: egui::Vec2,
) -> egui::Pos2 {
    let max_x = content.right() - estimated_size.x;
    let max_y = content.bottom() - estimated_size.y;
    let x = if max_x <= content.left() {
        content.left()
    } else {
        position.x.clamp(content.left(), max_x)
    };
    let y = if max_y <= content.top() {
        content.top()
    } else {
        position.y.clamp(content.top(), max_y)
    };
    egui::pos2(x, y)
}

fn cascade_menu_size(rows: usize) -> egui::Vec2 {
    let frame_padding = f32::from(style::MENU_FRAME_MARGIN) * 2.0 + 2.0;
    egui::vec2(
        CASCADE_MENU_WIDTH + frame_padding,
        rows as f32 * style::MENU_ROW_HEIGHT + frame_padding,
    )
}

fn cascade_child_position(parent: egui::Rect, content: egui::Rect, rows: usize) -> egui::Pos2 {
    let size = cascade_menu_size(rows);
    let right = parent.right() + CASCADE_MENU_GAP;
    let left = parent.left() - size.x - CASCADE_MENU_GAP;
    let x = if right + size.x <= content.right() || left < content.left() {
        right
    } else {
        left
    };
    let max_y = (content.bottom() - size.y).max(content.top());
    egui::pos2(
        x.clamp(
            content.left(),
            (content.right() - size.x).max(content.left()),
        ),
        parent.top().clamp(content.top(), max_y),
    )
}

fn icon_toolbar_button(
    ui: &mut egui::Ui,
    icon: SvgIcon,
    selected: bool,
    tooltip: &str,
    color: egui::Color32,
) -> bool {
    let size = egui::vec2(30.0, 30.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let visuals = if selected {
        &ui.style().visuals.widgets.active
    } else if response.hovered() {
        &ui.style().visuals.widgets.hovered
    } else {
        &ui.style().visuals.widgets.inactive
    };
    ui.painter().rect(
        rect,
        4.0,
        visuals.bg_fill,
        visuals.bg_stroke,
        egui::StrokeKind::Outside,
    );
    paint_svg_icon(ui.painter(), rect.shrink(6.0), icon, color);
    response.on_hover_text(tooltip).clicked()
}

fn simple_toolbar_button(ui: &mut egui::Ui, text: &str, selected: bool) -> bool {
    ui.add(egui::Button::new(text).selected(selected)).clicked()
}

fn thumbnail_label(label: &str, item_width: f32) -> String {
    let max_chars = if item_width >= 180.0 {
        26
    } else if item_width >= 120.0 {
        20
    } else {
        14
    };
    ellipsize_middle(label, max_chars)
}

fn ellipsize_middle(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }

    let desired_tail = 7usize.min(max_chars.saturating_sub(4));
    let head = max_chars.saturating_sub(3 + desired_tail).max(4);
    let tail = desired_tail.min(chars.len().saturating_sub(head + 3));

    let prefix = chars.iter().take(head).collect::<String>();
    let suffix = chars
        .iter()
        .rev()
        .take(tail)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{prefix}...{suffix}")
}

fn format_system_time(value: SystemTime, locale: &str) -> String {
    let local: DateTime<Local> = value.into();
    local.format(locale_datetime_pattern(locale)).to_string()
}

fn locale_datetime_pattern(locale: &str) -> &'static str {
    let normalized = normalize_locale_tag(Some(locale));
    match normalized.as_str() {
        "ja" | "ja_JP" => "%Y/%m/%d %H:%M",
        "zh" | "zh_CN" | "zh_TW" | "ko" | "ko_KR" => "%Y/%m/%d %H:%M",
        "en_US" => "%m/%d/%Y %I:%M %p",
        "en_GB" | "en_AU" => "%d/%m/%Y %H:%M",
        "de" | "de_DE" | "ru" | "ru_RU" => "%d.%m.%Y %H:%M",
        "fr" | "fr_FR" | "it" | "it_IT" | "es" | "es_ES" | "pt" | "pt_BR" => "%d/%m/%Y %H:%M",
        _ if normalized.starts_with("en_") => "%m/%d/%Y %I:%M %p",
        _ if normalized.starts_with("ja")
            || normalized.starts_with("zh")
            || normalized.starts_with("ko") =>
        {
            "%Y/%m/%d %H:%M"
        }
        _ if normalized.starts_with("de")
            || normalized.starts_with("ru")
            || normalized.starts_with("tr") =>
        {
            "%d.%m.%Y %H:%M"
        }
        _ if normalized.starts_with("fr")
            || normalized.starts_with("it")
            || normalized.starts_with("es")
            || normalized.starts_with("pt") =>
        {
            "%d/%m/%Y %H:%M"
        }
        _ => "%Y-%m-%d %H:%M",
    }
}

fn format_human_size(value: u64) -> String {
    if value < 1024 {
        return format!("{} B", format_grouped_u64(value));
    }
    let kb = value as f64 / 1024.0;
    if kb < 100_000.0 {
        return format!("{:.0} KB", kb);
    }
    let mb = kb / 1024.0;
    if mb < 100_000.0 {
        return format!("{:.1} MB", mb);
    }
    let gb = mb / 1024.0;
    format!("{:.1} GB", gb)
}

fn format_grouped_u64(value: u64) -> String {
    let text = value.to_string();
    let mut out = String::new();
    for (index, ch) in text.chars().rev().enumerate() {
        if index != 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn file_format_label(path: &std::path::Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_uppercase())
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn append_exif_sections(lines: &mut Vec<String>, path: &std::path::Path) {
    lines.push(String::new());
    lines.push("[EXIF / Camera]".to_string());
    let Some(tags) = load_wml2_exif_tags(path) else {
        append_empty_exif_sections(lines);
        return;
    };

    append_exif_tag_group(
        lines,
        &tags,
        &[
            ExifTagSpec::primary(0x010f),
            ExifTagSpec::primary(0x0110),
            ExifTagSpec::exif(0xa434),
            ExifTagSpec::primary(0x0131),
        ],
    );
    lines.push(String::new());
    lines.push("[EXIF / Shooting]".to_string());
    append_exif_tag_group(
        lines,
        &tags,
        &[
            ExifTagSpec::exif(0x9003),
            ExifTagSpec::exif(0x829a),
            ExifTagSpec::exif(0x829d),
            ExifTagSpec::exif(0x8827),
            ExifTagSpec::exif(0x920a),
            ExifTagSpec::exif(0x9204),
            ExifTagSpec::exif(0x9209),
            ExifTagSpec::exif(0xa403),
        ],
    );
    lines.push(String::new());
    lines.push("[EXIF / GPS]".to_string());
    append_exif_tag_group(
        lines,
        &tags,
        &[
            ExifTagSpec::gps(0x0002),
            ExifTagSpec::gps(0x0004),
            ExifTagSpec::gps(0x0006),
            ExifTagSpec::gps(0x001d),
            ExifTagSpec::gps(0x0007),
        ],
    );
}

fn append_empty_exif_sections(lines: &mut Vec<String>) {
    lines.push("(not available)".to_string());
    lines.push(String::new());
    lines.push("[EXIF / Shooting]".to_string());
    lines.push("(not available)".to_string());
    lines.push(String::new());
    lines.push("[EXIF / GPS]".to_string());
    lines.push("(not available)".to_string());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExifIfdKind {
    Primary,
    Exif,
    Gps,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExifTagSpec {
    ifd: ExifIfdKind,
    tagid: u16,
}

impl ExifTagSpec {
    const fn primary(tagid: u16) -> Self {
        Self {
            ifd: ExifIfdKind::Primary,
            tagid,
        }
    }

    const fn exif(tagid: u16) -> Self {
        Self {
            ifd: ExifIfdKind::Exif,
            tagid,
        }
    }

    const fn gps(tagid: u16) -> Self {
        Self {
            ifd: ExifIfdKind::Gps,
            tagid,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ExifTagSets {
    primary: Vec<TiffHeader>,
    exif: Vec<TiffHeader>,
    gps: Vec<TiffHeader>,
}

fn load_wml2_exif_tags(path: &std::path::Path) -> Option<ExifTagSets> {
    let image = image_from_file(path.to_string_lossy().into_owned()).ok()?;
    let metadata = image.metadata.as_ref()?;
    exif_tags_from_metadata(metadata)
}

fn exif_tags_from_metadata(metadata: &Metadata) -> Option<ExifTagSets> {
    exif_tags_from_value(metadata.get("EXIF"))
        .or_else(|| exif_tags_from_value(metadata.get("Tiff headers")))
}

fn exif_tags_from_value(value: Option<&DataMap>) -> Option<ExifTagSets> {
    let headers = match value? {
        DataMap::Exif(headers) => headers.clone(),
        DataMap::Raw(bytes) => parse_exif(bytes).ok()?,
        _ => return None,
    };

    Some(ExifTagSets {
        primary: headers.headers,
        exif: headers.exif.unwrap_or_default(),
        gps: headers.gps.unwrap_or_default(),
    })
}

fn append_exif_tag_group(lines: &mut Vec<String>, tags: &ExifTagSets, specs: &[ExifTagSpec]) {
    let mut found = 0usize;
    for spec in specs {
        if let Some(line) = format_exif_tag_line(tags, *spec) {
            lines.push(line);
            found += 1;
        }
    }
    if found == 0 {
        lines.push("(not available)".to_string());
    }
}

fn format_exif_tag_line(tags: &ExifTagSets, spec: ExifTagSpec) -> Option<String> {
    let headers = match spec.ifd {
        ExifIfdKind::Primary => &tags.primary,
        ExifIfdKind::Exif => &tags.exif,
        ExifIfdKind::Gps => &tags.gps,
    };
    let header = headers
        .iter()
        .find(|header| header.tagid == spec.tagid as usize)?;
    let name = match spec.ifd {
        ExifIfdKind::Gps => gps_mapper(spec.tagid, &header.data, header.length).0,
        _ => tag_mapper(spec.tagid, &header.data, header.length).0,
    };
    let value = normalize_backslash_display(
        format_tiff_data(&header.data, header.length)
            .trim()
            .trim_end_matches('\0'),
    );
    Some(format!("{name}: {value}"))
}

fn format_tiff_data(data: &DataPack, length: usize) -> String {
    match data {
        DataPack::Ascii(value) => value.clone(),
        DataPack::Bytes(values) => format_scalar_list(values.iter().take(length).copied()),
        DataPack::SByte(values) => format_scalar_list(values.iter().take(length).copied()),
        DataPack::Short(values) => format_scalar_list(values.iter().take(length).copied()),
        DataPack::Long(values) => format_scalar_list(values.iter().take(length).copied()),
        DataPack::SShort(values) => format_scalar_list(values.iter().take(length).copied()),
        DataPack::SLong(values) => format_scalar_list(values.iter().take(length).copied()),
        DataPack::Float(values) => format_scalar_list(values.iter().take(length).copied()),
        DataPack::Double(values) => format_scalar_list(values.iter().take(length).copied()),
        DataPack::Rational(values) => values
            .iter()
            .take(length)
            .map(|value| format!("{}/{}", value.n, value.d))
            .collect::<Vec<_>>()
            .join(" "),
        DataPack::SRational(values) => values
            .iter()
            .take(length)
            .map(|value| format!("{}/{}", value.n, value.d))
            .collect::<Vec<_>>()
            .join(" "),
        DataPack::Unkown(values) | DataPack::Undef(values) => values
            .iter()
            .take(length)
            .map(|value| format!("{value:02x}"))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn format_scalar_list<T>(values: impl Iterator<Item = T>) -> String
where
    T: std::fmt::Display,
{
    values
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_backslash_display(text: &str) -> String {
    if !text.contains("\\\\") {
        return text.to_string();
    }

    if let Some(rest) = text.strip_prefix("\\\\") {
        return format!("\\\\{}", rest.replace("\\\\", "\\"));
    }

    text.replace("\\\\", "\\")
}

#[cfg(test)]
#[path = "../../../../tests/support/src/ui/menu/fileviewer/mod_tests.rs"]
mod tests;
