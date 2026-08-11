use crate::bounded_io::{BoundedReadError, read_bounded_for_test};
use crate::reading_plan::{
    MAX_PREFETCH_SPREADS, MAX_READING_PAGES, READING_WIRE_VERSION, ReadingPlanError,
    ReadingPlanRequest, encode_reading_wire, plan_reading,
};
use crate::registry::HandleRegistry;
use crate::session::{
    BridgeState, MAX_ANDROID_ARCHIVE_ENTRY_BYTES, MAX_ANDROID_ARCHIVE_INPUT_BYTES,
    MAX_ANDROID_ARCHIVE_RETAINED_BYTES, MAX_ANDROID_ENCODED_INPUT_BYTES,
    MAX_NATIVE_ANIMATION_FRAMES, MAX_NATIVE_IMAGE_RGBA_BYTES, MAX_NATIVE_POSTER_PIXELS,
    NativeErrorCode, NativeSessionHandle, RgbaEncodeRequest, core_error, limit_error,
    validate_archive_retained_layout_for_test, validate_native_image_layout_for_test,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use wml2viewer_core::image::{
    AnimationFrame, DecodeRequest, DecodedImage, EncodeFormat, EncodeRequest, RgbaImage, decode,
    encode,
};
use wml2viewer_core::{CoreError, CoreErrorKind};

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn new(label: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            ".test-wml2viewer-{label}-{}-{id}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn create_sparse_file(path: &Path, length: u64) {
    let file = std::fs::File::create(path).unwrap();
    file.set_len(length).unwrap();
}

#[test]
fn bounded_reader_rejects_growth_beyond_metadata_size() {
    let error = read_bounded_for_test(std::io::Cursor::new([0_u8; 9]), 0, 8).unwrap_err();
    assert!(matches!(
        error,
        BoundedReadError::Limit {
            dimension: "test_bytes"
        }
    ));
}

#[test]
fn reading_planner_wire_uses_core_spreads_navigation_and_prefetch() {
    let source_ids = [7, 7, 7, 7, 7, 7];
    let portrait = [true; 6];
    let covers = [true, false, false, false, false, false];
    let request = ReadingPlanRequest {
        source_ids: &source_ids,
        portrait: &portrait,
        covers: &covers,
        current_index: 2,
        landscape: true,
        layout: 0,
        direction: 1,
        cover_alone: true,
        maximum_prefetch_spreads: 2,
    };
    let plan = plan_reading(request).unwrap();

    assert_eq!(plan.anchor_index, 1);
    assert_eq!(plan.logical_indices, [1, 2]);
    assert_eq!(plan.visual_indices, [2, 1]);
    assert_eq!(plan.previous_anchor, Some(0));
    assert_eq!(plan.next_anchor, Some(3));
    assert_eq!(plan.preload_indices, [3, 4, 5]);
    assert_eq!(
        encode_reading_wire(&plan).unwrap(),
        [
            READING_WIRE_VERSION,
            15,
            1,
            0,
            3,
            2,
            2,
            3,
            1,
            2,
            2,
            1,
            3,
            4,
            5,
        ]
    );

    let boundary_source_ids = [7, 7, 7, 8, 9, 9];
    let boundary_plan = plan_reading(ReadingPlanRequest {
        source_ids: &boundary_source_ids,
        maximum_prefetch_spreads: 1,
        ..request
    })
    .unwrap();
    assert_eq!(boundary_plan.preload_indices, [3]);
}

#[test]
fn reading_planner_rejects_invalid_inputs_and_caps_pages_and_prefetch() {
    let source_ids = [1];
    let portrait = [true];
    let covers = [false];
    let request = |current_index, layout, direction, maximum_prefetch_spreads| ReadingPlanRequest {
        source_ids: &source_ids,
        portrait: &portrait,
        covers: &covers,
        current_index,
        landscape: true,
        layout,
        direction,
        cover_alone: true,
        maximum_prefetch_spreads,
    };

    assert_eq!(
        plan_reading(request(1, 0, 1, 1)),
        Err(ReadingPlanError::InvalidCurrentIndex)
    );
    assert_eq!(
        plan_reading(request(0, 3, 1, 1)),
        Err(ReadingPlanError::InvalidLayout)
    );
    assert_eq!(
        plan_reading(request(0, 0, 2, 1)),
        Err(ReadingPlanError::InvalidDirection)
    );
    assert_eq!(
        plan_reading(request(0, 0, 1, (MAX_PREFETCH_SPREADS + 1) as i32)),
        Err(ReadingPlanError::InvalidPrefetchLimit)
    );

    let maximum_source_ids = vec![1; MAX_READING_PAGES];
    let maximum_portrait = vec![true; MAX_READING_PAGES];
    let maximum_covers = vec![false; MAX_READING_PAGES];
    assert!(
        plan_reading(ReadingPlanRequest {
            source_ids: &maximum_source_ids,
            portrait: &maximum_portrait,
            covers: &maximum_covers,
            current_index: (MAX_READING_PAGES - 1) as i32,
            landscape: true,
            layout: 1,
            direction: 0,
            cover_alone: true,
            maximum_prefetch_spreads: MAX_PREFETCH_SPREADS as i32,
        })
        .is_ok()
    );
    let oversized_source_ids = vec![1; MAX_READING_PAGES + 1];
    let oversized_portrait = vec![true; MAX_READING_PAGES + 1];
    let oversized_covers = vec![false; MAX_READING_PAGES + 1];
    assert_eq!(
        plan_reading(ReadingPlanRequest {
            source_ids: &oversized_source_ids,
            portrait: &oversized_portrait,
            covers: &oversized_covers,
            current_index: 0,
            landscape: true,
            layout: 0,
            direction: 1,
            cover_alone: true,
            maximum_prefetch_spreads: 1,
        }),
        Err(ReadingPlanError::TooManyPages)
    );
}

fn png_bytes() -> Vec<u8> {
    let image = DecodedImage {
        poster: RgbaImage::new(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128]).unwrap(),
        animation: Vec::new(),
        loop_count: None,
    };
    encode(EncodeRequest {
        image: &image,
        format: EncodeFormat::Png,
    })
    .unwrap()
}

