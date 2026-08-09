use std::ffi::CString;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::OnceLock;

unsafe extern "C" {
    fn wml2viewer_ios_keychain_save_password(
        reference: *const c_char,
        password: *const c_char,
    ) -> i32;
}

#[derive(Clone, Debug)]
struct IosDirectories {
    app_support: PathBuf,
    documents: PathBuf,
    caches: PathBuf,
}

static DIRECTORIES: OnceLock<IosDirectories> = OnceLock::new();

pub fn initialize(app_support: PathBuf, documents: PathBuf, caches: PathBuf) {
    let _ = DIRECTORIES.set(IosDirectories {
        app_support,
        documents,
        caches,
    });
}

fn directories() -> Option<&'static IosDirectories> {
    DIRECTORIES.get()
}

pub fn default_config_dir() -> Option<PathBuf> {
    directories().map(|dirs| dirs.app_support.join("config"))
}

pub fn default_download_dir() -> Option<PathBuf> {
    directories().map(|dirs| dirs.documents.join("downloads"))
}

pub fn default_temp_dir() -> Option<PathBuf> {
    directories().map(|dirs| dirs.caches.join("wml2viewer"))
}

pub fn available_roots() -> Vec<PathBuf> {
    imported_root()
        .into_iter()
        .filter(|path| path.is_dir())
        .collect()
}

pub fn imported_root() -> Option<PathBuf> {
    let dirs = directories()?;
    let current = dirs.app_support.join("current.json");
    if let Ok(text) = std::fs::read_to_string(current)
        && let Ok(snapshot) = serde_json::from_str::<SnapshotReference>(&text)
    {
        let root = dirs.documents.join("snapshots").join(snapshot.generation);
        let root = root.join(snapshot.relative_root);
        if root.is_dir() {
            return Some(root);
        }
    }
    Some(dirs.documents.join("imported"))
}

pub fn request_folder_import() -> bool {
    let Some(dirs) = directories() else {
        return false;
    };
    if std::fs::create_dir_all(&dirs.app_support).is_err() {
        return false;
    }
    std::fs::write(dirs.app_support.join("picker.request"), []).is_ok()
}

pub fn request_file_import() -> bool {
    let Some(dirs) = directories() else {
        return false;
    };
    if std::fs::create_dir_all(&dirs.app_support).is_err() {
        return false;
    }
    std::fs::write(dirs.app_support.join("filepicker.request"), []).is_ok()
}

pub fn save_smb_password(reference: &str, password: &str) -> bool {
    let Ok(reference) = CString::new(reference) else {
        return false;
    };
    let Ok(password) = CString::new(password) else {
        return false;
    };
    unsafe { wml2viewer_ios_keychain_save_password(reference.as_ptr(), password.as_ptr()) == 0 }
}

pub fn take_completed_import() -> Option<PathBuf> {
    let dirs = directories()?;
    let ready = dirs.app_support.join("import.ready");
    if !ready.exists() {
        return None;
    }
    std::fs::remove_file(ready).ok()?;
    let root = imported_root()?;
    root.is_dir().then_some(root)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportStatus {
    Presenting,
    Importing,
    Cancelled,
    Failed,
}

pub fn take_import_status() -> Option<ImportStatus> {
    let dirs = directories()?;
    let status_path = dirs.app_support.join("import.status.json");
    let text = std::fs::read_to_string(&status_path).ok()?;
    let _ = std::fs::remove_file(status_path);
    let status = serde_json::from_str::<ImportStatusReference>(&text).ok()?;
    match status.state.as_str() {
        "presenting" => Some(ImportStatus::Presenting),
        "importing" => Some(ImportStatus::Importing),
        "cancelled" => Some(ImportStatus::Cancelled),
        "failed" => Some(ImportStatus::Failed),
        _ => None,
    }
}

pub fn imported_selected_path() -> Option<PathBuf> {
    let dirs = directories()?;
    let text = std::fs::read_to_string(dirs.app_support.join("current.json")).ok()?;
    let snapshot = serde_json::from_str::<SnapshotReference>(&text).ok()?;
    let selected = snapshot.selected_relative_path?;
    let root = dirs
        .documents
        .join("snapshots")
        .join(snapshot.generation)
        .join(snapshot.relative_root);
    let path = snapshot_selected_path(&root, &selected);
    path.is_file().then_some(path)
}

fn snapshot_selected_path(root: &std::path::Path, selected: &std::path::Path) -> PathBuf {
    root.join(selected)
}

pub fn display_path(path: &std::path::Path) -> String {
    let Some(dirs) = directories() else {
        return path.display().to_string();
    };
    let current = dirs.app_support.join("current.json");
    let snapshot = std::fs::read_to_string(current)
        .ok()
        .and_then(|text| serde_json::from_str::<SnapshotReference>(&text).ok());
    let Some(snapshot) = snapshot else {
        return display_app_container_path(path, &dirs.documents);
    };
    let root = dirs
        .documents
        .join("snapshots")
        .join(&snapshot.generation)
        .join(&snapshot.relative_root);
    if path == root {
        return snapshot
            .display_name
            .filter(|name| !name.is_empty())
            .map(|name| format!("読み込み済み: {name}"))
            .unwrap_or_else(|| "読み込み済みフォルダ".to_string());
    }
    path.strip_prefix(&root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(|relative| {
            snapshot
                .display_name
                .filter(|name| !name.is_empty())
                .map(|name| format!("読み込み済み: {name} / {}", relative.display()))
                .unwrap_or_else(|| format!("読み込み済み / {}", relative.display()))
        })
        .unwrap_or_else(|| display_app_container_path(path, &dirs.documents))
}

fn display_app_container_path(path: &std::path::Path, documents: &std::path::Path) -> String {
    let Some(relative) = path.strip_prefix(documents).ok() else {
        let label = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("読み込み対象");
        return if path.is_dir() {
            format!("外部フォルダ / {label}")
        } else {
            format!("外部ファイル / {label}")
        };
    };
    if relative.as_os_str().is_empty() {
        return "アプリ内Documents".to_string();
    }
    format!("アプリ内Documents / {}", relative.display())
}

#[derive(serde::Deserialize)]
struct SnapshotReference {
    generation: String,
    relative_root: PathBuf,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    selected_relative_path: Option<PathBuf>,
}

#[derive(serde::Deserialize)]
struct ImportStatusReference {
    state: String,
}

pub fn system_locale() -> Option<String> {
    std::env::var("AppleLanguages")
        .ok()
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

#[cfg(test)]
mod tests {
    use super::{ImportStatus, default_config_dir, snapshot_selected_path};
    use std::path::Path;

    #[test]
    fn ios_config_dir_is_unavailable_before_host_initialization() {
        assert!(default_config_dir().is_none());
    }

    #[test]
    fn import_status_has_explicit_terminal_states() {
        assert_ne!(ImportStatus::Cancelled, ImportStatus::Failed);
        assert_ne!(ImportStatus::Presenting, ImportStatus::Importing);
    }

    #[test]
    fn selected_path_is_relative_to_snapshot_root() {
        let root = Path::new(".test-ios-snapshot/folder");
        assert_eq!(
            snapshot_selected_path(root, Path::new("cover.png")),
            Path::new(".test-ios-snapshot/folder/cover.png")
        );
    }
}
