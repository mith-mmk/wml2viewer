use crate::drawers::image::{load_canvas_from_bytes_with_hint, load_canvas_from_file};
use crate::filesystem::{
    is_browser_container, list_browser_entries, load_virtual_image_bytes, resolve_start_path,
};
use crate::options::{NavigationSortOption, ZipWorkaroundOptions};
use crate::filesystem;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct BenchResult {
    pub name: &'static str,
    pub iterations: usize,
    pub total: Duration,
    pub average: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveBenchmarkMethod {
    Default,
    OnlineCache,
    TempCopy,
}

#[derive(Clone, Debug)]
pub struct ArchiveBenchmarkResult {
    pub method: ArchiveBenchmarkMethod,
    pub images: usize,
    pub metadata_scan: Duration,
    pub metadata_sort: Duration,
    pub archive_read: Duration,
    pub decode_total: Duration,
    pub total: Duration,
    pub average_decode: Duration,
}

pub fn benchmark_decode(path: &Path, iterations: usize) -> Result<BenchResult, String> {
    let iterations = iterations.max(1);
    let started = Instant::now();
    for _ in 0..iterations {
        load_canvas_from_file(path).map_err(|err| err.to_string())?;
    }
    let total = started.elapsed();
    Ok(BenchResult {
        name: "decode",
        iterations,
        average: total / iterations as u32,
        total,
    })
}

pub fn benchmark_browser_scan(
    path: &Path,
    iterations: usize,
    sort: NavigationSortOption,
) -> Result<BenchResult, String> {
    let iterations = iterations.max(1);
    let started = Instant::now();
    for _ in 0..iterations {
        let _entries = list_browser_entries(path, sort);
    }
    let total = started.elapsed();
    Ok(BenchResult {
        name: "browser-scan",
        iterations,
        average: total / iterations as u32,
        total,
    })
}

pub fn benchmark_archive_read(path: &Path, iterations: usize) -> Result<BenchResult, String> {
    let iterations = iterations.max(1);
    let entries = list_browser_entries(path, NavigationSortOption::OsName);
    let Some(first_entry) = entries.first() else {
        return Err("no readable archive entries".to_string());
    };

    let started = Instant::now();
    for _ in 0..iterations {
        let load_path = resolve_start_path(first_entry)
            .ok_or_else(|| "failed to resolve start path".to_string())?;
        if let Some(bytes) = load_virtual_image_bytes(first_entry) {
            let _ = bytes.len();
        } else if !load_path.exists() {
            return Err("failed to read archive entry".to_string());
        }
    }
    let total = started.elapsed();
    Ok(BenchResult {
        name: "archive-read",
        iterations,
        average: total / iterations as u32,
        total,
    })
}

pub fn benchmark_archive_detailed(
    path: &Path,
    method: ArchiveBenchmarkMethod,
) -> Result<ArchiveBenchmarkResult, String> {
    if !is_browser_container(path) {
        return Err("archive benchmark expects a container path".to_string());
    }
    if path.extension().and_then(|ext| ext.to_str()) != Some("zip") {
        return Err("archive benchmark currently supports zip archives only".to_string());
    }

    let workaround = match method {
        ArchiveBenchmarkMethod::Default => ZipWorkaroundOptions {
            threshold_mb: u64::MAX / (1024 * 1024),
            local_cache: false,
        },
        ArchiveBenchmarkMethod::OnlineCache => ZipWorkaroundOptions {
            threshold_mb: 1,
            local_cache: false,
        },
        ArchiveBenchmarkMethod::TempCopy => ZipWorkaroundOptions {
            threshold_mb: 1,
            local_cache: true,
        },
    };
    filesystem::set_archive_zip_workaround(workaround);

    let started_total = Instant::now();
    let started_scan = Instant::now();
    let mut entries = filesystem::load_zip_entries_unsorted(path)
        .ok_or_else(|| "failed to load archive metadata".to_string())?;
    let metadata_scan = started_scan.elapsed();

    let started_sort = Instant::now();
    filesystem::sort_zip_entries(&mut entries);
    let metadata_sort = started_sort.elapsed();

    let images = entries.len();
    if images == 0 {
        return Err("no readable archive entries".to_string());
    }

    let browser_entries = list_browser_entries(path, NavigationSortOption::OsName);
    let first_entry = browser_entries
        .first()
        .ok_or_else(|| "failed to list archive entries".to_string())?;

    let started_read = Instant::now();
    let load_path = resolve_start_path(first_entry)
        .ok_or_else(|| "failed to resolve first archive entry".to_string())?;
    if let Some(bytes) = load_virtual_image_bytes(first_entry) {
        let _ = bytes.len();
    } else if !load_path.exists() {
        return Err("failed to read first archive entry".to_string());
    }
    let archive_read = started_read.elapsed();

    let started_decode = Instant::now();
    for entry in &browser_entries {
        let Some(load_path) = resolve_start_path(entry) else {
            continue;
        };
        if let Some(bytes) = load_virtual_image_bytes(entry) {
            let _ = load_canvas_from_bytes_with_hint(&bytes, Some(&load_path));
        } else {
            let _ = load_canvas_from_file(&load_path);
        }
    }
    let decode_total = started_decode.elapsed();
    let total = started_total.elapsed();

    Ok(ArchiveBenchmarkResult {
        method,
        images,
        metadata_scan,
        metadata_sort,
        archive_read,
        decode_total,
        total,
        average_decode: decode_total / images as u32,
    })
}
