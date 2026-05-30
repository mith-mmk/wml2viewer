use std::fs::File;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use crate::benchlog::log_global_bench_event;
use oxiarc_archive::LzhReader;

use super::{compare_natural_str, is_supported_image};

#[derive(Clone, Debug)]
pub(crate) struct LhaEntryRecord {
    pub index: usize,
    pub name: String,
    pub size: u64,
}

pub(crate) fn load_lha_entries(path: &Path) -> Option<Vec<LhaEntryRecord>> {
    let mut entries = load_lha_entries_unsorted(path)?;
    sort_lha_entries(&mut entries);
    Some(entries)
}

pub(crate) fn load_lha_entries_unsorted(path: &Path) -> Option<Vec<LhaEntryRecord>> {
    let file = File::open(path).ok()?;
    let reader = LzhReader::new(file).ok()?;
    let mut entries = Vec::new();
    for (index, entry) in reader.entries().iter().enumerate() {
        let normalized = normalize_lha_name(&entry.name);
        if normalized.is_empty() {
            continue;
        }
        let entry_path = PathBuf::from(&normalized);
        if !is_supported_image(&entry_path) {
            continue;
        }
        entries.push(LhaEntryRecord {
            index,
            name: normalized,
            size: entry.size,
        });
    }
    Some(entries)
}

pub(crate) fn sort_lha_entries(entries: &mut [LhaEntryRecord]) {
    entries.sort_by(|left, right| compare_natural_str(&left.name, &right.name, false));
}

pub(crate) fn load_lha_entry_bytes(path: &Path, entry_index: usize) -> Option<Vec<u8>> {
    match load_lha_entry_bytes_delharc(path, entry_index) {
        Ok(bytes) => Some(bytes),
        Err(primary_message) => match load_lha_entry_bytes_result(path, entry_index) {
            Ok(bytes) => Some(bytes),
            Err(fallback_message) => {
                let message = format!(
                    "primary extractor failed: {primary_message}; fallback extractor failed: {fallback_message}"
                );
                log_global_bench_event(
                    "filesystem.lha.extract_failed",
                    serde_json::json!({
                        "archive_path": path.display().to_string(),
                        "entry_index": entry_index,
                        "message": message,
                    }),
                );
                None
            }
        },
    }
}

fn load_lha_entry_bytes_delharc(path: &Path, entry_index: usize) -> Result<Vec<u8>, String> {
    let mut reader = delharc::parse_file(path).map_err(|err| err.to_string())?;
    let mut current_index = 0usize;
    loop {
        if current_index == entry_index {
            let entry_name = reader.header().parse_pathname().display().to_string();
            if reader.header().is_directory() {
                return Err(format!("{entry_name}: directory entries cannot be decoded"));
            }
            if !reader.is_decoder_supported() {
                return Err(format!("{entry_name}: compression method is not supported"));
            }

            let mut bytes = Vec::with_capacity(reader.header().original_size as usize);
            let result = catch_unwind(AssertUnwindSafe(|| std::io::copy(&mut reader, &mut bytes)));
            match result {
                Ok(Ok(_)) => {}
                Ok(Err(err)) => return Err(format!("{entry_name}: {err}")),
                Err(_) => return Err(format!("{entry_name}: fallback extractor panicked")),
            }

            if let Err(err) = reader.crc_check() {
                log_global_bench_event(
                    "filesystem.lha.fallback_crc_failed",
                    serde_json::json!({
                        "archive_path": path.display().to_string(),
                        "entry_index": entry_index,
                        "entry_name": entry_name,
                        "message": err.to_string(),
                    }),
                );
            }
            return Ok(bytes);
        }

        if !reader.next_file().map_err(|err| err.to_string())? {
            break;
        }
        current_index += 1;
    }

    Err(format!("entry index {entry_index} not found"))
}

fn load_lha_entry_bytes_result(path: &Path, entry_index: usize) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|err| err.to_string())?;
    let mut reader = LzhReader::new(file)
        .map_err(|err| err.to_string())?
        .lenient(true);
    let entry = reader
        .entries()
        .get(entry_index)
        .cloned()
        .ok_or_else(|| format!("entry index {entry_index} not found"))?;
    let mut buf = Vec::new();
    let result = catch_unwind(AssertUnwindSafe(|| reader.extract(&entry, &mut buf)));
    match result {
        Ok(Ok(_)) => {}
        Ok(Err(err)) => return Err(format!("{}: {err}", entry.name)),
        Err(_) => return Err(format!("{}: extractor panicked", entry.name)),
    }
    Ok(buf)
}

pub(crate) fn lha_entry_record(path: &Path, entry_index: usize) -> Option<LhaEntryRecord> {
    load_lha_entries(path)?
        .into_iter()
        .find(|entry| entry.index == entry_index)
}

fn normalize_lha_name(name: &str) -> String {
    name.replace('\\', "/").trim_start_matches('/').to_string()
}
