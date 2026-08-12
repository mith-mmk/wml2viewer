use super::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            ".test-wml2viewer-ios-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn begin(session: u64) -> u64 {
    let request = wml2viewer_ios_request_next(session);
    assert_ne!(request, 0);
    assert_eq!(wml2viewer_ios_request_begin(session, request), TRUE);
    request
}

#[test]
fn session_request_and_release_contract_is_idempotent() {
    let session = wml2viewer_ios_session_create();
    assert_ne!(session, 0);
    let request = begin(session);
    assert_eq!(wml2viewer_ios_request_is_current(session, request), TRUE);
    assert_eq!(wml2viewer_ios_request_cancel(session, request), TRUE);
    assert_eq!(wml2viewer_ios_request_cancel(session, request), FALSE);
    assert_eq!(wml2viewer_ios_session_release(session), TRUE);
    assert_eq!(wml2viewer_ios_session_release(session), FALSE);
}

#[test]
fn invalid_pointer_length_pairs_fail_without_unwinding() {
    let session = wml2viewer_ios_session_create();
    let request = begin(session);
    assert_eq!(
        unsafe { wml2viewer_ios_decode_local(session, request, ptr::null(), 1, ptr::null(), 0) },
        0
    );
    assert_eq!(wml2viewer_ios_request_error_code(session, request), 2);
    assert_eq!(wml2viewer_ios_session_release(session), TRUE);
}

#[test]
fn encode_returns_owned_bytes_with_borrowed_view() {
    let session = wml2viewer_ios_session_create();
    let request = begin(session);
    let rgba = [255, 0, 0, 255];
    let format = b"png";
    let bytes = unsafe {
        wml2viewer_ios_encode_rgba(
            session,
            request,
            rgba.as_ptr(),
            rgba.len(),
            1,
            1,
            4,
            format.as_ptr(),
            format.len(),
        )
    };
    assert_ne!(bytes, 0);
    let mut pointer = ptr::null();
    let mut length = 0;
    assert_eq!(
        unsafe { wml2viewer_ios_bytes_view(bytes, &mut pointer, &mut length) },
        TRUE
    );
    assert!(!pointer.is_null());
    assert!(length > 8);
    assert_eq!(wml2viewer_ios_bytes_release(bytes), TRUE);
    assert_eq!(wml2viewer_ios_bytes_release(bytes), FALSE);
    assert_eq!(wml2viewer_ios_session_release(session), TRUE);
}

#[test]
fn archive_names_use_two_pass_utf8_copy() {
    let directory = TestDirectory::new();
    let path = directory.path().join("pages.wmltxt");
    std::fs::write(&path, b"#!WMLViewer2 ListedFile\npages/001.png\n").unwrap();
    let session = wml2viewer_ios_session_create();
    let request = begin(session);
    let path_bytes = path.to_str().unwrap().as_bytes();
    let archive = unsafe {
        wml2viewer_ios_archive_open_local(
            session,
            request,
            path_bytes.as_ptr(),
            path_bytes.len(),
            b"wmltxt".as_ptr(),
            6,
        )
    };
    assert_ne!(archive, 0);
    assert_eq!(wml2viewer_ios_archive_entry_count(archive), 1);
    let mut required = 0;
    assert_eq!(
        unsafe { wml2viewer_ios_archive_entry_name(archive, 0, ptr::null_mut(), 0, &mut required) },
        TRUE
    );
    let mut name = vec![0; required];
    assert_eq!(
        unsafe {
            wml2viewer_ios_archive_entry_name(
                archive,
                0,
                name.as_mut_ptr(),
                name.len(),
                &mut required,
            )
        },
        TRUE
    );
    assert_eq!(name, b"pages/001.png");
    assert_eq!(wml2viewer_ios_image_release(archive), FALSE);
    assert_eq!(wml2viewer_ios_archive_release(archive), TRUE);
    assert_eq!(wml2viewer_ios_session_release(session), TRUE);
}

#[test]
fn reading_wire_supports_size_query_and_checked_copy() {
    let source_ids = [7, 7, 7];
    let portrait = [1, 1, 1];
    let covers = [1, 0, 0];
    let mut required = 0;
    let queried = unsafe {
        wml2viewer_ios_plan_reading_v1(
            source_ids.as_ptr(),
            portrait.as_ptr(),
            covers.as_ptr(),
            source_ids.len(),
            1,
            1,
            0,
            1,
            1,
            1,
            ptr::null_mut(),
            0,
            &mut required,
        )
    };
    assert_eq!(queried, TRUE);
    assert!(required >= 8);
    let mut wire = vec![0; required];
    assert_eq!(
        unsafe {
            wml2viewer_ios_plan_reading_v1(
                source_ids.as_ptr(),
                portrait.as_ptr(),
                covers.as_ptr(),
                source_ids.len(),
                1,
                1,
                0,
                1,
                1,
                1,
                wire.as_mut_ptr(),
                wire.len(),
                &mut required,
            )
        },
        TRUE
    );
    assert_eq!(wire[0], 1);
    assert_eq!(wire[1] as usize, wire.len());
}

#[test]
fn error_text_is_stable_nonlocalized_utf8() {
    let session = wml2viewer_ios_session_create();
    let request = begin(session);
    assert_eq!(wml2viewer_ios_request_cancel(session, request), TRUE);
    let mut length = 0;
    assert_eq!(
        unsafe {
            wml2viewer_ios_request_error_key(session, request, ptr::null_mut(), 0, &mut length)
        },
        TRUE
    );
    let mut key = vec![0; length];
    assert_eq!(
        unsafe {
            wml2viewer_ios_request_error_key(
                session,
                request,
                key.as_mut_ptr(),
                key.len(),
                &mut length,
            )
        },
        TRUE
    );
    assert_eq!(key, b"cancelled");
    assert_eq!(wml2viewer_ios_session_release(session), TRUE);
}

#[test]
fn public_header_declares_every_c_export() {
    let header = include_str!("../include/wml2viewer_ios.h");
    let exports = [
        "wml2viewer_ios_session_create",
        "wml2viewer_ios_session_release",
        "wml2viewer_ios_request_next",
        "wml2viewer_ios_request_begin",
        "wml2viewer_ios_request_cancel",
        "wml2viewer_ios_request_is_current",
        "wml2viewer_ios_decode_local",
        "wml2viewer_ios_image_release",
        "wml2viewer_ios_image_width",
        "wml2viewer_ios_image_height",
        "wml2viewer_ios_image_stride",
        "wml2viewer_ios_image_rgba",
        "wml2viewer_ios_image_frame_count",
        "wml2viewer_ios_image_loop_count",
        "wml2viewer_ios_image_frame_duration_ms",
        "wml2viewer_ios_image_frame",
        "wml2viewer_ios_archive_open_local",
        "wml2viewer_ios_archive_release",
        "wml2viewer_ios_archive_entry_count",
        "wml2viewer_ios_archive_entry_name",
        "wml2viewer_ios_archive_entry_size",
        "wml2viewer_ios_archive_entry_decode",
        "wml2viewer_ios_archive_entry_materialize",
        "wml2viewer_ios_bytes_release",
        "wml2viewer_ios_bytes_view",
        "wml2viewer_ios_encode_rgba",
        "wml2viewer_ios_request_error_code",
        "wml2viewer_ios_request_error_key",
        "wml2viewer_ios_request_error_args_json",
        "wml2viewer_ios_plan_reading_v1",
    ];
    for export in exports {
        assert!(header.contains(export), "header is missing {export}");
    }
    assert_eq!(header.matches("wml2viewer_ios_").count(), exports.len());
}
