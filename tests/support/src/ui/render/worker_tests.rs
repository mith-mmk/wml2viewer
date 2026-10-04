use super::{
    RenderCommand, RenderLoadMetrics, RenderResult, load_render_page, spawn_render_worker,
};
use crate::drawers::affine::InterpolationAlgorithm;
use crate::drawers::canvas::Canvas;
use crate::drawers::image::LoadedImage;
use crate::options::NavigationSortOption;
use crate::ui::viewer::options::RenderScaleMode;
use oxiarc_archive::LzhWriter;
use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

#[test]
fn resize_uses_the_displayed_source_even_after_a_cache_hit() {
    let image = |width, height| LoadedImage {
        canvas: Canvas::new(width, height),
        animation: Vec::new(),
        loop_count: None,
    };
    let (tx, rx, join) = spawn_render_worker(image(1, 1));
    tx.send(RenderCommand::ResizeCurrent {
        request_id: 42,
        source: image(7, 9),
        zoom: 1.0,
        method: InterpolationAlgorithm::Bilinear,
        scale_mode: RenderScaleMode::FastGpu,
        max_texture_side: 4,
    })
    .unwrap();
    match rx.recv_timeout(Duration::from_secs(2)).unwrap() {
        RenderResult::Loaded {
            request_id,
            source,
            prepared_texture,
            ..
        } => {
            assert_eq!(request_id, 42);
            assert_eq!((source.canvas.width(), source.canvas.height()), (7, 9));
            let prepared =
                prepared_texture.expect("resize should prepare the texture off the UI thread");
            assert_eq!(prepared.image.size, [3, 4]);
        }
        RenderResult::Failed { message, .. } => panic!("resize failed: {message}"),
    }
    tx.send(RenderCommand::Shutdown).unwrap();
    join.join().unwrap();
}

const TINY_PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E',
    b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
];

fn make_lha_with_entries(path: &Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let mut lha = LzhWriter::new(file);
    for (name, bytes) in entries {
        lha.add_file(name, bytes).unwrap();
    }
    lha.finish().unwrap();
}

#[test]
fn render_load_metrics_default_is_zeroed() {
    let metrics = RenderLoadMetrics::default();

    assert_eq!(metrics.resolve_ms, 0);
    assert_eq!(metrics.read_ms, 0);
    assert_eq!(metrics.decode_ms, 0);
    assert_eq!(metrics.resize_ms, 0);
    assert!(!metrics.used_virtual_bytes);
    assert!(!metrics.decoded_from_bytes);
    assert!(metrics.source_bytes_len.is_none());
    assert!(metrics.resolved_path.is_none());
}

#[test]
fn prepared_texture_matches_ui_downscale_and_color_conversion() {
    use crate::ui::render::layout::canvas_to_color_image;
    use crate::ui::render::texture::downscale_for_texture_limit;

    let mut canvas = Canvas::new(5, 4);
    for y in 0..4 {
        for x in 0..5 {
            let offset = ((y * 5 + x) * 4) as usize;
            canvas.buffer_mut()[offset..offset + 4].copy_from_slice(&[
                x as u8 * 33,
                y as u8 * 41,
                77,
                255,
            ]);
        }
    }
    let method = InterpolationAlgorithm::Bilinear;
    let prepared = super::prepare_texture(&canvas, 2, method);
    let (expected_canvas, expected_scale) = downscale_for_texture_limit(&canvas, 2, method);
    assert_eq!(prepared.display_scale, expected_scale);
    assert_eq!(*prepared.image, canvas_to_color_image(&expected_canvas));
}

#[test]
fn render_loads_lha_virtual_child() {
    let dir = crate::test_support::make_test_dir("render");
    let archive = dir.join("images.lzh");
    make_lha_with_entries(&archive, &[("001.png", TINY_PNG)]);
    let child = crate::filesystem::list_browser_entries(&archive, NavigationSortOption::OsName)
        .into_iter()
        .next()
        .expect("LZH should expose a virtual image child");
    let latest_request_id = AtomicU64::new(1);

    let page = load_render_page(
        &child,
        1,
        &latest_request_id,
        1.0,
        InterpolationAlgorithm::Bilinear,
        RenderScaleMode::FastGpu,
        2,
    )
    .expect("render load should not fail")
    .expect("render load should complete");

    assert!(page.metrics.used_virtual_bytes);
    assert!(page.metrics.decoded_from_bytes);
    assert_eq!(page.source.canvas.width(), 1);
    assert_eq!(page.source.canvas.height(), 1);

    let _ = fs::remove_dir_all(dir);
}
