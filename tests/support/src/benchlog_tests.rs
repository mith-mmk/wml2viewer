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

