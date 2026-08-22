//! Callback-based image I/O primitives and the default RGBA image buffer.
//!
//! Decoders push pixels into a [`DrawCallback`] implementation. Encoders pull
//! pixels from a [`PickCallback`] implementation. [`ImageBuffer`] provides the
//! default in-memory implementation for both directions and stores animation
//! frames as RGBA sub-rectangles.
type Error = Box<dyn std::error::Error>;
use crate::color::RGBA;
use crate::error::ImgError;
use crate::error::ImgErrorKind;
use crate::metadata::DataMap;
use crate::util::ImageFormat;
use crate::util::format_check;
use crate::warning::ImgWarnings;
use bin_rs::reader::*;
use std::collections::HashMap;
#[cfg(not(target_family = "wasm"))]
use std::io::BufRead;
#[cfg(not(target_family = "wasm"))]
use std::io::BufReader;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
#[cfg(not(target_family = "wasm"))]
use std::path::Path;

/* Dynamic Select Callback System */

/// Legacy multi-image control values kept for compatibility.
#[derive(Debug)]
pub enum DrawNextOptions {
    Continue,
    NextImage,
    ClearNext,
    WaitTime(usize),
    None,
}

pub type Response = Result<Option<CallbackResponse>, Error>;

/// Receives decoded image data from format decoders.
pub trait DrawCallback: Sync + Send {
    /// Returns allocation limits for decoders that can enforce them before
    /// expanding compressed data. Existing callbacks remain unlimited.
    fn decode_limits(&self) -> Option<DecodeLimits> {
        None
    }

    /// Gives long-running decoders a cooperative cancellation point.
    fn check_decode_control(&self) -> Result<(), Error> {
        Ok(())
    }

    /// Creates a typed limit error. Bounded adapters override this to attach
    /// the allocation counters accepted before the rejection.
    fn decode_limit_error(&self, kind: DecodeLimitKind, attempted: u128, limit: u128) -> Error {
        Box::new(DecodeLimitError::new(kind, attempted, limit, 0, 0))
    }

    /// Announces an animation frame before its compressed payload is decoded.
    /// The default keeps existing callback implementations source compatible.
    fn preflight_frame(&mut self, _width: usize, _height: usize) -> Result<(), Error> {
        self.check_decode_control()
    }

    /// Initializes the target for a new image or animation canvas.
    fn init(
        &mut self,
        width: usize,
        height: usize,
        option: Option<InitOptions>,
    ) -> Result<Option<CallbackResponse>, Error>;
    /// Writes RGBA pixels into a rectangle.
    fn draw(
        &mut self,
        start_x: usize,
        start_y: usize,
        width: usize,
        height: usize,
        data: &[u8],
        option: Option<DrawOptions>,
    ) -> Result<Option<CallbackResponse>, Error>;
    fn terminate(
        &mut self,
        _term: Option<TerminateOptions>,
    ) -> Result<Option<CallbackResponse>, Error>;
    /// Signals a new animation frame or image boundary.
    fn next(&mut self, _next: Option<NextOptions>) -> Result<Option<CallbackResponse>, Error>;
    /// Emits verbose decoder output.
    fn verbose(
        &mut self,
        _verbose: &str,
        _: Option<VerboseOptions>,
    ) -> Result<Option<CallbackResponse>, Error>;
    /// Stores decoded metadata.
    fn set_metadata(
        &mut self,
        key: &str,
        value: DataMap,
    ) -> Result<Option<CallbackResponse>, Error>;
}

/// Allocation limits used by the opt-in bounded decode API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodeLimits {
    /// Maximum pixels in the canvas or any individual frame rectangle.
    pub maximum_frame_pixels: u64,
    /// Maximum number of decoded animation frames.
    pub maximum_frames: usize,
    /// Maximum aggregate bytes for the poster and composited RGBA frames.
    pub maximum_rgba_bytes: usize,
}

impl DecodeLimits {
    /// Unlimited settings used by the legacy decode entry points.
    pub const UNLIMITED: Self = Self {
        maximum_frame_pixels: u64::MAX,
        maximum_frames: usize::MAX,
        maximum_rgba_bytes: usize::MAX,
    };
}

/// Category of a bounded decode rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeLimitKind {
    Pixels,
    Frames,
    RgbaBytes,
    DecodedBytes,
}

/// Error returned before a bounded decode would exceed its budget.
#[derive(Debug)]
pub struct DecodeLimitError {
    kind: DecodeLimitKind,
    attempted: u128,
    limit: u128,
    accepted_frames: usize,
    accepted_rgba_bytes: usize,
}

impl DecodeLimitError {
    pub(crate) fn new(
        kind: DecodeLimitKind,
        attempted: u128,
        limit: u128,
        accepted_frames: usize,
        accepted_rgba_bytes: usize,
    ) -> Self {
        Self {
            kind,
            attempted,
            limit,
            accepted_frames,
            accepted_rgba_bytes,
        }
    }

    pub const fn kind(&self) -> DecodeLimitKind {
        self.kind
    }

    pub const fn attempted(&self) -> u128 {
        self.attempted
    }

    pub const fn limit(&self) -> u128 {
        self.limit
    }

    pub const fn accepted_frames(&self) -> usize {
        self.accepted_frames
    }

    pub const fn accepted_rgba_bytes(&self) -> usize {
        self.accepted_rgba_bytes
    }
}

impl std::fmt::Display for DecodeLimitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "decode {:?} limit exceeded: attempted {}, limit {}",
            self.kind, self.attempted, self.limit
        )
    }
}

impl std::error::Error for DecodeLimitError {}

/// Cooperative cancellation returned by the bounded decode API.
#[derive(Debug)]
pub struct DecodeCancelledError;

impl std::fmt::Display for DecodeCancelledError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("image decode cancelled")
    }
}

impl std::error::Error for DecodeCancelledError {}

/// Supplies image data to encoders.
pub trait PickCallback: Sync + Send {
    /// Returns the base image profile used for encoding.
    fn encode_start(
        &mut self,
        option: Option<EncoderOptions>,
    ) -> Result<Option<ImageProfiles>, Error>;
    /// Reads an RGBA rectangle from the image source.
    fn encode_pick(
        &mut self,
        start_x: usize,
        start_y: usize,
        width: usize,
        height: usize,
        option: Option<PickOptions>,
    ) -> Result<Option<Vec<u8>>, Error>;
    /// Finalizes encoding.
    fn encode_end(&mut self, _: Option<EndOptions>) -> Result<(), Error>;
    /// Returns source metadata, if available.
    fn metadata(&mut self) -> Result<Option<HashMap<String, DataMap>>, Error>;
}

/// Encoder-specific startup options.
#[derive(Debug)]
pub struct EncoderOptions {}

/// Encoder read options for partial pixel fetches.
#[derive(Debug)]
pub struct PickOptions {}

