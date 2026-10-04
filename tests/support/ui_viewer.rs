use super::*;
use crate::drawers::canvas::Canvas;
use crate::ui::menu::fileviewer::state::FilerViewMode;
use crate::ui::viewer::options::WindowUiTheme;
use std::fs;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

fn dummy_loaded_image(width: u32, height: u32) -> LoadedImage {
    LoadedImage {
        canvas: Canvas::new(width, height),
        animation: Vec::new(),
        loop_count: None,
    }
}

fn dummy_preloaded_entry(path: &str) -> PreloadedEntry {
    PreloadedEntry {
        navigation_path: PathBuf::from(path),
        load_path: Some(PathBuf::from(path)),
        zoom: 1.0,
        display: DisplayedPageState {
            source: dummy_loaded_image(4, 4),
            rendered: dummy_loaded_image(4, 4),
            texture: None,
            texture_display_scale: 1.0,
            prepared_texture: None,
        },
    }
}

fn dummy_filer_entry(path: &str) -> FilerEntry {
    FilerEntry {
        path: PathBuf::from(path),
        label: path.to_string(),
        is_container: false,
        sort_as_container: false,
        metadata: Default::default(),
    }
}

fn make_temp_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|path| {
                path.parent()
                    .and_then(|deps| deps.parent())
                    .map(Path::to_path_buf)
            })
        })
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .join(".test_wml2viewer");
    fs::create_dir_all(&base).unwrap();
    let dir = base.join(format!(".test_viewer_{unique}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_test_viewer() -> ViewerApp {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx);
    let mut config = AppConfig::default();
    config.render.zoom_option = ZoomOption::None;
    config.viewer.manga_mode = false;
    let mut app = ViewerApp::new(
        &cc,
        PathBuf::from("a.png"),
        PathBuf::from("a.png"),
        dummy_loaded_image(4, 4),
        dummy_loaded_image(4, 4),
        config,
        None,
        None,
        false,
        None,
        false,
        None,
    );
    app.active_request = None;
    app.pending_navigation_path = None;
    app
}

fn initialize_test_navigator(app: &mut ViewerApp, path: &Path) {
    app.current_navigation_path = path.to_path_buf();
    app.current_path = path.to_path_buf();
    app.sync_navigation_sort_with_filer_sort();
    if app.active_fs_request_id.is_none() {
        app.init_filesystem(path.to_path_buf()).unwrap();
    }
    let result = app
        .fs_rx
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    let FilesystemResult::NavigatorReady {
        navigation_path, ..
    } = result
    else {
        panic!("navigator did not become ready");
    };
    assert_eq!(navigation_path.as_deref(), Some(path));
    app.navigator_ready = true;
    app.active_fs_request_id = None;
}

fn receive_test_navigation(app: &mut ViewerApp) -> PathBuf {
    let result = app
        .fs_rx
        .as_ref()
        .unwrap()
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    app.active_fs_request_id = None;
    app.last_navigation_at = None;
    match result {
        FilesystemResult::PathResolved {
            navigation_path, ..
        } => navigation_path,
        FilesystemResult::CurrentSet => panic!("received CurrentSet instead of navigation"),
        FilesystemResult::NavigatorReady {
            navigation_path, ..
        } => panic!("received NavigatorReady: {navigation_path:?}"),
        FilesystemResult::NoPath { request_id } => {
            panic!("navigation request {request_id} returned NoPath")
        }
    }
}

#[test]
fn additional_review_descending_archive_edges_match_hidden_filer_controls() {
    use std::io::Write;
    for extension in ["zip", "lha"] {
        let root = make_temp_dir();
        let archive = root.join(format!("pages.{extension}"));
        let file = fs::File::create(&archive).unwrap();
        if extension == "zip" {
            let mut writer = zip::ZipWriter::new(file);
            for name in ["c.png", "a.png", "b.png"] {
                writer
                    .start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(b"image fixture").unwrap();
            }
            writer.finish().unwrap();
        } else {
            let mut writer = oxiarc_archive::LzhWriter::new(file);
            for name in ["c.png", "a.png", "b.png"] {
                writer.add_file(name, b"image fixture").unwrap();
            }
            writer.finish().unwrap();
        }
        let entries =
            crate::filesystem::list_openable_entries(&archive, NavigationSortOption::OsName);
        let mut app = make_test_viewer();
        app.show_filer = false;
        app.filer.ascending = false;
        initialize_test_navigator(&mut app, &entries[1]);
        app.first_image().unwrap();
        assert_eq!(
            receive_test_navigation(&mut app),
            entries[2],
            "{extension}: first"
        );
        app.last_image().unwrap();
        assert_eq!(
            receive_test_navigation(&mut app),
            entries[0],
            "{extension}: last"
        );
        app.pending_viewer_navigation = Some(PendingViewerNavigation::First);
        app.flush_pending_viewer_navigation();
        assert_eq!(
            receive_test_navigation(&mut app),
            entries[2],
            "{extension}: queued first"
        );
        app.pending_viewer_navigation = Some(PendingViewerNavigation::Last);
        app.flush_pending_viewer_navigation();
        assert_eq!(
            receive_test_navigation(&mut app),
            entries[0],
            "{extension}: queued last"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

fn filer_test_frame(
    app: &mut ViewerApp,
    subfiler: bool,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let ctx = app.egui_ctx.clone();
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1000.0, 800.0),
            )),
            events,
            ..Default::default()
        },
        |ctx| {
            if subfiler {
                app.subfiler_ui(ctx);
            } else {
                app.filer_ui(ctx);
            }
        },
    )
}