fn animated_image() -> DecodedImage {
    let red = RgbaImage::new(1, 1, vec![255, 0, 0, 255]).unwrap();
    let green = RgbaImage::new(1, 1, vec![0, 255, 0, 255]).unwrap();
    DecodedImage {
        poster: red.clone(),
        animation: vec![
            AnimationFrame {
                image: red,
                duration_ms: 20,
            },
            AnimationFrame {
                image: green,
                duration_ms: 30,
            },
        ],
        loop_count: Some(2),
    }
}

fn rgba_request<'a>(
    rgba: &'a [u8],
    width: i32,
    height: i32,
    stride: i32,
    format: &'a str,
) -> RgbaEncodeRequest<'a> {
    RgbaEncodeRequest {
        rgba,
        width,
        height,
        stride,
        format,
    }
}

#[test]
fn registry_handles_are_nonzero_and_not_reused() {
    let registry = HandleRegistry::default();
    let other_kind = HandleRegistry::default();
    let first = registry.insert("first").unwrap();
    assert_ne!(first, 0);
    assert_eq!(&*registry.get(first).unwrap(), &"first");
    let other = other_kind.insert(7_u8).unwrap();
    assert_ne!(first, other);
    assert!(registry.get(other).is_none());
    assert!(other_kind.get(first).is_none());
    assert!(registry.remove(first).is_some());
    assert!(registry.remove(first).is_none());

    let second = registry.insert("second").unwrap();
    assert!(second > first);
    assert_eq!(registry.len(), 1);
}

#[test]
fn request_ids_must_be_allocated_and_begin_monotonically() {
    let bridge = BridgeState::default();
    let session = bridge.create_session().unwrap();
    assert!(!bridge.begin_request(session, 1));

    let first = bridge.next_request_id(session).unwrap();
    assert_eq!(first, 1);
    assert!(bridge.begin_request(session, first));
    assert!(bridge.is_request_current(session, first));

    let second = bridge.next_request_id(session).unwrap();
    assert_eq!(second, 2);
    assert!(bridge.begin_request(session, second));
    assert!(!bridge.is_request_current(session, first));
    assert!(bridge.is_request_current(session, second));
    assert!(!bridge.begin_request(session, first));
}

#[test]
fn cancel_and_release_are_idempotent() {
    let bridge = BridgeState::default();
    let session = bridge.create_session().unwrap();
    let request = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, request));
    assert!(bridge.cancel_request(session, request));
    assert!(!bridge.cancel_request(session, request));
    assert!(!bridge.is_request_current(session, request));
    assert!(bridge.release_session(session));
    assert!(!bridge.release_session(session));
}