/// Encoder shutdown options.
#[derive(Debug)]
pub struct EndOptions {}

#[allow(unused)]
/// Canvas initialization options passed to [`DrawCallback::init`].
#[derive(Debug)]
pub struct InitOptions {
    /// Animation loop count when known.
    pub loop_count: u32,
    /// Optional RGBA background color for the canvas.
    pub background: Option<RGBA>,
    /// Whether the decoded source is animated.
    pub animation: bool,
}

impl InitOptions {
    /// Creates default initialization options.
    pub fn new() -> Option<Self> {
        Some(Self {
            loop_count: 1,
            background: None,
            animation: false,
        })
    }
}

/// Decoder-specific draw options.
#[derive(Debug)]
pub struct DrawOptions {}

/// Decoder termination options.
#[derive(Debug)]
pub struct TerminateOptions {}

/// Verbose logging options.
#[derive(Debug)]
pub struct VerboseOptions {}

/// Frame transition commands used by animated decoders.
#[derive(Debug)]
pub enum NextOption {
    /// Continue the current animation.
    Continue,
    /// Start a new frame or image.
    Next,
    /// Dispose the previous frame.
    Dispose,
    /// Clear the current frame and abort.
    ClearAbort,
    /// End the current animation stream.
    Terminate,
}

/// Disposal mode for an animation frame.
#[derive(Debug)]
pub enum NextDispose {
    /// Leave the previous frame as-is.
    None,
    /// Compatibility value retained from older callers.
    Override,
    /// Clear the frame area to the background color.
    Background,
    /// Restore the previous composited canvas.
    Previous,
}

/// Blend mode for an animation frame.
///
/// `Source` means "alpha-blend onto the existing canvas". For APNG this maps to
/// `OVER`, and for animated WebP it matches alpha blending. `Override` replaces
/// the covered destination pixels.
#[derive(Debug)]
pub enum NextBlend {
    Source,
    Override,
}

#[allow(unused)]
/// A rectangle on the destination canvas.
#[derive(Debug)]
pub struct ImageRect {
    pub start_x: i32,
    pub start_y: i32,
    pub width: usize,
    pub height: usize,
}

#[allow(unused)]
/// Per-frame animation control values.
#[derive(Debug)]
pub struct NextOptions {
    /// Transition command for this frame.
    pub flag: NextOption,
    /// Delay before advancing to the next frame, in milliseconds.
    pub await_time: u64,
    /// Frame rectangle on the animation canvas.
    pub image_rect: Option<ImageRect>,
    /// Frame disposal method.
    pub dispose_option: Option<NextDispose>,
    /// Frame blending method.
    pub blend: Option<NextBlend>,
}

impl NextOptions {
    /// Creates default frame options.
    pub fn new() -> Self {
        NextOptions {
            flag: NextOption::Continue,
            await_time: 0,
            image_rect: None,
            dispose_option: None,
            blend: None,
        }
    }

    /// Creates a frame option with only a delay.
    pub fn wait(ms_time: u64) -> Self {
        NextOptions {
            flag: NextOption::Continue,
            await_time: ms_time,
            image_rect: None,
            dispose_option: None,
            blend: None,
        }
    }
}

/// Decoder response command.
#[derive(std::cmp::PartialEq, Debug)]
pub enum ResponseCommand {
    Abort,
    Continue,
}

/// Response returned by callbacks.
#[derive(Debug)]
pub struct CallbackResponse {
    pub response: ResponseCommand,
}

impl CallbackResponse {
    /// Builds an abort response.
    pub fn abort() -> Self {
        Self {
            response: ResponseCommand::Abort,
        }
    }

    /// Builds a continue response.
    pub fn cont() -> Self {
        Self {
            response: ResponseCommand::Continue,
        }
    }
}

/// Static image properties returned from [`PickCallback::encode_start`].
#[derive(Debug)]
pub struct ImageProfiles {
    /// Canvas width in pixels.
    pub width: usize,
    /// Canvas height in pixels.
    pub height: usize,
    /// Optional background color for formats that support it.
    pub background: Option<RGBA>,
    /// Source metadata.
    ///
    /// Built-in encoders may also consume reserved keys inserted by built-in
    /// `PickCallback` implementations for animation transport.
    pub metadata: Option<HashMap<String, DataMap>>,
}

pub(crate) const ENCODE_ANIMATION_FRAMES_KEY: &str = "wml2.animation.frames";
pub(crate) const ENCODE_ANIMATION_LOOP_COUNT_KEY: &str = "wml2.animation.loop_count";

pub(crate) fn encode_animation_frame_key(index: usize, field: &str) -> String {
    format!("wml2.animation.frame.{index}.{field}")
}

fn encode_animation_dispose(dispose: &Option<NextDispose>) -> u64 {
    match dispose {
        Some(NextDispose::Background) => 1,
        Some(NextDispose::Previous) => 2,
        _ => 0,
    }
}

fn encode_animation_blend(blend: &Option<NextBlend>) -> u64 {
    match blend {
        Some(NextBlend::Source) => 1,
        _ => 0,
    }
}

fn append_animation_metadata(
    metadata: &mut HashMap<String, DataMap>,
    animation: &[AnimationLayer],
    loop_count: u32,
) {
    metadata.insert(
        ENCODE_ANIMATION_FRAMES_KEY.to_string(),
        DataMap::UInt(animation.len() as u64),
    );
    metadata.insert(
        ENCODE_ANIMATION_LOOP_COUNT_KEY.to_string(),
        DataMap::UInt(loop_count as u64),
    );

    for (index, layer) in animation.iter().enumerate() {
        metadata.insert(
            encode_animation_frame_key(index, "width"),
            DataMap::UInt(layer.width as u64),
        );
        metadata.insert(
            encode_animation_frame_key(index, "height"),
            DataMap::UInt(layer.height as u64),
        );
        metadata.insert(
            encode_animation_frame_key(index, "start_x"),
            DataMap::SInt(layer.start_x as i64),
        );
        metadata.insert(
            encode_animation_frame_key(index, "start_y"),
            DataMap::SInt(layer.start_y as i64),
        );
        metadata.insert(
            encode_animation_frame_key(index, "delay_ms"),
            DataMap::UInt(layer.control.await_time),
        );
        metadata.insert(
            encode_animation_frame_key(index, "dispose"),
            DataMap::UInt(encode_animation_dispose(&layer.control.dispose_option)),
        );
        metadata.insert(
            encode_animation_frame_key(index, "blend"),
            DataMap::UInt(encode_animation_blend(&layer.control.blend)),
        );
        metadata.insert(
            encode_animation_frame_key(index, "buffer"),
            DataMap::Raw(layer.buffer.clone()),
        );
    }
}

