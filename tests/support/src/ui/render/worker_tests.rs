use super::{RenderLoadMetrics, load_render_page};
use crate::drawers::affine::InterpolationAlgorithm;
use crate::options::NavigationSortOption;
use crate::ui::viewer::options::RenderScaleMode;
use oxiarc_archive::LzhWriter;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::time::{SystemTime, UNIX_EPOCH};

const TINY_PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E',
    b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
];

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
    let dir = base.join(format!(".test_render_{unique}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

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
fn render_loads_lha_virtual_child() {
    let dir = make_temp_dir();
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
    )
    .expect("render load should not fail")
    .expect("render load should complete");

    assert!(page.metrics.used_virtual_bytes);
    assert!(page.metrics.decoded_from_bytes);
    assert_eq!(page.source.canvas.width(), 1);
    assert_eq!(page.source.canvas.height(), 1);

    let _ = fs::remove_dir_all(dir);
}