#[test]
fn stale_request_cannot_publish_an_image() {
    let bridge = BridgeState::default();
    let session = bridge.create_session().unwrap();
    let stale = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, stale));
    let current = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, current));

    assert!(
        bridge
            .decode_bytes(session, stale, &png_bytes(), Some("image/png"))
            .is_none()
    );
    assert_eq!(bridge.image_count(), 0);
}

#[test]
fn image_handle_owns_pixels_until_explicit_release() {
    let bridge = BridgeState::default();
    let session = bridge.create_session().unwrap();
    let request = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, request));

    let handle = bridge
        .decode_bytes(session, request, &png_bytes(), Some("image/png"))
        .unwrap();
    let image = bridge.image(handle).unwrap();
    assert_eq!((image.width(), image.height(), image.stride()), (2, 1, 8));
    assert_eq!(image.pixels_len(), 8);

    assert!(bridge.release_session(session));
    assert!(bridge.image(handle).is_some());
    assert!(bridge.release_image(handle));
    assert!(!bridge.release_image(handle));
    assert!(bridge.image(handle).is_none());
}

#[test]
fn request_errors_distinguish_invalid_stale_cancel_io_decode_encode_and_limit() {
    let bridge = BridgeState::default();
    let missing_session = NativeSessionHandle::from_jlong(12345).unwrap();
    assert_eq!(
        bridge.request_error(missing_session, 1).code(),
        NativeErrorCode::InvalidHandle
    );

    let session = bridge.create_session().unwrap();
    assert!(!bridge.begin_request(session, 1));
    assert_eq!(
        bridge.request_error(session, 1).code(),
        NativeErrorCode::InvalidRequest
    );

    let stale = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, stale));
    let current = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, current));
    assert!(
        bridge
            .decode_bytes(session, stale, &png_bytes(), None)
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, stale).code(),
        NativeErrorCode::StaleRequest
    );

    assert!(
        bridge
            .decode_bytes(session, current, b"not an image", None)
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, current).code(),
        NativeErrorCode::Decode
    );

    let limit_request = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, limit_request));
    let mut oversized_png = png_bytes();
    oversized_png[16..20].copy_from_slice(&4_097_u32.to_be_bytes());
    oversized_png[20..24].copy_from_slice(&4_096_u32.to_be_bytes());
    assert!(
        bridge
            .decode_bytes(session, limit_request, &oversized_png, Some("image/png"))
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, limit_request).code(),
        NativeErrorCode::Limit
    );

    let io_request = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, io_request));
    let missing =
        std::env::temp_dir().join(format!(".test-wml2viewer-missing-{}", std::process::id()));
    assert!(
        bridge
            .decode_path(session, io_request, &missing, None)
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, io_request).code(),
        NativeErrorCode::Io
    );

    let cancelled = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, cancelled));
    assert!(bridge.cancel_request(session, cancelled));
    assert_eq!(
        bridge.request_error(session, cancelled).code(),
        NativeErrorCode::Cancelled
    );
    assert_eq!(limit_error("width").code(), NativeErrorCode::Limit);
    let encode_error = core_error(&CoreError::encode("encoder failed"));
    assert_eq!(encode_error.code(), NativeErrorCode::Encode);
    assert_eq!(encode_error.key(), "encode");
}

#[test]
fn rgba_encoder_supports_png_jpeg_webp_and_strided_rows() {
    let bridge = BridgeState::default();
    let session = bridge.create_session().unwrap();
    let rgba = [
        255, 0, 0, 255, 0, 255, 0, 255, 9, 9, 9, 9, 0, 0, 255, 255, 255, 255, 255, 255, 8, 8, 8, 8,
    ];
    let tight_pixels = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];

    for (format, mime) in [
        ("png", "image/png"),
        ("jpeg", "image/jpeg"),
        ("webp", "image/webp"),
    ] {
        let request = bridge.next_request_id(session).unwrap();
        assert!(bridge.begin_request(session, request));
        let encoded = bridge
            .encode_rgba(session, request, rgba_request(&rgba, 2, 2, 12, format))
            .unwrap();
        let decoded = {
            let bytes = bridge.native_bytes(encoded).unwrap();
            decode(DecodeRequest {
                bytes: bytes.as_slice(),
                format_hint: Some(mime),
            })
            .unwrap()
        };
        assert_eq!((decoded.poster.width(), decoded.poster.height()), (2, 2));
        if format == "png" {
            assert_eq!(decoded.poster.pixels(), tight_pixels);
        }
        assert!(bridge.release_bytes(encoded));
        assert!(!bridge.release_bytes(encoded));
    }
}