/// One decoded animation frame stored as an RGBA sub-rectangle.
pub struct AnimationLayer {
    /// Frame width in pixels.
    pub width: usize,
    /// Frame height in pixels.
    pub height: usize,
    /// Frame x offset on the canvas.
    pub start_x: i32,
    /// Frame y offset on the canvas.
    pub start_y: i32,
    /// RGBA frame pixels in row-major order.
    pub buffer: Vec<u8>,
    /// Frame timing and composition settings.
    pub control: NextOptions,
}

#[allow(unused)]
/// Default in-memory RGBA image store used by decoders and encoders.
pub struct ImageBuffer {
    /// Canvas width in pixels.
    pub width: usize,
    /// Canvas height in pixels.
    pub height: usize,
    /// Optional background color.
    pub background_color: Option<RGBA>,
    /// Base canvas RGBA pixels.
    pub buffer: Option<Vec<u8>>,
    /// Animation frames, if present.
    pub animation: Option<Vec<AnimationLayer>>,
    /// Current animation frame index while decoding.
    pub current: Option<usize>,
    /// Animation loop count, if known.
    pub loop_count: Option<u32>,
    /// Delay of the first frame, if known.
    pub first_wait_time: Option<u64>,
    fnverbose: fn(&str) -> Result<Option<CallbackResponse>, Error>,
    /// Arbitrary metadata collected during decode.
    pub metadata: Option<HashMap<String, DataMap>>,
}

fn default_verbose(_: &str) -> Result<Option<CallbackResponse>, Error> {
    Ok(None)
}

fn checked_rgba_len(width: usize, height: usize, context: &str) -> Result<usize, Error> {
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("{context} buffer size overflow"),
            )) as Error
        })
}

pub(crate) fn allocate_rgba(
    width: usize,
    height: usize,
    background: Option<&RGBA>,
    context: &str,
) -> Result<Vec<u8>, Error> {
    let length = checked_rgba_len(width, height, context)?;
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(length).map_err(|error| {
        Box::new(ImgError::new_const(
            ImgErrorKind::DecodeError,
            format!("{context} allocation failed: {error}"),
        )) as Error
    })?;
    buffer.resize(length, 0);
    if let Some(background) = background {
        for pixel in buffer.chunks_exact_mut(4) {
            pixel.copy_from_slice(&[
                background.red,
                background.green,
                background.blue,
                background.alpha,
            ]);
        }
    }
    Ok(buffer)
}

impl ImageBuffer {
    /// Creates an empty image buffer.
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            background_color: None,
            buffer: None,
            animation: None,
            current: None,
            loop_count: None,
            first_wait_time: None,
            fnverbose: default_verbose,
            metadata: None,
        }
    }

    /// Creates an image buffer from an RGBA pixel buffer.
    pub fn from_buffer(width: usize, height: usize, buf: Vec<u8>) -> Self {
        Self {
            width,
            height,
            background_color: None,
            buffer: Some(buf),
            animation: None,
            current: None,
            loop_count: None,
            first_wait_time: None,
            fnverbose: default_verbose,
            metadata: None,
        }
    }

    /// Enables or disables animation storage.
    pub fn set_animation(&mut self, flag: bool) {
        if flag {
            self.animation = Some(Vec::new())
        } else {
            self.animation = None
        }
    }

    /// Installs a verbose logging callback used by decoders.
    pub fn set_verbose(&mut self, verbose: fn(&str) -> Result<Option<CallbackResponse>, Error>) {
        self.fnverbose = verbose;
    }
}

impl DrawCallback for ImageBuffer {
    /// Initializes the in-memory canvas.
    fn init(
        &mut self,
        width: usize,
        height: usize,
        option: Option<InitOptions>,
    ) -> Result<Option<CallbackResponse>, Error> {
        self.width = width;
        self.height = height;
        if let Some(option) = option {
            self.background_color = option.background;
            if option.animation {
                self.set_animation(true);
            }
            self.loop_count = Some(option.loop_count);
        }
        self.buffer = Some(allocate_rgba(
            width,
            height,
            self.background_color.as_ref(),
            "image",
        )?);

        Ok(None)
    }

