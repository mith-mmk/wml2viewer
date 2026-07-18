use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_TEST_DIR_ID: AtomicU64 = AtomicU64::new(0);

pub(crate) fn make_test_dir(prefix: &str) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe().ok().and_then(|path| {
                path.parent()
                    .and_then(|deps| deps.parent())
                    .map(PathBuf::from)
            })
        })
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .join(".test_wml2viewer");
    fs::create_dir_all(&base).unwrap();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let process_id = std::process::id();
    loop {
        let sequence = NEXT_TEST_DIR_ID.fetch_add(1, Ordering::Relaxed);
        let dir = base.join(format!(
            ".test_{prefix}_{process_id}_{timestamp}_{sequence}"
        ));
        match fs::create_dir(&dir) {
            Ok(()) => return dir,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => panic!("failed to create test directory {}: {error}", dir.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::make_test_dir;
    use std::collections::HashSet;
    use std::fs;

    #[test]
    fn test_directories_are_unique_under_parallel_creation() {
        let handles: Vec<_> = (0..32)
            .map(|_| std::thread::spawn(|| make_test_dir("support")))
            .collect();
        let directories: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        let unique: HashSet<_> = directories.iter().collect();

        assert_eq!(unique.len(), directories.len());
        assert!(directories.iter().all(|directory| {
            directory
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".test_"))
        }));

        for directory in directories {
            fs::remove_dir_all(directory).unwrap();
        }
    }
}
