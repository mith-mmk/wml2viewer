use super::load_listed_file_entries;
use std::fs;

#[test]
fn listed_file_requires_magic_header() {
    let dir = crate::test_support::make_test_dir("listed_file");
    let path = dir.join("sample.wml");
    fs::write(&path, "plain text\nfoo.png\n").unwrap();

    let entries = load_listed_file_entries(&path);
    assert!(entries.is_none());

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn listed_file_resolves_relative_paths_from_parent_dir() {
    let dir = crate::test_support::make_test_dir("listed_file");
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
