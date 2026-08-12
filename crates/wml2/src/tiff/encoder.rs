//! TIFF encoder implementation.

use crate::color::RGBA;
use crate::draw::{
    ENCODE_ANIMATION_FRAMES_KEY, EncodeOptions as DrawEncodeOptions, ImageProfiles,
    encode_animation_frame_key,
};
use crate::encoder::lzw::encode_tiff;
use crate::error::{ImgError, ImgErrorKind};
#[cfg(feature = "tiff-jpeg")]
use crate::jpeg::encoder::{encode_rgba as encode_jpeg_rgba, quality_from_draw_options};
use crate::metadata::{DataMap, get_exif_option};
use crate::tiff::header::{
    DataPack, Rational, TiffHeader, TiffHeaders, read_tags, tiff_pages_to_bytes,
};
use bin_rs::Endian;
use bin_rs::reader::BytesReader;

type Error = Box<dyn std::error::Error>;

const TIFF_METADATA_KEY: &str = "Tiff headers";
const EXIF_METADATA_KEY: &str = "EXIF";
const ICC_PROFILE_METADATA_KEY: &str = "ICC Profile";
const DEFAULT_RESOLUTION: u32 = 72;

#[derive(Debug)]
struct AnimationFrame {
    width: usize,
    height: usize,
    x_offset: usize,
    y_offset: usize,
    blend: bool,
    dispose: u8,
    buffer: Vec<u8>,
}

#[derive(Debug)]
struct AnimationInfo {
    background: RGBA,
    frames: Vec<AnimationFrame>,
}

struct PagePlan {
    headers: TiffHeaders,
    pixel_data: Vec<u8>,
}

#[derive(Clone, Copy)]
enum TiffCompressionMode {
    None,
    Lzw { is_lsb: bool },
    Jpeg { quality: usize },
}

impl TiffCompressionMode {
    fn code(self) -> u16 {
        match self {
            Self::None => 1,
            Self::Lzw { .. } => 5,
            Self::Jpeg { .. } => 7,
        }
    }

    fn fill_order(self) -> u16 {
        match self {
            Self::Lzw { is_lsb: true } => 2,
            _ => 1,
        }
    }

    fn supports_alpha(self) -> bool {
        !matches!(self, Self::Jpeg { .. })
    }
}

fn as_u64(value: Option<&DataMap>, key: &str) -> Result<u64, Error> {
    match value {
        Some(DataMap::UInt(value)) => Ok(*value),
        Some(DataMap::SInt(value)) if *value >= 0 => Ok(*value as u64),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            format!("{key} is not an unsigned integer"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} metadata not found"),
        ))),
    }
}

fn as_i64(value: Option<&DataMap>, key: &str) -> Result<i64, Error> {
    match value {
        Some(DataMap::SInt(value)) => Ok(*value),
        Some(DataMap::UInt(value)) => i64::try_from(*value).map_err(|_| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("{key} is too large"),
            )) as Error
        }),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            format!("{key} is not an integer"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} metadata not found"),
        ))),
    }
}

fn as_raw(value: Option<&DataMap>, key: &str) -> Result<Vec<u8>, Error> {
    match value {
        Some(DataMap::Raw(value)) => Ok(value.clone()),
        Some(_) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            format!("{key} is not raw metadata"),
        ))),
        None => Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            format!("{key} metadata not found"),
        ))),
    }
}

