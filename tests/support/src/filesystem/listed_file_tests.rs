use super::load_listed_file_entries;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn make_temp_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_data");
    fs::create_dir_all(&base).unwrap();
    let dir = base.join(format!(".test_listed_file_{unique}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn listed_file_requires_magic_header() {
    let dir = make_temp_dir();
    let path = dir.join("sample.wml");
    fs::write(&path, "plain text\nfoo.png\n").unwrap();

    let entries = load_listed_file_entries(&path);
    assert!(entries.is_none());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn listed_file_resolves_relative_paths_from_parent_dir() {
    let dir = make_temp_dir();
    let list_dir = dir.join("lists");
    fs::create_dir_all(&list_dir).unwrap();
    let path = list_dir.join("sample.wmltxt");
    fs::write(
        &path,
        "#!WMLViewer2 ListedFile 1.0\n../images/a.png\nsub/b.jpg\n@ PATH=ignored\n",
    )
    .unwrap();

    let entries = load_listed_file_entries(&path).unwrap();
    assert_eq!(
        entries,
        vec![list_dir.join("../images/a.png"), list_dir.join("sub/b.jpg")]
    );

    let _ = fs::remove_dir_all(dir);
}
