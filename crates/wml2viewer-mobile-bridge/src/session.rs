//! Platform-neutral mobile bridge state and resource ownership.

use crate::bounded_io::{BoundedReadError, read_file_bounded};
use crate::registry::HandleRegistry;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use wml2viewer_core::archive::{ArchiveLimits, VirtualContainer, VirtualSourceFormat};
use wml2viewer_core::image::{
    AnimationFrame, DecodeLimits, DecodeRequest, DecodedImage, EncodeFormat, EncodeRequest,
    RgbaImage, decode_with_limits, encode,
};
use wml2viewer_core::reading::SourceId;
use wml2viewer_core::{CoreError, CoreErrorKind};

const MAX_REQUEST_ID: u64 = i64::MAX as u64;
const MAX_RETAINED_REQUEST_ERRORS: usize = 32;
const MAX_ENCODE_PIXELS: u64 = 100_000_000;
const MAX_ENCODE_STRIDE_BYTES: usize = 16 * 1024 * 1024;
const MAX_MOBILE_FILE_INPUT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_MOBILE_ENCODED_INPUT_BYTES: u64 = MAX_MOBILE_FILE_INPUT_BYTES;
pub const MAX_MOBILE_ARCHIVE_INPUT_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_MOBILE_ARCHIVE_RETAINED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_OWNED_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_NATIVE_POSTER_PIXELS: u64 = 4_096 * 4_096;
pub const MAX_NATIVE_IMAGE_RGBA_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_NATIVE_ANIMATION_FRAMES: usize = 4_096;
const MOBILE_DECODE_LIMITS: DecodeLimits = DecodeLimits {
    maximum_frame_pixels: MAX_NATIVE_POSTER_PIXELS,
    maximum_frames: MAX_NATIVE_ANIMATION_FRAMES,
    maximum_rgba_bytes: MAX_NATIVE_IMAGE_RGBA_BYTES,
};
const MOBILE_ARCHIVE_LIMITS: ArchiveLimits = ArchiveLimits {
    maximum_entry_bytes: 64 * 1024 * 1024,
    ..ArchiveLimits::DEFAULT
};
pub const MAX_MOBILE_ARCHIVE_ENTRY_BYTES: u64 = MOBILE_ARCHIVE_LIMITS.maximum_entry_bytes;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct NativeSessionHandle(u64);

impl NativeSessionHandle {
    pub const fn as_raw(self) -> u64 {
        self.0
    }

    pub fn from_signed_raw(raw: i64) -> Option<Self> {
        let value = u64::try_from(raw).ok()?;
        (value != 0).then_some(Self(value))
    }

