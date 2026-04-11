#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use wml2viewer::{app, dependent};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    if args.clean_target.as_deref() == Some("system") {
        dependent::clean_system_integration()?;
        return Ok(());
    }
    app::run(args.image_path, args.config_path)
}

struct CliArgs {
    image_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    clean_target: Option<String>,
}

fn parse_args() -> Result<CliArgs, Box<dyn Error>> {
    let mut args = env::args_os();
    let program = args.next().unwrap_or_else(|| OsString::from("wml2viewer"));
    let mut positional_args = Vec::new();
    let mut config_path = None;
    let mut clean_target = None;

    while let Some(arg) = args.next() {
        if let Some(path) = parse_config_equals(&arg) {
            config_path = Some(path);
            continue;
        }

        if let Some(target) = parse_clean_equals(&arg) {
            clean_target = Some(target);
            continue;
        }

        if arg == "--config" {
            let Some(path) = args.next() else {
                return Err(usage_error(&program));
            };
            config_path = Some(PathBuf::from(path));
            continue;
        }

        if arg == "--clean" {
            let Some(target) = args.next() else {
                return Err(usage_error(&program));
            };
            clean_target = Some(target.to_string_lossy().into_owned());
            continue;
        }

        if is_ignorable_shell_argument(&arg) {
            continue;
        }

        positional_args.push(PathBuf::from(arg));
    }

    let image_path = pick_image_path(positional_args);

    Ok(CliArgs {
        image_path,
        config_path,
        clean_target,
    })
}

fn parse_config_equals(arg: &OsString) -> Option<PathBuf> {
    let text = arg.to_string_lossy();
    text.strip_prefix("--config=").map(PathBuf::from)
}

fn parse_clean_equals(arg: &OsString) -> Option<String> {
    let text = arg.to_string_lossy();
    text.strip_prefix("--clean=").map(ToOwned::to_owned)
}

fn is_ignorable_shell_argument(arg: &OsString) -> bool {
    matches!(arg.to_string_lossy().as_ref(), "/dde" | "-Embedding" | "--")
}

fn pick_image_path(args: Vec<PathBuf>) -> Option<PathBuf> {
    if args.is_empty() {
        return None;
    }

    args.iter()
        .rev()
        .find(|path| path.exists())
        .cloned()
        .or_else(|| args.into_iter().next())
}

fn usage_error(program: &OsString) -> Box<dyn Error> {
    let program = Path::new(program)
        .file_name()
        .unwrap_or(program.as_os_str())
        .to_string_lossy();
    Box::new(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("Usage: {program} [--config <path>] [--clean system] [path]"),
    ))
}
