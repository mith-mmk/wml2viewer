use crate::dependent::default_temp_dir;
use crate::dependent::plugins::{PluginModuleConfig, PluginProviderConfig};
use crate::drawers::canvas::Canvas;
use crate::drawers::image::LoadedImage;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn default_provider() -> PluginProviderConfig {
    let search_path = if cfg!(target_os = "windows") {
        vec![
            PathBuf::from("./susie64/plugins"),
            PathBuf::from("../susie64/plugins"),
            PathBuf::from("./"),
        ]
    } else {
        Vec::new()
    };

    PluginProviderConfig {
        enable: false,
        priority: 100,
        search_path,
        modules: Vec::new(),
    }
}

#[cfg(target_os = "windows")]
pub(super) fn decode_from_file(
    path: &Path,
    _config: &PluginProviderConfig,
    module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    let module_path = module.and_then(|module| module.path.as_ref())?;
    decode_with_susie_module(module_path, path)
}

#[cfg(not(target_os = "windows"))]
pub(super) fn decode_from_file(
    _path: &Path,
    _config: &PluginProviderConfig,
    _module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    None
}

#[cfg(target_os = "windows")]
pub(super) fn decode_from_bytes(
    data: &[u8],
    path_hint: Option<&Path>,
    config: &PluginProviderConfig,
    module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    let input = temp_input_path(path_hint)?;
    std::fs::write(&input, data).ok()?;
    let decoded = decode_from_file(&input, config, module);
    let _ = std::fs::remove_file(&input);
    decoded
}

#[cfg(not(target_os = "windows"))]
pub(super) fn decode_from_bytes(
    _data: &[u8],
    _path_hint: Option<&Path>,
    _config: &PluginProviderConfig,
    _module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    None
}

fn temp_input_path(path_hint: Option<&Path>) -> Option<PathBuf> {
    let ext = path_hint
        .and_then(|path| path.extension().and_then(|ext| ext.to_str()))
        .unwrap_or("bin");
    let root = default_temp_dir()?.join("plugins").join("susie64");
    std::fs::create_dir_all(&root).ok()?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(root.join(format!("susie-input-{unique}.{ext}")))
}

#[cfg(target_os = "windows")]
fn decode_with_susie_module(module_path: &Path, image_path: &Path) -> Option<LoadedImage> {
    use std::ffi::c_void;
    use std::mem::MaybeUninit;
    use windows_sys::Win32::Foundation::{FreeLibrary, HLOCAL, LocalFree};
    use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;

    type IsSupportedW = unsafe extern "system" fn(*const u16, *const c_void) -> i32;
    type GetPictureW = unsafe extern "system" fn(
        *const u16,
        isize,
        u32,
        *mut HLOCAL,
        *mut HLOCAL,
        Option<unsafe extern "system" fn(i32, i32, isize) -> i32>,
        isize,
    ) -> i32;

    unsafe extern "system" fn dummy_progress(_num: i32, _denom: i32, _data: isize) -> i32 {
        0
    }

    let module_utf16 = to_utf16(module_path);
    let module = unsafe { LoadLibraryW(module_utf16.as_ptr()) };
    if module.is_null() {
        return None;
    }

    let supported = (|| {
        let path_utf16 = to_utf16(image_path);
        let mut header = [0u8; 2048];
        let read = std::fs::read(image_path).ok()?;
        let copy_len = read.len().min(header.len());
        header[..copy_len].copy_from_slice(&read[..copy_len]);

        let proc = unsafe { get_proc::<IsSupportedW>(module, "IsSupportedW") }?;
        if unsafe { proc(path_utf16.as_ptr(), header.as_ptr().cast()) } == 0 {
            return None;
        }

        let get_picture = unsafe { get_proc::<GetPictureW>(module, "GetPictureW") }?;
        let mut info_handle = MaybeUninit::<HLOCAL>::zeroed();
        let mut bitmap_handle = MaybeUninit::<HLOCAL>::zeroed();
        let result = unsafe {
            get_picture(
                path_utf16.as_ptr(),
                0,
                0,
                info_handle.as_mut_ptr(),
                bitmap_handle.as_mut_ptr(),
                Some(dummy_progress),
                0,
            )
        };
        if result != 0 {
            return None;
        }

        let info_handle = unsafe { info_handle.assume_init() };
        let bitmap_handle = unsafe { bitmap_handle.assume_init() };
        let image = unsafe { bitmap_handles_to_loaded_image(info_handle, bitmap_handle) };
        let _ = unsafe { LocalFree(info_handle) };
        let _ = unsafe { LocalFree(bitmap_handle) };
        image
    })();

    let _ = unsafe { FreeLibrary(module) };
    supported
}

