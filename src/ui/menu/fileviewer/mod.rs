mod icons;
pub(crate) mod state;
pub(crate) mod thumbnail;
pub(crate) mod worker;

use crate::dependent::{download_http_url, normalize_locale_tag};
use crate::drawers::image::SaveFormat;
use crate::filesystem::resolve_start_path;
use crate::ui::i18n::UiTextKey;
use crate::ui::menu::fileviewer::icons::{SvgIcon, paint_svg_icon};
use crate::ui::menu::fileviewer::state::{FilerEntry, FilerSortField, FilerViewMode, NameSortMode};
use crate::ui::viewer::ViewerApp;
use crate::ui::viewer::options::PaneSide;
use chrono::{DateTime, Local};
use eframe::egui;
use std::time::SystemTime;

impl ViewerApp {
    pub(crate) fn left_click_menu_ui(&mut self, ctx: &egui::Context) {
        if !self.show_left_menu {
            return;
        }

        let mut open = true;
        egui::Window::new(self.text(UiTextKey::Menu))
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .fixed_pos(self.left_menu_pos)
            .open(&mut open)
            .show(ctx, |ui| {
                if ui.button(self.text(UiTextKey::Next)).clicked() {
                    let _ = self.next_image();
                    self.show_left_menu = false;
                }
                if ui.button(self.text(UiTextKey::Previous)).clicked() {
                    let _ = self.prev_image();
                    self.show_left_menu = false;
                }
                if ui.button(self.text(UiTextKey::ToggleSettings)).clicked() {
                    if self.show_settings {
                        self.close_settings_dialog();
                    } else {
                        self.open_settings_dialog();
                    }
                    self.show_left_menu = false;
                }
                if ui.button(self.text(UiTextKey::ToggleFiler)).clicked() {
                    self.show_filer = !self.show_filer;
                    self.pending_fit_recalc = true;
                    self.show_left_menu = false;
                }
                if ui.button(self.text(UiTextKey::ToggleManga)).clicked() {
                    self.options.manga_mode = !self.options.manga_mode;
                    self.pending_fit_recalc = true;
                    self.show_left_menu = false;
                }
                ui.separator();
                ui.label(self.text(UiTextKey::SaveAs));
                for format in SaveFormat::all() {
                    if ui
                        .selectable_label(self.save_dialog.format == format, format.to_string())
                        .clicked()
                    {
                        self.save_dialog.format = format;
                        self.open_save_dialog();
                        self.show_left_menu = false;
                    }
                }
            });
        self.show_left_menu = open;
    }