fn shape_text_position(shape: &egui::Shape, text: &str) -> Option<egui::Pos2> {
    match shape {
        egui::Shape::Text(shape) if shape.galley.text() == text => {
            Some(shape.pos + shape.galley.size() * 0.5)
        }
        egui::Shape::Vec(shapes) => shapes
            .iter()
            .find_map(|shape| shape_text_position(shape, text)),
        _ => None,
    }
}

#[test]
fn thumbnail_activation_can_clear_listing_while_visible_tiles_remain() {
    for subfiler in [false, true] {
        let root = make_temp_dir();
        let mut app = make_test_viewer();
        app.current_navigation_path = root.join("current.png");
        app.current_path = app.current_navigation_path.clone();
        app.show_filer = !subfiler;
        app.show_subfiler = subfiler;
        app.options.manga_right_to_left = false;
        app.filer.directory = Some(root.clone());
        app.filer.view_mode = FilerViewMode::ThumbnailSmall;
        let mut first = dummy_filer_entry("00_folder");
        first.path = root.join(if subfiler { "00.png" } else { "00_folder" });
        first.is_container = !subfiler;
        let mut second = dummy_filer_entry("01.png");
        second.path = root.join("01.png");
        if !subfiler {
            fs::create_dir(&first.path).unwrap();
        }
        app.filer.entries = vec![first, second];
        app.filer.pending_request_id = subfiler.then_some(77);
        let _ = filer_test_frame(&mut app, subfiler, vec![]);
        let output = filer_test_frame(&mut app, subfiler, vec![]);
        let text = if subfiler { "..." } else { "00_folder" };
        let mut pos = output
            .shapes
            .iter()
            .find_map(|shape| shape_text_position(&shape.shape, text))
            .expect("visible tile text");
        if !subfiler {
            pos.y -= 40.0;
        }
        let _ = filer_test_frame(
            &mut app,
            subfiler,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let _ = filer_test_frame(
            &mut app,
            subfiler,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(
            app.filer.entries.is_empty(),
            "tile activation should clear the live listing ({subfiler})"
        );
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn closing_refreshing_subfiler_allows_a_new_scan_when_reopened() {
    let root = make_temp_dir();
    let mut app = make_test_viewer();
    app.current_navigation_path = root.join("a.png");
    app.current_path = app.current_navigation_path.clone();
    app.show_filer = false;
    app.show_subfiler = true;
    app.filer.directory = Some(root.clone());
    app.filer.pending_request_id = Some(77);
    app.filer.pending_user_request = Some(FilerUserRequest::Refresh {
        directory: root.clone(),
        selected: None,
    });
    app.set_show_subfiler(false);
    assert!(app.filer.pending_request_id.is_none());
    assert!(app.filer.pending_user_request.is_none());
    app.set_show_subfiler(true);
    assert!(app.filer.pending_request_id.is_some());
    assert_ne!(app.filer.pending_request_id, Some(77));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn grayscale_toggle_invalidates_cached_and_inflight_preloads() {
    let mut app = make_test_viewer();
    app.preload_cache.push_back(dummy_preloaded_entry("b.png"));
    app.active_preload_request_id = Some(77);
    app.pending_preload_navigation_path = Some(PathBuf::from("c.png"));
    let ctx = app.egui_ctx.clone();
    app.apply_viewer_action(&ctx, ViewerAction::ToggleGrayscale);
    assert!(app.options.grayscale);
    assert!(app.preload_cache.is_empty());
    assert!(app.active_preload_request_id.is_none());
    assert!(app.pending_preload_navigation_path.is_none());
    assert!(!app.try_take_preloaded(Path::new("b.png")));
}

#[test]
fn cpu_preloads_match_navigation_zoom_and_reject_old_zoomed_pages() {
    let root = make_temp_dir();
    let first = root.join("a.png");
    let second = root.join("b.png");
    fs::write(&first, []).unwrap();
    fs::write(&second, []).unwrap();
    let mut app = make_test_viewer();
    app.current_navigation_path = first;
    app.navigator_ready = true;
    app.render_options.scale_mode = RenderScaleMode::PreciseCpu;
    app.zoom = 2.0;
    let (tx, rx) = mpsc::channel();
    app.preload_tx = tx;
    app.schedule_preload();
    let RenderCommand::LoadPath { zoom, path, .. } =
        rx.recv_timeout(Duration::from_secs(2)).unwrap()
    else {
        panic!("expected preload");
    };
    assert_eq!(path, second);
    assert_eq!(zoom, 1.0);

    let mut cached = dummy_preloaded_entry("b.png");
    cached.navigation_path = second.clone();
    cached.zoom = zoom;
    app.preload_cache.push_back(cached);
    assert!(app.try_take_preloaded(&second));
    assert_eq!(app.zoom, 1.0);
    assert_eq!(app.rendered.canvas.width(), app.source.canvas.width());
    let mut stale = dummy_preloaded_entry("c.png");
    stale.zoom = 2.0;
    stale.display.rendered = dummy_loaded_image(8, 8);
    app.preload_cache.push_back(stale);
    assert!(!app.try_take_preloaded(Path::new("c.png")));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn system_theme_tracks_the_current_os_theme_after_explicit_theme_selection() {
    let ctx = egui::Context::default();
    let _ = ctx.run(
        egui::RawInput {
            system_theme: Some(egui::Theme::Dark),
            ..Default::default()
        },
        |_| {},
    );
    crate::ui::theme::apply_window_theme(&ctx, WindowUiTheme::Light);
    assert!(!ctx.style().visuals.dark_mode);
    let _ = ctx.run(
        egui::RawInput {
            system_theme: Some(egui::Theme::Light),
            ..Default::default()
        },
        |_| {},
    );
    crate::ui::theme::apply_window_theme(&ctx, WindowUiTheme::System);
    assert!(!ctx.style().visuals.dark_mode);
    let _ = ctx.run(
        egui::RawInput {
            system_theme: Some(egui::Theme::Dark),
            ..Default::default()
        },
        |_| {},
    );
    assert!(ctx.style().visuals.dark_mode);
}

#[test]
fn build_settings_draft_starts_from_effective_keymap() {
    let config = AppConfig::default();
    let draft = build_settings_draft(&config);
    let defaults = crate::options::default_key_mapping();

    assert_eq!(draft.key_mapping_rows.len(), defaults.len());
    assert!(
        draft
            .key_mapping_rows
            .iter()
            .any(|row| row.binding == KeyBinding::new("F5") && row.action == ViewerAction::Reload)
    );
}

#[test]
fn build_settings_draft_canonicalizes_legacy_num_aliases() {
    let mut config = AppConfig::default();
    config.input.replace_default_keymap = true;
    config.input.key_mapping.insert(
        KeyBinding::new("Num0").with_shift(),
        ViewerAction::ZoomReset,
    );

    let draft = build_settings_draft(&config);

    assert!(draft.key_mapping_rows.iter().any(|row| {
        row.binding == KeyBinding::new("Numpad0").with_shift()
            && row.action == ViewerAction::ZoomReset
    }));
    assert!(
        !draft
            .key_mapping_rows
            .iter()
            .any(|row| row.binding == KeyBinding::new("Num0").with_shift())
    );
}

#[test]
fn remember_preloaded_entry_in_cache_keeps_two_most_recent_entries() {
    let mut cache = VecDeque::new();
    remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("a"));
    remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("b"));
    remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("c"));

    let paths = cache
        .iter()
        .map(|entry| entry.navigation_path.clone())
        .collect::<Vec<_>>();

    assert_eq!(paths, vec![PathBuf::from("c"), PathBuf::from("b")]);
}

#[test]
fn remember_preloaded_entry_in_cache_refreshes_existing_entry_recency() {
    let mut cache = VecDeque::new();
    remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("a"));
    remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("b"));
    remember_preloaded_entry_in_cache(&mut cache, dummy_preloaded_entry("a"));

    let paths = cache
        .iter()
        .map(|entry| entry.navigation_path.clone())
        .collect::<Vec<_>>();

    assert_eq!(paths, vec![PathBuf::from("a"), PathBuf::from("b")]);
}

#[test]
fn should_prioritize_companion_preload_until_visible_companion_is_ready() {
    let desired = Path::new("companion");

    assert!(should_prioritize_companion_preload(
        Some(desired),
        None,
        false,
    ));
    assert!(should_prioritize_companion_preload(
        Some(desired),
        Some(desired),
        false,
    ));
    assert!(!should_prioritize_companion_preload(
        Some(desired),
        Some(desired),
        true,
    ));
    assert!(!should_prioritize_companion_preload(None, None, false));
}

#[test]
fn transition_progress_clamps_to_complete() {
    let now = Instant::now();
    let mut started = Some(now - Duration::from_millis(250));
    let progress = transition_progress(&mut started, now, Duration::from_millis(100));

    assert_eq!(progress, 1.0);
}

#[test]
fn transition_clock_starts_on_first_draw() {
    let now = Instant::now();
    let mut started = None;
    assert_eq!(
        transition_progress(&mut started, now, Duration::from_millis(300)),
        0.0
    );
    assert_eq!(started, Some(now));
    let halfway = transition_progress(
        &mut started,
        now + Duration::from_millis(150),
        Duration::from_millis(300),
    );
    assert!((halfway - 0.5).abs() < 0.001);
    assert_eq!(
        transition_progress(
            &mut started,
            now + Duration::from_millis(300),
            Duration::from_millis(300),
        ),
        1.0
    );
}

#[test]
fn transition_frame_stats_report_percentiles_and_refresh_overruns() {
    let samples = (1..=100)
        .map(|gap_ms| TransitionFrameSample {
            gap: Duration::from_millis(gap_ms),
            expected_frame_period: Duration::from_millis(16),
        })
        .collect::<Vec<_>>();

    let stats = transition_frame_stats(&samples);

    assert_eq!(stats.p50_gap, Some(Duration::from_millis(50)));
    assert_eq!(stats.p95_gap, Some(Duration::from_millis(95)));
    assert_eq!(stats.p99_gap, Some(Duration::from_millis(99)));
    assert_eq!(stats.gaps_over_two_frame_periods, 68);
}

#[test]
fn transition_frame_stats_handle_no_draw_intervals() {
    let stats = transition_frame_stats(&[]);

    assert_eq!(stats.p50_gap, None);
    assert_eq!(stats.p95_gap, None);
    assert_eq!(stats.p99_gap, None);
    assert_eq!(stats.gaps_over_two_frame_periods, 0);
}

#[test]
fn transition_frame_recorder_counts_each_frame_once_and_includes_completion_gap() {
    let start = Instant::now();
    let period = Duration::from_millis(16);
    let mut recorder = TransitionFrameRecorder::default();

    recorder.record(12, start, period);
    recorder.record(12, start + Duration::from_millis(5), period);
    recorder.record(13, start + Duration::from_millis(16), period);
    recorder.record(14, start + Duration::from_millis(400), period);

    assert_eq!(recorder.draw_count, 3);
    assert_eq!(recorder.samples.len(), 2);
    assert_eq!(recorder.samples[0].gap, period);
    assert_eq!(recorder.samples[1].gap, Duration::from_millis(384));
}

#[test]
fn transition_frame_recorder_bounds_samples_without_losing_overrun_counts() {
    let mut recorder = TransitionFrameRecorder::default();
    let mut now = Instant::now();
    recorder.record(0, now, Duration::from_millis(1));
    let total = TRANSITION_FRAME_SAMPLE_CAPACITY + 100;
    for frame in 1..=total {
        now += Duration::from_millis(if frame % 10 == 0 { 3 } else { 1 });
        recorder.record(frame as u64, now, Duration::from_millis(1));
    }

    assert_eq!(recorder.samples.len(), TRANSITION_FRAME_SAMPLE_CAPACITY);
    assert_eq!(recorder.total_gap_count, total as u64);
    assert_eq!(recorder.gaps_over_two_frame_periods, (total / 10) as u64);
    assert_eq!(recorder.max_gap, Some(Duration::from_millis(3)));
}

#[test]
fn spiral_reveal_is_continuous_at_turn_boundaries() {
    let full = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 400.0));
    for boundary in [1.0 / 3.0, 2.0 / 3.0] {
        let before = spiral_reveal_rect(full, boundary - 0.0001);
        let after = spiral_reveal_rect(full, boundary + 0.0001);
        assert!((before.width() - after.width()).abs() < 2.0);
        assert!((before.height() - after.height()).abs() < 2.0);
    }
    let mut previous = spiral_reveal_rect(full, 0.0);
    for step in 1..=100 {
        let current = spiral_reveal_rect(full, step as f32 / 100.0);
        assert!(current.width() >= previous.width());
        assert!(current.height() >= previous.height());
        previous = current;
    }
}

