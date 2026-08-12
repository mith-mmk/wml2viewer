use crate::CoreErrorKind;
use crate::image::{
    AnimationFrame, DecodeLimits, DecodeRequest, DecodedImage, EncodeFormat, EncodeRequest,
    RgbaImage, decode, decode_with_limits, decoded_from_wml2_for_test, encode,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use wml2::draw::{AnimationLayer, ImageBuffer, NextOptions};

fn sample() -> DecodedImage {
    DecodedImage {
        poster: RgbaImage::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128]).unwrap(),
        animation: Vec::new(),
        loop_count: None,
    }
}

fn animated_sample() -> DecodedImage {
    let poster_pixels = (0..16)
        .flat_map(|index| {
            if index % 2 == 0 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            }
        })
        .collect();
    let poster = RgbaImage::new(4, 4, poster_pixels).unwrap();
    let red = RgbaImage::new(4, 4, [255, 0, 0, 255].repeat(16)).unwrap();
    let green = RgbaImage::new(4, 4, [0, 255, 0, 255].repeat(16)).unwrap();
    DecodedImage {
        poster,
        animation: vec![
            AnimationFrame {
                image: red,
                duration_ms: 50,
            },
            AnimationFrame {
                image: green,
                duration_ms: 90,
            },
        ],
        loop_count: Some(2),
    }
}

#[test]
fn rgba_image_rejects_mismatched_buffer() {
    let error = RgbaImage::new(2, 2, vec![0; 15]).unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::InvalidInput);
}

#[test]
fn png_round_trip_preserves_rgba_pixels() {
    let original = sample();
    let bytes = encode(EncodeRequest {
        image: &original,
        format: EncodeFormat::Png,
    })
    .unwrap();
    let decoded = decode(DecodeRequest {
        bytes: &bytes,
        format_hint: Some("image/png"),
    })
    .unwrap();
    assert_eq!(decoded.poster, original.poster);
    assert!(!decoded.is_animated());
}

#[test]
fn animated_png_gif_and_webp_round_trip_frame_metadata() {
    let original = animated_sample();
    for (format, hint) in [
        (EncodeFormat::Png, "image/png"),
        (EncodeFormat::Gif, "image/gif"),
        (EncodeFormat::Webp, "image/webp"),
    ] {
        let bytes = encode(EncodeRequest {
            image: &original,
            format,
        })
        .unwrap();
        let decoded = decode(DecodeRequest {
            bytes: &bytes,
            format_hint: Some(hint),
        })
        .unwrap_or_else(|error| panic!("{hint}: {error}"));
        assert!(decoded.is_animated(), "{hint}");
        assert_eq!(decoded.frame_count(), 2, "{hint}");
        assert_eq!(decoded.loop_count, Some(2), "{hint}");
        assert_eq!(decoded.animation[0].duration_ms, 50, "{hint}");
        assert_eq!(decoded.animation[1].duration_ms, 90, "{hint}");
        assert_eq!(decoded.poster.width(), 4, "{hint}");
        assert_eq!(decoded.poster.height(), 4, "{hint}");
    }
}

#[test]
fn empty_input_reports_decode_error() {
    let error = decode(DecodeRequest {
        bytes: &[],
        format_hint: None,
    })
    .unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::Decode);
}

#[test]
fn bounded_decode_maps_wml2_pixel_rejection_to_core_limit() {
    let original = sample();
    let mut png = encode(EncodeRequest {
        image: &original,
        format: EncodeFormat::Png,
    })
    .unwrap();
    png[16..20].copy_from_slice(&10_001_u32.to_be_bytes());
    png[20..24].copy_from_slice(&10_000_u32.to_be_bytes());

    let error = decode_with_limits(
        DecodeRequest {
            bytes: &png,
            format_hint: Some("image/png"),
        },
        DecodeLimits {
            maximum_frame_pixels: 100_000_000,
            maximum_frames: 4_096,
            maximum_rgba_bytes: 512 * 1024 * 1024,
        },
        None,
    )
    .unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::Limit);
}

#[test]
fn cancellation_is_checked_again_before_animation_composition() {
    let mut image = ImageBuffer::from_buffer(1, 1, vec![0, 0, 0, 255]);
    image.animation = Some(vec![AnimationLayer {
        width: 1,
        height: 1,
        start_x: 0,
        start_y: 0,
        buffer: vec![255, 0, 0, 255],
        control: NextOptions::wait(10),
    }]);
    let calls = AtomicUsize::new(0);
    let probe = || calls.fetch_add(1, Ordering::Relaxed) >= 1;
    let error = decoded_from_wml2_for_test(image, Some(&probe)).unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::Cancelled);
    assert!(calls.load(Ordering::Relaxed) >= 2);
}
