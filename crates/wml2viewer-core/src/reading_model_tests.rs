use crate::reading::{
    MoveDirection, PageDescriptor, PageId, PageLayout, PageSession, SourceId, SpreadOptions,
    Viewport, build_spread, next_spread_start, plan_preload, previous_spread_start, spread_start,
};

fn page(id: u64, source: u64, width: u32, height: u32, is_cover: bool) -> PageDescriptor {
    PageDescriptor {
        id: PageId(id),
        source_id: SourceId(source),
        pixel_width: width,
        pixel_height: height,
        is_cover,
    }
}

fn portrait(id: u64) -> PageDescriptor {
    page(id, 1, 800, 1200, false)
}

fn wide_viewport() -> Viewport {
    Viewport {
        width: 1600.0,
        height: 900.0,
    }
}

#[test]
fn auto_and_explicit_layouts_choose_expected_page_count() {
    let pages = [portrait(1), portrait(2)];
    let portrait_viewport = Viewport {
        width: 800.0,
        height: 1200.0,
    };
    assert_eq!(
        build_spread(&pages, 0, portrait_viewport, SpreadOptions::default())
            .unwrap()
            .pages,
        vec![PageId(1)]
    );
    let single = SpreadOptions {
        layout: PageLayout::Single,
        ..SpreadOptions::default()
    };
    assert_eq!(
        build_spread(&pages, 0, wide_viewport(), single)
            .unwrap()
            .pages,
        vec![PageId(1)]
    );
    let forced = SpreadOptions {
        layout: PageLayout::Spread,
        ..SpreadOptions::default()
    };
    assert_eq!(
        build_spread(&pages, 0, portrait_viewport, forced)
            .unwrap()
            .pages,
        vec![PageId(1), PageId(2)]
    );
}

#[test]
fn cover_and_source_boundaries_never_pair() {
    let pages = [
        page(1, 1, 800, 1200, true),
        page(2, 1, 800, 1200, false),
        page(3, 2, 800, 1200, false),
    ];
    assert_eq!(
        build_spread(&pages, 0, wide_viewport(), SpreadOptions::default())
            .unwrap()
            .pages,
        vec![PageId(1)]
    );
    assert_eq!(
        build_spread(&pages, 1, wide_viewport(), SpreadOptions::default())
            .unwrap()
            .pages,
        vec![PageId(2)]
    );
}

#[test]
fn right_to_left_reverses_visual_order_only() {
    let spread = build_spread(
        &[portrait(1), portrait(2)],
        0,
        wide_viewport(),
        SpreadOptions::default(),
    )
    .unwrap();
    assert_eq!(spread.pages, vec![PageId(1), PageId(2)]);
    assert_eq!(spread.visual_order, vec![PageId(2), PageId(1)]);
}

#[test]
fn landscape_page_disables_pairing() {
    let spread = build_spread(
        &[page(1, 1, 1600, 900, false), portrait(2)],
        0,
        wide_viewport(),
        SpreadOptions::default(),
    )
    .unwrap();
    assert_eq!(spread.pages, vec![PageId(1)]);
}

#[test]
fn rotation_preserves_selected_page_in_canonical_spread() {
    let pages = vec![
        page(1, 1, 800, 1200, true),
        portrait(2),
        portrait(3),
        portrait(4),
    ];
    let mut session = PageSession::new(
        pages,
        Viewport {
            width: 800.0,
            height: 1200.0,
        },
        SpreadOptions::default(),
    );
    assert!(session.set_current_page(PageId(3)));
    session.set_viewport(wide_viewport());
    let spread = session.current_spread().unwrap();
    assert_eq!(session.current_page_id(), Some(PageId(3)));
    assert_eq!(spread.pages, vec![PageId(2), PageId(3)]);
}

#[test]
fn next_spread_preload_plans_exactly_one_spread() {
    let pages = (1..=6).map(portrait).collect::<Vec<_>>();
    let mut session = PageSession::new(pages, wide_viewport(), SpreadOptions::default());
    assert_eq!(
        session.next_spread_preload().pages,
        vec![PageId(3), PageId(4)]
    );
    assert_eq!(
        session.move_to(MoveDirection::Forward).unwrap().pages,
        vec![PageId(3), PageId(4)]
    );
    assert_eq!(
        session.move_to(MoveDirection::Backward).unwrap().pages,
        vec![PageId(1), PageId(2)]
    );
}

#[test]
fn stateless_spread_navigation_matches_logical_selection_and_preload() {
    let pages = vec![
        page(0, 1, 800, 1200, true),
        page(1, 1, 800, 1200, false),
        page(2, 1, 800, 1200, false),
        page(3, 1, 1600, 900, false),
        page(4, 1, 800, 1200, false),
        page(5, 1, 800, 1200, false),
    ];
    let options = SpreadOptions::default();
    let anchor = spread_start(&pages, 2, wide_viewport(), options).unwrap();
    let spread = build_spread(&pages, anchor, wide_viewport(), options).unwrap();

    assert_eq!(anchor, 1);
    assert_eq!(spread.pages, vec![PageId(1), PageId(2)]);
    assert_eq!(
        previous_spread_start(&pages, anchor, wide_viewport(), options),
        Some(0)
    );
    assert_eq!(
        next_spread_start(&pages, anchor, wide_viewport(), options),
        Some(3)
    );
    assert_eq!(
        plan_preload(
            &pages,
            &spread,
            MoveDirection::Forward,
            wide_viewport(),
            options,
            2,
        )
        .pages,
        vec![PageId(3), PageId(4), PageId(5)]
    );
}