#[test]
fn slide_transition_effect_reverses_for_backward_navigation() {
    assert_eq!(
        transition_effect_for_direction(
            TransitionEffect::SlideRightToLeft,
            Some(ImageTransitionDirection::Backward),
        ),
        TransitionEffect::SlideLeftToRight,
    );
    assert_eq!(
        transition_effect_for_direction(
            TransitionEffect::SlideLeftToRight,
            Some(ImageTransitionDirection::Backward),
        ),
        TransitionEffect::SlideRightToLeft,
    );
    assert_eq!(
        transition_effect_for_direction(
            TransitionEffect::SlideTopToBottom,
            Some(ImageTransitionDirection::Backward),
        ),
        TransitionEffect::SlideBottomToTop,
    );
    assert_eq!(
        transition_effect_for_direction(
            TransitionEffect::SlideBottomToTop,
            Some(ImageTransitionDirection::Backward),
        ),
        TransitionEffect::SlideTopToBottom,
    );
}

#[test]
fn outgoing_slide_keeps_original_image_dimensions_when_next_image_is_smaller() {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    let previous = egui::Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(600.0, 600.0));
    let next = egui::Rect::from_min_size(egui::pos2(300.0, 150.0), egui::vec2(200.0, 300.0));
    let halfway = slide_transition_offset(TransitionEffect::SlideRightToLeft, viewport.size(), 0.5);

    assert_eq!(previous.translate(halfway.previous).size(), previous.size());
    assert_eq!(next.translate(halfway.current).size(), next.size());
    assert_eq!(halfway.previous.x, -400.0);
    assert_eq!(halfway.current.x, 400.0);
    let done = slide_transition_offset(TransitionEffect::SlideRightToLeft, viewport.size(), 1.0);
    assert!(!slide_layer_clip(viewport, done.previous).is_positive());
    assert_eq!(slide_layer_clip(viewport, done.current), viewport);
}

