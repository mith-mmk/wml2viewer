use std::fs;
use std::path::{Path, PathBuf};

const LISTED_FILE_HEADER: &str = "#!WMLViewer2 ListedFile";

pub(crate) fn load_listed_file_entries(path: &Path) -> Option<Vec<PathBuf>> {
    if !is_listed_file_path(path) {
        return None;
    }

    let text = fs::read_to_string(path).ok()?;
    parse_listed_file_text(path, &text)
}

fn is_listed_file_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("wmltxt"))
        .unwrap_or(false)
}

fn parse_listed_file_text(path: &Path, text: &str) -> Option<Vec<PathBuf>> {
    let mut lines = text.lines();
    let first_line = lines.next()?.trim();
    if !first_line.starts_with(LISTED_FILE_HEADER) {
        return None;
    }

    let base_dir = path
        .parent()
        .map(Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut entries = Vec::new();
    for raw_line in lines {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }

        let candidate = PathBuf::from(line);
        let resolved = if candidate.is_absolute() {
            candidate
        } else {
            base_dir.join(candidate)
        };
        entries.push(resolved);
    }

    Some(entries)
}

#[cfg(test)]
mod tests {
    use super::load_listed_file_entries;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("wml2viewer_listed_file_{unique}"));
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
}