    pub const fn from_raw(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct NativeImageHandle(u64);

impl NativeImageHandle {
    pub const fn as_raw(self) -> u64 {
        self.0
    }

    pub fn from_signed_raw(raw: i64) -> Option<Self> {
        let value = u64::try_from(raw).ok()?;
        (value != 0).then_some(Self(value))
    }

    pub const fn from_raw(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct NativeArchiveHandle(u64);

impl NativeArchiveHandle {
    pub const fn as_raw(self) -> u64 {
        self.0
    }

    pub fn from_signed_raw(raw: i64) -> Option<Self> {
        let value = u64::try_from(raw).ok()?;
        (value != 0).then_some(Self(value))
    }

    pub const fn from_raw(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct NativeBytesHandle(u64);

impl NativeBytesHandle {
    pub const fn as_raw(self) -> u64 {
        self.0
    }

    pub fn from_signed_raw(raw: i64) -> Option<Self> {
        let value = u64::try_from(raw).ok()?;
        (value != 0).then_some(Self(value))
    }

    pub const fn from_raw(raw: u64) -> Option<Self> {
        if raw == 0 { None } else { Some(Self(raw)) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum NativeErrorCode {
    None = 0,
    InvalidHandle = 1,
    InvalidRequest = 2,
    StaleRequest = 3,
    Cancelled = 4,
    Io = 5,
    Decode = 6,
    Limit = 7,
    Encode = 8,
}

impl NativeErrorCode {
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::InvalidHandle => "invalid_handle",
            Self::InvalidRequest => "invalid_request",
            Self::StaleRequest => "stale_request",
            Self::Cancelled => "cancelled",
            Self::Io => "io",
            Self::Decode => "decode",
            Self::Limit => "limit",
            Self::Encode => "encode",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRequestError {
    code: NativeErrorCode,
    args_json: String,
}

impl NativeRequestError {
    fn new(code: NativeErrorCode, args_json: impl Into<String>) -> Self {
        Self {
            code,
            args_json: args_json.into(),
        }
    }

    pub fn invalid_handle() -> Self {
        Self::new(NativeErrorCode::InvalidHandle, "{}")
    }

    pub const fn code(&self) -> NativeErrorCode {
        self.code
    }

    pub const fn key(&self) -> &'static str {
        self.code.key()
    }

    pub fn args_json(&self) -> &str {
        &self.args_json
    }
}

#[derive(Debug, Default)]
struct RequestState {
    last_allocated_id: u64,
    highest_started_id: u64,
    current_id: Option<u64>,
    errors: BTreeMap<u64, NativeRequestError>,
}

impl RequestState {
    fn next_request_id(&mut self) -> Option<u64> {
        let next = self.last_allocated_id.checked_add(1)?;
        if next > MAX_REQUEST_ID {
            return None;
        }
        self.last_allocated_id = next;
        Some(next)
    }

    fn begin(&mut self, request_id: u64) -> bool {
        if request_id == 0
            || request_id > self.last_allocated_id
            || request_id <= self.highest_started_id
        {
            self.record_error(
                request_id,
                NativeRequestError::new(NativeErrorCode::InvalidRequest, "{}"),
            );
            return false;
        }
        self.highest_started_id = request_id;
        self.current_id = Some(request_id);
        self.errors.remove(&request_id);
        true
    }

    fn cancel(&mut self, request_id: u64) -> bool {
        if self.current_id != Some(request_id) {
            return false;
        }
        self.current_id = None;
        self.record_error(
            request_id,
            NativeRequestError::new(NativeErrorCode::Cancelled, "{}"),
        );
        true
    }

    fn is_current(&self, request_id: u64) -> bool {
        request_id != 0 && self.current_id == Some(request_id)
    }

    fn record_error(&mut self, request_id: u64, error: NativeRequestError) {
        self.errors.insert(request_id, error);
        while self.errors.len() > MAX_RETAINED_REQUEST_ERRORS {
            let Some(oldest) = self.errors.keys().next().copied() else {
                break;
            };
            self.errors.remove(&oldest);
        }
    }

    fn clear_error(&mut self, request_id: u64) {
        self.errors.remove(&request_id);
    }

    fn error(&self, request_id: u64) -> Option<NativeRequestError> {
        self.errors.get(&request_id).cloned()
    }
}

struct NativeFrame {
    width: i32,
    height: i32,
    stride: i32,
    pixels: Box<[u8]>,
}

impl NativeFrame {
    fn from_rgba(image: RgbaImage) -> Result<Self, NativeRequestError> {
        let width = i32::try_from(image.width()).map_err(|_| limit_error("width"))?;
        let height = i32::try_from(image.height()).map_err(|_| limit_error("height"))?;
        let stride = i32::try_from(image.stride()).map_err(|_| limit_error("stride"))?;
        Ok(Self {
            width,
            height,
            stride,
            pixels: image.into_pixels().into_boxed_slice(),
        })
    }
}

pub struct NativeImage {
    poster: Arc<NativeFrame>,
    animation: Vec<(Arc<NativeFrame>, u64)>,
    loop_count: Option<u32>,
}

#[derive(Clone, Copy)]
struct NativeImageLimits {
    maximum_animation_frames: usize,
    maximum_rgba_bytes: usize,
}

const MOBILE_NATIVE_IMAGE_LIMITS: NativeImageLimits = NativeImageLimits {
    maximum_animation_frames: MAX_NATIVE_ANIMATION_FRAMES,
    maximum_rgba_bytes: MAX_NATIVE_IMAGE_RGBA_BYTES,
};

fn validate_native_image_layout(
    poster_pixels: u64,
    animation_frame_count: usize,
    rgba_lengths: impl IntoIterator<Item = usize>,
    limits: NativeImageLimits,
) -> Result<(), NativeRequestError> {
    if poster_pixels > MAX_NATIVE_POSTER_PIXELS {
        return Err(limit_error("poster_pixels"));
    }
    if animation_frame_count > limits.maximum_animation_frames {
        return Err(limit_error("animation_frames"));
    }
    let mut total = 0_usize;
    for length in rgba_lengths {
        total = total
            .checked_add(length)
            .ok_or_else(|| limit_error("rgba_bytes"))?;
        if total > limits.maximum_rgba_bytes {
            return Err(limit_error("rgba_bytes"));
        }
    }
    Ok(())
}

#[cfg(test)]
pub fn validate_native_image_layout_for_test(
    poster_pixels: u64,
    animation_frame_count: usize,
    rgba_lengths: impl IntoIterator<Item = usize>,
) -> Result<(), NativeRequestError> {
    validate_native_image_layout(
        poster_pixels,
        animation_frame_count,
        rgba_lengths,
        MOBILE_NATIVE_IMAGE_LIMITS,
    )
}

impl NativeImage {
    fn from_decoded(decoded: DecodedImage) -> Result<Self, NativeRequestError> {
        validate_native_image_layout(
            u64::from(decoded.poster.width()) * u64::from(decoded.poster.height()),
            decoded.animation.len(),
            std::iter::once(decoded.poster.pixels().len()).chain(
                decoded
                    .animation
                    .iter()
                    .map(|frame| frame.image.pixels().len()),
            ),
            MOBILE_NATIVE_IMAGE_LIMITS,
        )?;
        let poster = Arc::new(NativeFrame::from_rgba(decoded.poster)?);
        let animation = decoded
            .animation
            .into_iter()
            .map(|AnimationFrame { image, duration_ms }| {
                Ok((Arc::new(NativeFrame::from_rgba(image)?), duration_ms))
            })
            .collect::<Result<Vec<_>, NativeRequestError>>()?;
        Ok(Self {
            poster,
            animation,
            loop_count: decoded.loop_count,
        })
    }

    fn from_frame(frame: Arc<NativeFrame>) -> Self {
        Self {
            poster: frame,
            animation: Vec::new(),
            loop_count: None,
        }
    }

    pub fn width(&self) -> i32 {
        self.poster.width
    }

    pub fn height(&self) -> i32 {
        self.poster.height
    }

    pub fn stride(&self) -> i32 {
        self.poster.stride
    }

    pub fn pixels_ptr(&self) -> *mut u8 {
        self.poster.pixels.as_ptr().cast_mut()
    }

    pub fn pixels_len(&self) -> usize {
        self.poster.pixels.len()
    }

    #[cfg(test)]
    pub fn pixels(&self) -> &[u8] {
        &self.poster.pixels
    }

    pub fn frame_count(&self) -> usize {
        self.animation.len().max(1)
    }

    pub fn loop_count(&self) -> Option<u32> {
        self.loop_count
    }

    pub fn frame_duration_ms(&self, index: usize) -> Option<u64> {
        if self.animation.is_empty() {
            (index == 0).then_some(0)
        } else {
            self.animation.get(index).map(|(_, duration)| *duration)
        }
    }

    fn frame(&self, index: usize) -> Option<Arc<NativeFrame>> {
        if self.animation.is_empty() {
            (index == 0).then(|| self.poster.clone())
        } else {
            self.animation.get(index).map(|(frame, _)| frame.clone())
        }
    }
}

pub struct NativeArchive {
    container: VirtualContainer,
    listed_base_directory: Option<PathBuf>,
    encoded_bytes: u64,
}

impl NativeArchive {
    pub fn entry_count(&self) -> usize {
        self.container.entries().len()
    }

    pub fn entry_name(&self, index: usize) -> Option<&str> {
        self.container
            .entries()
            .get(index)
            .map(|entry| entry.name.as_str())
    }

    pub fn entry_size(&self, index: usize) -> Option<u64> {
        self.container
            .entries()
            .get(index)
            .and_then(|entry| entry.uncompressed_size)
    }

    fn materialize(&self, index: usize) -> Result<Vec<u8>, CoreError> {
        let bytes = self.container.materialize_entry(index, |relative| {
            let base = self.listed_base_directory.as_deref().ok_or_else(|| {
                CoreError::invalid_input("listed-file base directory is unavailable")
            })?;
            read_listed_relative(base, relative)
        })?;
        validate_archive_retained_layout(self.encoded_bytes, bytes.len() as u64)?;
        Ok(bytes)
    }
}

fn validate_archive_retained_layout(archive_bytes: u64, entry_bytes: u64) -> Result<(), CoreError> {
    let retained = archive_bytes
        .checked_add(entry_bytes)
        .ok_or_else(|| CoreError::limit("archive retained byte size overflow"))?;
    if retained > MAX_MOBILE_ARCHIVE_RETAINED_BYTES {
        return Err(CoreError::limit("archive retained byte limit exceeded"));
    }
    Ok(())
}

#[cfg(test)]
pub fn validate_archive_retained_layout_for_test(
    archive_bytes: u64,
    entry_bytes: u64,
) -> Result<(), CoreError> {
    validate_archive_retained_layout(archive_bytes, entry_bytes)
}

pub struct NativeBytes {
    bytes: Box<[u8]>,
}

impl NativeBytes {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes: bytes.into_boxed_slice(),
        }
    }

    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.bytes.as_ptr().cast_mut()
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    #[cfg(test)]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }
}

pub struct RgbaEncodeRequest<'a> {
    pub rgba: &'a [u8],
    pub width: i32,
    pub height: i32,
    pub stride: i32,
    pub format: &'a str,
}

pub struct BridgeState {
    sessions: HandleRegistry<Mutex<RequestState>>,
    images: HandleRegistry<NativeImage>,
    archives: HandleRegistry<NativeArchive>,
    byte_buffers: HandleRegistry<NativeBytes>,
    next_source_id: AtomicU64,
}

impl Default for BridgeState {
    fn default() -> Self {
        Self {
            sessions: HandleRegistry::default(),
            images: HandleRegistry::default(),
            archives: HandleRegistry::default(),
            byte_buffers: HandleRegistry::default(),
            next_source_id: AtomicU64::new(1),
        }
    }
}

impl BridgeState {
    pub fn create_session(&self) -> Option<NativeSessionHandle> {
        self.sessions
            .insert(Mutex::new(RequestState::default()))
            .map(NativeSessionHandle)
    }

    pub fn release_session(&self, handle: NativeSessionHandle) -> bool {
        self.sessions.remove(handle.0).is_some()
    }

    pub fn next_request_id(&self, handle: NativeSessionHandle) -> Option<u64> {
        let session = self.sessions.get(handle.0)?;
        let mut state = lock(&session);
        let next = state.next_request_id();
        if next.is_none() {
            state.record_error(0, NativeRequestError::new(NativeErrorCode::Limit, "{}"));
        }
        next
    }

    pub fn begin_request(&self, handle: NativeSessionHandle, request_id: u64) -> bool {
        let Some(session) = self.sessions.get(handle.0) else {
            return false;
        };
        lock(&session).begin(request_id)
    }

    pub fn cancel_request(&self, handle: NativeSessionHandle, request_id: u64) -> bool {
        let Some(session) = self.sessions.get(handle.0) else {
            return false;
        };
        lock(&session).cancel(request_id)
    }

    pub fn is_request_current(&self, handle: NativeSessionHandle, request_id: u64) -> bool {
        let Some(session) = self.sessions.get(handle.0) else {
            return false;
        };
        lock(&session).is_current(request_id)
    }

    pub fn require_current_request(&self, handle: NativeSessionHandle, request_id: u64) -> bool {
        if self.is_request_current(handle, request_id) {
            return true;
        }
        self.record_stale_unless_cancelled(handle, request_id);
        false
    }

    pub fn request_error(
        &self,
        handle: NativeSessionHandle,
        request_id: u64,
    ) -> NativeRequestError {
        let Some(session) = self.sessions.get(handle.0) else {
            return NativeRequestError::invalid_handle();
        };
        lock(&session)
            .error(request_id)
            .unwrap_or_else(|| NativeRequestError::new(NativeErrorCode::None, "{}"))
    }

    pub fn record_invalid_argument(
        &self,
        handle: NativeSessionHandle,
        request_id: u64,
        argument: &'static str,
    ) {
        self.record_error(handle, request_id, invalid_argument_error(argument));
    }

    pub fn decode_path(
        &self,
        handle: NativeSessionHandle,
        request_id: u64,
        path: &Path,
        format_hint: Option<&str>,
    ) -> Option<NativeImageHandle> {
        if !self.is_request_current(handle, request_id) {
            self.record_stale_unless_cancelled(handle, request_id);
            return None;
        }
        let bytes = match read_file_bounded(path, MAX_MOBILE_ENCODED_INPUT_BYTES, "input_bytes") {
            Ok(bytes) => bytes,
            Err(error) => {
                self.record_error(handle, request_id, bounded_read_native_error(error));
                return None;
            }
        };
        self.decode_bytes(handle, request_id, &bytes, format_hint)
    }

    pub fn decode_bytes(
        &self,
        handle: NativeSessionHandle,
        request_id: u64,
        bytes: &[u8],
        format_hint: Option<&str>,
    ) -> Option<NativeImageHandle> {
        if !self.is_request_current(handle, request_id) {
            self.record_stale_unless_cancelled(handle, request_id);
            return None;
        }
        let cancel_probe = || !self.is_request_current(handle, request_id);
        let decoded = match decode_with_limits(
            DecodeRequest { bytes, format_hint },
            MOBILE_DECODE_LIMITS,
            Some(&cancel_probe),
        ) {
            Ok(decoded) => decoded,
            Err(error) => {
                if error.kind() == CoreErrorKind::Cancelled {
                    self.record_stale_unless_cancelled(handle, request_id);
                } else {
                    self.record_error(handle, request_id, core_error(&error));
                }
                return None;
            }
        };
        let image = match NativeImage::from_decoded(decoded) {
            Ok(image) => image,
            Err(error) => {
                self.record_error(handle, request_id, error);
                return None;
            }
        };

        // Decoding can be expensive. Re-check after it completes so a cancelled
        // or superseded request cannot publish a stale image.
        if !self.is_request_current(handle, request_id) {
            self.record_stale_unless_cancelled(handle, request_id);
            return None;
        }
        let handle_value = self.images.insert(image).map(NativeImageHandle);
        if handle_value.is_some() {
            self.clear_error(handle, request_id);
        } else {
            self.record_error(
                handle,
                request_id,
                NativeRequestError::new(NativeErrorCode::Limit, "{}"),
            );
        }
        handle_value
    }

    pub fn encode_rgba(
        &self,
        handle: NativeSessionHandle,
        request_id: u64,
        request: RgbaEncodeRequest<'_>,
    ) -> Option<NativeBytesHandle> {
        if !self.require_current_request(handle, request_id) {
            return None;
        }
        let width = match u32::try_from(request.width) {
            Ok(width) if width > 0 => width,
            _ => {
                self.record_invalid_argument(handle, request_id, "width");
                return None;
            }
        };
        let height = match u32::try_from(request.height) {
            Ok(height) if height > 0 => height,
            _ => {
                self.record_invalid_argument(handle, request_id, "height");
                return None;
            }
        };
        let stride = match usize::try_from(request.stride) {
            Ok(stride) => stride,
            Err(_) => {
                self.record_invalid_argument(handle, request_id, "stride");
                return None;
            }
        };
        let encode_format = match parse_encode_format(request.format) {
            Some(format) => format,
            None => {
                self.record_invalid_argument(handle, request_id, "format");
                return None;
            }
        };

        let pixel_count = u64::from(width) * u64::from(height);
        if pixel_count > MAX_ENCODE_PIXELS {
            self.record_error(handle, request_id, limit_error("pixels"));
            return None;
        }
        let tight_stride = match usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
        {
            Some(stride) => stride,
            None => {
                self.record_error(handle, request_id, limit_error("stride"));
                return None;
            }
        };
        if stride < tight_stride {
            self.record_invalid_argument(handle, request_id, "stride");
            return None;
        }
        if stride > MAX_ENCODE_STRIDE_BYTES {
            self.record_error(handle, request_id, limit_error("stride"));
            return None;
        }
        let height_usize = height as usize;
        let required = match stride.checked_mul(height_usize) {
            Some(required) if required <= MAX_OWNED_BYTES => required,
            _ => {
                self.record_error(handle, request_id, limit_error("input_bytes"));
                return None;
            }
        };
        if request.rgba.len() < required {
            self.record_invalid_argument(handle, request_id, "rgba_capacity");
            return None;
        }
        let tight_len = match tight_stride.checked_mul(height_usize) {
            Some(length) if length <= MAX_OWNED_BYTES => length,
            _ => {
                self.record_error(handle, request_id, limit_error("input_bytes"));
                return None;
            }
        };
        let mut pixels = Vec::new();
        if pixels.try_reserve_exact(tight_len).is_err() {
            self.record_error(handle, request_id, limit_error("input_bytes"));
            return None;
        }
        for row in 0..height_usize {
            let start = row * stride;
            pixels.extend_from_slice(&request.rgba[start..start + tight_stride]);
        }
        let poster = match RgbaImage::new(width, height, pixels) {
            Ok(image) => image,
            Err(error) => {
                self.record_error(handle, request_id, core_error(&error));
                return None;
            }
        };
        let encoded = match encode(EncodeRequest {
            image: &DecodedImage {
                poster,
                animation: Vec::new(),
                loop_count: None,
            },
            format: encode_format,
        }) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.record_error(handle, request_id, core_error(&error));
                return None;
            }
        };
        self.publish_bytes(handle, request_id, encoded)
    }

    pub fn image(&self, handle: NativeImageHandle) -> Option<Arc<NativeImage>> {
        self.images.get(handle.0)
    }

    pub fn release_image(&self, handle: NativeImageHandle) -> bool {
        self.images.remove(handle.0).is_some()
    }

    pub fn image_frame(
        &self,
        handle: NativeImageHandle,
        index: usize,
    ) -> Option<NativeImageHandle> {
        let image = self.image(handle)?;
        let frame = image.frame(index)?;
        self.images
            .insert(NativeImage::from_frame(frame))
            .map(NativeImageHandle)
    }

    pub fn open_archive(
        &self,
        session_handle: NativeSessionHandle,
        request_id: u64,
        path: &Path,
        format: &str,
    ) -> Option<NativeArchiveHandle> {
        if !self.is_request_current(session_handle, request_id) {
            self.record_stale_unless_cancelled(session_handle, request_id);
            return None;
        }
        let source_format = match parse_source_format(format) {
            Some(format) => format,
            None => {
                self.record_error(
                    session_handle,
                    request_id,
                    NativeRequestError::new(
                        NativeErrorCode::InvalidRequest,
                        r#"{"argument":"format"}"#,
                    ),
                );
                return None;
            }
        };
        let bytes = match read_file_bounded(path, MAX_MOBILE_ARCHIVE_INPUT_BYTES, "archive_bytes") {
            Ok(bytes) => bytes,
            Err(error) => {
                self.record_error(session_handle, request_id, bounded_read_native_error(error));
                return None;
            }
        };
        let source_id = match self.allocate_source_id() {
            Some(source_id) => source_id,
            None => {
                self.record_error(
                    session_handle,
                    request_id,
                    NativeRequestError::new(NativeErrorCode::Limit, "{}"),
                );
                return None;
            }
        };
        let encoded_bytes = bytes.len() as u64;
        let container =
            match VirtualContainer::open(source_format, source_id, bytes, MOBILE_ARCHIVE_LIMITS) {
                Ok(container) => container,
                Err(error) => {
                    self.record_error(session_handle, request_id, core_error(&error));
                    return None;
                }
            };
        if !self.is_request_current(session_handle, request_id) {
            self.record_stale_unless_cancelled(session_handle, request_id);
            return None;
        }
        let listed_base_directory = (source_format == VirtualSourceFormat::ListedFile)
            .then(|| path.parent().map(Path::to_path_buf))
            .flatten();
        let archive = NativeArchive {
            container,
            listed_base_directory,
            encoded_bytes,
        };
        let handle = self.archives.insert(archive).map(NativeArchiveHandle);
        if handle.is_some() {
            self.clear_error(session_handle, request_id);
        } else {
            self.record_error(
                session_handle,
                request_id,
                NativeRequestError::new(NativeErrorCode::Limit, "{}"),
            );
        }
        handle
    }

    pub fn archive(&self, handle: NativeArchiveHandle) -> Option<Arc<NativeArchive>> {
        self.archives.get(handle.0)
    }

    pub fn release_archive(&self, handle: NativeArchiveHandle) -> bool {
        self.archives.remove(handle.0).is_some()
    }

    pub fn materialize_archive_entry(
        &self,
        session_handle: NativeSessionHandle,
        request_id: u64,
        archive_handle: NativeArchiveHandle,
        index: usize,
    ) -> Option<NativeBytesHandle> {
        if !self.is_request_current(session_handle, request_id) {
            self.record_stale_unless_cancelled(session_handle, request_id);
            return None;
        }
        let Some(archive) = self.archive(archive_handle) else {
            self.record_error(
                session_handle,
                request_id,
                NativeRequestError::new(NativeErrorCode::InvalidHandle, r#"{"kind":"archive"}"#),
            );
            return None;
        };
        let bytes = match archive.materialize(index) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.record_error(session_handle, request_id, core_error(&error));
                return None;
            }
        };

        self.publish_bytes(session_handle, request_id, bytes)
    }

    pub fn native_bytes(&self, handle: NativeBytesHandle) -> Option<Arc<NativeBytes>> {
        self.byte_buffers.get(handle.0)
    }

    pub fn release_bytes(&self, handle: NativeBytesHandle) -> bool {
        self.byte_buffers.remove(handle.0).is_some()
    }

    fn publish_bytes(
        &self,
        session_handle: NativeSessionHandle,
        request_id: u64,
        bytes: Vec<u8>,
    ) -> Option<NativeBytesHandle> {
        // Encoding or decompressing can be expensive. Re-check before
        // publishing so cancelled or superseded work cannot escape as current.
        if !self.require_current_request(session_handle, request_id) {
            return None;
        }
        if bytes.len() > MAX_OWNED_BYTES {
            self.record_error(session_handle, request_id, limit_error("output_bytes"));
            return None;
        }
        let handle = self
            .byte_buffers
            .insert(NativeBytes::new(bytes))
            .map(NativeBytesHandle);
        if handle.is_some() {
            self.clear_error(session_handle, request_id);
        } else {
            self.record_error(
                session_handle,
                request_id,
                NativeRequestError::new(NativeErrorCode::Limit, "{}"),
            );
        }
        handle
    }

    pub fn decode_archive_entry(
        &self,
        session_handle: NativeSessionHandle,
        request_id: u64,
        archive_handle: NativeArchiveHandle,
        index: usize,
        format_hint: Option<&str>,
    ) -> Option<NativeImageHandle> {
        if !self.is_request_current(session_handle, request_id) {
            self.record_stale_unless_cancelled(session_handle, request_id);
            return None;
        }
        let Some(archive) = self.archive(archive_handle) else {
            self.record_error(
                session_handle,
                request_id,
                NativeRequestError::new(NativeErrorCode::InvalidHandle, r#"{"kind":"archive"}"#),
            );
            return None;
        };
        let bytes = match archive.materialize(index) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.record_error(session_handle, request_id, core_error(&error));
                return None;
            }
        };
        self.decode_bytes(session_handle, request_id, &bytes, format_hint)
    }

    fn allocate_source_id(&self) -> Option<SourceId> {
        self.next_source_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                (current < i64::MAX as u64).then_some(current + 1)
            })
            .ok()
            .map(SourceId)
    }

