use crate::CoreErrorKind;
use crate::image::{
    AnimationFrame, DecodeLimits, DecodeRequest, DecodedImage, EncodeFormat, EncodeRequest,
    RgbaImage, decode, decode_with_limits, decoded_from_wml2_for_test,
    decoded_from_wml2_with_limits_for_test, encode,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use wml2::draw::{AnimationLayer, ImageBuffer, NextOptions};

fn sample() -> DecodedImage {
    DecodedImage {
        poster: RgbaImage::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128]).unwrap(),
        animation: Vec::new(),
        loop_count: None,
        diagnostics: Default::default(),
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
        diagnostics: Default::default(),
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
fn bounded_decode_checks_cancellation_at_wml2_callbacks() {
    let original = sample();
    let png = encode(EncodeRequest {
        image: &original,
        format: EncodeFormat::Png,
    })
    .unwrap();
    let calls = AtomicUsize::new(0);
    let probe = || calls.fetch_add(1, Ordering::Relaxed) >= 1;
    let error = decode_with_limits(
        DecodeRequest {
            bytes: &png,
            format_hint: Some("image/png"),
        },
        DecodeLimits::UNLIMITED,
        Some(&probe),
    )
    .unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::Cancelled);
    assert!(calls.load(Ordering::Relaxed) >= 2);
}

#[test]
fn jpeg_xl_display_uses_reconstruction_profile_in_both_decode_paths() {
    // WML2 0.0.40 fixture and independent libjxl/LittleCMS reference; see
    // crates/fixtures/jxl-icc-fixture.md for provenance and hashes.
    let bytes = include_bytes!("../../fixtures/jxl-icc-lossless.jxl");
    let expected = include_bytes!("../../fixtures/jxl-icc-lossless.srgb.rgba");
    for bounded in [false, true] {
        let request = DecodeRequest {
            bytes,
            format_hint: Some("wrong-extension"),
        };
        let image = if bounded {
            decode_with_limits(request, DecodeLimits::UNLIMITED, None)
        } else {
            decode(request)
        }
        .unwrap();
        assert_eq!((image.poster.width(), image.poster.height()), (7, 5));
        assert_eq!(
            image.diagnostics.output_color(0).conversion,
            wml2::color_outcome::Conversion::AppliedToSrgb
        );
        assert_eq!(
            image.diagnostics.output_color(0).output,
            wml2::color_outcome::OutputColorSpace::Srgb
        );
        for key in [
            "JPEG XL Source ICC Profile",
            "JPEG XL Reconstruction ICC Profile",
            "ICC Profile",
        ] {
            assert!(
                image
                    .diagnostics
                    .metadata
                    .as_ref()
                    .unwrap()
                    .contains_key(key)
            );
        }
        for (actual, reference) in image
            .poster
            .pixels()
            .chunks_exact(4)
            .zip(expected.chunks_exact(4))
        {
            for channel in 0..3 {
                assert!(
                    actual[channel].abs_diff(reference[channel]) <= 2,
                    "{actual:?} vs {reference:?}"
                );
            }
            assert_eq!(actual[3], reference[3]);
        }
    }
}

#[test]
fn managed_jpeg_xl_respects_pixel_workspace_and_cancellation_limits() {
    let bytes = include_bytes!("../../fixtures/jxl-icc-lossless.jxl");
    let request = || DecodeRequest {
        bytes,
        format_hint: None,
    };
    for limits in [
        DecodeLimits {
            maximum_frame_pixels: 34,
            ..DecodeLimits::UNLIMITED
        },
        DecodeLimits {
            maximum_rgba_bytes: 128,
            ..DecodeLimits::UNLIMITED
        },
    ] {
        assert_eq!(
            decode_with_limits(request(), limits, None)
                .unwrap_err()
                .kind(),
            CoreErrorKind::Limit
        );
    }
    let calls = AtomicUsize::new(0);
    let probe = || calls.fetch_add(1, Ordering::Relaxed) >= 1;
    assert_eq!(
        decode_with_limits(request(), DecodeLimits::UNLIMITED, Some(&probe))
            .unwrap_err()
            .kind(),
        CoreErrorKind::Cancelled
    );
    assert!(calls.load(Ordering::Relaxed) >= 2);
}

