//! JNI bridge for the Compose Android frontend.
//!
//! Image and encoded-byte buffers are owned by their native handles and remain
//! valid until the matching release function succeeds. Kotlin must not retain
//! or access a returned direct buffer after releasing its handle.

mod bounded_io;
mod reading_plan;
mod registry;
mod session;

pub use session::{
    NativeArchiveHandle, NativeBytesHandle, NativeErrorCode, NativeImageHandle, NativeRequestError,
    NativeSessionHandle,
};

use crate::reading_plan::{
    MAX_READING_PAGES, ReadingPlanRequest, encode_reading_wire, plan_reading,
};
use crate::session::{RgbaEncodeRequest, bridge};
use jni::EnvUnowned;
use jni::errors::LogErrorAndDefault;
use jni::objects::{
    JBooleanArray, JByteBuffer, JIntArray, JLongArray, JObject, JString, Reference,
};
use jni::sys::{JNI_FALSE, JNI_TRUE, jboolean, jint, jlong};
use std::path::Path;

const INVALID_HANDLE: jlong = 0;

fn boolean(value: bool) -> jboolean {
    if value { JNI_TRUE } else { JNI_FALSE }
}

fn request_id(raw: jlong) -> Option<u64> {
    let value = u64::try_from(raw).ok()?;
    (value != 0).then_some(value)
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_planReading<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    source_ids: JLongArray<'caller>,
    portrait: JBooleanArray<'caller>,
    covers: JBooleanArray<'caller>,
    current_index: jint,
    landscape: jboolean,
    layout: jint,
    direction: jint,
    cover_alone: jboolean,
    maximum_prefetch_spreads: jint,
) -> JObject<'caller> {
    unowned_env
        .with_env(|env| -> jni::errors::Result<JObject<'caller>> {
            if source_ids.is_null() || portrait.is_null() || covers.is_null() {
                return Ok(JObject::null());
            }
            let page_count = source_ids.len(env)?;
            if page_count == 0
                || page_count > MAX_READING_PAGES
                || portrait.len(env)? != page_count
                || covers.len(env)? != page_count
            {
                return Ok(JObject::null());
            }

            let mut source_id_values = vec![0_i64; page_count];
            let mut portrait_values = vec![JNI_FALSE; page_count];
            let mut cover_values = vec![JNI_FALSE; page_count];
            source_ids.get_region(env, 0, &mut source_id_values)?;
            portrait.get_region(env, 0, &mut portrait_values)?;
            covers.get_region(env, 0, &mut cover_values)?;
            let portrait_values = portrait_values
                .into_iter()
                .map(|value| value != JNI_FALSE)
                .collect::<Vec<_>>();
            let cover_values = cover_values
                .into_iter()
                .map(|value| value != JNI_FALSE)
                .collect::<Vec<_>>();
            let request = ReadingPlanRequest {
                source_ids: &source_id_values,
                portrait: &portrait_values,
                covers: &cover_values,
                current_index,
                landscape: landscape != JNI_FALSE,
                layout,
                direction,
                cover_alone: cover_alone != JNI_FALSE,
                maximum_prefetch_spreads,
            };
            let Some(wire) = plan_reading(request)
                .and_then(|plan| encode_reading_wire(&plan))
                .ok()
            else {
                return Ok(JObject::null());
            };
            let output = JIntArray::new(env, wire.len())?;
            output.set_region(env, 0, &wire)?;
            Ok(output.into())
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_createSession<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
) -> jlong {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jlong> {
            Ok(bridge()
                .create_session()
                .and_then(|handle| i64::try_from(handle.as_raw()).ok())
                .unwrap_or(INVALID_HANDLE))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_releaseSession<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jboolean {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jboolean> {
            let released = NativeSessionHandle::from_jlong(raw_handle)
                .is_some_and(|handle| bridge().release_session(handle));
            Ok(boolean(released))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_nextRequestId<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jlong {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jlong> {
            let next = NativeSessionHandle::from_jlong(raw_handle)
                .and_then(|handle| bridge().next_request_id(handle))
                .and_then(|value| i64::try_from(value).ok())
                .unwrap_or(0);
            Ok(next)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_beginRequest<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_request_id: jlong,
) -> jboolean {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jboolean> {
            let begun = NativeSessionHandle::from_jlong(raw_handle)
                .zip(request_id(raw_request_id))
                .is_some_and(|(handle, id)| bridge().begin_request(handle, id));
            Ok(boolean(begun))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_cancelRequest<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_request_id: jlong,
) -> jboolean {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jboolean> {
            let cancelled = NativeSessionHandle::from_jlong(raw_handle)
                .zip(request_id(raw_request_id))
                .is_some_and(|(handle, id)| bridge().cancel_request(handle, id));
            Ok(boolean(cancelled))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_decode<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_request_id: jlong,
    path: JString<'caller>,
    mime: JString<'caller>,
) -> jlong {
    unowned_env
        .with_env(|env| -> jni::errors::Result<jlong> {
            let Some(handle) = NativeSessionHandle::from_jlong(raw_handle) else {
                return Ok(INVALID_HANDLE);
            };
            let Some(id) = request_id(raw_request_id) else {
                return Ok(INVALID_HANDLE);
            };
            let path = path.try_to_string(env)?;
            let mime = if mime.is_null() {
                None
            } else {
                Some(mime.try_to_string(env)?)
            };
            Ok(bridge()
                .decode_path(handle, id, Path::new(&path), mime.as_deref())
                .and_then(|image| i64::try_from(image.as_raw()).ok())
                .unwrap_or(INVALID_HANDLE))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_encodeRgba<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_session: jlong,
    raw_request_id: jlong,
    rgba: JByteBuffer<'caller>,
    raw_width: jint,
    raw_height: jint,
    raw_stride: jint,
    format: JString<'caller>,
) -> jlong {
    unowned_env
        .with_env(|env| -> jni::errors::Result<jlong> {
            let Some(session) = NativeSessionHandle::from_jlong(raw_session) else {
                return Ok(INVALID_HANDLE);
            };
            let Some(id) = request_id(raw_request_id) else {
                return Ok(INVALID_HANDLE);
            };
            if !bridge().require_current_request(session, id) {
                return Ok(INVALID_HANDLE);
            }
            if rgba.is_null() {
                bridge().record_invalid_argument(session, id, "rgba");
                return Ok(INVALID_HANDLE);
            }
            if format.is_null() {
                bridge().record_invalid_argument(session, id, "format");
                return Ok(INVALID_HANDLE);
            }
            let format = match format.try_to_string(env) {
                Ok(format) => format,
                Err(_) => {
                    bridge().record_invalid_argument(session, id, "format");
                    return Ok(INVALID_HANDLE);
                }
            };
            let capacity = match env.get_direct_buffer_capacity(&rgba) {
                Ok(capacity) => capacity,
                Err(_) => {
                    bridge().record_invalid_argument(session, id, "rgba");
                    return Ok(INVALID_HANDLE);
                }
            };
            let address = match env.get_direct_buffer_address(&rgba) {
                Ok(address) => address,
                Err(_) => {
                    bridge().record_invalid_argument(session, id, "rgba");
                    return Ok(INVALID_HANDLE);
                }
            };
            // SAFETY: JNI guarantees the direct buffer allocation covers its
            // reported capacity. encode_rgba copies every used row before this
            // native call returns and never stores a borrow of the Java buffer.
            let rgba = if capacity == 0 {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(address.cast_const(), capacity) }
            };
            Ok(bridge()
                .encode_rgba(
                    session,
                    id,
                    RgbaEncodeRequest {
                        rgba,
                        width: raw_width,
                        height: raw_height,
                        stride: raw_stride,
                        format: &format,
                    },
                )
                .and_then(|handle| i64::try_from(handle.as_raw()).ok())
                .unwrap_or(INVALID_HANDLE))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_imageWidth<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jint {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jint> {
            let width = NativeImageHandle::from_jlong(raw_handle)
                .and_then(|handle| bridge().image(handle))
                .map(|image| image.width())
                .unwrap_or(0);
            Ok(width)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_imageHeight<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jint {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jint> {
            let height = NativeImageHandle::from_jlong(raw_handle)
                .and_then(|handle| bridge().image(handle))
                .map(|image| image.height())
                .unwrap_or(0);
            Ok(height)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_imageStride<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jint {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jint> {
            let stride = NativeImageHandle::from_jlong(raw_handle)
                .and_then(|handle| bridge().image(handle))
                .map(|image| image.stride())
                .unwrap_or(0);
            Ok(stride)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_imageBuffer<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> JObject<'caller> {
    unowned_env
        .with_env(|env| -> jni::errors::Result<JObject<'caller>> {
            let Some(image) =
                NativeImageHandle::from_jlong(raw_handle).and_then(|handle| bridge().image(handle))
            else {
                return Ok(JObject::null());
            };

            // SAFETY: NativeImage owns this stable boxed allocation. The Kotlin
            // wrapper exposes it read-only and keeps the handle alive until all
            // pixel copies have completed.
            let buffer =
                unsafe { env.new_direct_byte_buffer(image.pixels_ptr(), image.pixels_len())? };
            Ok(buffer.into())
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_releaseImage<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jboolean {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jboolean> {
            let released = NativeImageHandle::from_jlong(raw_handle)
                .is_some_and(|handle| bridge().release_image(handle));
            Ok(boolean(released))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_isRequestCurrent<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_request_id: jlong,
) -> jboolean {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jboolean> {
            let current = NativeSessionHandle::from_jlong(raw_handle)
                .zip(request_id(raw_request_id))
                .is_some_and(|(handle, id)| bridge().is_request_current(handle, id));
            Ok(boolean(current))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_requestErrorCode<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_request_id: jlong,
) -> jint {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jint> {
            let error = NativeSessionHandle::from_jlong(raw_handle)
                .map(|handle| {
                    bridge().request_error(handle, request_id(raw_request_id).unwrap_or(0))
                })
                .unwrap_or_else(NativeRequestError::invalid_handle);
            Ok(error.code() as jint)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_requestErrorKey<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_request_id: jlong,
) -> JObject<'caller> {
    unowned_env
        .with_env(|env| -> jni::errors::Result<JObject<'caller>> {
            let error = NativeSessionHandle::from_jlong(raw_handle)
                .map(|handle| {
                    bridge().request_error(handle, request_id(raw_request_id).unwrap_or(0))
                })
                .unwrap_or_else(NativeRequestError::invalid_handle);
            Ok(env.new_string(error.key())?.into())
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_requestErrorArgsJson<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_request_id: jlong,
) -> JObject<'caller> {
    unowned_env
        .with_env(|env| -> jni::errors::Result<JObject<'caller>> {
            let error = NativeSessionHandle::from_jlong(raw_handle)
                .map(|handle| {
                    bridge().request_error(handle, request_id(raw_request_id).unwrap_or(0))
                })
                .unwrap_or_else(NativeRequestError::invalid_handle);
            Ok(env.new_string(error.args_json())?.into())
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_imageFrameCount<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jint {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jint> {
            let count = NativeImageHandle::from_jlong(raw_handle)
                .and_then(|handle| bridge().image(handle))
                .and_then(|image| i32::try_from(image.frame_count()).ok())
                .unwrap_or(0);
            Ok(count)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_imageLoopCount<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jlong {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jlong> {
            let count = NativeImageHandle::from_jlong(raw_handle)
                .and_then(|handle| bridge().image(handle))
                .and_then(|image| image.loop_count())
                .map(i64::from)
                .unwrap_or(-1);
            Ok(count)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_imageFrameDurationMs<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_index: jint,
) -> jlong {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jlong> {
            let duration = NativeImageHandle::from_jlong(raw_handle)
                .zip(usize::try_from(raw_index).ok())
                .and_then(|(handle, index)| bridge().image(handle)?.frame_duration_ms(index))
                .and_then(|value| i64::try_from(value).ok())
                .unwrap_or(-1);
            Ok(duration)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_imageFrame<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_index: jint,
) -> jlong {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jlong> {
            let frame = NativeImageHandle::from_jlong(raw_handle)
                .zip(usize::try_from(raw_index).ok())
                .and_then(|(handle, index)| bridge().image_frame(handle, index))
                .and_then(|handle| i64::try_from(handle.as_raw()).ok())
                .unwrap_or(INVALID_HANDLE);
            Ok(frame)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_openArchive<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_session: jlong,
    raw_request_id: jlong,
    path: JString<'caller>,
    format: JString<'caller>,
) -> jlong {
    unowned_env
        .with_env(|env| -> jni::errors::Result<jlong> {
            let Some(session) = NativeSessionHandle::from_jlong(raw_session) else {
                return Ok(INVALID_HANDLE);
            };
            let Some(id) = request_id(raw_request_id) else {
                return Ok(INVALID_HANDLE);
            };
            let path = path.try_to_string(env)?;
            let format = format.try_to_string(env)?;
            Ok(bridge()
                .open_archive(session, id, Path::new(&path), &format)
                .and_then(|handle| i64::try_from(handle.as_raw()).ok())
                .unwrap_or(INVALID_HANDLE))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_releaseArchive<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jboolean {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jboolean> {
            let released = NativeArchiveHandle::from_jlong(raw_handle)
                .is_some_and(|handle| bridge().release_archive(handle));
            Ok(boolean(released))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_archiveEntryCount<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jint {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jint> {
            let count = NativeArchiveHandle::from_jlong(raw_handle)
                .and_then(|handle| bridge().archive(handle))
                .and_then(|archive| i32::try_from(archive.entry_count()).ok())
                .unwrap_or(0);
            Ok(count)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_archiveEntryName<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_index: jint,
) -> JObject<'caller> {
    unowned_env
        .with_env(|env| -> jni::errors::Result<JObject<'caller>> {
            let Some(name) = NativeArchiveHandle::from_jlong(raw_handle)
                .zip(usize::try_from(raw_index).ok())
                .and_then(|(handle, index)| {
                    bridge()
                        .archive(handle)?
                        .entry_name(index)
                        .map(str::to_owned)
                })
            else {
                return Ok(JObject::null());
            };
            Ok(env.new_string(name)?.into())
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_archiveEntrySize<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
    raw_index: jint,
) -> jlong {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jlong> {
            let size = NativeArchiveHandle::from_jlong(raw_handle)
                .zip(usize::try_from(raw_index).ok())
                .and_then(|(handle, index)| bridge().archive(handle)?.entry_size(index))
                .and_then(|size| i64::try_from(size).ok())
                .unwrap_or(-1);
            Ok(size)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_materializeArchiveEntry<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_session: jlong,
    raw_request_id: jlong,
    raw_archive: jlong,
    raw_index: jint,
) -> jlong {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jlong> {
            let Some(session) = NativeSessionHandle::from_jlong(raw_session) else {
                return Ok(INVALID_HANDLE);
            };
            let Some(id) = request_id(raw_request_id) else {
                return Ok(INVALID_HANDLE);
            };
            let Some(archive) = NativeArchiveHandle::from_jlong(raw_archive) else {
                return Ok(INVALID_HANDLE);
            };
            let Some(index) = usize::try_from(raw_index).ok() else {
                return Ok(INVALID_HANDLE);
            };
            Ok(bridge()
                .materialize_archive_entry(session, id, archive, index)
                .and_then(|handle| i64::try_from(handle.as_raw()).ok())
                .unwrap_or(INVALID_HANDLE))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_bytesLength<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jlong {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jlong> {
            let length = NativeBytesHandle::from_jlong(raw_handle)
                .and_then(|handle| bridge().native_bytes(handle))
                .and_then(|bytes| i64::try_from(bytes.len()).ok())
                .unwrap_or(-1);
            Ok(length)
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_bytesBuffer<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> JObject<'caller> {
    unowned_env
        .with_env(|env| -> jni::errors::Result<JObject<'caller>> {
            let Some(bytes) = NativeBytesHandle::from_jlong(raw_handle)
                .and_then(|handle| bridge().native_bytes(handle))
            else {
                return Ok(JObject::null());
            };

            // SAFETY: NativeBytes owns this stable boxed allocation. The
            // Kotlin Closeable keeps the handle alive while using the view.
            let buffer = unsafe { env.new_direct_byte_buffer(bytes.as_mut_ptr(), bytes.len())? };
            Ok(buffer.into())
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_releaseBytes<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_handle: jlong,
) -> jboolean {
    unowned_env
        .with_env(|_| -> jni::errors::Result<jboolean> {
            let released = NativeBytesHandle::from_jlong(raw_handle)
                .is_some_and(|handle| bridge().release_bytes(handle));
            Ok(boolean(released))
        })
        .resolve::<LogErrorAndDefault>()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_mith_1mmk_wml2viewer_nativebridge_NativeBridge_decodeArchiveEntry<
    'caller,
>(
    mut unowned_env: EnvUnowned<'caller>,
    _this: JObject<'caller>,
    raw_session: jlong,
    raw_request_id: jlong,
    raw_archive: jlong,
    raw_index: jint,
    mime: JString<'caller>,
) -> jlong {
    unowned_env
        .with_env(|env| -> jni::errors::Result<jlong> {
            let Some(session) = NativeSessionHandle::from_jlong(raw_session) else {
                return Ok(INVALID_HANDLE);
            };
            let Some(id) = request_id(raw_request_id) else {
                return Ok(INVALID_HANDLE);
            };
            let Some(archive) = NativeArchiveHandle::from_jlong(raw_archive) else {
                return Ok(INVALID_HANDLE);
            };
            let Some(index) = usize::try_from(raw_index).ok() else {
                return Ok(INVALID_HANDLE);
            };
            let mime = if mime.is_null() {
                None
            } else {
                Some(mime.try_to_string(env)?)
            };
            Ok(bridge()
                .decode_archive_entry(session, id, archive, index, mime.as_deref())
                .and_then(|handle| i64::try_from(handle.as_raw()).ok())
                .unwrap_or(INVALID_HANDLE))
        })
        .resolve::<LogErrorAndDefault>()
}

#[cfg(test)]
mod tests;
