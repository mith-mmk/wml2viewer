#[cfg(target_os = "windows")]
use crate::dependent::default_temp_dir;
use crate::dependent::plugins::{PluginModuleConfig, PluginProviderConfig};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use crate::drawers::canvas::Canvas;
use crate::drawers::image::LoadedImage;
use std::path::Path;
#[cfg(target_os = "windows")]
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn default_provider() -> PluginProviderConfig {
    PluginProviderConfig {
        enable: false,
        priority: 280,
        search_path: Vec::new(),
        modules: Vec::new(),
    }
}

#[cfg(target_os = "windows")]
pub(super) fn decode_from_file(
    path: &Path,
    _module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    use windows::Win32::Foundation::{GENERIC_ACCESS_RIGHTS, RPC_E_CHANGED_MODE};
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, IWICBitmapDecoder, IWICImagingFactory,
        WICDecodeMetadataCacheOnDemand,
    };
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::core::PCWSTR;

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED).ok() };
    let _guard = match initialized {
        Ok(()) => Some(ComGuard(true)),
        Err(err) if err.code() == RPC_E_CHANGED_MODE => Some(ComGuard(false)),
        Err(_) => None,
    }?;

    let factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }.ok()?;

    let wide = to_utf16(path);
    let decoder: IWICBitmapDecoder = unsafe {
        factory.CreateDecoderFromFilename(
            PCWSTR(wide.as_ptr()),
            None,
            GENERIC_ACCESS_RIGHTS(0x8000_0000),
            WICDecodeMetadataCacheOnDemand,
        )
    }
    .ok()?;

    decode_with_factory(&factory, &decoder)
}

