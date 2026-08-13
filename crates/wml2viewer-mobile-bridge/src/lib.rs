//! Platform-neutral handle, request, image, archive, and reading contracts for mobile adapters.

mod bounded_io;
mod reading_plan;
mod registry;
mod session;

pub use reading_plan::{
    MAX_PREFETCH_SPREADS, MAX_READING_PAGES, READING_WIRE_HEADER_INTS, READING_WIRE_VERSION,
    ReadingPlan, ReadingPlanError, ReadingPlanRequest, encode_reading_wire, plan_reading,
};
pub use session::{
    BridgeState, MAX_MOBILE_ARCHIVE_ENTRY_BYTES, MAX_MOBILE_ARCHIVE_INPUT_BYTES,
    MAX_MOBILE_ARCHIVE_RETAINED_BYTES, MAX_MOBILE_ENCODED_INPUT_BYTES, MAX_NATIVE_ANIMATION_FRAMES,
    MAX_NATIVE_IMAGE_RGBA_BYTES, MAX_NATIVE_POSTER_PIXELS, NativeArchive, NativeArchiveHandle,
    NativeBytes, NativeBytesHandle, NativeErrorCode, NativeImage, NativeImageHandle,
    NativeRequestError, NativeSessionHandle, RgbaEncodeRequest, bridge,
};
pub use wml2viewer_core::internal_decoder_extensions;

#[cfg(test)]
mod tests;