    /// Draws part of the current image or animation frame.
    fn draw(
        &mut self,
        start_x: usize,
        start_y: usize,
        width: usize,
        height: usize,
        data: &[u8],
        _: Option<DrawOptions>,
    ) -> Result<Option<CallbackResponse>, Error> {
        if self.buffer.is_none() {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::NotInitializedImageBuffer,
                "in draw".to_string(),
            )));
        }
        let buffer;
        let (w, h, raws);
        if self.current.is_none() {
            if start_x >= self.width || start_y >= self.height {
                return Ok(None);
            }
            let requested_end_x = start_x.checked_add(width).ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::OutboundIndex,
                    "draw rectangle x overflow".to_string(),
                )) as Error
            })?;
            let requested_end_y = start_y.checked_add(height).ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::OutboundIndex,
                    "draw rectangle y overflow".to_string(),
                )) as Error
            })?;
            w = self.width.min(requested_end_x) - start_x;
            h = self.height.min(requested_end_y) - start_y;
            raws = self.width;
            buffer = self.buffer.as_deref_mut().ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::NotInitializedImageBuffer,
                    "buffer is not initialized".to_string(),
                )) as Error
            })?;
        } else if let Some(animation) = &mut self.animation {
            let current = self.current.ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::IllegalData,
                    "animation frame is not selected".to_string(),
                )) as Error
            })?;
            if start_x >= animation[current].width || start_y >= animation[current].height {
                return Ok(None);
            }
            let requested_end_x = start_x.checked_add(width).ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::OutboundIndex,
                    "draw rectangle x overflow".to_string(),
                )) as Error
            })?;
            let requested_end_y = start_y.checked_add(height).ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::OutboundIndex,
                    "draw rectangle y overflow".to_string(),
                )) as Error
            })?;
            w = animation[current].width.min(requested_end_x) - start_x;
            h = animation[current].height.min(requested_end_y) - start_y;
            raws = animation[current].width;
            buffer = &mut animation[current].buffer;
        } else {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::NotInitializedImageBuffer,
                "in animation".to_string(),
            )));
        }

        for y in 0..h {
            let scanline_src = y * width * 4;
            let scanline_dest = (start_y + y) * raws * 4;
            for x in 0..w {
                let offset_src = scanline_src + x * 4;
                let offset_dest = scanline_dest + (x + start_x) * 4;
                if offset_src + 3 >= data.len() {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::OutboundIndex,
                        "decoder buffer in draw".to_string(),
                    )));
                }
                if offset_dest + 3 >= buffer.len() {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::OutboundIndex,
                        "image buffer in draw".to_string(),
                    )));
                }
                buffer[offset_dest] = data[offset_src];
                buffer[offset_dest + 1] = data[offset_src + 1];
                buffer[offset_dest + 2] = data[offset_src + 2];
                buffer[offset_dest + 3] = data[offset_src + 3];
            }
        }
        Ok(None)
    }

    /// Finalizes decoding.
    fn terminate(
        &mut self,
        _: Option<TerminateOptions>,
    ) -> Result<Option<CallbackResponse>, Error> {
        Ok(None)
    }

    /// Starts a new animation frame in the buffer.
    fn next(&mut self, opt: Option<NextOptions>) -> Result<Option<CallbackResponse>, Error> {
        if self.animation.is_some() {
            if let Some(opt) = opt {
                if self.current.is_none() {
                    self.current = Some(0);
                    self.first_wait_time = Some(opt.await_time);
                } else {
                    self.current = Some(
                        self.current.ok_or_else(|| {
                            Box::new(ImgError::new_const(
                                ImgErrorKind::IllegalData,
                                "animation frame is not selected".to_string(),
                            )) as Error
                        })? + 1,
                    );
                }
                let (width, height, start_x, start_y);
                if let Some(ref rect) = opt.image_rect {
                    width = rect.width;
                    height = rect.height;
                    start_x = rect.start_x;
                    start_y = rect.start_y;
                } else {
                    width = self.width;
                    height = self.height;
                    start_x = 0;
                    start_y = 0;
                }
                let buffer = allocate_rgba(width, height, None, "animation frame")?;
                let layer = AnimationLayer {
                    width,
                    height,
                    start_x,
                    start_y,
                    buffer,
                    control: opt,
                };

                if let Some(animation) = self.animation.as_mut() {
                    animation.try_reserve(1).map_err(|error| {
                        Box::new(ImgError::new_const(
                            ImgErrorKind::DecodeError,
                            format!("animation frame list allocation failed: {error}"),
                        )) as Error
                    })?;
                    animation.push(layer);
                } else {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::NotInitializedImageBuffer,
                        "animation buffer is not initialized".to_string(),
                    )));
                }

                return Ok(Some(CallbackResponse::cont()));
            }
        }
        Ok(Some(CallbackResponse::abort()))
    }

    /// Passes through verbose decoder output.
    fn verbose(
        &mut self,
        str: &str,
        _: Option<VerboseOptions>,
    ) -> Result<Option<CallbackResponse>, Error> {
        (self.fnverbose)(str)
    }

    /// Stores decoded metadata on the buffer.
    fn set_metadata(
        &mut self,
        key: &str,
        value: DataMap,
    ) -> Result<Option<CallbackResponse>, Error> {
        let hashmap = if let Some(ref mut hashmap) = self.metadata {
            hashmap
        } else {
            self.metadata = Some(HashMap::new());
            self.metadata.as_mut().ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::IllegalData,
                    "metadata store is not initialized".to_string(),
                )) as Error
            })?
        };
        hashmap.insert(key.to_string(), value);

        Ok(None)
    }
}

/// Cooperative cancellation callback used by bounded decoding.
pub type DecodeCancelProbe<'a> = &'a (dyn Fn() -> bool + Send + Sync);

struct LimitedImageBuffer<'a> {
    image: ImageBuffer,
    limits: DecodeLimits,
    cancel_probe: Option<DecodeCancelProbe<'a>>,
    canvas_rgba_bytes: usize,
    accepted_frames: usize,
    accepted_rgba_bytes: usize,
    pending_frame: Option<(usize, usize)>,
}

impl<'a> LimitedImageBuffer<'a> {
    fn new(limits: DecodeLimits, cancel_probe: Option<DecodeCancelProbe<'a>>) -> Self {
        Self {
            image: ImageBuffer::new(),
            limits,
            cancel_probe,
            canvas_rgba_bytes: 0,
            accepted_frames: 0,
            accepted_rgba_bytes: 0,
            pending_frame: None,
        }
    }

    fn limit_error(&self, kind: DecodeLimitKind, attempted: u128, limit: u128) -> Error {
        Box::new(DecodeLimitError::new(
            kind,
            attempted,
            limit,
            self.accepted_frames,
            self.accepted_rgba_bytes,
        ))
    }

    fn check_cancelled(&self) -> Result<(), Error> {
        if self.cancel_probe.is_some_and(|probe| probe()) {
            return Err(Box::new(DecodeCancelledError));
        }
        Ok(())
    }

    fn checked_frame_rgba(&self, width: usize, height: usize) -> Result<usize, Error> {
        let pixels = (width as u128).checked_mul(height as u128).ok_or_else(|| {
            self.limit_error(
                DecodeLimitKind::Pixels,
                u128::MAX,
                self.limits.maximum_frame_pixels as u128,
            )
        })?;
        if pixels > self.limits.maximum_frame_pixels as u128 {
            return Err(self.limit_error(
                DecodeLimitKind::Pixels,
                pixels,
                self.limits.maximum_frame_pixels as u128,
            ));
        }
        let rgba = pixels.checked_mul(4).ok_or_else(|| {
            self.limit_error(
                DecodeLimitKind::RgbaBytes,
                u128::MAX,
                self.limits.maximum_rgba_bytes as u128,
            )
        })?;
        usize::try_from(rgba).map_err(|_| {
            self.limit_error(
                DecodeLimitKind::RgbaBytes,
                rgba,
                self.limits.maximum_rgba_bytes as u128,
            )
        })
    }

    fn reserve_frame(&mut self, width: usize, height: usize) -> Result<(), Error> {
        self.check_cancelled()?;
        self.checked_frame_rgba(width, height)?;
        let attempted_frames = self.accepted_frames.checked_add(1).ok_or_else(|| {
            self.limit_error(
                DecodeLimitKind::Frames,
                u128::MAX,
                self.limits.maximum_frames as u128,
            )
        })?;
        if attempted_frames > self.limits.maximum_frames {
            return Err(self.limit_error(
                DecodeLimitKind::Frames,
                attempted_frames as u128,
                self.limits.maximum_frames as u128,
            ));
        }
        let attempted_rgba = self
            .accepted_rgba_bytes
            .checked_add(self.canvas_rgba_bytes)
            .ok_or_else(|| {
                self.limit_error(
                    DecodeLimitKind::RgbaBytes,
                    u128::MAX,
                    self.limits.maximum_rgba_bytes as u128,
                )
            })?;
        if attempted_rgba > self.limits.maximum_rgba_bytes {
            return Err(self.limit_error(
                DecodeLimitKind::RgbaBytes,
                attempted_rgba as u128,
                self.limits.maximum_rgba_bytes as u128,
            ));
        }
        self.accepted_frames = attempted_frames;
        self.accepted_rgba_bytes = attempted_rgba;
        self.pending_frame = Some((width, height));
        Ok(())
    }
}