fn tiff_compression(option: &DrawEncodeOptions<'_>) -> Result<TiffCompressionMode, Error> {
    let Some(value) = option
        .options
        .as_ref()
        .and_then(|map| map.get("compression"))
    else {
        return Ok(TiffCompressionMode::None);
    };

    match value {
        DataMap::Ascii(value) => match value.to_ascii_lowercase().as_str() {
            "none" | "uncompressed" => Ok(TiffCompressionMode::None),
            "lzw" | "lzw_msb" => Ok(TiffCompressionMode::Lzw { is_lsb: false }),
            "lzw_lsb" => Ok(TiffCompressionMode::Lzw { is_lsb: true }),
            #[cfg(feature = "tiff-jpeg")]
            "jpeg" | "jpg" => Ok(TiffCompressionMode::Jpeg {
                quality: quality_from_draw_options(option),
            }),
            #[cfg(not(feature = "tiff-jpeg"))]
            "jpeg" | "jpg" => Err(Box::new(ImgError::new_const(
                ImgErrorKind::NoSupportFormat,
                "TIFF JPEG compression support is disabled by feature flags".to_string(),
            ))),
            _ => Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("unsupported TIFF compression: {value}"),
            ))),
        },
        DataMap::UInt(1) | DataMap::SInt(1) => Ok(TiffCompressionMode::None),
        DataMap::UInt(5) | DataMap::SInt(5) => Ok(TiffCompressionMode::Lzw { is_lsb: false }),
        #[cfg(feature = "tiff-jpeg")]
        DataMap::UInt(7) | DataMap::SInt(7) => Ok(TiffCompressionMode::Jpeg {
            quality: quality_from_draw_options(option),
        }),
        #[cfg(not(feature = "tiff-jpeg"))]
        DataMap::UInt(7) | DataMap::SInt(7) => Err(Box::new(ImgError::new_const(
            ImgErrorKind::NoSupportFormat,
            "TIFF JPEG compression support is disabled by feature flags".to_string(),
        ))),
        _ => Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "TIFF compression must be `none`, `lzw`, `lzw_msb`, `lzw_lsb`, `jpeg`, 1, 5, or 7"
                .to_string(),
        ))),
    }
}

fn parse_animation_info(profile: &ImageProfiles) -> Result<Option<AnimationInfo>, Error> {
    let Some(metadata) = &profile.metadata else {
        return Ok(None);
    };
    let Some(DataMap::UInt(frame_count)) = metadata.get(ENCODE_ANIMATION_FRAMES_KEY) else {
        return Ok(None);
    };
    if *frame_count == 0 {
        return Ok(None);
    }

    let background = profile.background.clone().unwrap_or(RGBA {
        red: 0,
        green: 0,
        blue: 0,
        alpha: 0,
    });

    let mut frames = Vec::with_capacity(*frame_count as usize);
    for index in 0..*frame_count as usize {
        let width_key = encode_animation_frame_key(index, "width");
        let height_key = encode_animation_frame_key(index, "height");
        let start_x_key = encode_animation_frame_key(index, "start_x");
        let start_y_key = encode_animation_frame_key(index, "start_y");
        let dispose_key = encode_animation_frame_key(index, "dispose");
        let blend_key = encode_animation_frame_key(index, "blend");
        let buffer_key = encode_animation_frame_key(index, "buffer");

        let width =
            usize::try_from(as_u64(metadata.get(&width_key), &width_key)?).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    format!("{width_key} is too large"),
                )) as Error
            })?;
        let height =
            usize::try_from(as_u64(metadata.get(&height_key), &height_key)?).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    format!("{height_key} is too large"),
                )) as Error
            })?;
        let x_offset = as_i64(metadata.get(&start_x_key), &start_x_key)?;
        let y_offset = as_i64(metadata.get(&start_y_key), &start_y_key)?;
        let dispose =
            u8::try_from(as_u64(metadata.get(&dispose_key), &dispose_key)?).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    format!("{dispose_key} is too large"),
                )) as Error
            })?;
        let blend = as_u64(metadata.get(&blend_key), &blend_key)? != 0;
        let buffer = as_raw(metadata.get(&buffer_key), &buffer_key)?;

        if width == 0 || height == 0 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} has zero size"),
            )));
        }
        if x_offset < 0 || y_offset < 0 {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} has negative offset"),
            )));
        }
        let x_offset = x_offset as usize;
        let y_offset = y_offset as usize;
        let end_x = x_offset.checked_add(width).ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} x range overflows"),
            )) as Error
        })?;
        let end_y = y_offset.checked_add(height).ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} y range overflows"),
            )) as Error
        })?;
        if end_x > profile.width || end_y > profile.height {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} exceeds the canvas"),
            )));
        }
        let expected_len = width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    format!("animation frame {index} buffer size overflows"),
                )) as Error
            })?;
        if buffer.len() != expected_len {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                format!("animation frame {index} buffer size mismatch"),
            )));
        }

        frames.push(AnimationFrame {
            width,
            height,
            x_offset,
            y_offset,
            blend,
            dispose,
            buffer,
        });
    }

    Ok(Some(AnimationInfo { background, frames }))
}

