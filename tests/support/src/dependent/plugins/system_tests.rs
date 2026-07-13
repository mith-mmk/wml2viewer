use super::{decode_from_bytes, decode_from_file};
use png::{BitDepth, ColorType, Encoder};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const TINY_PNG: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E',
    b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
];

fn temp_png_path() -> PathBuf {
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
                    .map(std::path::Path::to_path_buf)
            })
        })
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .join(".test_wml2viewer");
    std::fs::create_dir_all(&base).unwrap();
    let path = base.join(format!(".test_system_decoder_{unique}.png"));
    std::fs::write(&path, TINY_PNG).unwrap();
    path
}

#[test]
fn system_decoder_loads_png_sample() {
    let path = temp_png_path();
    let decoded = decode_from_file(&path, None).expect("system decoder should load PNG");
    assert_eq!(decoded.canvas.width(), 1);
    assert_eq!(decoded.canvas.height(), 1);
    assert_eq!(decoded.canvas.buffer(), [255, 0, 0, 255]);
    let _ = std::fs::remove_file(path);
}

#[test]
fn system_decoder_loads_png_bytes() {
    let decoded = decode_from_bytes(TINY_PNG, Some(PathBuf::from("sample.png").as_path()), None)
        .expect("system decoder should load PNG bytes");
    assert_eq!(decoded.canvas.width(), 1);
    assert_eq!(decoded.canvas.height(), 1);
    assert_eq!(decoded.canvas.buffer(), [255, 0, 0, 255]);
}

#[test]
fn system_decoder_rejects_empty_and_corrupt_bytes() {
    assert!(decode_from_bytes(&[], None, None).is_none());
    assert!(decode_from_bytes(b"not an image", None, None).is_none());
}

#[test]
fn system_decoder_preserves_top_to_bottom_row_order() {
    let mut png = Vec::new();
    {
        let mut encoder = Encoder::new(&mut png, 1, 2);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&[255, 0, 0, 255, 0, 0, 255, 255])
            .unwrap();
    }

    let decoded = decode_from_bytes(&png, Some(PathBuf::from("rows.png").as_path()), None)
        .expect("system decoder should load generated PNG");
    assert_eq!(decoded.canvas.width(), 1);
    assert_eq!(decoded.canvas.height(), 2);
    assert_eq!(decoded.canvas.buffer(), [255, 0, 0, 255, 0, 0, 255, 255]);
}
