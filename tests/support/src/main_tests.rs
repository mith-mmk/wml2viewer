use super::parse_args_from;
use std::ffi::OsString;
use std::path::PathBuf;

#[test]
fn parse_args_supports_bench_flag() {
    let args = vec![
        OsString::from("wml2viewer"),
        OsString::from("--bench"),
        OsString::from("sample.zip"),
    ];

    let parsed = parse_args_from(args).unwrap();

    assert!(parsed.bench_enabled);
    assert_eq!(parsed.image_path, Some(PathBuf::from("sample.zip")));
}

#[test]
fn parse_args_supports_config_and_bench_together() {
    let args = vec![
        OsString::from("wml2viewer"),
        OsString::from("--config"),
        OsString::from("config.toml"),
        OsString::from("--bench"),
    ];

    let parsed = parse_args_from(args).unwrap();

    assert!(parsed.bench_enabled);
    assert_eq!(parsed.config_path, Some(PathBuf::from("config.toml")));
}

#[test]
fn parse_args_supports_bench_scenario() {
    let args = vec![
        OsString::from("wml2viewer"),
        OsString::from("--bench"),
        OsString::from("--bench-scenario"),
        OsString::from("zip_subfiler"),
    ];

    let parsed = parse_args_from(args).unwrap();

    assert!(parsed.bench_enabled);
    assert_eq!(parsed.bench_scenario.as_deref(), Some("zip_subfiler"));
}

#[test]
fn parse_args_supports_log_flag() {
    let args = vec![
        OsString::from("wml2viewer"),
        OsString::from("--log"),
        OsString::from("sample.zip"),
    ];

    let parsed = parse_args_from(args).unwrap();

    assert!(parsed.log_enabled);
    assert_eq!(parsed.image_path, Some(PathBuf::from("sample.zip")));
}
