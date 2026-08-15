//! Stateless Android wire adapter for the platform-independent reading model.

use wml2viewer_core::reading::{
    MoveDirection, PageDescriptor, PageId, PageLayout, ReadingDirection, SourceId, SpreadOptions,
    Viewport, build_spread, next_spread_start, plan_preload, previous_spread_start, spread_start,
};

pub const MAX_READING_PAGES: usize = 4_096;
pub const MAX_PREFETCH_SPREADS: usize = 64;

pub const READING_WIRE_VERSION: i32 = 1;
pub const READING_WIRE_HEADER_INTS: usize = 8;
const WIRE_NONE: i32 = -1;

#[derive(Clone, Copy, Debug)]
pub struct ReadingPlanRequest<'a> {
    pub source_ids: &'a [i64],
    pub portrait: &'a [bool],
    pub covers: &'a [bool],
    pub current_index: i32,
    pub landscape: bool,
    pub layout: i32,
    pub direction: i32,
    pub cover_alone: bool,
    pub maximum_prefetch_spreads: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadingPlan {
    pub anchor_index: i32,
    pub logical_indices: Vec<i32>,
    pub visual_indices: Vec<i32>,
    pub previous_anchor: Option<i32>,
    pub next_anchor: Option<i32>,
    pub preload_indices: Vec<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadingPlanError {
    EmptyPages,
    TooManyPages,
    LengthMismatch,
    InvalidCurrentIndex,
    InvalidLayout,
    InvalidDirection,
    InvalidPrefetchLimit,
    WireLimit,
}

pub fn plan_reading(request: ReadingPlanRequest<'_>) -> Result<ReadingPlan, ReadingPlanError> {
    let page_count = request.source_ids.len();
    if page_count == 0 {
        return Err(ReadingPlanError::EmptyPages);
    }
    if page_count > MAX_READING_PAGES {
        return Err(ReadingPlanError::TooManyPages);
    }
    if request.portrait.len() != page_count || request.covers.len() != page_count {
        return Err(ReadingPlanError::LengthMismatch);
    }
    let current_index = usize::try_from(request.current_index)
        .ok()
        .filter(|index| *index < page_count)
        .ok_or(ReadingPlanError::InvalidCurrentIndex)?;
    let maximum_prefetch_spreads = usize::try_from(request.maximum_prefetch_spreads)
        .ok()
        .filter(|count| *count <= MAX_PREFETCH_SPREADS)
        .ok_or(ReadingPlanError::InvalidPrefetchLimit)?;
    let layout = match request.layout {
        0 => PageLayout::Auto,
        1 => PageLayout::Single,
        2 => PageLayout::Spread,
        _ => return Err(ReadingPlanError::InvalidLayout),
    };
    let direction = match request.direction {
        0 => ReadingDirection::LeftToRight,
        1 => ReadingDirection::RightToLeft,
        _ => return Err(ReadingPlanError::InvalidDirection),
    };
    let viewport = if request.landscape {
        Viewport {
            width: 2.0,
            height: 1.0,
        }
    } else {
        Viewport {
            width: 1.0,
            height: 2.0,
        }
    };
    let options = SpreadOptions {
        layout,
        direction,
        cover_alone: request.cover_alone,
        ..SpreadOptions::default()
    };
    let pages = request
        .source_ids
        .iter()
        .zip(request.portrait)
        .zip(request.covers)
        .enumerate()
        .map(|(index, ((source_id, portrait), cover))| PageDescriptor {
            id: PageId(index as u64),
            source_id: SourceId(u64::from_ne_bytes(source_id.to_ne_bytes())),
            pixel_width: if *portrait { 1 } else { 2 },
            pixel_height: 1,
            is_cover: *cover,
        })
        .collect::<Vec<_>>();

    let anchor = spread_start(&pages, current_index, viewport, options)
        .ok_or(ReadingPlanError::InvalidCurrentIndex)?;
    let spread =
        build_spread(&pages, anchor, viewport, options).ok_or(ReadingPlanError::WireLimit)?;
    let previous_anchor = previous_spread_start(&pages, anchor, viewport, options);
    let next_anchor = next_spread_start(&pages, anchor, viewport, options);
    let preload = plan_preload(
        &pages,
        &spread,
        MoveDirection::Forward,
        viewport,
        options,
        maximum_prefetch_spreads,
    );

    Ok(ReadingPlan {
        anchor_index: index_to_wire(anchor)?,
        logical_indices: page_ids_to_wire(&spread.pages)?,
        visual_indices: page_ids_to_wire(&spread.visual_order)?,
        previous_anchor: previous_anchor.map(index_to_wire).transpose()?,
        next_anchor: next_anchor.map(index_to_wire).transpose()?,
        preload_indices: page_ids_to_wire(&preload.pages)?,
    })
}

/// Wire v1 fields are: version, total length, anchor, previous anchor,
/// next anchor, logical count, visual count, preload count, followed by the
/// three index lists in that order. Missing anchors are encoded as `-1`.
pub fn encode_reading_wire(plan: &ReadingPlan) -> Result<Vec<i32>, ReadingPlanError> {
    let logical_count = count_to_wire(plan.logical_indices.len())?;
    let visual_count = count_to_wire(plan.visual_indices.len())?;
    let preload_count = count_to_wire(plan.preload_indices.len())?;
    let total_length = READING_WIRE_HEADER_INTS
        .checked_add(plan.logical_indices.len())
        .and_then(|length| length.checked_add(plan.visual_indices.len()))
        .and_then(|length| length.checked_add(plan.preload_indices.len()))
        .ok_or(ReadingPlanError::WireLimit)?;
    let total_length_wire = count_to_wire(total_length)?;
    let mut wire = Vec::new();
    wire.try_reserve_exact(total_length)
        .map_err(|_| ReadingPlanError::WireLimit)?;
    wire.extend_from_slice(&[
        READING_WIRE_VERSION,
        total_length_wire,
        plan.anchor_index,
        plan.previous_anchor.unwrap_or(WIRE_NONE),
        plan.next_anchor.unwrap_or(WIRE_NONE),
        logical_count,
        visual_count,
        preload_count,
    ]);
    wire.extend_from_slice(&plan.logical_indices);
    wire.extend_from_slice(&plan.visual_indices);
    wire.extend_from_slice(&plan.preload_indices);
    Ok(wire)
}

fn page_ids_to_wire(page_ids: &[PageId]) -> Result<Vec<i32>, ReadingPlanError> {
    page_ids
        .iter()
        .map(|page_id| i32::try_from(page_id.0).map_err(|_| ReadingPlanError::WireLimit))
        .collect()
}

fn index_to_wire(index: usize) -> Result<i32, ReadingPlanError> {
    i32::try_from(index).map_err(|_| ReadingPlanError::WireLimit)
}

fn count_to_wire(count: usize) -> Result<i32, ReadingPlanError> {
    i32::try_from(count).map_err(|_| ReadingPlanError::WireLimit)
}