#[test]
fn rgba_encoder_rejects_invalid_limits_stale_and_cancelled_requests() {
    let bridge = BridgeState::default();
    let session = bridge.create_session().unwrap();

    let invalid_stride = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, invalid_stride));
    assert!(
        bridge
            .encode_rgba(
                session,
                invalid_stride,
                rgba_request(&[0; 8], 2, 1, 4, "png"),
            )
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, invalid_stride).code(),
        NativeErrorCode::InvalidRequest
    );

    let invalid_capacity = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, invalid_capacity));
    assert!(
        bridge
            .encode_rgba(
                session,
                invalid_capacity,
                rgba_request(&[0; 4], 2, 1, 8, "png"),
            )
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, invalid_capacity).code(),
        NativeErrorCode::InvalidRequest
    );

    let invalid_format = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, invalid_format));
    assert!(
        bridge
            .encode_rgba(
                session,
                invalid_format,
                rgba_request(&[0; 4], 1, 1, 4, "gif"),
            )
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, invalid_format).code(),
        NativeErrorCode::InvalidRequest
    );

    let over_limit = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, over_limit));
    assert!(
        bridge
            .encode_rgba(
                session,
                over_limit,
                rgba_request(&[], 10_001, 10_000, 40_004, "png"),
            )
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, over_limit).code(),
        NativeErrorCode::Limit
    );

    let stale = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, stale));
    let current = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, current));
    assert!(
        bridge
            .encode_rgba(session, stale, rgba_request(&[0; 4], 1, 1, 4, "png"),)
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, stale).code(),
        NativeErrorCode::StaleRequest
    );

    let cancelled = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, cancelled));
    assert!(bridge.cancel_request(session, cancelled));
    assert!(
        bridge
            .encode_rgba(session, cancelled, rgba_request(&[0; 4], 1, 1, 4, "png"),)
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, cancelled).code(),
        NativeErrorCode::Cancelled
    );
    assert_eq!(bridge.byte_buffer_count(), 0);
}

#[test]
fn animation_frames_have_independent_owned_handles() {
    let bridge = BridgeState::default();
    let parent = bridge.insert_decoded_for_test(animated_image()).unwrap();
    let parent_image = bridge.image(parent).unwrap();
    assert_eq!(parent_image.frame_count(), 2);
    assert_eq!(parent_image.loop_count(), Some(2));
    assert_eq!(parent_image.frame_duration_ms(0), Some(20));
    assert_eq!(parent_image.frame_duration_ms(1), Some(30));
    assert_eq!(parent_image.frame_duration_ms(2), None);
    assert_eq!(parent_image.pixels(), [255, 0, 0, 255]);
    assert!(bridge.image_frame(parent, 2).is_none());

    let child = bridge.image_frame(parent, 1).unwrap();
    let sibling = bridge.image_frame(parent, 1).unwrap();
    assert_eq!(
        bridge.image(child).unwrap().pixels_ptr(),
        bridge.image(sibling).unwrap().pixels_ptr()
    );
    assert!(bridge.release_image(parent));
    assert!(!bridge.release_image(parent));
    let child_image = bridge.image(child).unwrap();
    assert_eq!(child_image.frame_count(), 1);
    assert_eq!(child_image.loop_count(), None);
    assert_eq!(child_image.frame_duration_ms(0), Some(0));
    assert_eq!(child_image.pixels(), [0, 255, 0, 255]);
    assert!(bridge.release_image(child));
    assert!(!bridge.release_image(child));
    assert_eq!(bridge.image(sibling).unwrap().pixels(), [0, 255, 0, 255]);
    assert!(bridge.release_image(sibling));
}

