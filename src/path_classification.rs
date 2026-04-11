use std::path::{Component, Path, Prefix};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BenchPathMatch {
    pub class_name: String,
    pub raw_root: String,
}

pub fn bench_path_match(path: &Path) -> Option<BenchPathMatch> {
    let mapping = load_bench_path_mapping();
    let normalized = normalize_bench_path(path);
    mapping
        .into_iter()
        .find(|entry| normalized.starts_with(&entry.normalized_root))
        .map(|entry| BenchPathMatch {
            class_name: entry.class_name,
            raw_root: entry.raw_root,
        })
}

pub fn is_bench_network_path(path: &Path) -> bool {
    bench_path_match(path)
        .map(|entry| entry.class_name == "ネットワーク")
        .unwrap_or(false)
}

pub fn normalize_bench_path(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_ascii_lowercase()
}

fn load_bench_path_mapping() -> Vec<BenchPathEntry> {
    let Some(path) = locate_bench_datapath() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut current_class = String::from("unclassified");
    let mut entries = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(class_name) = trimmed.strip_prefix("## ") {
            current_class = class_name.to_string();
            continue;
        }
        let Some(root) = trimmed.strip_prefix("- ") else {
            continue;
        };
        let normalized_root = normalize_bench_path(Path::new(root));
        if normalized_root.is_empty() {
            continue;
        }
        entries.push(BenchPathEntry {
            class_name: current_class.clone(),
            raw_root: root.to_string(),
            normalized_root,
        });
    }
    entries
}

struct BenchPathEntry {
    class_name: String,
    raw_root: String,
    normalized_root: String,
}

fn locate_bench_datapath() -> Option<std::path::PathBuf> {
    for base in candidate_search_roots() {
        for ancestor in base.ancestors() {
            let candidate = ancestor.join(".test").join("datapath.md");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn candidate_search_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        roots.push(current_dir);
    }
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(parent) = exe_path.parent() {
            roots.push(parent.to_path_buf());
        }
    }
    roots
}

pub fn is_probably_network_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    if text.starts_with(r"\\") || text.starts_with(r"//") {
        return true;
    }

    if is_bench_network_path(path) {
        return true;
    }

    #[cfg(windows)]
    {
        return is_windows_remote_drive(path);
    }

    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn is_windows_remote_drive(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::GetDriveTypeW;
    use windows::core::PCWSTR;

    const DRIVE_REMOTE: u32 = 4;

    let Some(root) = windows_drive_root(path) else {
        return false;
    };
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe { GetDriveTypeW(PCWSTR::from_raw(wide.as_ptr())) == DRIVE_REMOTE }
}

#[cfg(windows)]
fn windows_drive_root(path: &Path) -> Option<std::path::PathBuf> {
    let mut components = path.components();
    let prefix = match components.next()? {
        Component::Prefix(prefix) => prefix.kind(),
        _ => return None,
    };
    match prefix {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
            Some(std::path::PathBuf::from(format!("{}:\\", char::from(letter))))
        }
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
            Some(std::path::PathBuf::from(format!(
                r"\\{}\{}\",
                server.to_string_lossy(),
                share.to_string_lossy()
            )))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        bench_path_match, candidate_search_roots, is_bench_network_path, normalize_bench_path,
    };
    use std::path::Path;

    #[test]
    fn normalize_bench_path_unifies_separator_and_case() {
        let normalized = normalize_bench_path(Path::new("F:/Comics/Series"));
        assert_eq!(normalized, "f:\\comics\\series");
    }

    #[test]
    fn bench_path_match_matches_datapath_roots() {
        let entry = bench_path_match(Path::new("F:\\benchmark\\archive\\test.zip"))
            .expect("bench path match");

        assert_eq!(entry.class_name, "ネットワーク");
        assert_eq!(entry.raw_root, "F:\\benchmark");
    }

    #[test]
    fn bench_network_path_recognizes_configured_network_roots() {
        assert!(is_bench_network_path(Path::new(
            "F:\\benchmark\\archive\\test.zip"
        )));
        assert!(!is_bench_network_path(Path::new(
            "D:\\data\\samples\\images\\sample.png"
        )));
    }

    #[test]
    fn candidate_search_roots_are_not_empty() {
        assert!(!candidate_search_roots().is_empty());
    }
}
