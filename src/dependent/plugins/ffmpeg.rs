use crate::dependent::default_temp_dir;
use crate::dependent::plugins::{PluginModuleConfig, PluginProviderConfig};
use crate::drawers::image::{
    LoadedImage, load_canvas_from_file_internal, load_canvas_from_path_or_bytes_internal,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(super) fn default_provider() -> PluginProviderConfig {
    PluginProviderConfig {
        enable: false,
        priority: 100,
        search_path: vec![
            PathBuf::from("./plugins/ffmpeg"),
            PathBuf::from("../plugins/ffmpeg"),
            PathBuf::from("./ffmpeg"),
            PathBuf::from("./"),
        ],
        modules: Vec::new(),
    }
}

/* // unix
pub(super) fn default_provider() -> PluginProviderConfig {
    PluginProviderConfig {
        enable: false,
        priority: 100,
        search_path: vec![
            PathBuf::from("./plugins/ffmpeg"),
            PathBuf::from("../plugins/ffmpeg"),
            PathBuf::from("./"),
            PathBuf::from("~/.wml2/plugins"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/usr/bin"),
        ],
        modules: Vec::new(),
    }
}


*/

pub(super) fn decode_from_file(
    path: &Path,
    config: &PluginProviderConfig,
    module: Option<&PluginModuleConfig>,
) -> Option<LoadedImage> {
    let executable = find_ffmpeg_executable(config, module)?;
    let output = temp_file_path("ffmpeg-output", "bmp")?;
    let mut command = Command::new(executable);
    command
        .arg("-v")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(path)
        .arg("-frames:v")
        .arg("1")
        .arg(&output);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let status = command.status().ok()?;
    if !status.success() {
        let _ = std::fs::remove_file(&output);
        return None;
    }
    let decoded = load_canvas_from_file_internal(&output).ok();
    let _ = std::fs::remove_file(&output);
    decoded
}

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
    decoded.or_else(|| load_canvas_from_path_or_bytes_internal(data, path_hint).ok())
}

fn find_ffmpeg_executable(
    config: &PluginProviderConfig,
    module: Option<&PluginModuleConfig>,
) -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Some(module_path) = module.and_then(|module| module.path.as_ref()) {
        if let Some(parent) = module_path.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    roots.extend(config.search_path.iter().cloned());

    for root in roots {
        for candidate in ffmpeg_candidates(&root) {
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn ffmpeg_candidates(root: &Path) -> [PathBuf; 2] {
    [root.join("ffmpeg.exe"), root.join("ffmpeg")]
}

fn temp_input_path(path_hint: Option<&Path>) -> Option<PathBuf> {
    let ext = path_hint
        .and_then(|path| path.extension().and_then(|ext| ext.to_str()))
        .unwrap_or("bin");
    temp_file_path("ffmpeg-input", ext)
}

fn temp_file_path(prefix: &str, ext: &str) -> Option<PathBuf> {
    let root = default_temp_dir()?.join("plugins").join("ffmpeg");
    std::fs::create_dir_all(&root).ok()?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(root.join(format!("{prefix}-{unique}.{ext}")))
}