#[test]
fn native_image_limits_accept_streaming_frames_and_reject_checked_boundaries() {
    assert_eq!(MAX_NATIVE_POSTER_PIXELS, 4_096 * 4_096);
    assert_eq!(MAX_NATIVE_IMAGE_RGBA_BYTES, 128 * 1024 * 1024);
    let bridge = BridgeState::default();
    let pixel = RgbaImage::new(1, 1, vec![0, 0, 0, 255]).unwrap();

    let streaming_frames = DecodedImage {
        poster: pixel.clone(),
        animation: (0..65)
            .map(|_| AnimationFrame {
                image: pixel.clone(),
                duration_ms: 10,
            })
            .collect(),
        loop_count: Some(0),
    };
    let parent = bridge.insert_decoded_for_test(streaming_frames).unwrap();
    assert_eq!(bridge.image(parent).unwrap().frame_count(), 65);
    let child = bridge.image_frame(parent, 64).unwrap();
    let sibling = bridge.image_frame(parent, 64).unwrap();
    assert_eq!(
        bridge.image(child).unwrap().pixels_ptr(),
        bridge.image(sibling).unwrap().pixels_ptr()
    );
    assert!(bridge.release_image(parent));
    assert_eq!(bridge.image(child).unwrap().pixels(), [0, 0, 0, 255]);
    assert!(bridge.release_image(child));
    assert_eq!(bridge.image(sibling).unwrap().pixels(), [0, 0, 0, 255]);
    assert!(bridge.release_image(sibling));

    let too_many_frames = DecodedImage {
        poster: pixel.clone(),
        animation: (0..=MAX_NATIVE_ANIMATION_FRAMES)
            .map(|_| AnimationFrame {
                image: pixel.clone(),
                duration_ms: 10,
            })
            .collect(),
        loop_count: Some(0),
    };
    let error = bridge.insert_decoded_for_test(too_many_frames).unwrap_err();
    assert_eq!(error.code(), NativeErrorCode::Limit);
    assert_eq!(error.args_json(), r#"{"dimension":"animation_frames"}"#);
    assert_eq!(bridge.image_count(), 0);

    assert!(
        validate_native_image_layout_for_test(
            MAX_NATIVE_POSTER_PIXELS,
            1,
            [MAX_NATIVE_IMAGE_RGBA_BYTES],
        )
        .is_ok()
    );
    let aggregate = validate_native_image_layout_for_test(
        MAX_NATIVE_POSTER_PIXELS,
        1,
        [MAX_NATIVE_IMAGE_RGBA_BYTES, 1],
    )
    .unwrap_err();
    assert_eq!(aggregate.code(), NativeErrorCode::Limit);
    assert_eq!(aggregate.args_json(), r#"{"dimension":"rgba_bytes"}"#);

    let poster =
        validate_native_image_layout_for_test(MAX_NATIVE_POSTER_PIXELS + 1, 0, [4]).unwrap_err();
    assert_eq!(poster.code(), NativeErrorCode::Limit);
    assert_eq!(poster.args_json(), r#"{"dimension":"poster_pixels"}"#);
}

#[test]
fn oversized_sparse_inputs_stop_before_decode_and_archive_parse() {
    let directory = TestDirectory::new("oversized-input");
    let oversized_decode_path = directory.path().join("oversized-decode.bin");
    let oversized_archive_path = directory.path().join("oversized-archive.bin");
    create_sparse_file(&oversized_decode_path, MAX_ANDROID_ENCODED_INPUT_BYTES + 1);
    create_sparse_file(&oversized_archive_path, MAX_ANDROID_ARCHIVE_INPUT_BYTES + 1);

    let bridge = BridgeState::default();
    let session = bridge.create_session().unwrap();
    let decode_request = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, decode_request));
    assert!(
        bridge
            .decode_path(
                session,
                decode_request,
                &oversized_decode_path,
                Some("image/png"),
            )
            .is_none()
    );
    let decode_error = bridge.request_error(session, decode_request);
    assert_eq!(decode_error.code(), NativeErrorCode::Limit);
    assert_eq!(decode_error.args_json(), r#"{"dimension":"input_bytes"}"#);
    assert_eq!(bridge.image_count(), 0);

    let archive_request = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, archive_request));
    assert!(
        bridge
            .open_archive(session, archive_request, &oversized_archive_path, "zip")
            .is_none()
    );
    let archive_error = bridge.request_error(session, archive_request);
    assert_eq!(archive_error.code(), NativeErrorCode::Limit);
    assert_eq!(
        archive_error.args_json(),
        r#"{"dimension":"archive_bytes"}"#
    );
    assert_eq!(bridge.archive_count(), 0);
    assert!(bridge.release_session(session));
}

