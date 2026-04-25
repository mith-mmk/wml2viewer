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
#[path = "../../tests/support/src/filesystem/listed_file_tests.rs"]
mod tests;