#[test]
fn scanning_a_folder_shows_waiting_card_before_first_image() {
    assert!(waiting_card_should_show(false, true));
    assert!(waiting_card_should_show(true, false));
    assert!(!waiting_card_should_show(false, false));
}

#[test]
fn hidden_filer_defers_large_directory_scan_until_a_filer_is_visible() {
    assert!(should_defer_filer_scan(false, false));
    assert!(!should_defer_filer_scan(true, false));
    assert!(!should_defer_filer_scan(false, true));
}

#[test]
fn directory_load_commits_the_path_returned_by_the_render_worker() {
    let folder = crate::test_support::make_test_dir("loaded-navigation-path");
    let first = folder.join("001.png");
    let loaded = folder.join("002.png");
    std::fs::write(&first, b"first").unwrap();
    std::fs::write(&loaded, b"second").unwrap();

    assert_eq!(
        resolved_navigation_path_for_load(folder, Some(&loaded)),
        loaded
    );
}

#[test]
fn non_slide_transition_effects_ignore_navigation_direction() {
    for effect in [
        TransitionEffect::None,
        TransitionEffect::Fade,
        TransitionEffect::SpiralWipeIn,
        TransitionEffect::SpiralWipeOut,
    ] {
        assert_eq!(
            transition_effect_for_direction(effect, Some(ImageTransitionDirection::Backward)),
            effect,
        );
    }
}