#[test]
fn bounded_animation_composition_counts_full_canvas_frames() {
    let mut image = ImageBuffer::from_buffer(2, 2, vec![0, 0, 0, 255].repeat(4));
    image.animation = Some(vec![
        AnimationLayer {
            width: 1,
            height: 1,
            start_x: 0,
            start_y: 0,
            buffer: vec![255, 0, 0, 255],
            control: NextOptions::wait(10),
        },
        AnimationLayer {
            width: 1,
            height: 1,
            start_x: 1,
            start_y: 1,
            buffer: vec![0, 255, 0, 255],
            control: NextOptions::wait(10),
        },
    ]);

    // WML2 stores the 16-byte canvas plus two 4-byte layer rectangles, but
    // Viewer composition retains three full 16-byte canvases.
    let error = decoded_from_wml2_with_limits_for_test(
        image,
        DecodeLimits {
            maximum_frame_pixels: 4,
            maximum_frames: 2,
            maximum_rgba_bytes: 40,
        },
        None,
    )
    .unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::Limit);
    assert!(error.to_string().contains("requires 48 RGBA bytes"));
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

#[test]
fn jpeg_xl_animations_preserve_srgb_pixels_timing_and_loops() {
    // Synthetic MIT-licensed fixtures and independent libjxl pixel oracles.
    // See crates/fixtures/jxl-animation-fixtures.md.
    for (name, bytes, reference) in [
        (
            "xyb-icc16",
            include_bytes!("../../fixtures/jxl-animation-xyb-icc16.jxl").as_slice(),
            include_bytes!("../../fixtures/jxl-animation-xyb-icc16.rgba16le").as_slice(),
        ),
        (
            "grayalpha8",
            include_bytes!("../../fixtures/jxl-animation-grayalpha8.jxl").as_slice(),
            include_bytes!("../../fixtures/jxl-animation-grayalpha8.rgba16le").as_slice(),
        ),
    ] {
        for bounded in [false, true] {
            let request = DecodeRequest {
                bytes,
                format_hint: Some("wrong-extension"),
            };
            let image = if bounded {
                decode_with_limits(request, DecodeLimits::UNLIMITED, None)
            } else {
                decode(request)
            }
            .unwrap();
            assert_eq!(
                (image.poster.width(), image.poster.height()),
                (32, 24),
                "{name}"
            );
            assert_eq!(image.frame_count(), 3, "{name}");
            for frame in 0..3 {
                let output = image.diagnostics.display_output_index(frame, true).unwrap();
                assert_eq!(output, frame + 1);
                assert_eq!(
                    image.diagnostics.output_color(output).conversion,
                    wml2::color_outcome::Conversion::AppliedToSrgb
                );
            }
            assert_eq!(image.loop_count, Some(0), "{name}");
            assert_eq!(image.poster, image.animation[0].image, "{name}");
            assert_eq!(
                image
                    .animation
                    .iter()
                    .map(|frame| frame.duration_ms)
                    .collect::<Vec<_>>(),
                [50, 90, 130],
                "{name}"
            );
            assert_eq!(reference.len(), 3 * 32 * 24 * 8);
            for (frame, oracle) in image
                .animation
                .iter()
                .zip(reference.chunks_exact(32 * 24 * 8))
            {
                assert_eq!((frame.image.width(), frame.image.height()), (32, 24));
                for (index, (actual, word)) in frame
                    .image
                    .pixels()
                    .iter()
                    .zip(oracle.chunks_exact(2))
                    .enumerate()
                {
                    let expected =
                        ((u32::from(u16::from_le_bytes([word[0], word[1]])) + 128) / 257) as u8;
                    let tolerance = if index % 4 == 3 { 0 } else { 1 };
                    assert!(
                        actual.abs_diff(expected) <= tolerance,
                        "{name} channel {index}: {actual} vs {expected}"
                    );
                }
            }
        }
    }
}

