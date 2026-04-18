use crate::options::ViewerAction;
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
    match function {
        FilesystemFunction::MoveFile => match params.destination_path {
            Some(destination) => Ok(format!(
                "planned: move {} -> {}",
                target_path.display(),
                destination.display()
            )),
            None => Err("move requires destination path".to_string()),
        },
        FilesystemFunction::CopyFile => match params.destination_path {
            Some(destination) => Ok(format!(
                "planned: copy {} -> {}",
                target_path.display(),
                destination.display()
            )),
            None => Err("copy requires destination path".to_string()),
        },
        FilesystemFunction::DeleteFile => Ok(format!("planned: delete {}", target_path.display())),
        FilesystemFunction::RenameFile => match params.rename_to {
            Some(name) if !name.trim().is_empty() => Ok(format!(
                "planned: rename {} -> {}",
                target_path.display(),
                name
            )),
            _ => Err("rename requires new name".to_string()),
        },
    }
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

// Keep compatibility with existing request wording ("call_fanction").
pub fn call_fanction(
    target_path: &Path,
    function: FilesystemFunction,
    params: FunctionParams,
) -> Result<String, String> {
    call_function(target_path, function, params)
}

#[cfg(test)]
mod tests {
    use super::{FilesystemFunction, FunctionParams, call_function, call_function_for_action};
    use crate::options::ViewerAction;
    use std::path::Path;

    #[test]
    fn move_without_destination_returns_error() {
        let result = call_function(
            Path::new("C:/tmp/a.png"),
            FilesystemFunction::MoveFile,
            FunctionParams::default(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn delete_returns_planned_message() {
        let result = call_function(
            Path::new("C:/tmp/a.png"),
            FilesystemFunction::DeleteFile,
            FunctionParams::default(),
        )
        .expect("delete should produce message");
        assert!(result.contains("planned: delete"));
    }

    #[test]
    fn call_function_for_action_routes_filesystem_actions_only() {
        let result = call_function_for_action(
            Path::new("C:/tmp/a.png"),
            ViewerAction::DeleteFile,
            FunctionParams::default(),
        )
        .expect("filesystem action should be routed")
        .expect("delete should produce message");
        assert!(result.contains("planned: delete"));

        assert!(
            call_function_for_action(
                Path::new("C:/tmp/a.png"),
                ViewerAction::NextImage,
                FunctionParams::default(),
            )
            .is_none()
        );
    }
}
