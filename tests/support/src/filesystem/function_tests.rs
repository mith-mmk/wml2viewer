use super::{FilesystemFunction, FunctionParams, call_function};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_test_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("wml2viewer-filefn-{unique}"))
}

#[test]
fn move_without_destination_returns_error() {
    let root = temp_test_dir();
    std::fs::create_dir_all(&root).expect("test dir should be created");
    let src = root.join("a.png");
    std::fs::write(&src, b"abc").expect("source should be written");

    let result = call_function(
        &src,
        FilesystemFunction::MoveFile,
        FunctionParams::default(),
    );
    assert!(result.is_err());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rename_rejects_extension_change() {
    let root = temp_test_dir();
    std::fs::create_dir_all(&root).expect("test dir should be created");
    let src = root.join("a.png");
    std::fs::write(&src, b"abc").expect("source should be written");

    let result = call_function(
        &src,
        FilesystemFunction::RenameFile,
        FunctionParams {
            rename_to: Some("b.jpg".to_string()),
            ..FunctionParams::default()
        },
    );
    assert!(result.is_err());

    let _ = std::fs::remove_dir_all(&root);
}