#[test]
fn jpeg_xl_animation_limits_and_cooperative_cancellation_are_preserved() {
    let bytes = include_bytes!("../../fixtures/jxl-animation-xyb-icc16.jxl");
    let request = || DecodeRequest {
        bytes,
        format_hint: None,
    };
    for limits in [
        DecodeLimits {
            maximum_frames: 2,
            ..DecodeLimits::UNLIMITED
        },
        DecodeLimits {
            maximum_frame_pixels: 32 * 24 - 1,
            ..DecodeLimits::UNLIMITED
        },
        DecodeLimits {
            maximum_rgba_bytes: 32 * 24 * 4,
            ..DecodeLimits::UNLIMITED
        },
    ] {
        assert_eq!(
            decode_with_limits(request(), limits, None)
                .unwrap_err()
                .kind(),
            CoreErrorKind::Limit
        );
    }
    let calls = AtomicUsize::new(0);
    let probe = || calls.fetch_add(1, Ordering::Relaxed) >= 3;
    assert_eq!(
        decode_with_limits(request(), DecodeLimits::UNLIMITED, Some(&probe))
            .unwrap_err()
            .kind(),
        CoreErrorKind::Cancelled
    );
    assert!(calls.load(Ordering::Relaxed) >= 4);
    // Cancellation must not leave shared state that prevents a fresh decode.
    assert_eq!(
        decode_with_limits(request(), DecodeLimits::UNLIMITED, None)
            .unwrap()
            .frame_count(),
        3
    );
}

