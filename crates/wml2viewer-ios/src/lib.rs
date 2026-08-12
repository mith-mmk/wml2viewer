//! Checked C ABI adapter for the iOS frontend.
//!
//! All returned image and byte pointers are borrowed from their owning handles.
//! They become invalid immediately after the corresponding release succeeds.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::ptr;
use wml2viewer_mobile_bridge::{
    MAX_READING_PAGES, NativeArchiveHandle, NativeBytesHandle, NativeImageHandle,
    NativeSessionHandle, ReadingPlanRequest, RgbaEncodeRequest, bridge, encode_reading_wire,
    plan_reading,
};

const FALSE: u8 = 0;
const TRUE: u8 = 1;
const MAX_PATH_BYTES: usize = 16 * 1024;
const MAX_TEXT_BYTES: usize = 4 * 1024;
const MAX_RGBA_INPUT_BYTES: usize = 512 * 1024 * 1024;

fn ffi<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(fallback)
}

fn boolean(value: bool) -> u8 {
    if value { TRUE } else { FALSE }
}

unsafe fn input_slice<'a, T>(pointer: *const T, length: usize) -> Option<&'a [T]> {
    if length == 0 {
        return Some(&[]);
    }
    if pointer.is_null() {
        return None;
    }
    Some(unsafe { std::slice::from_raw_parts(pointer, length) })
}

unsafe fn input_utf8<'a>(pointer: *const u8, length: usize, maximum: usize) -> Option<&'a str> {
    if length > maximum {
        return None;
    }
    let bytes = unsafe { input_slice(pointer, length) }?;
    std::str::from_utf8(bytes).ok()
}

unsafe fn write_value<T: Copy>(output: *mut T, value: T) -> bool {
    if output.is_null() {
        return false;
    }
    unsafe { output.write(value) };
    true
}

unsafe fn write_utf8(
    value: &str,
    output: *mut u8,
    capacity: usize,
    output_length: *mut usize,
) -> bool {
    if !unsafe { write_value(output_length, value.len()) } {
        return false;
    }
    if capacity == 0 {
        return output.is_null();
    }
    if output.is_null() || capacity < value.len() {
        return false;
    }
    unsafe { ptr::copy_nonoverlapping(value.as_ptr(), output, value.len()) };
    true
}