#[cfg(target_os = "windows")]
unsafe fn bitmap_handles_to_loaded_image(
    info_handle: windows_sys::Win32::Foundation::HLOCAL,
    bitmap_handle: windows_sys::Win32::Foundation::HLOCAL,
) -> Option<LoadedImage> {
    use windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER;
    use windows_sys::Win32::System::Memory::{LocalLock, LocalUnlock};

    let info_ptr = unsafe { LocalLock(info_handle) } as *const BITMAPINFOHEADER;
    let pixels_ptr = unsafe { LocalLock(bitmap_handle) } as *const u8;
    if info_ptr.is_null() || pixels_ptr.is_null() {
        if !info_ptr.is_null() {
            let _ = unsafe { LocalUnlock(info_handle) };
        }
        if !pixels_ptr.is_null() {
            let _ = unsafe { LocalUnlock(bitmap_handle) };
        }
        return None;
    }

    let info = unsafe { &*info_ptr };
    let width = info.biWidth.unsigned_abs();
    let height = info.biHeight.unsigned_abs();
    let bits = info.biBitCount;
    if width == 0 || height == 0 || !(bits == 24 || bits == 32) {
        let _ = unsafe { LocalUnlock(info_handle) };
        let _ = unsafe { LocalUnlock(bitmap_handle) };
        return None;
    }

    let stride = (((width as usize * bits as usize) + 31) / 32) * 4;
    let bottom_up = info.biHeight > 0;
    let mut canvas = Canvas::new(width, height);
    for y in 0..height as usize {
        let src_y = if bottom_up {
            height as usize - 1 - y
        } else {
            y
        };
        let row = unsafe { std::slice::from_raw_parts(pixels_ptr.add(src_y * stride), stride) };
        for x in 0..width as usize {
            let dst = (y * width as usize + x) * 4;
            let src = x * (bits as usize / 8);
            let blue = row.get(src).copied().unwrap_or(0);
            let green = row.get(src + 1).copied().unwrap_or(0);
            let red = row.get(src + 2).copied().unwrap_or(0);
            let alpha = if bits == 32 {
                row.get(src + 3).copied().unwrap_or(255)
            } else {
                255
            };
            let buffer = canvas.buffer_mut();
            buffer[dst] = red;
            buffer[dst + 1] = green;
            buffer[dst + 2] = blue;
            buffer[dst + 3] = alpha;
        }
    }

    let _ = unsafe { LocalUnlock(info_handle) };
    let _ = unsafe { LocalUnlock(bitmap_handle) };
    Some(LoadedImage {
        canvas,
        animation: Vec::new(),
        loop_count: None,
    })
}

#[cfg(target_os = "windows")]
unsafe fn get_proc<T>(module: windows_sys::Win32::Foundation::HMODULE, name: &str) -> Option<T> {
    use std::ffi::CString;
    use windows_sys::Win32::System::LibraryLoader::GetProcAddress;

    let name = CString::new(name).ok()?;
    let proc = unsafe { GetProcAddress(module, name.as_ptr().cast()) };
    if proc.is_none() {
        return None;
    }
    Some(unsafe { std::mem::transmute_copy(&proc) })
}

#[cfg(target_os = "windows")]
fn to_utf16(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