#[test]
fn jpeg_xl_cancellation_reaches_decoder_before_invalid_payload() {
    // The signature selects the JXL route but the payload cannot decode. The
    // first probe runs in our wrapper; cancellation on the second call must
    // reach WML2 before parsing. Post-decode composition checks cannot make
    // this pass if the decoder probe is accidentally disconnected.
    let request = || DecodeRequest {
        bytes: &[0xff, 0x0a],
        format_hint: None,
    };
    assert_eq!(
        decode_with_limits(request(), DecodeLimits::UNLIMITED, None)
            .unwrap_err()
            .kind(),
        CoreErrorKind::Decode
    );
    let calls = AtomicUsize::new(0);
    let probe = || calls.fetch_add(1, Ordering::Relaxed) >= 1;
    assert_eq!(
        decode_with_limits(request(), DecodeLimits::UNLIMITED, Some(&probe))
            .unwrap_err()
            .kind(),
        CoreErrorKind::Cancelled
    );
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[path = "cmyk_test_profile.rs"]
mod cmyk_test_profile;

#[test]
fn cmyk_jpeg_icc_is_applied_in_both_viewer_decode_paths() {
    for jpeg in [
        include_bytes!("../../fixtures/jpeg-cmyk-blocks.jpg").as_slice(),
        include_bytes!("../../fixtures/jpeg-cmyk-blocks-progressive.jpg").as_slice(),
    ] {
        let mut payload = b"ICC_PROFILE\0\x01\x01".to_vec();
        payload.extend(cmyk_test_profile::cmyk_profile());
        let mut tagged = jpeg[..2].to_vec();
        tagged.extend([255, 0xe2]);
        tagged.extend(((payload.len() + 2) as u16).to_be_bytes());
        tagged.extend(payload);
        tagged.extend(&jpeg[2..]);
        for bounded in [false, true] {
            for managed in [false, true] {
                let request = DecodeRequest {
                    bytes: if managed { &tagged } else { jpeg },
                    format_hint: Some("wrong-extension"),
                };
                let image = if bounded {
                    decode_with_limits(request, DecodeLimits::UNLIMITED, None)
                } else {
                    decode(request)
                }
                .unwrap();
                assert_eq!((image.poster.width(), image.poster.height()), (16, 16));
                use wml2::color_outcome::{
                    ApproximationMethod, ApproximationReason, Conversion, OutputColorSpace,
                };
                let outcome = image.diagnostics.output_color(0);
                if managed {
                    assert_eq!(outcome.conversion, Conversion::AppliedToSrgb);
                    assert_eq!(outcome.output, OutputColorSpace::Srgb);
                } else {
                    assert_eq!(
                        outcome.conversion,
                        Conversion::Approximation(
                            ApproximationReason::MissingProfile,
                            ApproximationMethod::AdditiveCmyk
                        )
                    );
                    assert_eq!(outcome.output, OutputColorSpace::DeviceApproximation);
                }
                for (index, pixel) in image.poster.pixels().chunks_exact(4).enumerate() {
                    let block = (index / 16 / 8 * 2 + index % 16 / 8) as u8;
                    let legacy = [165 + block * 12, 185 - block * 8, 195 - block * 10];
                    let inverted_ink = [180 + block * 12, 200 - block * 8, 210 - block * 10];
                    for channel in 0..3 {
                        if managed {
                            // Independent analytic sRGB oracle for the linear-light
                            // subtractive CMYK fixture, not the ICC implementation.
                            let linear = f64::from(inverted_ink[channel]) / 255.0 * (240.0 / 255.0);
                            let expected =
                                ((1.055 * linear.powf(1.0 / 2.4) - 0.055) * 255.0).round() as u8;
                            assert!(
                                pixel[channel].abs_diff(expected) <= 3,
                                "bounded={bounded}, block={block}, {pixel:?} vs {expected}"
                            );
                            assert!(pixel[channel].abs_diff(legacy[channel]) > 10);
                        } else {
                            assert_eq!(pixel[channel], legacy[channel]);
                        }
                    }
                    assert_eq!(pixel[3], 255);
                }
            }
        }
    }
}

#[test]
fn decode_diagnostics_never_infer_or_inherit_color_records() {
    use wml2::color_outcome::{ColorOutcome, Conversion, OutputColorSpace, metadata_key};
    use wml2::metadata::DataMap;
    let applied = ColorOutcome {
        conversion: Conversion::AppliedToSrgb,
        output: OutputColorSpace::Srgb,
    };
    let mut raw = ImageBuffer::from_buffer(1, 1, vec![10, 20, 30, 255]);
    raw.metadata = Some(std::collections::HashMap::from([
        (metadata_key(0), applied.to_metadata()),
        (
            metadata_key(2),
            DataMap::Raw(vec![b'W', b'C', b'O', 2, 1, 0, 0, 1]),
        ),
        (
            "Source ICC Profile".into(),
            DataMap::ICCProfile(vec![1, 2, 3]),
        ),
        ("Format".into(), DataMap::Ascii("Tiff".into())),
    ]));
    let original_metadata = raw.metadata.clone();
    let image = decoded_from_wml2_for_test(raw, None).unwrap();
    assert_eq!(image.diagnostics.metadata, original_metadata);
    assert_eq!(image.poster.pixels(), &[10, 20, 30, 255]);
    assert_eq!(image.diagnostics.output_color(0), applied);
    assert_eq!(image.diagnostics.output_color(1), ColorOutcome::default());
    assert_eq!(image.diagnostics.output_color(2), ColorOutcome::default());
    assert_eq!(image.diagnostics.display_output_index(0, true), Some(1));
    assert_eq!(image.diagnostics.display_output_index(1, true), Some(2));
    assert_eq!(image.diagnostics.display_output_index(0, false), Some(0));
    let mut diagnostics = image.diagnostics;
    diagnostics
        .metadata
        .as_mut()
        .unwrap()
        .remove(&metadata_key(0));
    diagnostics
        .warnings
        .push("ICC conversion applied successfully".into());
    assert_eq!(diagnostics.output_color(0), ColorOutcome::default());
    assert_eq!(
        crate::image::DecodeDiagnostics::default().display_output_index(0, true),
        None
    );
}

#[test]
fn invalid_cmyk_profile_preserves_warnings_and_fallback_without_pixel_changes() {
    use wml2::color_outcome::{ApproximationMethod, ApproximationReason, Conversion};
    let jpeg = include_bytes!("../../fixtures/jpeg-cmyk-blocks.jpg");
    let mut payload = b"ICC_PROFILE\0\x01\x01".to_vec();
    payload.extend([0_u8; 128]);
    let mut tagged = jpeg[..2].to_vec();
    tagged.extend([255, 0xe2]);
    tagged.extend(((payload.len() + 2) as u16).to_be_bytes());
    tagged.extend(payload);
    tagged.extend(&jpeg[2..]);
    let legacy = decode(DecodeRequest {
        bytes: jpeg,
        format_hint: None,
    })
    .unwrap();
    let mut reference_warnings = None;
    for bounded in [false, true] {
        let request = DecodeRequest {
            bytes: &tagged,
            format_hint: None,
        };
        let image = if bounded {
            decode_with_limits(request, DecodeLimits::UNLIMITED, None)
        } else {
            decode(request)
        }
        .unwrap();
        assert_eq!(image.poster, legacy.poster);
        assert_eq!(
            image.diagnostics.output_color(0).conversion,
            Conversion::Approximation(
                ApproximationReason::InvalidProfile,
                ApproximationMethod::AdditiveCmyk
            )
        );
        assert!(!image.diagnostics.warnings.is_empty());
        if let Some(expected) = &reference_warnings {
            assert_eq!(&image.diagnostics.warnings, expected);
        }
        reference_warnings = Some(image.diagnostics.warnings);
    }
}

// Minimal uncompressed little-endian TIFF pages, generated in-memory so the
// differing page records and the exact callback-to-viewer mapping are testable.
fn diagnostic_tiff_pages() -> Vec<u8> {
    let mut bytes = b"II\x2a\x00\x08\x00\x00\x00".to_vec();
    let mut previous_next = None;
    for (channels, photometric, pixels) in [
        (4_u16, 5_u32, vec![1, 2, 3, 4]),
        (3_u16, 2_u32, vec![9, 8, 7]),
        (4_u16, 5_u32, vec![10, 20, 30, 40]),
    ] {
        let directory = bytes.len();
        if let Some(offset) = previous_next {
            bytes[offset..offset + 4].copy_from_slice(&(directory as u32).to_le_bytes());
        }
        let bits_offset = directory + 2 + 9 * 12 + 4;
        let pixel_offset = bits_offset + usize::from(channels) * 2;
        bytes.extend(9_u16.to_le_bytes());
        for (tag, kind, count, value) in [
            (256_u16, 4_u16, 1_u32, 1_u32),
            (257, 4, 1, 1),
            (258, 3, u32::from(channels), bits_offset as u32),
            (259, 3, 1, 1),
            (262, 3, 1, photometric),
            (273, 4, 1, pixel_offset as u32),
            (277, 3, 1, u32::from(channels)),
            (278, 4, 1, 1),
            (279, 4, 1, pixels.len() as u32),
        ] {
            bytes.extend(tag.to_le_bytes());
            bytes.extend(kind.to_le_bytes());
            bytes.extend(count.to_le_bytes());
            bytes.extend(value.to_le_bytes());
        }
        previous_next = Some(bytes.len());
        bytes.extend(0_u32.to_le_bytes());
        for _ in 0..channels {
            bytes.extend(8_u16.to_le_bytes());
        }
        bytes.extend(pixels);
    }
    bytes
}

#[test]
fn tiff_pages_keep_exact_output_records_and_existing_composited_pixels() {
    use wml2::color_outcome::{ApproximationMethod, ApproximationReason, ColorOutcome, Conversion};
    let bytes = diagnostic_tiff_pages();
    for bounded in [false, true] {
        let request = DecodeRequest {
            bytes: &bytes,
            format_hint: None,
        };
        let image = if bounded {
            decode_with_limits(request, DecodeLimits::UNLIMITED, None)
        } else {
            decode(request)
        }
        .unwrap();
        assert_eq!(image.animation.len(), 2);
        // Existing compositor displays the second TIFF page first. Do not silently
        // relabel it with the first page's valid CMYK conversion record.
        assert_eq!(image.poster.pixels(), &[9, 8, 7, 255]);
        assert_eq!(image.diagnostics.display_output_index(0, true), Some(1));
        assert_eq!(image.diagnostics.output_color(1), ColorOutcome::default());
        assert_eq!(image.diagnostics.display_output_index(1, true), Some(2));
        for index in [0, 2] {
            assert_eq!(
                image.diagnostics.output_color(index).conversion,
                Conversion::Approximation(
                    ApproximationReason::MissingProfile,
                    ApproximationMethod::MultiplicativeCmyk
                )
            );
        }
    }
}
