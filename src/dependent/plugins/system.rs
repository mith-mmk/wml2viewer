use crate::dependent::default_temp_dir;
use crate::dependent::plugins::{PluginModuleConfig, PluginProviderConfig};
use crate::drawers::canvas::Canvas;
use crate::drawers::image::LoadedImage;
use std::path::Path;
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

#[cfg(not(target_os = "windows"))]
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

#[cfg(not(target_os = "windows"))]
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

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::decode_from_file;
    use std::path::PathBuf;

    fn sample_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("samples")
            .join(name)
    }

    #[test]
    fn system_decoder_loads_png_sample() {
        let decoded = decode_from_file(&sample_path("WML2Viewer.png"), None);
        assert!(decoded.is_some());
    }
}