    fn record_stale_unless_cancelled(&self, handle: NativeSessionHandle, request_id: u64) {
        let Some(session) = self.sessions.get(handle.0) else {
            return;
        };
        let mut state = lock(&session);
        if state
            .error(request_id)
            .is_some_and(|error| error.code() == NativeErrorCode::Cancelled)
        {
            return;
        }
        state.record_error(
            request_id,
            NativeRequestError::new(NativeErrorCode::StaleRequest, "{}"),
        );
    }

    fn record_error(
        &self,
        handle: NativeSessionHandle,
        request_id: u64,
        error: NativeRequestError,
    ) {
        if let Some(session) = self.sessions.get(handle.0) {
            lock(&session).record_error(request_id, error);
        }
    }

    fn clear_error(&self, handle: NativeSessionHandle, request_id: u64) {
        if let Some(session) = self.sessions.get(handle.0) {
            lock(&session).clear_error(request_id);
        }
    }

    #[cfg(test)]
    pub fn image_count(&self) -> usize {
        self.images.len()
    }

    #[cfg(test)]
    pub fn archive_count(&self) -> usize {
        self.archives.len()
    }

    #[cfg(test)]
    pub fn byte_buffer_count(&self) -> usize {
        self.byte_buffers.len()
    }

    #[cfg(test)]
    pub fn insert_decoded_for_test(
        &self,
        decoded: DecodedImage,
    ) -> Result<NativeImageHandle, NativeRequestError> {
        let image = NativeImage::from_decoded(decoded)?;
        self.images
            .insert(image)
            .map(NativeImageHandle)
            .ok_or_else(|| limit_error("handles"))
    }
}