#[test]
fn android_archive_input_entry_and_combined_retention_are_bounded() {
    assert_eq!(MAX_ANDROID_ARCHIVE_INPUT_BYTES, 64 * 1024 * 1024);
    assert_eq!(MAX_ANDROID_ARCHIVE_ENTRY_BYTES, 64 * 1024 * 1024);
    assert_eq!(MAX_ANDROID_ARCHIVE_RETAINED_BYTES, 128 * 1024 * 1024);
    assert!(
        validate_archive_retained_layout_for_test(
            MAX_ANDROID_ARCHIVE_INPUT_BYTES,
            MAX_ANDROID_ARCHIVE_ENTRY_BYTES,
        )
        .is_ok()
    );

    let error = validate_archive_retained_layout_for_test(
        MAX_ANDROID_ARCHIVE_INPUT_BYTES,
        MAX_ANDROID_ARCHIVE_ENTRY_BYTES + 1,
    )
    .unwrap_err();
    assert_eq!(error.kind(), CoreErrorKind::Limit);
    assert_eq!(core_error(&error).code(), NativeErrorCode::Limit);
}

#[test]
fn oversized_sparse_listed_target_is_not_materialized_or_decoded() {
    let directory = TestDirectory::new("oversized-listed-entry");
    let target_path = directory.path().join("oversized.bin");
    let listed_path = directory.path().join("pages.wmltxt");
    create_sparse_file(&target_path, MAX_ANDROID_ARCHIVE_ENTRY_BYTES + 1);
    std::fs::write(&listed_path, "#!WMLViewer2 ListedFile\noversized.bin\n").unwrap();

    let bridge = BridgeState::default();
    let session = bridge.create_session().unwrap();
    let request = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, request));
    let archive = bridge
        .open_archive(session, request, &listed_path, "wmltxt")
        .unwrap();

    assert!(
        bridge
            .materialize_archive_entry(session, request, archive, 0)
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, request).code(),
        NativeErrorCode::Limit
    );
    assert_eq!(bridge.byte_buffer_count(), 0);

    assert!(
        bridge
            .decode_archive_entry(session, request, archive, 0, Some("image/png"))
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, request).code(),
        NativeErrorCode::Limit
    );
    assert_eq!(bridge.image_count(), 0);
    assert!(bridge.release_archive(archive));
    assert!(bridge.release_session(session));
}

#[test]
fn listed_archive_materializes_decodes_and_double_releases_safely() {
    let bridge = BridgeState::default();
    let session = bridge.create_session().unwrap();
    let request = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, request));

    let directory = TestDirectory::new("listed");
    let image_path = directory.path().join("001.png");
    let listed_path = directory.path().join("pages.wmltxt");
    let expected_bytes = png_bytes();
    std::fs::write(&image_path, &expected_bytes).unwrap();
    std::fs::write(&listed_path, "#!WMLViewer2 ListedFile\n001.png\n").unwrap();

    let archive = bridge
        .open_archive(session, request, &listed_path, "wmltxt")
        .unwrap();
    let archive_value = bridge.archive(archive).unwrap();
    assert_eq!(archive_value.entry_count(), 1);
    assert_eq!(archive_value.entry_name(0), Some("001.png"));

    let encoded = bridge
        .materialize_archive_entry(session, request, archive, 0)
        .unwrap();
    {
        let bytes = bridge.native_bytes(encoded).unwrap();
        assert_eq!(bytes.as_slice(), expected_bytes);
    }
    assert_eq!(bridge.byte_buffer_count(), 1);

    let current = bridge.next_request_id(session).unwrap();
    assert!(bridge.begin_request(session, current));
    assert!(
        bridge
            .materialize_archive_entry(session, request, archive, 0)
            .is_none()
    );
    assert_eq!(
        bridge.request_error(session, request).code(),
        NativeErrorCode::StaleRequest
    );
    assert_eq!(bridge.byte_buffer_count(), 1);

    let image = bridge
        .decode_archive_entry(session, current, archive, 0, Some("image/png"))
        .unwrap();
    assert_eq!(bridge.image(image).unwrap().width(), 2);
    assert!(bridge.release_image(image));
    assert!(bridge.release_archive(archive));
    assert!(!bridge.release_archive(archive));
    assert!(bridge.release_session(session));

    assert_eq!(
        bridge.native_bytes(encoded).unwrap().as_slice(),
        expected_bytes
    );
    assert!(bridge.release_bytes(encoded));
    assert!(!bridge.release_bytes(encoded));
    assert_eq!(bridge.byte_buffer_count(), 0);
}
