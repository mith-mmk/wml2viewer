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
    use std::time::{SystemTime, UNIX_EPOCH};

    const TINY_PNG: &[u8] = &[
        0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
        b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
        0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78,
        0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99,
        0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
    ];

    fn temp_png_path() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data");
        std::fs::create_dir_all(&base).unwrap();
        let path = base.join(format!(".test_system_decoder_{unique}.png"));
        std::fs::write(&path, TINY_PNG).unwrap();
        path
    }

    #[test]
    fn system_decoder_loads_png_sample() {
        let path = temp_png_path();
        let decoded = decode_from_file(&path, None);
        assert!(decoded.is_some());
        let _ = std::fs::remove_file(path);
    }
}