fn fill_canvas(width: usize, height: usize, background: &RGBA) -> Vec<u8> {
    let pixel_count = width
        .checked_mul(height)
        .expect("validated TIFF canvas dimensions should not overflow");
    let mut canvas = Vec::with_capacity(
        pixel_count
            .checked_mul(4)
            .expect("validated TIFF canvas dimensions should not overflow"),
    );
    for _ in 0..pixel_count {
        canvas.push(background.red);
        canvas.push(background.green);
        canvas.push(background.blue);
        canvas.push(background.alpha);
    }
    canvas
}

fn source_over(dst: &mut [u8], src: &[u8]) {
    let src_alpha = src[3] as u32;
    if src_alpha == 0 {
        return;
    }
    if src_alpha == 255 {
        dst.copy_from_slice(src);
        return;
    }

    let dst_alpha = dst[3] as u32;
    let out_alpha = src_alpha + ((dst_alpha * (255 - src_alpha) + 127) / 255);
    if out_alpha == 0 {
        dst.copy_from_slice(&[0, 0, 0, 0]);
        return;
    }

    for channel in 0..3 {
        let src_premul = src[channel] as u32 * src_alpha;
        let dst_premul = dst[channel] as u32 * dst_alpha;
        let out_premul = src_premul + ((dst_premul * (255 - src_alpha) + 127) / 255);
        dst[channel] = ((out_premul * 255 + (out_alpha / 2)) / out_alpha) as u8;
    }
    dst[3] = out_alpha as u8;
}

fn apply_animation_frame(canvas: &mut [u8], canvas_width: usize, frame: &AnimationFrame) {
    for y in 0..frame.height {
        let src_row = y * frame.width * 4;
        let dst_row = (frame.y_offset + y) * canvas_width * 4;
        for x in 0..frame.width {
            let src_offset = src_row + x * 4;
            let dst_offset = dst_row + (frame.x_offset + x) * 4;
            if frame.blend {
                source_over(
                    &mut canvas[dst_offset..dst_offset + 4],
                    &frame.buffer[src_offset..src_offset + 4],
                );
            } else {
                canvas[dst_offset..dst_offset + 4]
                    .copy_from_slice(&frame.buffer[src_offset..src_offset + 4]);
            }
        }
    }
}

fn clear_animation_frame(
    canvas: &mut [u8],
    canvas_width: usize,
    frame: &AnimationFrame,
    background: &RGBA,
) {
    for y in 0..frame.height {
        let dst_row = (frame.y_offset + y) * canvas_width * 4;
        for x in 0..frame.width {
            let dst_offset = dst_row + (frame.x_offset + x) * 4;
            canvas[dst_offset] = background.red;
            canvas[dst_offset + 1] = background.green;
            canvas[dst_offset + 2] = background.blue;
            canvas[dst_offset + 3] = background.alpha;
        }
    }
}

fn compose_animation_pages(
    profile: &ImageProfiles,
    animation: AnimationInfo,
) -> Result<Vec<Vec<u8>>, Error> {
    let mut canvas = fill_canvas(profile.width, profile.height, &animation.background);
    let mut pages = Vec::with_capacity(animation.frames.len());

    for frame in &animation.frames {
        let previous = matches!(frame.dispose, 2).then(|| canvas.clone());
        apply_animation_frame(&mut canvas, profile.width, frame);
        pages.push(canvas.clone());

        match frame.dispose {
            1 => clear_animation_frame(&mut canvas, profile.width, frame, &animation.background),
            2 => {
                canvas = previous.ok_or_else(|| {
                    Box::new(ImgError::new_const(
                        ImgErrorKind::EncodeError,
                        "missing previous canvas for dispose=previous".to_string(),
                    )) as Error
                })?;
            }
            _ => {}
        }
    }

    Ok(pages)
}