pub fn bridge() -> &'static BridgeState {
    static BRIDGE: OnceLock<BridgeState> = OnceLock::new();
    BRIDGE.get_or_init(BridgeState::default)
}

fn parse_source_format(format: &str) -> Option<VirtualSourceFormat> {
    match format.trim().to_ascii_lowercase().as_str() {
        "zip" => Some(VirtualSourceFormat::Zip),
        "lha" | "lzh" => Some(VirtualSourceFormat::Lha),
        "wmltxt" => Some(VirtualSourceFormat::ListedFile),
        _ => None,
    }
}

fn parse_encode_format(format: &str) -> Option<EncodeFormat> {
    match format.trim().to_ascii_lowercase().as_str() {
        "png" | "image/png" => Some(EncodeFormat::Png),
        "jpeg" | "jpg" | "image/jpeg" => Some(EncodeFormat::Jpeg),
        "webp" | "image/webp" => Some(EncodeFormat::Webp),
        _ => None,
    }
}

fn read_listed_relative(base: &Path, relative: &str) -> Result<Vec<u8>, CoreError> {
    let canonical_base =
        std::fs::canonicalize(base).map_err(|error| CoreError::io(error.to_string()))?;
    let candidate = relative
        .split('/')
        .fold(canonical_base.clone(), |path, component| {
            path.join(component)
        });
    let canonical_candidate =
        std::fs::canonicalize(&candidate).map_err(|error| CoreError::io(error.to_string()))?;
    if !canonical_candidate.starts_with(&canonical_base) {
        return Err(CoreError::archive(
            "listed-file target escapes its base directory",
        ));
    }
    read_file_bounded(
        &canonical_candidate,
        MAX_MOBILE_ARCHIVE_ENTRY_BYTES,
        "entry_bytes",
    )
    .map_err(bounded_read_core_error)
}