#[test]
fn folder_refresh_signature_changes_when_directory_changes() {
    let root = make_temp_dir();
    let before = folder_refresh_signature(&root).expect("initial signature");

    fs::write(root.join("added.png"), []).unwrap();
    let after = folder_refresh_signature(&root).expect("updated signature");

    assert_ne!(before, after);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn snapshot_only_clears_refresh_user_request() {
    assert!(should_clear_filer_user_request_after_snapshot(Some(
        &FilerUserRequest::Refresh {
            directory: PathBuf::from("dir"),
            selected: None,
        },
    )));
    assert!(!should_clear_filer_user_request_after_snapshot(Some(
        &FilerUserRequest::BrowseDirectory {
            directory: PathBuf::from("dir"),
        },
    )));
    assert!(!should_clear_filer_user_request_after_snapshot(Some(
        &FilerUserRequest::SelectFile {
            navigation_path: PathBuf::from("dir\\file"),
        },
    )));
}

#[test]
fn zip_to_zip_bench_plan_is_available() {
    let (name, actions) = bench_automation_plan(Some("zip_to_zip"));

    assert_eq!(name, "zip_to_zip");
    assert!(actions.contains(&BenchAction::BrowseSiblingContainer));
}

#[test]
fn zip_to_zip_random_bench_plan_is_available() {
    let (name, actions) = bench_automation_plan(Some("zip_to_zip_random"));

    assert_eq!(name, "zip_to_zip_random");
    assert!(actions.contains(&BenchAction::BrowseRandomContainer));
    assert!(actions.contains(&BenchAction::SelectRandomFileFromFiler));
    assert!(actions.contains(&BenchAction::Next));
    assert!(actions.contains(&BenchAction::Prev));
    assert_eq!(
        actions
            .iter()
            .filter(|action| **action == BenchAction::BrowseRandomContainer)
            .count(),
        ZIP_TO_ZIP_RANDOM_WALK_ROUNDS,
    );
    assert_eq!(
        actions
            .iter()
            .filter(|action| **action == BenchAction::RefreshFiler)
            .count(),
        ZIP_TO_ZIP_RANDOM_WALK_ROUNDS,
    );
}

#[test]
fn snapshot_does_not_clear_browse_user_request_directly() {
    assert!(!should_clear_filer_user_request_after_snapshot(Some(
        &FilerUserRequest::BrowseDirectory {
            directory: PathBuf::from("dir"),
        },
    )));
}

#[test]
fn branch_change_requires_filesystem_reinit_after_load() {
    let first_archive_entry = Path::new("a.zip").join("__zipv__").join("0001.jpg");
    let second_archive_entry = Path::new("b.zip").join("__zipv__").join("0001.jpg");
    let first_archive_next_entry = Path::new("a.zip").join("__zipv__").join("0002.jpg");

    assert!(should_reinitialize_filesystem_after_load(
        &first_archive_entry,
        &second_archive_entry,
    ));
    assert!(!should_reinitialize_filesystem_after_load(
        &first_archive_entry,
        &first_archive_next_entry,
    ));
}

#[test]
fn load_failure_only_auto_advances_when_current_image_failed() {
    assert!(should_advance_after_load_failure(
        Path::new("dir\\current.png"),
        Some(Path::new("dir\\current.png")),
    ));
    assert!(!should_advance_after_load_failure(
        Path::new("dir\\current.png"),
        Some(Path::new("dir\\other.png")),
    ));
    assert!(!should_advance_after_load_failure(
        Path::new("dir\\current.png"),
        None,
    ));
}

#[test]
fn clears_matching_filer_select_request_for_current_path() {
    assert!(should_clear_filer_select_request_for_current(
        Some(&FilerUserRequest::SelectFile {
            navigation_path: PathBuf::from("dir\\current.png"),
        }),
        Path::new("dir\\current.png"),
    ));
    assert!(!should_clear_filer_select_request_for_current(
        Some(&FilerUserRequest::SelectFile {
            navigation_path: PathBuf::from("dir\\other.png"),
        }),
        Path::new("dir\\current.png"),
    ));
    assert!(!should_clear_filer_select_request_for_current(
        Some(&FilerUserRequest::BrowseDirectory {
            directory: PathBuf::from("dir"),
        }),
        Path::new("dir\\current.png"),
    ));
}

#[test]
fn clears_stale_filer_refresh_request_after_directory_change() {
    assert!(should_clear_stale_filer_refresh_request(
        Some(&FilerUserRequest::Refresh {
            directory: PathBuf::from("dir-a"),
            selected: Some(PathBuf::from("dir-a\\current.png")),
        }),
        Some(Path::new("dir-b")),
    ));
    assert!(!should_clear_stale_filer_refresh_request(
        Some(&FilerUserRequest::Refresh {
            directory: PathBuf::from("dir-a"),
            selected: Some(PathBuf::from("dir-a\\current.png")),
        }),
        Some(Path::new("dir-a")),
    ));
    assert!(!should_clear_stale_filer_refresh_request(
        Some(&FilerUserRequest::SelectFile {
            navigation_path: PathBuf::from("dir-a\\current.png"),
        }),
        Some(Path::new("dir-b")),
    ));
    assert!(!should_clear_stale_filer_refresh_request(
        Some(&FilerUserRequest::Refresh {
            directory: PathBuf::from("dir-a"),
            selected: None,
        }),
        None,
    ));
}

#[test]
fn clears_stale_committed_browse_only_when_filer_is_hidden_and_idle() {
    assert!(should_clear_stale_committed_browse_for_viewer_navigation(
        false, None,
    ));
    assert!(!should_clear_stale_committed_browse_for_viewer_navigation(
        true, None,
    ));
    assert!(!should_clear_stale_committed_browse_for_viewer_navigation(
        false,
        Some(&FilerUserRequest::BrowseDirectory {
            directory: PathBuf::from("dir-a"),
        }),
    ));
}

#[test]
fn clears_stale_committed_browse_when_filer_is_aligned_to_current_dir() {
    assert!(should_clear_stale_committed_browse_when_filer_aligned(
        Some(Path::new("dir-a")),
        Path::new("dir-a"),
        None,
    ));
    assert!(!should_clear_stale_committed_browse_when_filer_aligned(
        Some(Path::new("dir-b")),
        Path::new("dir-a"),
        None,
    ));
    assert!(!should_clear_stale_committed_browse_when_filer_aligned(
        Some(Path::new("dir-a")),
        Path::new("dir-a"),
        Some(&FilerUserRequest::BrowseDirectory {
            directory: PathBuf::from("dir-a"),
        }),
    ));
}

#[test]
fn clears_browse_or_refresh_request_when_filer_is_hidden() {
    assert!(should_clear_filer_request_on_hide(Some(
        &FilerUserRequest::BrowseDirectory {
            directory: PathBuf::from("dir-a"),
        }
    )));
    assert!(should_clear_filer_request_on_hide(Some(
        &FilerUserRequest::Refresh {
            directory: PathBuf::from("dir-a"),
            selected: None,
        }
    )));
    assert!(!should_clear_filer_request_on_hide(Some(
        &FilerUserRequest::SelectFile {
            navigation_path: PathBuf::from("dir-a\\current.png"),
        }
    )));
    assert!(!should_clear_filer_request_on_hide(None));
}

#[test]
fn hands_off_filer_control_when_viewer_navigation_starts() {
    assert!(should_handoff_filer_control_to_viewer_navigation(
        None,
        Some(Path::new("dir-a")),
    ));
    assert!(!should_handoff_filer_control_to_viewer_navigation(
        Some(&FilerUserRequest::BrowseDirectory {
            directory: PathBuf::from("dir-a"),
        }),
        Some(Path::new("dir-a")),
    ));
    assert!(!should_handoff_filer_control_to_viewer_navigation(
        None, None,
    ));
}

#[test]
fn cancels_browse_or_refresh_request_when_viewer_navigation_starts() {
    assert!(should_cancel_filer_request_for_viewer_navigation(Some(
        &FilerUserRequest::BrowseDirectory {
            directory: PathBuf::from("dir-a"),
        }
    )));
    assert!(should_cancel_filer_request_for_viewer_navigation(Some(
        &FilerUserRequest::Refresh {
            directory: PathBuf::from("dir-a"),
            selected: Some(PathBuf::from("dir-a\\a.png")),
        }
    )));
    assert!(!should_cancel_filer_request_for_viewer_navigation(Some(
        &FilerUserRequest::SelectFile {
            navigation_path: PathBuf::from("dir-a\\a.png"),
        }
    )));
}

#[test]
fn syncs_filer_selected_with_current_only_when_aligned_and_idle() {
    assert!(should_sync_filer_selected_with_current(
        None,
        Some(Path::new("dir-a")),
        Some(Path::new("dir-a")),
    ));
    assert!(!should_sync_filer_selected_with_current(
        Some(&FilerUserRequest::BrowseDirectory {
            directory: PathBuf::from("dir-a"),
        }),
        Some(Path::new("dir-a")),
        Some(Path::new("dir-a")),
    ));
    assert!(!should_sync_filer_selected_with_current(
        None,
        Some(Path::new("dir-a")),
        Some(Path::new("dir-b")),
    ));
}

#[test]
fn skips_edge_navigation_when_target_is_current() {
    assert!(should_skip_edge_navigation_for_same_target(
        Path::new("dir-a\\a.png"),
        Path::new("dir-a\\a.png"),
        PendingViewerNavigation::First,
    ));
    assert!(!should_skip_edge_navigation_for_same_target(
        Path::new("dir-a\\a.png"),
        Path::new("dir-a\\b.png"),
        PendingViewerNavigation::Last,
    ));
}

#[test]
fn skips_edge_navigation_when_container_edge_is_already_current() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("wml2viewer-edge-noop-{unique}"));
    let container = root.join("container");
    let first = container.join("001.png");
    let last = container.join("999.png");
    fs::create_dir_all(&container).unwrap();
    fs::write(&first, []).unwrap();
    fs::write(&last, []).unwrap();

    assert!(should_skip_edge_navigation_for_same_target(
        &first,
        &container,
        PendingViewerNavigation::First,
    ));
    assert!(should_skip_edge_navigation_for_same_target(
        &last,
        &container,
        PendingViewerNavigation::Last,
    ));
    assert!(!should_skip_edge_navigation_for_same_target(
        &first,
        &container,
        PendingViewerNavigation::Last,
    ));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn applies_edge_noop_only_when_filer_is_hidden_or_aligned() {
    assert!(should_apply_edge_noop(
        PendingViewerNavigation::Last,
        false,
        Some(Path::new("parent")),
        Some(Path::new("child")),
    ));
    assert!(should_apply_edge_noop(
        PendingViewerNavigation::First,
        true,
        Some(Path::new("same")),
        Some(Path::new("same")),
    ));
    assert!(!should_apply_edge_noop(
        PendingViewerNavigation::Last,
        true,
        Some(Path::new("parent")),
        Some(Path::new("child")),
    ));
    assert!(!should_apply_edge_noop(
        PendingViewerNavigation::Next,
        true,
        Some(Path::new("same")),
        Some(Path::new("same")),
    ));
}