fn rgba_has_alpha(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).any(|pixel| pixel[3] != 0xff)
}

fn rgba_to_tiff_samples(rgba: &[u8], with_alpha: bool) -> Vec<u8> {
    let channels = if with_alpha { 4 } else { 3 };
    let mut pixel_data = Vec::with_capacity(rgba.len() / 4 * channels);
    for pixel in rgba.chunks_exact(4) {
        pixel_data.push(pixel[0]);
        pixel_data.push(pixel[1]);
        pixel_data.push(pixel[2]);
        if with_alpha {
            pixel_data.push(pixel[3]);
        }
    }
    pixel_data
}

fn first_ifd_tags(tags: &[TiffHeader]) -> &[TiffHeader] {
    let mut split_index = tags.len();
    for index in 1..tags.len() {
        if tags[index].tagid < tags[index - 1].tagid {
            split_index = index;
            break;
        }
    }
    &tags[..split_index]
}

fn source_headers(profile: &ImageProfiles) -> Option<TiffHeaders> {
    let metadata = profile.metadata.as_ref()?;
    match metadata.get(TIFF_METADATA_KEY) {
        Some(DataMap::Exif(headers)) => Some(headers.clone()),
        _ => match metadata.get(EXIF_METADATA_KEY) {
            Some(DataMap::Exif(headers)) => Some(headers.clone()),
            _ => None,
        },
    }
}

fn exif_headers_from_bytes(bytes: &[u8]) -> Result<TiffHeaders, Error> {
    let mut reader = BytesReader::new(bytes);
    read_tags(&mut reader)
}

fn source_icc_profile(profile: &ImageProfiles, source: Option<&TiffHeaders>) -> Option<Vec<u8>> {
    if let Some(metadata) = &profile.metadata {
        if let Some(DataMap::ICCProfile(profile)) = metadata.get(ICC_PROFILE_METADATA_KEY) {
            return Some(profile.clone());
        }
    }

    let source = source?;
    first_ifd_tags(&source.headers)
        .iter()
        .find(|tag| tag.tagid == 0x8773)
        .and_then(|tag| match &tag.data {
            DataPack::Bytes(data) | DataPack::Undef(data) | DataPack::Unkown(data) => {
                Some(data.clone())
            }
            _ => None,
        })
}

fn should_copy_source_tag(tagid: usize) -> bool {
    !matches!(
        tagid,
        0x00fe
            | 0x00ff
            | 0x0100
            | 0x0101
            | 0x0102
            | 0x0103
            | 0x0106
            | 0x010a
            | 0x0111
            | 0x0112
            | 0x0115
            | 0x0116
            | 0x0117
            | 0x011c
            | 0x011e
            | 0x011f
            | 0x013d
            | 0x0140
            | 0x0142
            | 0x0143
            | 0x0144
            | 0x0145
            | 0x0152
            | 0x01b5
            | 0x0200
            | 0x0201
            | 0x0202
            | 0x0203
            | 0x0205
            | 0x0206
            | 0x0207
            | 0x0208
            | 0x0209
            | 0x0211
            | 0x0212
            | 0x0213
            | 0x0214
            | 0x8769
            | 0x8773
            | 0x8825
    )
}

fn upsert_tag(tags: &mut Vec<TiffHeader>, tag: TiffHeader) {
    if let Some(index) = tags.iter().position(|existing| existing.tagid == tag.tagid) {
        tags[index] = tag;
    } else {
        tags.push(tag);
    }
}

fn ensure_tag(tags: &mut Vec<TiffHeader>, tag: TiffHeader) {
    if !tags.iter().any(|existing| existing.tagid == tag.tagid) {
        tags.push(tag);
    }
}

