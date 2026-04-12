use crate::dependent::default_temp_dir;
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct BenchLogger {
    inner: Arc<BenchLoggerInner>,
}

struct BenchLoggerInner {
    file: Mutex<File>,
    path: PathBuf,
    started_at: Instant,
}

impl BenchLogger {
    pub fn create() -> io::Result<Self> {
        let directory = default_temp_dir().unwrap_or_else(|| std::env::temp_dir().join("wml2viewer"));
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!("state-{}.jsonl", timestamp_token()));
        Self::create_at_path(path)
    }

    fn create_at_path(path: PathBuf) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            inner: Arc::new(BenchLoggerInner {
                file: Mutex::new(file),
                path,
                started_at: Instant::now(),
            }),
        })
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub fn log(&self, event: &str, payload: Value) {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let elapsed_ms = self.inner.started_at.elapsed().as_millis();
        let line = json!({
            "timestamp_ms": timestamp_ms,
            "elapsed_ms": elapsed_ms,
            "event": event,
            "payload": payload,
        });

        if let Ok(mut file) = self.inner.file.lock() {
            let _ = writeln!(file, "{line}");
            let _ = file.flush();
        }
    }
}

pub fn set_global_bench_logger(logger: Option<BenchLogger>) {
    if let Ok(mut slot) = global_bench_logger().lock() {
        *slot = logger;
    }
}

pub fn log_global_bench_event(event: &str, payload: Value) {
    let logger = global_bench_logger()
        .lock()
        .ok()
        .and_then(|slot| slot.clone());
    if let Some(logger) = logger {
        logger.log(event, payload);
    }
}

fn global_bench_logger() -> &'static Mutex<Option<BenchLogger>> {
    static GLOBAL_BENCH_LOGGER: OnceLock<Mutex<Option<BenchLogger>>> = OnceLock::new();
    GLOBAL_BENCH_LOGGER.get_or_init(|| Mutex::new(None))
}

fn timestamp_token() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("{now}-{}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::{BenchLogger, log_global_bench_event, set_global_bench_logger};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn bench_logger_writes_jsonl_line() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir()
            .join("wml2viewer-tests")
            .join(format!("benchlog-{unique}.jsonl"));
        let logger = BenchLogger::create_at_path(path.clone()).unwrap();

        logger.log("test.event", serde_json::json!({ "value": 1 }));

        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("\"event\":\"test.event\""));
        assert!(text.contains("\"value\":1"));
    }

    #[test]
    fn global_bench_logger_writes_jsonl_line() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir()
            .join("wml2viewer-tests")
            .join(format!("benchlog-global-{unique}.jsonl"));
        let logger = BenchLogger::create_at_path(path.clone()).unwrap();

        set_global_bench_logger(Some(logger));
        log_global_bench_event("test.global", serde_json::json!({ "value": 2 }));
        set_global_bench_logger(None);

        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains("\"event\":\"test.global\""));
        assert!(text.contains("\"value\":2"));
    }
}