impl DrawCallback for LimitedImageBuffer<'_> {
    fn decode_limits(&self) -> Option<DecodeLimits> {
        Some(self.limits)
    }

    fn check_decode_control(&self) -> Result<(), Error> {
        self.check_cancelled()
    }

    fn decode_limit_error(&self, kind: DecodeLimitKind, attempted: u128, limit: u128) -> Error {
        self.limit_error(kind, attempted, limit)
    }

    fn preflight_frame(&mut self, width: usize, height: usize) -> Result<(), Error> {
        if self.pending_frame.is_some() {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::IllegalData,
                "decoder announced a second frame before advancing".to_string(),
            )));
        }
        self.reserve_frame(width, height)
    }

    fn init(
        &mut self,
        width: usize,
        height: usize,
        option: Option<InitOptions>,
    ) -> Result<Option<CallbackResponse>, Error> {
        self.check_cancelled()?;
        let canvas_rgba = self.checked_frame_rgba(width, height)?;
        if canvas_rgba > self.limits.maximum_rgba_bytes {
            return Err(self.limit_error(
                DecodeLimitKind::RgbaBytes,
                canvas_rgba as u128,
                self.limits.maximum_rgba_bytes as u128,
            ));
        }
        self.canvas_rgba_bytes = canvas_rgba;
        self.accepted_frames = 0;
        self.accepted_rgba_bytes = canvas_rgba;
        self.pending_frame = None;
        let response = self.image.init(width, height, option)?;
        self.check_cancelled()?;
        Ok(response)
    }

    fn draw(
        &mut self,
        start_x: usize,
        start_y: usize,
        width: usize,
        height: usize,
        data: &[u8],
        option: Option<DrawOptions>,
    ) -> Result<Option<CallbackResponse>, Error> {
        self.check_cancelled()?;
        self.checked_frame_rgba(width, height)?;
        let response = self
            .image
            .draw(start_x, start_y, width, height, data, option)?;
        self.check_cancelled()?;
        Ok(response)
    }

    fn terminate(
        &mut self,
        term: Option<TerminateOptions>,
    ) -> Result<Option<CallbackResponse>, Error> {
        self.check_cancelled()?;
        self.image.terminate(term)
    }

    fn next(&mut self, next: Option<NextOptions>) -> Result<Option<CallbackResponse>, Error> {
        self.check_cancelled()?;
        let dimensions = next
            .as_ref()
            .and_then(|next| next.image_rect.as_ref())
            .map(|rect| (rect.width, rect.height))
            .unwrap_or((self.image.width, self.image.height));
        match self.pending_frame.take() {
            Some(preflighted) if preflighted == dimensions => {}
            Some(_) => {
                return Err(Box::new(ImgError::new_const(
                    ImgErrorKind::IllegalData,
                    "animation frame dimensions changed after preflight".to_string(),
                )));
            }
            None => {
                self.reserve_frame(dimensions.0, dimensions.1)?;
                self.pending_frame = None;
            }
        }
        let response = self.image.next(next)?;
        self.check_cancelled()?;
        Ok(response)
    }

    fn verbose(
        &mut self,
        verbose: &str,
        option: Option<VerboseOptions>,
    ) -> Result<Option<CallbackResponse>, Error> {
        self.check_cancelled()?;
        self.image.verbose(verbose, option)
    }

    fn set_metadata(
        &mut self,
        key: &str,
        value: DataMap,
    ) -> Result<Option<CallbackResponse>, Error> {
        self.check_cancelled()?;
        self.image.set_metadata(key, value)
    }
}

pub(crate) fn check_declared_animation_budget(
    drawer: &dyn DrawCallback,
    canvas_width: usize,
    canvas_height: usize,
    frame_rectangles: impl IntoIterator<Item = (usize, usize)>,
) -> Result<(), Error> {
    drawer.check_decode_control()?;
    let Some(limits) = drawer.decode_limits() else {
        return Ok(());
    };
    let canvas_pixels = (canvas_width as u128)
        .checked_mul(canvas_height as u128)
        .ok_or_else(|| {
            drawer.decode_limit_error(
                DecodeLimitKind::Pixels,
                u128::MAX,
                limits.maximum_frame_pixels as u128,
            )
        })?;
    if canvas_pixels > limits.maximum_frame_pixels as u128 {
        return Err(drawer.decode_limit_error(
            DecodeLimitKind::Pixels,
            canvas_pixels,
            limits.maximum_frame_pixels as u128,
        ));
    }
    let canvas_rgba = canvas_pixels.checked_mul(4).ok_or_else(|| {
        drawer.decode_limit_error(
            DecodeLimitKind::RgbaBytes,
            u128::MAX,
            limits.maximum_rgba_bytes as u128,
        )
    })?;
    let mut frame_count = 0_usize;
    for (width, height) in frame_rectangles {
        let pixels = (width as u128).checked_mul(height as u128).ok_or_else(|| {
            drawer.decode_limit_error(
                DecodeLimitKind::Pixels,
                u128::MAX,
                limits.maximum_frame_pixels as u128,
            )
        })?;
        if pixels > limits.maximum_frame_pixels as u128 {
            return Err(drawer.decode_limit_error(
                DecodeLimitKind::Pixels,
                pixels,
                limits.maximum_frame_pixels as u128,
            ));
        }
        frame_count = frame_count.checked_add(1).ok_or_else(|| {
            drawer.decode_limit_error(
                DecodeLimitKind::Frames,
                u128::MAX,
                limits.maximum_frames as u128,
            )
        })?;
        if frame_count > limits.maximum_frames {
            return Err(drawer.decode_limit_error(
                DecodeLimitKind::Frames,
                frame_count as u128,
                limits.maximum_frames as u128,
            ));
        }
    }
    let aggregate = canvas_rgba
        .checked_mul(frame_count as u128 + 1)
        .ok_or_else(|| {
            drawer.decode_limit_error(
                DecodeLimitKind::RgbaBytes,
                u128::MAX,
                limits.maximum_rgba_bytes as u128,
            )
        })?;
    if aggregate > limits.maximum_rgba_bytes as u128 {
        return Err(drawer.decode_limit_error(
            DecodeLimitKind::RgbaBytes,
            aggregate,
            limits.maximum_rgba_bytes as u128,
        ));
    }
    Ok(())
}

pub(crate) fn check_decoded_output_bytes(
    drawer: &dyn DrawCallback,
    output_bytes: usize,
) -> Result<(), Error> {
    drawer.check_decode_control()?;
    if let Some(limits) = drawer.decode_limits() {
        if output_bytes > limits.maximum_rgba_bytes {
            return Err(drawer.decode_limit_error(
                DecodeLimitKind::DecodedBytes,
                output_bytes as u128,
                limits.maximum_rgba_bytes as u128,
            ));
        }
    }
    Ok(())
}