fn remove_tag(tags: &mut Vec<TiffHeader>, tagid: usize) {
    if let Some(index) = tags.iter().position(|existing| existing.tagid == tagid) {
        tags.remove(index);
    }
}

fn short_tag(tagid: usize, value: u16) -> TiffHeader {
    TiffHeader {
        tagid,
        data: DataPack::Short(vec![value]),
        length: 1,
    }
}

fn short_array_tag(tagid: usize, values: Vec<u16>) -> TiffHeader {
    TiffHeader {
        tagid,
        length: values.len(),
        data: DataPack::Short(values),
    }
}

fn rational_array_tag(tagid: usize, values: Vec<Rational>) -> TiffHeader {
    TiffHeader {
        tagid,
        length: values.len(),
        data: DataPack::Rational(values),
    }
}

fn long_tag(tagid: usize, value: u32) -> TiffHeader {
    TiffHeader {
        tagid,
        data: DataPack::Long(vec![value]),
        length: 1,
    }
}

fn undef_tag(tagid: usize, data: Vec<u8>) -> TiffHeader {
    TiffHeader {
        tagid,
        length: data.len(),
        data: DataPack::Undef(data),
    }
}

fn rational_tag(tagid: usize, numerator: u32, denominator: u32) -> TiffHeader {
    TiffHeader {
        tagid,
        data: DataPack::Rational(vec![Rational {
            n: numerator,
            d: denominator,
        }]),
        length: 1,
    }
}

fn build_page_headers(
    width: usize,
    height: usize,
    pixel_data_len: usize,
    with_alpha: bool,
    compression: TiffCompressionMode,
    source: Option<&TiffHeaders>,
    icc_profile: Option<&[u8]>,
) -> Result<TiffHeaders, Error> {
    let width = u32::try_from(width).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "TIFF width exceeds u32".to_string(),
        )) as Error
    })?;
    let height = u32::try_from(height).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "TIFF height exceeds u32".to_string(),
        )) as Error
    })?;
    let strip_byte_count = u32::try_from(pixel_data_len).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "TIFF strip byte count exceeds u32".to_string(),
        )) as Error
    })?;

    let mut headers = TiffHeaders::empty(Endian::LittleEndian);
    if let Some(source) = source {
        for tag in first_ifd_tags(&source.headers) {
            if should_copy_source_tag(tag.tagid) {
                upsert_tag(&mut headers.headers, tag.clone());
            }
        }
        headers.exif = source.exif.clone();
        headers.gps = source.gps.clone();
    }

    let samples_per_pixel = if with_alpha { 4 } else { 3 };
    let is_jpeg = matches!(compression, TiffCompressionMode::Jpeg { .. });
    upsert_tag(&mut headers.headers, long_tag(0x0100, width));
    upsert_tag(&mut headers.headers, long_tag(0x0101, height));
    upsert_tag(
        &mut headers.headers,
        short_array_tag(0x0102, vec![8; samples_per_pixel]),
    );
    upsert_tag(&mut headers.headers, short_tag(0x0103, compression.code()));
    upsert_tag(
        &mut headers.headers,
        short_tag(0x0106, if is_jpeg { 6 } else { 2 }),
    );
    upsert_tag(
        &mut headers.headers,
        short_tag(0x010a, compression.fill_order()),
    );
    upsert_tag(&mut headers.headers, long_tag(0x0111, 0));
    upsert_tag(&mut headers.headers, short_tag(0x0112, 1));
    upsert_tag(
        &mut headers.headers,
        short_tag(0x0115, samples_per_pixel as u16),
    );
    upsert_tag(&mut headers.headers, long_tag(0x0116, height));
    upsert_tag(&mut headers.headers, long_tag(0x0117, strip_byte_count));
    upsert_tag(&mut headers.headers, short_tag(0x011c, 1));

    ensure_tag(
        &mut headers.headers,
        rational_tag(0x011a, DEFAULT_RESOLUTION, 1),
    );
    ensure_tag(
        &mut headers.headers,
        rational_tag(0x011b, DEFAULT_RESOLUTION, 1),
    );
    ensure_tag(&mut headers.headers, short_tag(0x0128, 2));

    if with_alpha {
        upsert_tag(&mut headers.headers, short_array_tag(0x0152, vec![2]));
    } else {
        remove_tag(&mut headers.headers, 0x0152);
    }
    if let Some(profile) = icc_profile {
        upsert_tag(&mut headers.headers, undef_tag(0x8773, profile.to_vec()));
    }

    for tagid in [
        0x01b5, 0x0200, 0x0201, 0x0202, 0x0203, 0x0205, 0x0206, 0x0207, 0x0208, 0x0209,
    ] {
        remove_tag(&mut headers.headers, tagid);
    }

    if is_jpeg {
        upsert_tag(
            &mut headers.headers,
            rational_array_tag(
                0x0211,
                vec![
                    Rational { n: 299, d: 1000 },
                    Rational { n: 587, d: 1000 },
                    Rational { n: 114, d: 1000 },
                ],
            ),
        );
        upsert_tag(&mut headers.headers, short_array_tag(0x0212, vec![1, 1]));
        upsert_tag(&mut headers.headers, short_tag(0x0213, 1));
        upsert_tag(
            &mut headers.headers,
            rational_array_tag(
                0x0214,
                vec![
                    Rational { n: 0, d: 1 },
                    Rational { n: 255, d: 1 },
                    Rational { n: 128, d: 1 },
                    Rational { n: 255, d: 1 },
                    Rational { n: 128, d: 1 },
                    Rational { n: 255, d: 1 },
                ],
            ),
        );
    } else {
        for tagid in [0x0211, 0x0212, 0x0213, 0x0214] {
            remove_tag(&mut headers.headers, tagid);
        }
    }

    Ok(headers)
}

