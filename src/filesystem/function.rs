use crate::options::ViewerAction;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilesystemFunction {
    MoveFile,
    CopyFile,
    DeleteFile,
    RenameFile,
}

impl FilesystemFunction {
    pub const fn label(self) -> &'static str {
        match self {
            FilesystemFunction::MoveFile => "move",
            FilesystemFunction::CopyFile => "copy",
            FilesystemFunction::DeleteFile => "delete",
            FilesystemFunction::RenameFile => "rename",
        }
    }

    pub const fn from_viewer_action(action: ViewerAction) -> Option<Self> {
        match action {
            ViewerAction::MoveFile => Some(FilesystemFunction::MoveFile),
            ViewerAction::CopyFile => Some(FilesystemFunction::CopyFile),
            ViewerAction::DeleteFile => Some(FilesystemFunction::DeleteFile),
            ViewerAction::RenameFile => Some(FilesystemFunction::RenameFile),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionParams {
    pub destination_path: Option<PathBuf>,
    pub rename_to: Option<String>,
}

pub fn call_function(
    target_path: &Path,
    function: FilesystemFunction,
    params: FunctionParams,
) -> Result<String, String> {
    ensure_regular_existing_file(target_path)?;
    match function {
        FilesystemFunction::MoveFile => {
            let destination_dir = params
                .destination_path
                .ok_or_else(|| "move requires destination path".to_string())?;
            let destination = resolve_destination_file_path(target_path, &destination_dir)?;
            move_file(target_path, &destination)?;
            Ok(format!(
                "Moved: {} -> {}",
                target_path.display(),
                destination.display()
            ))
        }
        FilesystemFunction::CopyFile => {
            let destination_dir = params
                .destination_path
                .ok_or_else(|| "copy requires destination path".to_string())?;
            let destination = resolve_destination_file_path(target_path, &destination_dir)?;
            fs::copy(target_path, &destination).map_err(|err| err.to_string())?;
            Ok(format!(
                "Copied: {} -> {}",
                target_path.display(),
                destination.display()
            ))
        }
        FilesystemFunction::DeleteFile => {
            let deleted_via = if try_move_to_trash(target_path) {
                "trash"
            } else {
                fs::remove_file(target_path).map_err(|err| err.to_string())?;
                "delete"
            };
            Ok(format!(
                "Deleted ({deleted_via}): {}",
                target_path.display()
            ))
        }
        FilesystemFunction::RenameFile => {
            let new_name = params
                .rename_to
                .ok_or_else(|| "rename requires new name".to_string())?;
            let destination = resolve_rename_path(target_path, &new_name)?;
            fs::rename(target_path, &destination).map_err(|err| err.to_string())?;
            Ok(format!(
                "Renamed: {} -> {}",
                target_path.display(),
                destination.display()
            ))
        }
    }
}

fn ensure_regular_existing_file(target_path: &Path) -> Result<(), String> {
    if !target_path.exists() {
        return Err(format!("target does not exist: {}", target_path.display()));
    }
    if !target_path.is_file() {
        return Err(format!(
            "target is not a regular file: {}",
            target_path.display()
        ));
    }
    Ok(())
}

fn resolve_destination_file_path(
    target_path: &Path,
    destination_dir: &Path,
) -> Result<PathBuf, String> {
    if destination_dir.as_os_str().is_empty() {
        return Err("destination path is empty".to_string());
    }
    fs::create_dir_all(destination_dir).map_err(|err| err.to_string())?;
    let file_name = target_path
        .file_name()
        .ok_or_else(|| "target file name not found".to_string())?;
    Ok(destination_dir.join(file_name))
}

fn move_file(source: &Path, destination: &Path) -> Result<(), String> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            // Cross-device moves can fail with rename; fallback to copy+remove.
            fs::copy(source, destination).map_err(|copy_err| {
                format!("move failed: {rename_err}; copy fallback failed: {copy_err}")
            })?;
            fs::remove_file(source).map_err(|remove_err| {
                format!(
                    "move copy fallback partially failed: copied to {}, but remove failed: {remove_err}",
                    destination.display()
                )
            })
        }
    }
}

fn resolve_rename_path(target_path: &Path, rename_to: &str) -> Result<PathBuf, String> {
    let new_name = rename_to.trim();
    if new_name.is_empty() {
        return Err("rename requires new name".to_string());
    }
    if Path::new(new_name).components().count() != 1 {
        return Err("rename must be a single file name".to_string());
    }
    let old_ext = target_path.extension().and_then(|value| value.to_str());
    let new_ext = Path::new(new_name)
        .extension()
        .and_then(|value| value.to_str());
    if old_ext != new_ext {
        return Err("rename cannot change file extension".to_string());
    }
    let parent = target_path
        .parent()
        .ok_or_else(|| "target parent directory not found".to_string())?;
    Ok(parent.join(new_name))
}

#[cfg(target_os = "windows")]
fn try_move_to_trash(target_path: &Path) -> bool {
    use std::process::Command;

    let escaped_path = target_path.display().to_string().replace('\'', "''");
    let script = format!(
        "Add-Type -AssemblyName Microsoft.VisualBasic; \
[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{escaped_path}', \
[Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs, \
[Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin)"
    );
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(not(target_os = "windows"))]
fn try_move_to_trash(_target_path: &Path) -> bool {
    false
}

pub fn call_function_for_action(
    target_path: &Path,
    action: ViewerAction,
    params: FunctionParams,
) -> Option<Result<String, String>> {
    FilesystemFunction::from_viewer_action(action)
        .map(|function| call_function(target_path, function, params))
}

pub fn call_fanction_for_action(
    target_path: &Path,
    action: ViewerAction,
    params: FunctionParams,
) -> Option<Result<String, String>> {
    call_function_for_action(target_path, action, params)
}

pub fn call_fanction(
    target_path: &Path,
    function: FilesystemFunction,
    params: FunctionParams,
) -> Result<String, String> {
    call_function(target_path, function, params)
}

#[cfg(test)]
#[path = "../../tests/support/src/filesystem/function_tests.rs"]
mod tests;