pub(crate) fn decoded_metadata_limit(drawer: &dyn DrawCallback) -> Option<usize> {
    const MAX_BOUNDED_METADATA_BYTES: usize = 16 * 1024 * 1024;
    drawer
        .decode_limits()
        .map(|limits| limits.maximum_rgba_bytes.min(MAX_BOUNDED_METADATA_BYTES))
}

impl PickCallback for ImageBuffer {
    /// Exposes the image profile to encoders.
    fn encode_start(&mut self, _: Option<EncoderOptions>) -> Result<Option<ImageProfiles>, Error> {
        let mut metadata = self.metadata.clone();
        if let Some(animation) = &self.animation {
            if !animation.is_empty() {
                let hashmap = if let Some(ref mut metadata) = metadata {
                    metadata
                } else {
                    metadata = Some(HashMap::new());
                    metadata.as_mut().ok_or_else(|| {
                        Box::new(ImgError::new_const(
                            ImgErrorKind::IllegalData,
                            "metadata store is not initialized".to_string(),
                        )) as Error
                    })?
                };
                append_animation_metadata(hashmap, animation, self.loop_count.unwrap_or(0));
            }
        }
        let init = ImageProfiles {
            width: self.width,
            height: self.height,
            background: self.background_color.clone(),
            metadata,
        };
        Ok(Some(init))
    }

    /// Reads an RGBA rectangle from the base canvas.
    fn encode_pick(
        &mut self,
        start_x: usize,
        start_y: usize,
        width: usize,
        height: usize,
        _: Option<PickOptions>,
    ) -> Result<Option<Vec<u8>>, Error> {
        if self.buffer.is_none() {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::NotInitializedImageBuffer,
                "in pick".to_string(),
            )));
        }
        let buffersize = checked_rgba_len(width, height, "pick")?;
        let mut data = Vec::with_capacity(buffersize);
        let buffer = self.buffer.as_ref().ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::NotInitializedImageBuffer,
                "in pick".to_string(),
            )) as Error
        })?;

        if start_x >= self.width || start_y >= self.height {
            return Ok(None);
        }
        let requested_end_x = start_x.checked_add(width).ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::OutboundIndex,
                "pick rectangle x overflow".to_string(),
            )) as Error
        })?;
        let requested_end_y = start_y.checked_add(height).ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::OutboundIndex,
                "pick rectangle y overflow".to_string(),
            )) as Error
        })?;
        let w = self.width.min(requested_end_x) - start_x;
        let h = self.height.min(requested_end_y) - start_y;

        for y in 0..h {
            let scanline_src = (start_y + y) * self.width * 4;
            for x in 0..w {
                let offset_src = scanline_src + (start_x + x) * 4;
                if offset_src + 3 >= buffer.len() {
                    return Err(Box::new(ImgError::new_const(
                        ImgErrorKind::OutboundIndex,
                        "Image buffer in pick".to_string(),
                    )));
                }
                data.push(buffer[offset_src]);
                data.push(buffer[offset_src + 1]);
                data.push(buffer[offset_src + 2]);
                data.push(buffer[offset_src + 3]);
            }
            for _ in w..width {
                // 0 fill
                data.push(0x00);
                data.push(0x00);
                data.push(0x00);
                data.push(0x00);
            }
        }
        for _ in h..height {
            // 0 fill
            for _ in 0..width {
                data.push(0x00);
                data.push(0x00);
                data.push(0x00);
                data.push(0x00);
            }
        }

        Ok(Some(data))
    }

    /// Finalizes encoding.
    fn encode_end(&mut self, _: Option<EndOptions>) -> Result<(), Error> {
        Ok(())
    }

    /// Returns stored metadata.
    fn metadata(&mut self) -> Result<Option<HashMap<String, DataMap>>, Error> {
        if let Some(hashmap) = &self.metadata {
            Ok(Some(hashmap.clone()))
        } else {
            Ok(None)
        }
    }
}

/// Decoder configuration.
pub struct DecodeOptions<'a> {
    /// Enables format-specific verbose output when non-zero.
    pub debug_flag: usize,
    /// Destination callback implementation.
    pub drawer: &'a mut dyn DrawCallback,
}

/// Encoder configuration.
pub struct EncodeOptions<'a> {
    /// Enables encoder-specific verbose output when non-zero.
    pub debug_flag: usize,
    /// Source callback implementation.
    pub drawer: &'a mut dyn PickCallback,
    /// Encoder-specific options such as JPEG `quality`, TIFF `compression`,
    /// WebP `quality` and `optimize`, or `exif`.
    ///
    /// `exif` accepts raw serialized EXIF bytes, TIFF-style EXIF headers, or
    /// `Ascii("copy")` to reuse decoded source EXIF during [`convert`].
    pub options: Option<HashMap<String, DataMap>>,
}

/// Decodes an image from memory into an [`ImageBuffer`].
///
/// # Examples
/// ```rust
/// use wml2::draw::{ImageBuffer, image_from, image_to};
/// use wml2::util::ImageFormat;
///
/// let mut source = ImageBuffer::from_buffer(1, 1, vec![255, 0, 0, 255]);
/// let png = image_to(&mut source, ImageFormat::Png, None).unwrap();
///
/// let image = image_from(&png).unwrap();
/// assert_eq!(image.width, 1);
/// assert_eq!(image.height, 1);
/// ```
pub fn image_from(buffer: &[u8]) -> Result<ImageBuffer, Error> {
    image_load(buffer)
}

/// Decodes an image with explicit allocation limits.
///
/// Existing decode entry points remain unlimited for compatibility.
pub fn image_from_with_limits(buffer: &[u8], limits: DecodeLimits) -> Result<ImageBuffer, Error> {
    image_from_with_limits_and_cancel(buffer, limits, None)
}

/// Decodes an image with explicit limits and cooperative cancellation.
pub fn image_from_with_limits_and_cancel(
    buffer: &[u8],
    limits: DecodeLimits,
    cancel_probe: Option<DecodeCancelProbe<'_>>,
) -> Result<ImageBuffer, Error> {
    let mut target = LimitedImageBuffer::new(limits, cancel_probe);
    let mut option = DecodeOptions {
        debug_flag: 0,
        drawer: &mut target,
    };
    image_loader(buffer, &mut option)?;
    target.check_cancelled()?;
    Ok(target.image)
}

/// Decodes an image from a file into an [`ImageBuffer`].
///
/// # Examples
/// ```no_run
/// use wml2::draw::image_from_file;
///
/// let image = image_from_file("input.webp".to_string())?;
/// assert!(image.width > 0);
/// assert!(image.height > 0);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[cfg(not(target_family = "wasm"))]
pub fn image_from_file(filename: String) -> Result<ImageBuffer, Error> {
    let f = std::fs::File::open(filename)?;
    let reader = BufReader::new(f);
    let mut image = ImageBuffer::new();
    let mut option = DecodeOptions {
        debug_flag: 0x00,
        drawer: &mut image,
    };
    let _ = image_reader(reader, &mut option)?;
    Ok(image)
}