#[test]
fn maps_filer_sort_to_navigation_sort() {
    assert_eq!(
        navigation_sort_for_filer(FilerSortField::Name, NameSortMode::Os),
        NavigationSortOption::OsName,
    );
    assert_eq!(
        navigation_sort_for_filer(FilerSortField::Name, NameSortMode::CaseSensitive),
        NavigationSortOption::NameCaseSensitive,
    );
    assert_eq!(
        navigation_sort_for_filer(FilerSortField::Name, NameSortMode::CaseInsensitive),
        NavigationSortOption::NameCaseInsensitive,
    );
    assert_eq!(
        navigation_sort_for_filer(FilerSortField::Modified, NameSortMode::Os),
        NavigationSortOption::Date,
    );
    assert_eq!(
        navigation_sort_for_filer(FilerSortField::Size, NameSortMode::CaseInsensitive),
        NavigationSortOption::Size,
    );
}

#[test]
fn queues_filesystem_init_when_request_is_already_active() {
    assert!(should_queue_filesystem_init(Some(1)));
    assert!(!should_queue_filesystem_init(None));
}

#[test]
fn queued_filesystem_init_is_not_overwritten_by_navigation_queue() {
    let mut queued_init = None;
    queue_filesystem_init_path(&mut queued_init, PathBuf::from("dir-a"));
    let mut queued_navigation = None;
    queue_navigation_command(
        &mut queued_navigation,
        FilesystemCommand::Next {
            request_id: 0,
            policy: EndOfFolderOption::Recursive,
        },
        Some(ImageTransitionDirection::Forward),
    );
    queue_navigation_command(
        &mut queued_navigation,
        FilesystemCommand::Prev {
            request_id: 0,
            policy: EndOfFolderOption::Recursive,
        },
        Some(ImageTransitionDirection::Backward),
    );

    assert_eq!(queued_init, Some(PathBuf::from("dir-a")));
    assert!(matches!(
        queued_navigation,
        Some(QueuedNavigation {
            command: FilesystemCommand::Prev {
                policy: EndOfFolderOption::Recursive,
                ..
            },
            transition_direction: Some(ImageTransitionDirection::Backward),
        })
    ));
}