#[cfg(target_os = "macos")]
pub(super) fn decode_from_file(
    path: &Path,
    _module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    macos::decode_from_file(path)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(super) fn decode_from_file(
    _path: &Path,
    _module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    None
}

#[cfg(target_os = "windows")]
pub(super) fn decode_from_bytes(
    data: &[u8],
    path_hint: Option<&Path>,
    _module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    let ext = path_hint
        .and_then(|path| path.extension().and_then(|ext| ext.to_str()))
        .unwrap_or("bin");
    let root = default_temp_dir()?.join("plugins").join("system");
    std::fs::create_dir_all(&root).ok()?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let path = root.join(format!("system-input-{unique}.{ext}"));
    std::fs::write(&path, data).ok()?;
    let decoded = decode_from_file(&path, None);
    let _ = std::fs::remove_file(path);
    decoded
}

#[cfg(target_os = "macos")]
pub(super) fn decode_from_bytes(
    data: &[u8],
    _path_hint: Option<&Path>,
    _module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    macos::decode_from_bytes(data)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(super) fn decode_from_bytes(
    _data: &[u8],
    _path_hint: Option<&Path>,
    _module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    None
}

#[cfg(target_os = "windows")]
fn decode_with_factory(
    factory: &windows::Win32::Graphics::Imaging::IWICImagingFactory,
    decoder: &windows::Win32::Graphics::Imaging::IWICBitmapDecoder,
) -> Option<LoadedImage> {
    use windows::Win32::Graphics::Imaging::{
        GUID_WICPixelFormat32bppRGBA, IWICFormatConverter, WICBitmapDitherTypeNone,
        WICBitmapPaletteTypeCustom,
    };

    let frame = unsafe { decoder.GetFrame(0) }.ok()?;
    let converter: IWICFormatConverter = unsafe { factory.CreateFormatConverter() }.ok()?;
    unsafe {
        converter.Initialize(
            &frame,
            &GUID_WICPixelFormat32bppRGBA,
            WICBitmapDitherTypeNone,
            None,
            0.0,
            WICBitmapPaletteTypeCustom,
        )
    }
    .ok()?;

    let mut width = 0;
    let mut height = 0;
    unsafe { converter.GetSize(&mut width, &mut height) }.ok()?;
    if width == 0 || height == 0 {
        return None;
    }

    let stride = width.saturating_mul(4);
    let mut rgba = vec![0u8; stride.saturating_mul(height) as usize];
    unsafe { converter.CopyPixels(std::ptr::null(), stride, &mut rgba) }.ok()?;
    let canvas = Canvas::from_rgba(width, height, rgba).ok()?;
    Some(LoadedImage {
        canvas,
        animation: Vec::new(),
        loop_count: None,
    })
}

#[cfg(target_os = "windows")]
fn to_utf16(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{Canvas, LoadedImage};
    use objc2_core_foundation::{
        CFBoolean, CFData, CFDictionary, CFNumber, CFType, CFURL, CGPoint, CGRect, CGSize,
    };
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGBitmapInfo, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo,
        CGImageByteOrderInfo,
    };
    use objc2_image_io::{
        CGImageSource, kCGImageSourceCreateThumbnailFromImageAlways,
        kCGImageSourceCreateThumbnailWithTransform, kCGImageSourceThumbnailMaxPixelSize,
    };
    use std::path::Path;

    pub(super) fn decode_from_file(path: &Path) -> Option<LoadedImage> {
        let url = CFURL::from_file_path(path)?;
        let source = unsafe { CGImageSource::with_url(&url, None) }?;
        decode_source(&source)
    }

    pub(super) fn decode_from_bytes(data: &[u8]) -> Option<LoadedImage> {
        if data.is_empty() {
            return None;
        }
        let data = CFData::from_bytes(data);
        let source = unsafe { CGImageSource::with_data(&data, None) }?;
        decode_source(&source)
    }

    fn decode_source(source: &CGImageSource) -> Option<LoadedImage> {
        if unsafe { source.count() } == 0 {
            return None;
        }

        let raw = unsafe { source.image_at_index(0, None) }?;
        let max_dimension = CGImage::width(Some(&raw)).max(CGImage::height(Some(&raw)));
        if max_dimension == 0 || max_dimension > isize::MAX as usize {
            return None;
        }

        let max_dimension = CFNumber::new_isize(max_dimension as isize);
        let options = CFDictionary::<CFType, CFType>::from_slices(
            &[
                unsafe { kCGImageSourceCreateThumbnailFromImageAlways }.as_ref(),
                unsafe { kCGImageSourceCreateThumbnailWithTransform }.as_ref(),
                unsafe { kCGImageSourceThumbnailMaxPixelSize }.as_ref(),
            ],
            &[
                CFBoolean::new(true).as_ref(),
                CFBoolean::new(true).as_ref(),
                max_dimension.as_ref(),
            ],
        );
        let image =
            unsafe { source.thumbnail_at_index(0, Some(options.as_opaque())) }.unwrap_or(raw);
        decode_image(&image)
    }

    fn decode_image(image: &CGImage) -> Option<LoadedImage> {
        let width = CGImage::width(Some(image));
        let height = CGImage::height(Some(image));
        let width_u32 = u32::try_from(width).ok()?;
        let height_u32 = u32::try_from(height).ok()?;
        if width == 0 || height == 0 {
            return None;
        }

        let bytes_per_row = width.checked_mul(4)?;
        let buffer_len = bytes_per_row.checked_mul(height)?;
        let mut rgba = vec![0u8; buffer_len];
        let color_space = CGColorSpace::new_device_rgb()?;
        let bitmap_info = CGBitmapInfo(
            CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0,
        );
        let context = unsafe {
            CGBitmapContextCreate(
                rgba.as_mut_ptr().cast(),
                width,
                height,
                8,
                bytes_per_row,
                Some(&color_space),
                bitmap_info.0,
            )
        }?;

        CGContext::draw_image(
            Some(&context),
            CGRect::new(CGPoint::ZERO, CGSize::new(width as f64, height as f64)),
            Some(image),
        );
        drop(context);

        unpremultiply_rgba(&mut rgba);
        let canvas = Canvas::from_rgba(width_u32, height_u32, rgba).ok()?;
        Some(LoadedImage {
            canvas,
            animation: Vec::new(),
            loop_count: None,
        })
    }

    fn unpremultiply_rgba(rgba: &mut [u8]) {
        for pixel in rgba.chunks_exact_mut(4) {
            let alpha = u32::from(pixel[3]);
            if alpha == 0 {
                pixel[0] = 0;
                pixel[1] = 0;
                pixel[2] = 0;
                continue;
            }
            if alpha == 255 {
                continue;
            }
            for channel in &mut pixel[..3] {
                let value = (u32::from(*channel) * 255 + alpha / 2) / alpha;
                *channel = value.min(255) as u8;
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::unpremultiply_rgba;

        #[test]
        fn unpremultiply_handles_transparent_and_partial_alpha() {
            let mut rgba = [10, 20, 30, 0, 64, 32, 16, 128, 1, 2, 3, 255];
            unpremultiply_rgba(&mut rgba);
            assert_eq!(rgba, [0, 0, 0, 0, 128, 64, 32, 128, 1, 2, 3, 255]);
        }
    }
}

#[cfg(all(test, any(target_os = "windows", target_os = "macos")))]
#[path = "../../../tests/support/src/dependent/plugins/system_tests.rs"]
mod tests;
