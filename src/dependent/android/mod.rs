use std::path::PathBuf;
use std::sync::OnceLock;

static APP_FILES_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn initialize(app_files_dir: PathBuf) {
    let _ = APP_FILES_DIR.set(app_files_dir);
}

fn files_dir() -> Option<PathBuf> {
    APP_FILES_DIR.get().cloned()
}

pub fn imported_root() -> Option<PathBuf> {
    files_dir().map(|dir| dir.join("imported"))
}

pub fn request_folder_import() -> bool {
    let Some(dir) = files_dir() else {
        return false;
    };
    std::fs::create_dir_all(&dir).is_ok() && std::fs::write(dir.join("picker.request"), []).is_ok()
}

pub fn take_completed_import() -> Option<PathBuf> {
    let dir = files_dir()?;
    let ready = dir.join("import.ready");
    if !ready.exists() {
        return None;
    }
    std::fs::remove_file(ready).ok()?;
    let root = dir.join("imported");
    root.is_dir().then_some(root)
}

pub fn default_config_dir() -> Option<PathBuf> {
    files_dir().map(|dir| dir.join("config"))
}

pub fn available_roots() -> Vec<PathBuf> {
    imported_root()
        .into_iter()
        .filter(|path| path.is_dir())
        .collect()
}

pub fn system_locale() -> Option<String> {
    std::env::var("LC_ALL")
        .ok()
        .or_else(|| std::env::var("LC_MESSAGES").ok())
        .or_else(|| std::env::var("LANG").ok())
}

pub fn locale_font_candidates(_locale: &str) -> Vec<PathBuf> {
    Vec::new()
}

pub fn emoji_font_candidates() -> Vec<PathBuf> {
    Vec::new()
}

pub fn last_resort_font_candidates() -> Vec<PathBuf> {
    Vec::new()
}

pub fn pick_directory_dialog() -> Option<PathBuf> {
    request_folder_import().then_some(imported_root()?)
}

pub fn download_url_to_temp(_url: &str) -> Option<PathBuf> {
    None
}

pub fn default_download_dir() -> Option<PathBuf> {
    files_dir().map(|dir| dir.join("downloads"))
}

pub fn default_temp_dir() -> Option<PathBuf> {
    files_dir().map(|dir| dir.join("cache"))
}
