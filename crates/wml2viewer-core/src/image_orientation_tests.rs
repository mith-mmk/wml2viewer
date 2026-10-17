use crate::CoreErrorKind;
use crate::image::{
    DecodeLimits, DecodeRequest, decode, decode_with_limits, decoded_from_wml2_for_test,
};
use bin_rs::Endian;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use wml2::draw::{AnimationLayer, ImageBuffer, NextOptions, image_to};
use wml2::metadata::DataMap;
use wml2::metadata::exif::{ExifIfd, exif_to_vec, set_long, set_short};
use wml2::tiff::header::TiffHeaders;
use wml2::util::ImageFormat;

// Six distinct pixels in a non-square image, with distinct alpha values too.
fn pixels() -> Vec<u8> {
    (1..=6)
        .flat_map(|n| [n * 30, n * 20, n * 10, n * 40])
        .collect()
}

fn headers(orientation: u16, endian: Endian) -> TiffHeaders {
    let mut headers = TiffHeaders::empty(endian);
    set_short(&mut headers, ExifIfd::Primary, 0x0112, vec![orientation]);
    headers
}

fn tiff_with_orientation(orientation: u16, endian: Endian) -> Vec<u8> {
    // WML2's TIFF encoder resets Orientation to 1. Build an uncompressed
    // single-strip TIFF so the fixture retains the requested stored orientation.
    let mut headers = headers(orientation, endian);
    for (tag, values) in [
        (0x0102, vec![8, 8, 8, 8]), // BitsPerSample
        (0x0103, vec![1]),          // Compression: none
        (0x0106, vec![2]),          // RGB
        (0x0115, vec![4]),          // SamplesPerPixel
        (0x011c, vec![1]),          // Chunky
        (0x0152, vec![2]),          // Unassociated alpha
    ] {
        set_short(&mut headers, ExifIfd::Primary, tag, values);
    }
    for (tag, value) in [
        (0x0100, 3),
        (0x0101, 2),
        (0x0111, 0),
        (0x0116, 2),
        (0x0117, 24),
    ] {
        set_long(&mut headers, ExifIfd::Primary, tag, vec![value]);
    }
    let offset = exif_to_vec(&headers).unwrap().len() as u32;
    set_long(&mut headers, ExifIfd::Primary, 0x0111, vec![offset]);
    let mut bytes = exif_to_vec(&headers).unwrap();
    bytes.extend(pixels());
    bytes
}

fn expected_pixels(source: &[u8], orientation: u16) -> Vec<u8> {
    // Row-major display order for source rows ABC / DEF (CIPA Orientation).
    let order = match orientation {
        2 => [2, 1, 0, 5, 4, 3], // CBA / FED
        3 => [5, 4, 3, 2, 1, 0], // FED / CBA
        4 => [3, 4, 5, 0, 1, 2], // DEF / ABC
        5 => [0, 3, 1, 4, 2, 5], // AD / BE / CF
        6 => [3, 0, 4, 1, 5, 2], // DA / EB / FC
        7 => [5, 2, 4, 1, 3, 0], // FC / EB / DA
        8 => [2, 5, 1, 4, 0, 3], // CF / BE / AD
        _ => [0, 1, 2, 3, 4, 5],
    };
    order
        .into_iter()
        .flat_map(|i| source[i * 4..i * 4 + 4].iter().copied())
        .collect()
}

#[test]
fn exif_orientation_normalizes_all_eight_pixel_layouts() {
    for orientation in 1..=8 {
        for key in ["EXIF", "Tiff headers", "EXIF Raw"] {
            let mut image = ImageBuffer::from_buffer(3, 2, pixels());
            let headers = headers(orientation, Endian::LittleEndian);
            let value = if key == "EXIF Raw" {
                DataMap::Raw(exif_to_vec(&headers).unwrap())
            } else {
                DataMap::Exif(headers)
            };
            image.metadata = Some(HashMap::from([(key.to_string(), value)]));
            let decoded = decoded_from_wml2_for_test(image, None).unwrap();
            let size = if orientation >= 5 { (2, 3) } else { (3, 2) };
            assert_eq!(
                (decoded.poster.width(), decoded.poster.height()),
                size,
                "{key}: {orientation}"
            );
            assert_eq!(decoded.poster.stride(), size.0 * 4);
            assert_eq!(
                decoded.poster.pixels(),
                expected_pixels(&pixels(), orientation),
                "{key}: {orientation}"
            );
        }
    }
}