/// Encodes an image source and writes it to a file.
///
/// # Examples
/// ```no_run
/// use wml2::draw::{ImageBuffer, image_to_file};
/// use wml2::util::ImageFormat;
///
/// let mut image = ImageBuffer::from_buffer(1, 1, vec![255, 0, 0, 255]);
/// image_to_file("output.png".to_string(), &mut image, ImageFormat::Png)?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[cfg(not(target_family = "wasm"))]
pub fn image_to_file(
    filename: String,
    image: &mut dyn PickCallback,
    format: ImageFormat,
) -> Result<(), Error> {
    let f = std::fs::File::create(filename)?;
    let mut option = EncodeOptions {
        debug_flag: 0x00,
        drawer: image,
        options: None,
    };
    image_writer(f, &mut option, format)?;
    Ok(())
}

#[cfg(not(target_family = "wasm"))]
fn format_from_output_path(output_file: &str) -> Result<ImageFormat, Error> {
    let extension = Path::new(output_file)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase());

    match extension.as_deref() {
        Some("gif") => Ok(ImageFormat::Gif),
        Some("png") | Some("apng") => Ok(ImageFormat::Png),
        Some("jpg") | Some("jpeg") => Ok(ImageFormat::Jpeg),
        Some("bmp") => Ok(ImageFormat::Bmp),
        Some("tif") | Some("tiff") => Ok(ImageFormat::Tiff),
        Some("webp") => Ok(ImageFormat::Webp),
        Some(extension) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            format!("unsupported output extension: {extension}"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "output file has no extension".to_string(),
        ))),
    }
}

/// Converts an input image file to the format implied by `output_file`.
///
/// The output format is selected from the destination extension:
/// `.gif` uses the GIF encoder, `.png` and `.apng` use the PNG/APNG encoder,
/// `.jpg`/`.jpeg` use the JPEG encoder, `.bmp` uses the BMP encoder,
/// `.tif`/`.tiff` use the TIFF encoder, and `.webp` uses the WebP encoder.
/// Encoder-specific settings can be passed in `options`, for example JPEG
/// `quality`, TIFF `compression`, WebP `quality` and `optimize`, or
/// `exif = Ascii("copy")` to preserve source EXIF metadata.
///
/// # Examples
/// ```no_run
/// use std::collections::HashMap;
/// use wml2::draw::convert;
/// use wml2::metadata::DataMap;
///
/// let mut options = HashMap::new();
/// options.insert("quality".to_string(), DataMap::UInt(90));
///
/// convert(
///     "input.png".to_string(),
///     "output.jpg".to_string(),
///     Some(options),
/// )?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[cfg(not(target_family = "wasm"))]
pub fn convert(
    input_file: String,
    output_file: String,
    options: Option<HashMap<String, DataMap>>,
) -> Result<(), Error> {
    let format = format_from_output_path(&output_file)?;
    let f = std::fs::File::create(&output_file)?;
    let mut image = image_from_file(input_file)?;
    let mut encode = EncodeOptions {
        debug_flag: 0,
        drawer: &mut image,
        options,
    };

    image_writer(f, &mut encode, format)?;
    Ok(())
}

/// Decodes an image from memory into an [`ImageBuffer`].
///
/// # Examples
/// ```rust
/// use wml2::draw::{ImageBuffer, image_load, image_to};
/// use wml2::util::ImageFormat;
///
/// let mut source = ImageBuffer::from_buffer(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]);
/// let png = image_to(&mut source, ImageFormat::Png, None).unwrap();
///
/// let image = image_load(&png).unwrap();
/// assert_eq!(image.width, 2);
/// assert_eq!(image.height, 1);
/// ```
pub fn image_load(buffer: &[u8]) -> Result<ImageBuffer, Error> {
    let mut ib = ImageBuffer::new();
    let mut option = DecodeOptions {
        debug_flag: 0,
        drawer: &mut ib,
    };
    let mut reader = BytesReader::new(buffer);

    image_decoder(&mut reader, &mut option)?;
    Ok(ib)
}

/// Decodes an in-memory image into a custom [`DrawCallback`].
///
/// # Examples
/// ```rust
/// use wml2::draw::{DecodeOptions, ImageBuffer, image_loader, image_to};
/// use wml2::util::ImageFormat;
///
/// let mut source = ImageBuffer::from_buffer(1, 1, vec![255, 0, 0, 255]);
/// let png = image_to(&mut source, ImageFormat::Png, None).unwrap();
///
/// let mut target = ImageBuffer::new();
/// let mut options = DecodeOptions {
///     debug_flag: 0,
///     drawer: &mut target,
/// };
/// image_loader(&png, &mut options).unwrap();
/// assert_eq!(target.width, 1);
/// assert_eq!(target.height, 1);
/// ```
pub fn image_loader(
    buffer: &[u8],
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let mut reader = BytesReader::new(buffer);

    let r = image_decoder(&mut reader, option)?;
    Ok(r)
}

/// Decodes an image stream into a custom [`DrawCallback`].
///
/// # Examples
/// ```rust
/// use std::io::Cursor;
/// use wml2::draw::{DecodeOptions, ImageBuffer, image_reader, image_to};
/// use wml2::util::ImageFormat;
///
/// let mut source = ImageBuffer::from_buffer(1, 1, vec![255, 0, 0, 255]);
/// let png = image_to(&mut source, ImageFormat::Png, None).unwrap();
///
/// let mut target = ImageBuffer::new();
/// let mut options = DecodeOptions {
///     debug_flag: 0,
///     drawer: &mut target,
/// };
/// image_reader(Cursor::new(png), &mut options).unwrap();
/// assert_eq!(target.width, 1);
/// assert_eq!(target.height, 1);
/// ```
#[cfg(not(target_family = "wasm"))]
pub fn image_reader<R: BufRead + Seek>(
    reader: R,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let mut reader = StreamReader::new(reader);

    let r = image_decoder(&mut reader, option)?;
    Ok(r)
}

/// Encodes an image source to an arbitrary writer.
///
/// # Examples
/// ```rust
/// use wml2::draw::{EncodeOptions, ImageBuffer, image_writer};
/// use wml2::util::ImageFormat;
///
/// let mut image = ImageBuffer::from_buffer(1, 1, vec![255, 0, 0, 255]);
/// let mut options = EncodeOptions {
///     debug_flag: 0,
///     drawer: &mut image,
///     options: None,
/// };
/// let mut buffer = Vec::new();
/// image_writer(&mut buffer, &mut options, ImageFormat::Png).unwrap();
/// assert!(buffer.starts_with(&[0x89, b'P', b'N', b'G']));
/// ```
pub fn image_writer<W: Write>(
    mut writer: W,
    option: &mut EncodeOptions,
    format: ImageFormat,
) -> Result<Option<ImgWarnings>, Error> {
    let buffer = image_encoder(option, format)?;
    writer.write_all(&buffer)?;
    writer.flush()?;
    Ok(None)
}

