use std::sync::atomic::{AtomicBool, Ordering};
use wml2::draw::{
    DecodeCancelledError, DecodeLimitError, DecodeLimitKind, DecodeLimits, image_from_with_limits,
    image_from_with_limits_and_cancel,
};

const ANDROID_LIMITS: DecodeLimits = DecodeLimits {
    maximum_frame_pixels: 100_000_000,
    maximum_frames: 4_096,
    maximum_rgba_bytes: 512 * 1024 * 1024,
};

fn png_chunk(name: &[u8; 4], payload: &[u8], output: &mut Vec<u8>) {
    output.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    output.extend_from_slice(name);
    output.extend_from_slice(payload);
    output.extend_from_slice(&0_u32.to_be_bytes());
}

fn rgba_png(width: u32, height: u32, decompressed: &[u8]) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png_chunk(b"IHDR", &ihdr, &mut png);
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(decompressed, 9);
    png_chunk(b"IDAT", &compressed, &mut png);
    png_chunk(b"IEND", &[], &mut png);
    png
}

fn gif_with_frames(frame_count: usize) -> Vec<u8> {
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
    gif.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
    for _ in 0..frame_count {
        gif.push(0x2c);
        gif.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0, 0]);
        gif.extend_from_slice(&[2, 3, 0x44, 0x01, 0, 0]);
    }
    gif.push(0x3b);
    gif
}

fn decode_limit<'a>(error: &'a (dyn std::error::Error + 'static)) -> &'a DecodeLimitError {
    error
        .downcast_ref::<DecodeLimitError>()
        .unwrap_or_else(|| panic!("expected DecodeLimitError, got {error}"))
}

#[test]
fn compressed_png_bomb_stops_at_the_exact_scanline_budget() {
    let png = rgba_png(1, 1, &vec![0; 1024 * 1024]);
    assert!(png.len() < 4_096, "test bomb must stay compact");

    let error = image_from_with_limits(&png, ANDROID_LIMITS)
        .err()
        .expect("compressed bomb must fail");
    let limit = decode_limit(error.as_ref());
    assert_eq!(limit.kind(), DecodeLimitKind::DecodedBytes);
    assert_eq!(limit.limit(), 5);
    assert_eq!(limit.attempted(), 6);
    assert_eq!(limit.accepted_frames(), 0);
    assert_eq!(limit.accepted_rgba_bytes(), 4);
}

#[test]
fn huge_png_dimensions_stop_before_canvas_allocation() {
    let png = rgba_png(10_001, 10_000, &[0, 0, 0, 0, 0]);
    let error = image_from_with_limits(&png, ANDROID_LIMITS)
        .err()
        .expect("oversized canvas must fail");
    let limit = decode_limit(error.as_ref());
    assert_eq!(limit.kind(), DecodeLimitKind::Pixels);
    assert_eq!(limit.attempted(), 100_010_000);
    assert_eq!(limit.limit(), 100_000_000);
    assert_eq!(limit.accepted_rgba_bytes(), 0);
}

#[test]
fn sixty_five_small_gif_frames_remain_compatible() {
    let image = image_from_with_limits(&gif_with_frames(65), ANDROID_LIMITS).unwrap();
    assert_eq!(image.animation.as_ref().map(Vec::len), Some(65));
}

#[test]
fn gif_frame_4097_is_rejected_before_its_payload_decode() {
    let gif = gif_with_frames(4_097);
    assert!(gif.len() < 80 * 1024, "test animation must stay compact");
    let error = image_from_with_limits(&gif, ANDROID_LIMITS)
        .err()
        .expect("frame 4097 must fail");
    let limit = decode_limit(error.as_ref());
    assert_eq!(limit.kind(), DecodeLimitKind::Frames);
    assert_eq!(limit.attempted(), 4_097);
    assert_eq!(limit.limit(), 4_096);
    assert_eq!(limit.accepted_frames(), 4_096);
    assert_eq!(limit.accepted_rgba_bytes(), 4 + 4_096 * 4);
}

#[test]
fn animation_aggregate_is_charged_before_the_rejected_frame_decode() {
    let limits = DecodeLimits {
        maximum_rgba_bytes: 16,
        ..ANDROID_LIMITS
    };
    let error = image_from_with_limits(&gif_with_frames(4), limits)
        .err()
        .expect("aggregate overflow must fail");
    let limit = decode_limit(error.as_ref());
    assert_eq!(limit.kind(), DecodeLimitKind::RgbaBytes);
    assert_eq!(limit.attempted(), 20);
    assert_eq!(limit.limit(), 16);
    assert_eq!(limit.accepted_frames(), 3);
    assert_eq!(limit.accepted_rgba_bytes(), 16);
}

#[test]
fn cancellation_is_checked_before_canvas_allocation() {
    let cancelled = AtomicBool::new(true);
    let probe = || cancelled.load(Ordering::Relaxed);
    let error = image_from_with_limits_and_cancel(
        &rgba_png(1, 1, &[0, 0, 0, 0, 0]),
        ANDROID_LIMITS,
        Some(&probe),
    )
    .err()
    .expect("cancelled decode must fail");
    assert!(error.downcast_ref::<DecodeCancelledError>().is_some());
}
