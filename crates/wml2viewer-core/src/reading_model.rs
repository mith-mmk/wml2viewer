//! Logical page, spread, rotation, and preload planning without platform paths.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct SourceId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct PageId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageDescriptor {
    pub id: PageId,
    /// Stable opaque identity for the directory, archive, or listed-file source.
    pub source_id: SourceId,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub is_cover: bool,
}

impl PageDescriptor {
    pub const fn is_portrait(self) -> bool {
        self.pixel_width > 0 && self.pixel_height > 0 && self.pixel_height >= self.pixel_width
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

impl Viewport {
    pub fn is_landscape(self, minimum_aspect_ratio: f32) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
            && minimum_aspect_ratio.is_finite()
            && minimum_aspect_ratio > 0.0
            && self.width >= self.height * minimum_aspect_ratio
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReadingDirection {
    LeftToRight,
    #[default]
    RightToLeft,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageLayout {
    #[default]
    Auto,
    Single,
    Spread,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpreadOptions {
    pub layout: PageLayout,
    pub direction: ReadingDirection,
    pub cover_alone: bool,
    pub minimum_landscape_aspect_ratio: f32,
}

impl Default for SpreadOptions {
    fn default() -> Self {
        Self {
            layout: PageLayout::Auto,
            direction: ReadingDirection::RightToLeft,
            cover_alone: true,
            minimum_landscape_aspect_ratio: 1.4,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spread {
    pub start_index: usize,
    /// Pages in forward navigation order.
    pub pages: Vec<PageId>,
    /// Pages in physical left-to-right order.
    pub visual_order: Vec<PageId>,
}

impl Spread {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn next_index(&self) -> usize {
        self.start_index.saturating_add(self.page_count())
    }

    pub fn contains(&self, page: PageId) -> bool {
        self.pages.contains(&page)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveDirection {
    Forward,
    Backward,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PreloadPlan {
    pub pages: Vec<PageId>,
}

pub fn build_spread(
    pages: &[PageDescriptor],
    start_index: usize,
    viewport: Viewport,
    options: SpreadOptions,
) -> Option<Spread> {
    let current = *pages.get(start_index)?;
    let companion = pages
        .get(start_index + 1)
        .copied()
        .filter(|next| can_pair(current, *next, viewport, options));
    let mut navigation_pages = vec![current.id];
    if let Some(companion) = companion {
        navigation_pages.push(companion.id);
    }
    let visual_order = match (options.direction, companion) {
        (ReadingDirection::RightToLeft, Some(companion)) => vec![companion.id, current.id],
        _ => navigation_pages.clone(),
    };
    Some(Spread {
        start_index,
        pages: navigation_pages,
        visual_order,
    })
}

pub fn plan_preload(
    pages: &[PageDescriptor],
    current: &Spread,
    direction: MoveDirection,
    viewport: Viewport,
    options: SpreadOptions,
    maximum_spreads: usize,
) -> PreloadPlan {
    let mut planned = Vec::new();
    let mut cursor = match direction {
        MoveDirection::Forward => {
            next_spread_start(pages, current.start_index, viewport, options).unwrap_or(pages.len())
        }
        MoveDirection::Backward => {
            previous_spread_start(pages, current.start_index, viewport, options)
                .unwrap_or(current.start_index)
        }
    };
    for _ in 0..maximum_spreads {
        if direction == MoveDirection::Backward && cursor >= current.start_index {
            break;
        }
        let Some(spread) = build_spread(pages, cursor, viewport, options) else {
            break;
        };
        planned.extend(spread.pages.iter().copied());
        match direction {
            MoveDirection::Forward => cursor = spread.next_index(),
            MoveDirection::Backward => {
                let Some(previous) =
                    previous_spread_start(pages, spread.start_index, viewport, options)
                else {
                    break;
                };
                cursor = previous;
            }
        }
    }
    PreloadPlan { pages: planned }
}

#[derive(Clone, Debug)]
pub struct PageSession {
    pages: Vec<PageDescriptor>,
    current_page_index: usize,
    viewport: Viewport,
    spread_options: SpreadOptions,
}

impl Default for PageSession {
    fn default() -> Self {
        Self::new(Vec::new(), Viewport::default(), SpreadOptions::default())
    }
}

impl PageSession {
    pub fn new(
        pages: Vec<PageDescriptor>,
        viewport: Viewport,
        spread_options: SpreadOptions,
    ) -> Self {
        Self {
            pages,
            current_page_index: 0,
            viewport,
            spread_options,
        }
    }

    pub fn pages(&self) -> &[PageDescriptor] {
        &self.pages
    }

    /// Index of the logical page selected by the user, not the spread anchor.
    pub fn current_index(&self) -> usize {
        self.current_page_index
    }

    pub fn current_page_id(&self) -> Option<PageId> {
        self.pages.get(self.current_page_index).map(|page| page.id)
    }

    pub fn set_current_page(&mut self, page_id: PageId) -> bool {
        let Some(index) = self.pages.iter().position(|page| page.id == page_id) else {
            return false;
        };
        self.current_page_index = index;
        true
    }

    pub fn set_pages(&mut self, pages: Vec<PageDescriptor>) {
        let selected = self.current_page_id();
        self.pages = pages;
        self.current_page_index = selected
            .and_then(|id| self.pages.iter().position(|page| page.id == id))
            .unwrap_or_else(|| {
                self.current_page_index
                    .min(self.pages.len().saturating_sub(1))
            });
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
    }

    pub fn set_spread_options(&mut self, options: SpreadOptions) {
        self.spread_options = options;
    }

    pub fn current_spread(&self) -> Option<Spread> {
        let start = spread_start(
            &self.pages,
            self.current_page_index,
            self.viewport,
            self.spread_options,
        )?;
        build_spread(&self.pages, start, self.viewport, self.spread_options)
    }

    pub fn move_to(&mut self, direction: MoveDirection) -> Option<Spread> {
        let current = self.current_spread()?;
        let next_index = match direction {
            MoveDirection::Forward => next_spread_start(
                &self.pages,
                current.start_index,
                self.viewport,
                self.spread_options,
            )?,
            MoveDirection::Backward => previous_spread_start(
                &self.pages,
                current.start_index,
                self.viewport,
                self.spread_options,
            )?,
        };
        if next_index >= self.pages.len() {
            return None;
        }
        self.current_page_index = next_index;
        self.current_spread()
    }

    pub fn preload_plan(&self, direction: MoveDirection, maximum_spreads: usize) -> PreloadPlan {
        self.current_spread()
            .map(|spread| {
                plan_preload(
                    &self.pages,
                    &spread,
                    direction,
                    self.viewport,
                    self.spread_options,
                    maximum_spreads,
                )
            })
            .unwrap_or_default()
    }

    pub fn next_spread_preload(&self) -> PreloadPlan {
        self.preload_plan(MoveDirection::Forward, 1)
    }
}

fn can_pair(
    current: PageDescriptor,
    next: PageDescriptor,
    viewport: Viewport,
    options: SpreadOptions,
) -> bool {
    let layout_allows = match options.layout {
        PageLayout::Auto => viewport.is_landscape(options.minimum_landscape_aspect_ratio),
        PageLayout::Single => false,
        PageLayout::Spread => true,
    };
    layout_allows
        && current.is_portrait()
        && next.is_portrait()
        && current.source_id == next.source_id
        && !(options.cover_alone && (current.is_cover || next.is_cover))
}

/// Returns the canonical spread anchor containing the selected logical page.
pub fn spread_start(
    pages: &[PageDescriptor],
    page_index: usize,
    viewport: Viewport,
    options: SpreadOptions,
) -> Option<usize> {
    let selected = *pages.get(page_index)?;
    let mut source_start = page_index;
    while source_start > 0 && pages[source_start - 1].source_id == selected.source_id {
        source_start -= 1;
    }
    let mut cursor = source_start;
    while cursor <= page_index {
        let spread = build_spread(pages, cursor, viewport, options)?;
        if spread.contains(selected.id) {
            return Some(cursor);
        }
        let next = spread.next_index();
        if next <= cursor {
            return None;
        }
        cursor = next;
    }
    None
}

/// Returns the canonical anchor immediately before `current_index`.
pub fn previous_spread_start(
    pages: &[PageDescriptor],
    current_index: usize,
    viewport: Viewport,
    options: SpreadOptions,
) -> Option<usize> {
    if current_index == 0 {
        return None;
    }
    for candidate in (0..current_index).rev() {
        if build_spread(pages, candidate, viewport, options)
            .is_some_and(|spread| spread.next_index() == current_index)
        {
            return Some(candidate);
        }
    }
    Some(current_index - 1)
}

/// Returns the canonical anchor immediately after `current_index`.
pub fn next_spread_start(
    pages: &[PageDescriptor],
    current_index: usize,
    viewport: Viewport,
    options: SpreadOptions,
) -> Option<usize> {
    let next = build_spread(pages, current_index, viewport, options)?.next_index();
    (next < pages.len()).then_some(next)
}
