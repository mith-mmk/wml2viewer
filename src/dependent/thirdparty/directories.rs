use directories::{BaseDirs, ProjectDirs, UserDirs};
use std::path::PathBuf;

#[cfg(any(target_os = "windows", target_os = "macos", unix))]
pub fn default_config_dir() -> Option<PathBuf> {
    ProjectDirs::from("io.github", "mith-mmk", "wml2").map(|proj| proj.config_dir().to_path_buf())
}

#[cfg(any(target_os = "windows", target_os = "macos", unix))]
pub fn default_download_dir() -> Option<PathBuf> {
    UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(|path| path.to_path_buf()))
        .or_else(|| BaseDirs::new().map(|dirs| dirs.home_dir().join("Downloads")))
}

#[cfg(any(target_os = "windows", target_os = "macos", unix))]
pub fn default_temp_dir() -> Option<PathBuf> {
    Some(std::env::temp_dir().join("wml2viewer"))
}

#[cfg(any(target_os = "windows", target_os = "macos", unix))]
#[allow(dead_code)]
pub fn available_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/")];

    if let Some(base) = BaseDirs::new() {
        roots.push(base.home_dir().to_path_buf());
    }
    roots
}
