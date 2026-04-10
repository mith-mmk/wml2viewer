use eframe::egui;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SvgIcon {
    ThumbnailGrid,
    ThumbnailSmall,
    ThumbnailMedium,
    ThumbnailLarge,
    Detail,
    Sort,
    SortByDate,
    SortBySize,
    SortAsc,
    SortDesc,
    Filter,
    Folder,
    Archive,
    Up,
}

#[derive(Clone, Debug)]
enum SvgShape {
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    Rect {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        rx: f32,
    },
    Polyline {
        points: Vec<(f32, f32)>,
        closed: bool,
    },
}

pub(crate) fn paint_svg_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    icon: SvgIcon,
    color: egui::Color32,
) {
    let stroke = egui::Stroke::new((rect.width() / 12.0).max(1.5), color);
    let map = |x: f32, y: f32| -> egui::Pos2 {
        egui::pos2(
            rect.left() + rect.width() * (x / 24.0),
            rect.top() + rect.height() * (y / 24.0),
        )
    };

    for shape in icon_shapes(icon) {
        match shape {
            SvgShape::Line { x1, y1, x2, y2 } => {
                painter.line_segment([map(*x1, *y1), map(*x2, *y2)], stroke);
            }
            SvgShape::Rect {
                x,
                y,
                width,
                height,
                rx,
            } => {
                painter.rect_stroke(
                    egui::Rect::from_min_max(map(*x, *y), map(*x + *width, *y + *height)),
                    *rx,
                    stroke,
                    egui::StrokeKind::Outside,
                );
            }
            SvgShape::Polyline { points, closed } => {
                for pair in points.windows(2) {
                    let [(x1, y1), (x2, y2)] = [pair[0], pair[1]];
                    painter.line_segment([map(x1, y1), map(x2, y2)], stroke);
                }
                if *closed && points.len() > 2 {
                    let (x1, y1) = points[points.len() - 1];
                    let (x2, y2) = points[0];
                    painter.line_segment([map(x1, y1), map(x2, y2)], stroke);
                }
            }
        }
    }
}

fn icon_shapes(icon: SvgIcon) -> &'static [SvgShape] {
    static CACHE: OnceLock<HashMap<SvgIcon, Vec<SvgShape>>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            use SvgIcon::*;
            [
                (
                    ThumbnailGrid,
                    include_str!("../../../../resources/icons/thumbnail.svg"),
                ),
                (
                    ThumbnailSmall,
                    include_str!("../../../../resources/icons/thumbnail-small.svg"),
                ),
                (
                    ThumbnailMedium,
                    include_str!("../../../../resources/icons/thumbnail-middle.svg"),
                ),
                (
                    ThumbnailLarge,
                    include_str!("../../../../resources/icons/thumbnail-large.svg"),
                ),
                (
                    Detail,
                    include_str!("../../../../resources/icons/detail.svg"),
                ),
                (Sort, include_str!("../../../../resources/icons/sort.svg")),
                (
                    SortByDate,
                    include_str!("../../../../resources/icons/sort-by-date.svg"),
                ),
                (
                    SortBySize,
                    include_str!("../../../../resources/icons/sort-by-size.svg"),
                ),
                (
                    SortAsc,
                    include_str!("../../../../resources/icons/sort-asc.svg"),
                ),
                (
                    SortDesc,
                    include_str!("../../../../resources/icons/sort-desc.svg"),
                ),
                (
                    Filter,
                    include_str!("../../../../resources/icons/filter.svg"),
                ),
                (
                    Folder,
                    include_str!("../../../../resources/icons/folder.svg"),
                ),
                (
                    Archive,
                    include_str!("../../../../resources/icons/archive.svg"),
                ),
                (Up, include_str!("../../../../resources/icons/up.svg")),
            ]
            .into_iter()
            .map(|(icon, svg)| (icon, parse_svg_shapes(svg)))
            .collect()
        })
        .get(&icon)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

fn parse_svg_shapes(svg: &str) -> Vec<SvgShape> {
    let mut shapes = Vec::new();
    for line in svg.lines().map(str::trim) {
        if line.starts_with("<line ") {
            if let (Some(x1), Some(y1), Some(x2), Some(y2)) = (
                attr_f32(line, "x1"),
                attr_f32(line, "y1"),
                attr_f32(line, "x2"),
                attr_f32(line, "y2"),
            ) {
                shapes.push(SvgShape::Line { x1, y1, x2, y2 });
            }
        } else if line.starts_with("<rect ") {
            if let (Some(x), Some(y), Some(width), Some(height)) = (
                attr_f32(line, "x"),
                attr_f32(line, "y"),
                attr_f32(line, "width"),
                attr_f32(line, "height"),
            ) {
                shapes.push(SvgShape::Rect {
                    x,
                    y,
                    width,
                    height,
                    rx: attr_f32(line, "rx").unwrap_or(0.0),
                });
            }
        } else if line.starts_with("<polyline ") || line.starts_with("<polygon ") {
            if let Some(points) = attr_value(line, "points").map(parse_points) {
                shapes.push(SvgShape::Polyline {
                    points,
                    closed: line.starts_with("<polygon "),
                });
            }
        }
    }
    shapes
}

fn parse_points(raw: &str) -> Vec<(f32, f32)> {
    let values = raw
        .replace(',', " ")
        .split_whitespace()
        .filter_map(|part| part.parse::<f32>().ok())
        .collect::<Vec<_>>();
    values
        .chunks_exact(2)
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

fn attr_f32(line: &str, name: &str) -> Option<f32> {
    attr_value(line, name)?.parse().ok()
}

fn attr_value<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!("{name}=\"");
    let start = line.find(&pattern)? + pattern.len();
    let end = line[start..].find('"')?;
    Some(&line[start..start + end])
}