#[test]
fn encoded_exif_orientation_reaches_both_decode_paths() {
    for name in ["JPEG", "PNG", "WebP", "TIFF"] {
        let format = || match name {
            "JPEG" => ImageFormat::Jpeg,
            "PNG" => ImageFormat::Png,
            "WebP" => ImageFormat::Webp,
            _ => ImageFormat::Tiff,
        };
        let mut source = ImageBuffer::from_buffer(3, 2, pixels());
        let plain = image_to(&mut source, format(), None).unwrap();
        let baseline = decode(DecodeRequest {
            bytes: &plain,
            format_hint: None,
        })
        .unwrap();
        for endian in [Endian::LittleEndian, Endian::BigEndian] {
            for orientation in 1..=8 {
                let options = HashMap::from([(
                    "exif".to_string(),
                    DataMap::Exif(headers(orientation, endian)),
                )]);
                let bytes = if name == "TIFF" {
                    tiff_with_orientation(orientation, endian)
                } else {
                    image_to(&mut source, format(), Some(options)).unwrap()
                };
                for bounded in [false, true] {
                    let request = DecodeRequest {
                        bytes: &bytes,
                        format_hint: None,
                    };
                    let decoded = if bounded {
                        decode_with_limits(
                            request,
                            DecodeLimits {
                                maximum_frame_pixels: 6,
                                maximum_frames: 1,
                                // Also covers PNG filter bytes in WML2's inflate limit.
                                maximum_rgba_bytes: 64,
                            },
                            None,
                        )
                    } else {
                        decode(request)
                    }
                    .unwrap();
                    assert_eq!(
                        decoded.poster.pixels(),
                        expected_pixels(baseline.poster.pixels(), orientation),
                        "{name}, {endian:?}, {orientation}, bounded={bounded}"
                    );
                    let size = if orientation >= 5 { (2, 3) } else { (3, 2) };
                    assert_eq!((decoded.poster.width(), decoded.poster.height()), size);
                }
            }
        }
    }
}

#[test]
fn missing_invalid_and_unrelated_orientation_leave_pixels_unchanged() {
    let cases = [
        None,
        Some(("EXIF", DataMap::Exif(headers(0, Endian::LittleEndian)))),
        Some(("EXIF", DataMap::Exif(headers(9, Endian::LittleEndian)))),
        Some(("EXIF", DataMap::Raw(vec![1, 2, 3]))),
        // Photo CD has its own already-applied orientation in a different namespace.
        Some(("orientation", DataMap::UInt(2))),
    ];
    for metadata in cases {
        let mut image = ImageBuffer::from_buffer(3, 2, pixels());
        image.metadata = metadata.map(|(key, value)| HashMap::from([(key.to_string(), value)]));
        let decoded = decoded_from_wml2_for_test(image, None).unwrap();
        assert_eq!(decoded.poster.pixels(), pixels());
        assert_eq!((decoded.poster.width(), decoded.poster.height()), (3, 2));
    }
}

#[test]
fn animation_orientation_runs_after_compositing_partial_frames() {
    let mut image = ImageBuffer::from_buffer(3, 2, pixels());
    image.metadata = Some(HashMap::from([(
        "EXIF".to_string(),
        DataMap::Exif(headers(6, Endian::LittleEndian)),
    )]));
    image.loop_count = Some(3);
    image.animation = Some(vec![
        AnimationLayer {
            width: 1,
            height: 1,
            start_x: 2,
            start_y: 1,
            buffer: vec![12, 34, 56, 78],
            control: NextOptions::wait(25),
        },
        AnimationLayer {
            width: 1,
            height: 1,
            start_x: 0,
            start_y: 0,
            buffer: vec![98, 76, 54, 32],
            control: NextOptions::wait(75),
        },
    ]);
    let decoded = decoded_from_wml2_for_test(image, None).unwrap();
    let mut first = pixels();
    first[20..24].copy_from_slice(&[12, 34, 56, 78]);
    let mut second = first.clone();
    second[..4].copy_from_slice(&[98, 76, 54, 32]);
    assert_eq!(decoded.poster, decoded.animation[0].image);
    for (frame, expected) in decoded.animation.iter().zip([first, second]) {
        assert_eq!((frame.image.width(), frame.image.height()), (2, 3));
        assert_eq!(frame.image.pixels(), expected_pixels(&expected, 6));
    }
    assert_eq!(decoded.animation[0].duration_ms, 25);
    assert_eq!(decoded.animation[1].duration_ms, 75);
    assert_eq!(decoded.loop_count, Some(3));
}

#[test]
fn orientation_transform_checks_cancellation() {
    let mut image = ImageBuffer::from_buffer(3, 2, pixels());
    image.metadata = Some(HashMap::from([(
        "EXIF".to_string(),
        DataMap::Exif(headers(6, Endian::LittleEndian)),
    )]));
    let calls = AtomicUsize::new(0);
    // Pass the initial checks, then cancel inside orientation processing.
    let probe = || calls.fetch_add(1, Ordering::Relaxed) >= 2;
    let error = decoded_from_wml2_for_test(image, Some(&probe)).unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::Cancelled);
}