#[test]
fn queued_filesystem_work_prioritizes_init_before_navigation() {
    let mut queued_init = Some(PathBuf::from("dir-a"));
    let mut queued_navigation = Some(QueuedNavigation {
        command: FilesystemCommand::Next {
            request_id: 0,
            policy: EndOfFolderOption::Recursive,
        },
        transition_direction: Some(ImageTransitionDirection::Forward),
    });

    let first = take_next_queued_filesystem_work(&mut queued_init, &mut queued_navigation);
    let second = take_next_queued_filesystem_work(&mut queued_init, &mut queued_navigation);

    assert!(matches!(
        first,
        Some(PendingFilesystemWork::Init(path)) if path == PathBuf::from("dir-a")
    ));
    assert!(matches!(
        second,
        Some(PendingFilesystemWork::Command(QueuedNavigation {
            command: FilesystemCommand::Next {
                policy: EndOfFolderOption::Recursive,
                ..
            },
            transition_direction: Some(ImageTransitionDirection::Forward),
        }))
    ));
    assert!(queued_init.is_none());
    assert!(queued_navigation.is_none());
}

#[test]
fn defers_companion_sync_while_primary_load_is_active() {
    assert!(should_defer_companion_sync_during_primary_load(Some(
        ActiveRenderRequest::Load(7),
    )));
    assert!(!should_defer_companion_sync_during_primary_load(Some(
        ActiveRenderRequest::Resize(7),
    )));
    assert!(!should_defer_companion_sync_during_primary_load(None));
}