fn bounded_read_native_error(error: BoundedReadError) -> NativeRequestError {
    match error {
        BoundedReadError::Io(error) => io_error(&error),
        BoundedReadError::Limit { dimension } => limit_error(dimension),
    }
}

fn bounded_read_core_error(error: BoundedReadError) -> CoreError {
    match error {
        BoundedReadError::Io(error) => CoreError::io(error.to_string()),
        BoundedReadError::Limit { dimension } => {
            CoreError::limit(format!("{dimension} exceeds byte limit"))
        }
    }
}

pub fn limit_error(dimension: &str) -> NativeRequestError {
    NativeRequestError::new(
        NativeErrorCode::Limit,
        format!(r#"{{"dimension":"{dimension}"}}"#),
    )
}

fn invalid_argument_error(argument: &str) -> NativeRequestError {
    NativeRequestError::new(
        NativeErrorCode::InvalidRequest,
        format!(r#"{{"argument":"{argument}"}}"#),
    )
}

pub fn core_error(error: &CoreError) -> NativeRequestError {
    let code = match error.kind() {
        CoreErrorKind::Io => NativeErrorCode::Io,
        CoreErrorKind::Limit => NativeErrorCode::Limit,
        CoreErrorKind::Cancelled => NativeErrorCode::Cancelled,
        CoreErrorKind::Encode => NativeErrorCode::Encode,
        CoreErrorKind::Decode | CoreErrorKind::Archive | CoreErrorKind::InvalidInput => {
            NativeErrorCode::Decode
        }
    };
    NativeRequestError::new(code, "{}")
}

fn io_error(error: &std::io::Error) -> NativeRequestError {
    let kind = match error.kind() {
        std::io::ErrorKind::NotFound => "not_found",
        std::io::ErrorKind::PermissionDenied => "permission_denied",
        std::io::ErrorKind::InvalidData => "invalid_data",
        std::io::ErrorKind::UnexpectedEof => "unexpected_eof",
        _ => "other",
    };
    NativeRequestError::new(NativeErrorCode::Io, format!(r#"{{"kind":"{kind}"}}"#))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