fn request_id(raw: u64) -> Option<u64> {
    (raw != 0).then_some(raw)
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_session_create() -> u64 {
    ffi(0, || {
        bridge()
            .create_session()
            .map_or(0, |handle| handle.as_raw())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_session_release(raw_handle: u64) -> u8 {
    ffi(FALSE, || {
        boolean(
            NativeSessionHandle::from_raw(raw_handle)
                .is_some_and(|handle| bridge().release_session(handle)),
        )
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_request_next(raw_session: u64) -> u64 {
    ffi(0, || {
        NativeSessionHandle::from_raw(raw_session)
            .and_then(|handle| bridge().next_request_id(handle))
            .unwrap_or(0)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_request_begin(raw_session: u64, raw_request: u64) -> u8 {
    ffi(FALSE, || {
        boolean(
            NativeSessionHandle::from_raw(raw_session)
                .zip(request_id(raw_request))
                .is_some_and(|(session, request)| bridge().begin_request(session, request)),
        )
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_request_cancel(raw_session: u64, raw_request: u64) -> u8 {
    ffi(FALSE, || {
        boolean(
            NativeSessionHandle::from_raw(raw_session)
                .zip(request_id(raw_request))
                .is_some_and(|(session, request)| bridge().cancel_request(session, request)),
        )
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_request_is_current(raw_session: u64, raw_request: u64) -> u8 {
    ffi(FALSE, || {
        boolean(
            NativeSessionHandle::from_raw(raw_session)
                .zip(request_id(raw_request))
                .is_some_and(|(session, request)| bridge().is_request_current(session, request)),
        )
    })
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_decode_local(
    raw_session: u64,
    raw_request: u64,
    path: *const u8,
    path_length: usize,
    mime: *const u8,
    mime_length: usize,
) -> u64 {
    ffi(0, || {
        let Some(session) = NativeSessionHandle::from_raw(raw_session) else {
            return 0;
        };
        let Some(request) = request_id(raw_request) else {
            return 0;
        };
        let Some(path) = (unsafe { input_utf8(path, path_length, MAX_PATH_BYTES) }) else {
            bridge().record_invalid_argument(session, request, "path");
            return 0;
        };
        let mime = if mime_length == 0 {
            None
        } else {
            let Some(value) = (unsafe { input_utf8(mime, mime_length, MAX_TEXT_BYTES) }) else {
                bridge().record_invalid_argument(session, request, "mime");
                return 0;
            };
            Some(value)
        };
        bridge()
            .decode_path(session, request, Path::new(path), mime)
            .map_or(0, |handle| handle.as_raw())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_image_release(raw_handle: u64) -> u8 {
    ffi(FALSE, || {
        boolean(
            NativeImageHandle::from_raw(raw_handle)
                .is_some_and(|handle| bridge().release_image(handle)),
        )
    })
}

macro_rules! image_i32_property {
    ($name:ident, $getter:ident) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(raw_handle: u64) -> i32 {
            ffi(0, || {
                NativeImageHandle::from_raw(raw_handle)
                    .and_then(|handle| bridge().image(handle))
                    .map_or(0, |image| image.$getter())
            })
        }
    };
}

image_i32_property!(wml2viewer_ios_image_width, width);
image_i32_property!(wml2viewer_ios_image_height, height);
image_i32_property!(wml2viewer_ios_image_stride, stride);

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_image_rgba(
    raw_handle: u64,
    output_pointer: *mut *const u8,
    output_length: *mut usize,
) -> u8 {
    ffi(FALSE, || {
        if output_pointer.is_null() || output_length.is_null() {
            return FALSE;
        }
        let Some(image) = NativeImageHandle::from_raw(raw_handle).and_then(|h| bridge().image(h))
        else {
            return FALSE;
        };
        unsafe {
            output_pointer.write(image.pixels_ptr().cast_const());
            output_length.write(image.pixels_len());
        }
        TRUE
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_image_frame_count(raw_handle: u64) -> usize {
    ffi(0, || {
        NativeImageHandle::from_raw(raw_handle)
            .and_then(|handle| bridge().image(handle))
            .map_or(0, |image| image.frame_count())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_image_loop_count(raw_handle: u64) -> i64 {
    ffi(-1, || {
        NativeImageHandle::from_raw(raw_handle)
            .and_then(|handle| bridge().image(handle))
            .map_or(-1, |image| image.loop_count().map_or(-1, i64::from))
    })
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_image_frame_duration_ms(
    raw_handle: u64,
    index: usize,
    output_duration: *mut u64,
) -> u8 {
    ffi(FALSE, || {
        let Some(duration) = NativeImageHandle::from_raw(raw_handle)
            .and_then(|handle| bridge().image(handle))
            .and_then(|image| image.frame_duration_ms(index))
        else {
            return FALSE;
        };
        boolean(unsafe { write_value(output_duration, duration) })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_image_frame(raw_handle: u64, index: usize) -> u64 {
    ffi(0, || {
        NativeImageHandle::from_raw(raw_handle)
            .and_then(|handle| bridge().image_frame(handle, index))
            .map_or(0, |handle| handle.as_raw())
    })
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_archive_open_local(
    raw_session: u64,
    raw_request: u64,
    path: *const u8,
    path_length: usize,
    format: *const u8,
    format_length: usize,
) -> u64 {
    ffi(0, || {
        let Some(session) = NativeSessionHandle::from_raw(raw_session) else {
            return 0;
        };
        let Some(request) = request_id(raw_request) else {
            return 0;
        };
        let Some(path) = (unsafe { input_utf8(path, path_length, MAX_PATH_BYTES) }) else {
            bridge().record_invalid_argument(session, request, "path");
            return 0;
        };
        let Some(format) = (unsafe { input_utf8(format, format_length, MAX_TEXT_BYTES) }) else {
            bridge().record_invalid_argument(session, request, "format");
            return 0;
        };
        bridge()
            .open_archive(session, request, Path::new(path), format)
            .map_or(0, |handle| handle.as_raw())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_archive_release(raw_handle: u64) -> u8 {
    ffi(FALSE, || {
        boolean(
            NativeArchiveHandle::from_raw(raw_handle)
                .is_some_and(|handle| bridge().release_archive(handle)),
        )
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_archive_entry_count(raw_handle: u64) -> usize {
    ffi(0, || {
        NativeArchiveHandle::from_raw(raw_handle)
            .and_then(|handle| bridge().archive(handle))
            .map_or(0, |archive| archive.entry_count())
    })
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_archive_entry_name(
    raw_handle: u64,
    index: usize,
    output: *mut u8,
    capacity: usize,
    output_length: *mut usize,
) -> u8 {
    ffi(FALSE, || {
        let Some(archive) =
            NativeArchiveHandle::from_raw(raw_handle).and_then(|handle| bridge().archive(handle))
        else {
            return FALSE;
        };
        let Some(name) = archive.entry_name(index) else {
            return FALSE;
        };
        boolean(unsafe { write_utf8(name, output, capacity, output_length) })
    })
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_archive_entry_size(
    raw_handle: u64,
    index: usize,
    output_size: *mut u64,
    output_known: *mut u8,
) -> u8 {
    ffi(FALSE, || {
        if output_size.is_null() || output_known.is_null() {
            return FALSE;
        }
        let Some(archive) =
            NativeArchiveHandle::from_raw(raw_handle).and_then(|handle| bridge().archive(handle))
        else {
            return FALSE;
        };
        if index >= archive.entry_count() {
            return FALSE;
        }
        let size = archive.entry_size(index);
        unsafe {
            output_size.write(size.unwrap_or(0));
            output_known.write(boolean(size.is_some()));
        }
        TRUE
    })
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_archive_entry_decode(
    raw_session: u64,
    raw_request: u64,
    raw_archive: u64,
    index: usize,
    mime: *const u8,
    mime_length: usize,
) -> u64 {
    ffi(0, || {
        let Some((session, request, archive)) = NativeSessionHandle::from_raw(raw_session)
            .zip(request_id(raw_request))
            .zip(NativeArchiveHandle::from_raw(raw_archive))
            .map(|((session, request), archive)| (session, request, archive))
        else {
            return 0;
        };
        let mime = if mime_length == 0 {
            None
        } else {
            let Some(value) = (unsafe { input_utf8(mime, mime_length, MAX_TEXT_BYTES) }) else {
                bridge().record_invalid_argument(session, request, "mime");
                return 0;
            };
            Some(value)
        };
        bridge()
            .decode_archive_entry(session, request, archive, index, mime)
            .map_or(0, |handle| handle.as_raw())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_archive_entry_materialize(
    raw_session: u64,
    raw_request: u64,
    raw_archive: u64,
    index: usize,
) -> u64 {
    ffi(0, || {
        NativeSessionHandle::from_raw(raw_session)
            .zip(request_id(raw_request))
            .zip(NativeArchiveHandle::from_raw(raw_archive))
            .and_then(|((session, request), archive)| {
                bridge().materialize_archive_entry(session, request, archive, index)
            })
            .map_or(0, |handle| handle.as_raw())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_bytes_release(raw_handle: u64) -> u8 {
    ffi(FALSE, || {
        boolean(
            NativeBytesHandle::from_raw(raw_handle)
                .is_some_and(|handle| bridge().release_bytes(handle)),
        )
    })
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_bytes_view(
    raw_handle: u64,
    output_pointer: *mut *const u8,
    output_length: *mut usize,
) -> u8 {
    ffi(FALSE, || {
        if output_pointer.is_null() || output_length.is_null() {
            return FALSE;
        }
        let Some(bytes) = NativeBytesHandle::from_raw(raw_handle)
            .and_then(|handle| bridge().native_bytes(handle))
        else {
            return FALSE;
        };
        unsafe {
            output_pointer.write(bytes.as_mut_ptr().cast_const());
            output_length.write(bytes.len());
        }
        TRUE
    })
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_encode_rgba(
    raw_session: u64,
    raw_request: u64,
    rgba: *const u8,
    rgba_length: usize,
    width: i32,
    height: i32,
    stride: i32,
    format: *const u8,
    format_length: usize,
) -> u64 {
    ffi(0, || {
        let Some((session, request)) =
            NativeSessionHandle::from_raw(raw_session).zip(request_id(raw_request))
        else {
            return 0;
        };
        if rgba_length > MAX_RGBA_INPUT_BYTES {
            bridge().record_invalid_argument(session, request, "rgba_capacity");
            return 0;
        }
        let Some(rgba) = (unsafe { input_slice(rgba, rgba_length) }) else {
            bridge().record_invalid_argument(session, request, "rgba");
            return 0;
        };
        let Some(format) = (unsafe { input_utf8(format, format_length, MAX_TEXT_BYTES) }) else {
            bridge().record_invalid_argument(session, request, "format");
            return 0;
        };
        bridge()
            .encode_rgba(
                session,
                request,
                RgbaEncodeRequest {
                    rgba,
                    width,
                    height,
                    stride,
                    format,
                },
            )
            .map_or(0, |handle| handle.as_raw())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn wml2viewer_ios_request_error_code(raw_session: u64, raw_request: u64) -> i32 {
    ffi(1, || {
        NativeSessionHandle::from_raw(raw_session)
            .zip(request_id(raw_request))
            .map_or(1, |(session, request)| {
                bridge().request_error(session, request).code() as i32
            })
    })
}

unsafe fn write_request_error_text(
    raw_session: u64,
    raw_request: u64,
    output: *mut u8,
    capacity: usize,
    output_length: *mut usize,
    select: impl FnOnce(&wml2viewer_mobile_bridge::NativeRequestError) -> &str,
) -> u8 {
    ffi(FALSE, || {
        let Some((session, request)) =
            NativeSessionHandle::from_raw(raw_session).zip(request_id(raw_request))
        else {
            return FALSE;
        };
        let error = bridge().request_error(session, request);
        boolean(unsafe { write_utf8(select(&error), output, capacity, output_length) })
    })
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_request_error_key(
    raw_session: u64,
    raw_request: u64,
    output: *mut u8,
    capacity: usize,
    output_length: *mut usize,
) -> u8 {
    unsafe {
        write_request_error_text(
            raw_session,
            raw_request,
            output,
            capacity,
            output_length,
            |error| error.key(),
        )
    }
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_request_error_args_json(
    raw_session: u64,
    raw_request: u64,
    output: *mut u8,
    capacity: usize,
    output_length: *mut usize,
) -> u8 {
    unsafe {
        write_request_error_text(
            raw_session,
            raw_request,
            output,
            capacity,
            output_length,
            |error| error.args_json(),
        )
    }
}

/// # Safety
/// Non-null input and output pointers must cover the declared element count and remain valid
/// for the duration of the call. Borrowed outputs remain valid only while their handle lives.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wml2viewer_ios_plan_reading_v1(
    source_ids: *const i64,
    portrait: *const u8,
    covers: *const u8,
    page_count: usize,
    current_index: i32,
    landscape: u8,
    layout: i32,
    direction: i32,
    cover_alone: u8,
    maximum_prefetch_spreads: i32,
    output: *mut i32,
    capacity: usize,
    output_length: *mut usize,
) -> u8 {
    ffi(FALSE, || {
        if page_count == 0 || page_count > MAX_READING_PAGES || output_length.is_null() {
            return FALSE;
        }
        let Some(source_ids) = (unsafe { input_slice(source_ids, page_count) }) else {
            return FALSE;
        };
        let Some(portrait_raw) = (unsafe { input_slice(portrait, page_count) }) else {
            return FALSE;
        };
        let Some(covers_raw) = (unsafe { input_slice(covers, page_count) }) else {
            return FALSE;
        };
        let portrait = portrait_raw
            .iter()
            .map(|value| *value != 0)
            .collect::<Vec<_>>();
        let covers = covers_raw
            .iter()
            .map(|value| *value != 0)
            .collect::<Vec<_>>();
        let Some(wire) = plan_reading(ReadingPlanRequest {
            source_ids,
            portrait: &portrait,
            covers: &covers,
            current_index,
            landscape: landscape != 0,
            layout,
            direction,
            cover_alone: cover_alone != 0,
            maximum_prefetch_spreads,
        })
        .and_then(|plan| encode_reading_wire(&plan))
        .ok() else {
            return FALSE;
        };
        unsafe { output_length.write(wire.len()) };
        if capacity == 0 {
            return boolean(output.is_null());
        }
        if output.is_null() || capacity < wire.len() {
            return FALSE;
        }
        unsafe { ptr::copy_nonoverlapping(wire.as_ptr(), output, wire.len()) };
        TRUE
    })
}

#[cfg(test)]
mod tests;