fn set_strip_offset(headers: &mut TiffHeaders, offset: u32) -> Result<(), Error> {
    let Some(tag) = headers.headers.iter_mut().find(|tag| tag.tagid == 0x0111) else {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::EncodeError,
            "TIFF strip offsets tag missing".to_string(),
        )));
    };
    tag.length = 1;
    tag.data = DataPack::Long(vec![offset]);
    Ok(())
}

fn build_page_plan(
    width: usize,
    height: usize,
    rgba: &[u8],
    compression: TiffCompressionMode,
    source: Option<&TiffHeaders>,
    icc_profile: Option<&[u8]>,
) -> Result<PagePlan, Error> {
    let expected_len = width
        .checked_mul(height)
        .and_then(|pixel_count| pixel_count.checked_mul(4))
        .ok_or_else(|| {
            Box::new(ImgError::new_const(
                ImgErrorKind::InvalidParameter,
                "TIFF image dimensions overflow".to_string(),
            )) as Error
        })?;
    if rgba.len() != expected_len {
        return Err(Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "TIFF RGBA buffer size mismatch".to_string(),
        )));
    }

    let with_alpha = compression.supports_alpha() && rgba_has_alpha(rgba);
    let raw_pixel_data = rgba_to_tiff_samples(rgba, with_alpha);
    let pixel_data = match compression {
        TiffCompressionMode::None => raw_pixel_data,
        TiffCompressionMode::Lzw { is_lsb } => encode_tiff(&raw_pixel_data, is_lsb)?,
        #[cfg(feature = "tiff-jpeg")]
        TiffCompressionMode::Jpeg { quality } => encode_jpeg_rgba(width, height, rgba, quality)?,
        #[cfg(not(feature = "tiff-jpeg"))]
        TiffCompressionMode::Jpeg { .. } => {
            return Err(Box::new(ImgError::new_const(
                ImgErrorKind::NoSupportFormat,
                "TIFF JPEG compression support is disabled by feature flags".to_string(),
            )));
        }
    };
    let headers = build_page_headers(
        width,
        height,
        pixel_data.len(),
        with_alpha,
        compression,
        source,
        icc_profile,
    )?;
    Ok(PagePlan {
        headers,
        pixel_data,
    })
}