    pub(crate) fn filer_ui(&mut self, ctx: &egui::Context) {
        if !self.show_filer {
            return;
        }

        let content = ctx.content_rect();
        let max_width = if content.width() >= content.height() * 1.5 {
            (content.width() * 0.5).max(280.0)
        } else {
            420.0
        };

        let panel = match self.window_options.pane_side {
            PaneSide::Left => egui::SidePanel::left("filer_panel"),
            PaneSide::Right => egui::SidePanel::right("filer_panel"),
        };

        panel
            .resizable(true)
            .default_width(match self.filer.view_mode {
                FilerViewMode::ThumbnailLarge => 420.0,
                FilerViewMode::ThumbnailMedium => 360.0,
                _ => 300.0,
            })
            .min_width(240.0)
            .max_width(max_width)
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
                                self.request_filer_directory(root, None);
                            }
                        }
                    });
                if let Some(dir) = &self.filer.directory {
                    ui.label(dir.display().to_string());
                    if let Some(parent) = dir.parent() {
                        if icon_toolbar_button(ui, SvgIcon::Up, false, up_text, icon_color) {
                            self.request_filer_directory(parent.to_path_buf(), None);
                        }
                    }
                }
                ui.separator();
                if refresh_requested {
                    self.refresh_current_filer_directory();
                }
                let panel_width = ui.available_width();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_min_width(panel_width.max(160.0));
                        let entries = self.filer.entries.clone();
                        match self.filer.view_mode {
                            FilerViewMode::List | FilerViewMode::Detail => {
                                for entry in entries {
                                    self.filer_entry_row(ui, entry);
                                }
                            }
                            _ => {
                                let item_width = match self.filer.view_mode {
                                    FilerViewMode::ThumbnailSmall => 72.0,
                                    FilerViewMode::ThumbnailMedium => 112.0,
                                    FilerViewMode::ThumbnailLarge => 160.0,
                                    _ => 96.0,
                                } * self.filer.thumbnail_scale;
                                self.filer_thumbnail_grid(ui, entries, item_width);
                            }
                        }
                    });
            });
    }

    fn filer_entry_row(&mut self, ui: &mut egui::Ui, entry: FilerEntry) {
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

    fn filer_thumbnail_grid(
        &mut self,
        ui: &mut egui::Ui,
        entries: Vec<FilerEntry>,
        item_width: f32,
    ) {
        let available = ui.available_width().max(item_width);
        let spacing = 8.0;
        let columns = ((available + spacing) / (item_width.max(1.0) + spacing))
            .floor()
            .max(1.0) as usize;
        egui::Grid::new("filer_thumbnail_grid")
            .num_columns(columns)
            .spacing(egui::vec2(spacing, spacing))
            .show(ui, |ui| {
                for (index, entry) in entries.into_iter().enumerate() {
                    self.filer_thumbnail_tile(ui, entry, item_width);
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
    }

    fn filer_thumbnail_tile(&mut self, ui: &mut egui::Ui, entry: FilerEntry, item_width: f32) {
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

        egui::TopBottomPanel::bottom("subfiler_panel")
            .resizable(true)
            .default_height(110.0)
            .show(ctx, |ui| {
                let mut close_requested = false;
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
                egui::ScrollArea::horizontal().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut entries = self.filer.entries.clone();
                        if self.options.manga_right_to_left {
                            entries.reverse();
                        }
                        for entry in entries {
                            if entry.is_container {
                                continue;
                            }
                            self.ensure_thumbnail(&entry.path, 72);
                            let selected = self.current_navigation_path == entry.path;
                            let mut frame = egui::Frame::group(ui.style());
                            if selected {
                                frame.stroke =
                                    egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
                            }
                            frame.show(ui, |ui| {
                                if let Some(texture) = self.thumbnail_cache.get(&entry.path) {
                                    if ui
                                        .add(egui::Button::image(
                                            egui::Image::from_texture(texture)
                                                .fit_to_exact_size(egui::vec2(72.0, 72.0)),
                                        ))
                                        .clicked()
                                    {
                                        self.activate_filer_entry(entry.clone());
                                    }
                                } else if ui.button("...").clicked() {
                                    self.activate_filer_entry(entry.clone());
                                }
                            });
                        }
                    });
                });
                if close_requested {
                    self.show_subfiler = false;
                }
            });
    }

    fn activate_filer_entry(&mut self, entry: FilerEntry) {
        if entry.is_container {
            self.request_filer_directory(entry.path, None);
            return;
        }
        let navigation_path = entry.path.clone();
        let load_path = resolve_start_path(&navigation_path)
            .unwrap_or_else(|| navigation_path.clone());
        self.filer.selected = Some(navigation_path.clone());
        self.empty_mode = false;
        self.show_filer = false;
        self.pending_fit_recalc = true;
        self.set_filesystem_current(navigation_path.clone());
        let _ = self.request_load_target(navigation_path, load_path);
    }
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
        "fr" | "fr_FR" | "it" | "it_IT" | "es" | "es_ES" | "pt" | "pt_BR" => {
            "%d/%m/%Y %H:%M"
        }
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

#[cfg(test)]
mod tests {
    use super::locale_datetime_pattern;

    #[test]
    fn locale_datetime_pattern_supports_multiple_locales() {
        assert_eq!(locale_datetime_pattern("ja_JP"), "%Y/%m/%d %H:%M");
        assert_eq!(locale_datetime_pattern("en_US"), "%m/%d/%Y %I:%M %p");
        assert_eq!(locale_datetime_pattern("de_DE"), "%d.%m.%Y %H:%M");
        assert_eq!(locale_datetime_pattern("fr_FR"), "%d/%m/%Y %H:%M");
    }
}