#[test]
fn cancels_busy_filesystem_request_for_matching_filer_select() {
    let pending = FilerUserRequest::SelectFile {
        navigation_path: PathBuf::from("dir\\current.png"),
    };

    assert!(should_cancel_filesystem_request_for_filer_select(
        Some(&pending),
        Path::new("dir\\current.png"),
        Some(7),
    ));
    assert!(!should_cancel_filesystem_request_for_filer_select(
        Some(&pending),
        Path::new("dir\\other.png"),
        Some(7),
    ));
    assert!(!should_cancel_filesystem_request_for_filer_select(
        Some(&pending),
        Path::new("dir\\current.png"),
        None,
    ));
}

#[test]
fn detects_filer_snapshot_change_in_same_directory_only() {
    assert!(!filer_snapshot_changed_in_same_directory(
        None,
        Path::new("dir-a"),
        10
    ));
    assert!(!filer_snapshot_changed_in_same_directory(
        Some((Path::new("dir-a"), 10)),
        Path::new("dir-a"),
        10,
    ));
    assert!(filer_snapshot_changed_in_same_directory(
        Some((Path::new("dir-a"), 10)),
        Path::new("dir-a"),
        11,
    ));
    assert!(!filer_snapshot_changed_in_same_directory(
        Some((Path::new("dir-a"), 10)),
        Path::new("dir-b"),
        10,
    ));
}

#[test]
fn reinit_snapshot_only_when_current_is_missing_or_misaligned() {
    let entries = vec![
        dummy_filer_entry("dir\\001.png"),
        dummy_filer_entry("dir\\002.png"),
    ];
    assert!(!should_reinitialize_filesystem_from_filer_snapshot(
        Path::new("dir\\001.png"),
        Some(Path::new("dir")),
        Some(Path::new("dir")),
        &entries,
        Some(Path::new("dir\\001.png")),
    ));
    assert!(should_reinitialize_filesystem_from_filer_snapshot(
        Path::new("dir\\003.png"),
        Some(Path::new("dir")),
        Some(Path::new("dir")),
        &entries,
        Some(Path::new("dir\\001.png")),
    ));
    assert!(should_reinitialize_filesystem_from_filer_snapshot(
        Path::new("dir\\001.png"),
        Some(Path::new("dir")),
        Some(Path::new("dir")),
        &entries,
        Some(Path::new("dir\\002.png")),
    ));
    assert!(!should_reinitialize_filesystem_from_filer_snapshot(
        Path::new("dir\\001.png"),
        Some(Path::new("dir-a")),
        Some(Path::new("dir-b")),
        &entries,
        Some(Path::new("dir\\001.png")),
    ));
}

#[test]
fn spread_companion_path_for_navigation_uses_same_branch_neighbor() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("wml2viewer-spread-{unique}"));
    let first = root.join("001.png");
    let second = root.join("002.png");
    fs::create_dir_all(&root).unwrap();
    fs::write(&first, []).unwrap();
    fs::write(&second, []).unwrap();

    let companion =
        spread_companion_path_for_navigation(&first, NavigationSortOption::Name, 1, true);

    assert_eq!(companion.as_deref(), Some(second.as_path()));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn companion_lookup_reuses_result_until_key_changes() {
    let mut cache = None;
    let key = MangaCompanionLookupKey {
        navigation_path: PathBuf::from("001.png"),
        sort: NavigationSortOption::Name,
        direction: 1,
        branch_path: Some(PathBuf::from("pages")),
        branch_modified: None,
        branch_len: Some(2),
    };
    let lookups = std::cell::Cell::new(0);
    let resolve = || {
        lookups.set(lookups.get() + 1);
        Some(PathBuf::from("002.png"))
    };
    assert_eq!(
        cached_spread_companion_path(&mut cache, key.clone(), resolve),
        Some(PathBuf::from("002.png"))
    );
    assert_eq!(
        cached_spread_companion_path(&mut cache, key.clone(), resolve),
        Some(PathBuf::from("002.png"))
    );
    assert_eq!(lookups.get(), 1);

    let changed = MangaCompanionLookupKey {
        branch_len: Some(3),
        ..key
    };
    assert_eq!(
        cached_spread_companion_path(&mut cache, changed, resolve),
        Some(PathBuf::from("002.png"))
    );
    assert_eq!(lookups.get(), 2);
}

#[test]
fn startup_layout_settles_with_repaint_until_viewport_is_stable() {
    assert!(startup_layout_is_settling(
        1,
        egui::vec2(320.0, 240.0),
        egui::Vec2::ZERO,
    ));
    assert!(!startup_layout_is_settling(
        STARTUP_LAYOUT_SETTLE_FRAMES,
        egui::vec2(768.0, 432.0),
        egui::vec2(768.0, 432.0),
    ));
}

#[test]
fn fit_layout_recalculates_after_startup_settling_when_pending() {
    assert!(should_recalculate_fit_layout(
        false,
        egui::vec2(768.0, 432.0),
        egui::vec2(768.0, 432.0),
        true,
        &ZoomOption::FitScreen,
    ));
    assert!(!should_recalculate_fit_layout(
        true,
        egui::vec2(768.0, 432.0),
        egui::vec2(768.0, 432.0),
        true,
        &ZoomOption::FitScreen,
    ));
    assert!(!should_recalculate_fit_layout(
        false,
        egui::vec2(768.0, 432.0),
        egui::vec2(320.0, 240.0),
        true,
        &ZoomOption::None,
    ));
}