/// Detects the input format and dispatches to the matching decoder.
///
/// # Examples
/// ```rust
/// use bin_rs::reader::BytesReader;
/// use wml2::draw::{DecodeOptions, ImageBuffer, image_decoder, image_to};
/// use wml2::util::ImageFormat;
///
/// let mut source = ImageBuffer::from_buffer(1, 1, vec![255, 0, 0, 255]);
/// let png = image_to(&mut source, ImageFormat::Png, None).unwrap();
///
/// let mut reader = BytesReader::new(&png);
/// let mut target = ImageBuffer::new();
/// let mut options = DecodeOptions {
///     debug_flag: 0,
///     drawer: &mut target,
/// };
/// image_decoder(&mut reader, &mut options).unwrap();
/// assert_eq!(target.width, 1);
/// assert_eq!(target.height, 1);
/// ```
pub fn image_decoder<B: BinaryReader>(
    reader: &mut B,
    option: &mut DecodeOptions,
) -> Result<Option<ImgWarnings>, Error> {
    let current = reader.offset()?;
    let end = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(current))?;
    let sample_len = usize::try_from((end - current).min(128)).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "input sample size overflow".to_string(),
        )) as Error
    })?;
    let buffer = reader.read_bytes_no_move(sample_len)?;
    let format = format_check(&buffer);

    use crate::util::ImageFormat::*;
    match format {
        #[cfg(feature = "jpeg")]
        Jpeg => {
            return crate::jpeg::decoder::decode(reader, option);
        }
        #[cfg(feature = "bmp")]
        Bmp => {
            return crate::bmp::decoder::decode(reader, option);
        }
        #[cfg(feature = "ico")]
        Ico => {
            return crate::ico::decoder::decode(reader, option);
        }
        #[cfg(feature = "gif")]
        Gif => {
            return crate::gif::decoder::decode(reader, option);
        }
        #[cfg(feature = "png")]
        Png => {
            return crate::png::decoder::decode(reader, option);
        }
        #[cfg(feature = "webp")]
        Webp => {
            return crate::webp::decoder::decode(reader, option);
        }
        #[cfg(feature = "avif")]
        Avif => {
            return crate::avif::decoder::decode(reader, option);
        }
        #[cfg(feature = "tiff")]
        Tiff => {
            return crate::tiff::decoder::decode(reader, option);
        }
        #[cfg(all(feature = "mag", not(feature = "noretoro")))]
        Mag => {
            return crate::mag::decoder::decode(reader, option);
        }
        #[cfg(all(feature = "maki", not(feature = "noretoro")))]
        Maki => {
            return crate::maki::decoder::decode(reader, option);
        }
        #[cfg(all(feature = "pi", not(feature = "noretoro")))]
        Pi => {
            return crate::pi::decoder::decode(reader, option);
        }
        #[cfg(all(feature = "pic", not(feature = "noretoro")))]
        Pic => {
            return crate::pic::decoder::decode(reader, option);
        }
        #[cfg(all(feature = "vsp", not(feature = "noretoro")))]
        Vsp => {
            return crate::vsp::decoder::decode(reader, option);
        }
        _ => {}
    }

    #[cfg(all(feature = "pcd", not(feature = "noretoro")))]
    {
        let current = reader.seek(std::io::SeekFrom::Current(0))?;
        let pcd = (|| -> Result<bool, Error> {
            reader.seek(std::io::SeekFrom::Start(0x800))?;
            let mut id = [0u8; 7];
            reader.read_exact(&mut id)?;
            Ok(&id == b"PCD_IPI")
        })();
        reader.seek(std::io::SeekFrom::Start(current))?;
        if matches!(pcd, Ok(true)) {
            return crate::pcd::decoder::decode(reader, option);
        }
    }

    Err(Box::new(ImgError::new_const(
        ImgErrorKind::NoSupportFormat,
        "This buffer can not decode".to_string(),
    )))
}

/// Dispatches to the matching encoder for `format`.
///
/// # Examples
/// ```rust
/// use wml2::draw::{EncodeOptions, ImageBuffer, image_encoder};
/// use wml2::util::ImageFormat;
///
/// let mut image = ImageBuffer::from_buffer(1, 1, vec![255, 0, 0, 255]);
/// let mut options = EncodeOptions {
///     debug_flag: 0,
///     drawer: &mut image,
///     options: None,
/// };
/// let png = image_encoder(&mut options, ImageFormat::Png).unwrap();
/// assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
/// ```
pub fn image_encoder(option: &mut EncodeOptions, format: ImageFormat) -> Result<Vec<u8>, Error> {
    use crate::util::ImageFormat::*;

    match format {
        #[cfg(feature = "gif")]
        Gif => {
            return crate::gif::encoder::encode(option);
        }
        #[cfg(feature = "bmp")]
        Bmp => {
            return crate::bmp::encoder::encode(option);
        }
        #[cfg(feature = "jpeg")]
        Jpeg => {
            return crate::jpeg::encoder::encode(option);
        }
        #[cfg(feature = "png")]
        Png => {
            return crate::png::encoder::encode(option);
        }
        #[cfg(feature = "tiff")]
        Tiff => {
            return crate::tiff::encoder::encode(option);
        }
        #[cfg(feature = "webp")]
        Webp => {
            return crate::webp::encoder::encode(option);
        }
        _ => Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "This encoder no impl".to_string(),
        ))),
    }
}

/// Encodes an [`ImageBuffer`] into a memory buffer.
///
/// This is a convenience wrapper around [`image_encoder`] for callers that
/// already use the built-in [`ImageBuffer`] instead of a custom
/// [`PickCallback`] implementation. `options` accepts the same encoder-specific
/// keys as [`EncodeOptions::options`], such as JPEG `quality`, TIFF
/// `compression`, WebP `quality`/`optimize`, or `exif`.
///
/// # Examples
/// ```rust
/// use wml2::draw::{ImageBuffer, image_to};
/// use wml2::util::ImageFormat;
///
/// let mut image = ImageBuffer::from_buffer(1, 1, vec![255, 0, 0, 255]);
/// let png = image_to(&mut image, ImageFormat::Png, None).unwrap();
/// assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
/// ```
pub fn image_to(
    image: &mut ImageBuffer,
    format: ImageFormat,
    options: Option<HashMap<String, DataMap>>,
) -> Result<Vec<u8>, Error> {
    let mut option = EncodeOptions {
        debug_flag: 0,
        drawer: image,
        options,
    };
    image_encoder(&mut option, format)
}