fn build_animation_pages(
    profile: &ImageProfiles,
    compression: TiffCompressionMode,
    source: Option<&TiffHeaders>,
    icc_profile: Option<&[u8]>,
    animation: AnimationInfo,
) -> Result<Vec<PagePlan>, Error> {
    let canvases = compose_animation_pages(profile, animation)?;
    let mut pages = Vec::with_capacity(canvases.len());
    for (index, canvas) in canvases.iter().enumerate() {
        pages.push(build_page_plan(
            profile.width,
            profile.height,
            canvas,
            compression,
            if index == 0 { source } else { None },
            if index == 0 { icc_profile } else { None },
        )?);
    }
    Ok(pages)
}

/// Encodes an image source to TIFF.
///
/// Still images are written as a single TIFF page using uncompressed, LZW, or
/// JPEG compression depending on `compression`. Animated input is flattened
/// into full-canvas multi-page TIFF output so it can be round-tripped through
/// [`crate::draw::convert`]. JPEG-compressed TIFF pages reuse the JPEG encoder
/// and therefore store RGB only.
///
/// Supported `EncodeOptions.options` keys:
/// - `compression`: `none`, `lzw`, `lzw_msb`, `lzw_lsb`, or `jpeg`
/// - `quality`: JPEG quality when `compression=jpeg`
/// - `exif`: `Raw(bytes)`, `Exif(headers)`, or `Ascii("copy")`
pub fn encode(image: &mut DrawEncodeOptions<'_>) -> Result<Vec<u8>, Error> {
    let profile = image.drawer.encode_start(None)?;
    let profile = profile.ok_or_else(|| {
        Box::new(ImgError::new_const(
            ImgErrorKind::OutboundIndex,
            "Image profiles nothing".to_string(),
        )) as Error
    })?;
    let compression = tiff_compression(image)?;

    let source =
        if let Some(exif) = get_exif_option(image.options.as_ref(), profile.metadata.as_ref())? {
            Some(exif_headers_from_bytes(&exif)?)
        } else {
            source_headers(&profile)
        };
    let icc_profile = source_icc_profile(&profile, source.as_ref());

    let mut pages = if let Some(animation) = parse_animation_info(&profile)? {
        build_animation_pages(
            &profile,
            compression,
            source.as_ref(),
            icc_profile.as_deref(),
            animation,
        )?
    } else {
        let rgba = image
            .drawer
            .encode_pick(0, 0, profile.width, profile.height, None)?
            .ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::EncodeError,
                    "Image buffer nothing".to_string(),
                )) as Error
            })?;
        vec![build_page_plan(
            profile.width,
            profile.height,
            &rgba,
            compression,
            source.as_ref(),
            icc_profile.as_deref(),
        )?]
    };

    let provisional_headers: Vec<TiffHeaders> =
        pages.iter().map(|page| page.headers.clone()).collect();
    let provisional_len = tiff_pages_to_bytes(&provisional_headers)?.len();
    let mut strip_offset = u32::try_from(provisional_len).map_err(|_| {
        Box::new(ImgError::new_const(
            ImgErrorKind::InvalidParameter,
            "TIFF data offset exceeds u32".to_string(),
        )) as Error
    })?;

    for page in &mut pages {
        set_strip_offset(&mut page.headers, strip_offset)?;
        strip_offset = strip_offset
            .checked_add(u32::try_from(page.pixel_data.len()).map_err(|_| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    "TIFF page data exceeds u32".to_string(),
                )) as Error
            })?)
            .ok_or_else(|| {
                Box::new(ImgError::new_const(
                    ImgErrorKind::InvalidParameter,
                    "TIFF data offset overflow".to_string(),
                )) as Error
            })?;
    }

    let headers: Vec<TiffHeaders> = pages.iter().map(|page| page.headers.clone()).collect();
    let mut data = tiff_pages_to_bytes(&headers)?;
    for page in pages {
        data.extend_from_slice(&page.pixel_data);
    }

    image.drawer.encode_end(None)?;
    Ok(data)
}
