use crate::drawers::image::{load_canvas_from_bytes_with_hint, load_canvas_from_file};
use crate::filesystem;
use crate::filesystem::{
    BrowserNameSortMode, BrowserScanOptions, BrowserSortField, FilesystemCache, OpenedImageSource,
    benchmark_browser_scan_cached, ensure_local_archive_cache, is_browser_container,
    list_browser_entries, open_image_source, resolve_start_path, zip_archive_policy_debug,
};
use crate::options::{ArchiveBrowseOption, NavigationSortOption, ZipWorkaroundOptions};
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
    Direct,
    OnlineCache,
    TempCopy,
}

#[derive(Clone, Debug)]
pub struct ArchiveBenchmarkResult {
    pub method: ArchiveBenchmarkMethod,
    pub images: usize,
    pub access_kind: Option<&'static str>,
    pub is_network_path: bool,
    pub exceeds_size_threshold: bool,
    pub sampled_supported_entries: usize,
    pub stored_entries: usize,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    pub prefers_direct: bool,
    pub metadata_scan: Duration,
    pub metadata_sort: Duration,
    pub archive_read: Duration,
    pub decode_total: Duration,
    pub total: Duration,
    pub average_decode: Duration,
}

#[derive(Clone, Debug)]
pub struct FilerBenchmarkResult {
    pub iterations: usize,
    pub entry_count: usize,
    pub filtered_count: usize,
    pub listing: Duration,
    pub preview_filter: Duration,
    pub metadata: Duration,
    pub finalize: Duration,
    pub total: Duration,
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

pub fn benchmark_filer_scan(
    path: &Path,
    iterations: usize,
    sort: NavigationSortOption,
    archive_mode: ArchiveBrowseOption,
    sort_field: BrowserSortField,
    include_metadata: bool,
) -> Result<FilerBenchmarkResult, String> {
    let iterations = iterations.max(1);
    let mut total_listing = Duration::ZERO;
    let mut total_preview_filter = Duration::ZERO;
    let mut total_metadata = Duration::ZERO;
    let mut total_finalize = Duration::ZERO;
    let mut total_total = Duration::ZERO;
    let mut entry_count = 0usize;
    let mut filtered_count = 0usize;

    for _ in 0..iterations {
        let mut cache = FilesystemCache::new(sort, archive_mode);
        let result = benchmark_browser_scan_cached(
            path,
            &BrowserScanOptions {
                navigation_sort: sort,
                archive_mode,
                sort_field,
                include_metadata,
                ascending: true,
                separate_dirs: true,
                archive_as_container_in_sort: false,
                filter_text: String::new(),
                extension_filter: String::new(),
                name_sort_mode: BrowserNameSortMode::Os,
                thumbnail_hint_count: 0,
                thumbnail_hint_max_side: 0,
            },
            &mut cache,
        );
        total_listing += result.listing;
        total_preview_filter += result.preview_filter;
        total_metadata += result.metadata;
        total_finalize += result.finalize;
        total_total += result.total;
        entry_count = result.entry_count;
        filtered_count = result.filtered_count;
    }

    Ok(FilerBenchmarkResult {
        iterations,
        entry_count,
        filtered_count,
        listing: total_listing / iterations as u32,
        preview_filter: total_preview_filter / iterations as u32,
        metadata: total_metadata / iterations as u32,
        finalize: total_finalize / iterations as u32,
        total: total_total / iterations as u32,
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
        match open_image_source(first_entry) {
            Some(OpenedImageSource::Bytes { bytes, .. }) => {
                let _ = bytes.len();
            }
            Some(OpenedImageSource::File { path, .. }) => {
                if !path.exists() {
                    return Err("failed to read archive entry".to_string());
                }
            }
            None => {
                let load_path = resolve_start_path(first_entry)
                    .ok_or_else(|| "failed to resolve start path".to_string())?;
                if !load_path.exists() {
                    return Err("failed to read archive entry".to_string());
                }
            }
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

    let benchmark_path = match method {
        ArchiveBenchmarkMethod::TempCopy => ensure_local_archive_cache(path)
            .ok_or_else(|| "failed to prepare local archive cache".to_string())?,
        _ => path.to_path_buf(),
    };

    let workaround = match method {
        ArchiveBenchmarkMethod::Default => ZipWorkaroundOptions::default(),
        ArchiveBenchmarkMethod::Direct => ZipWorkaroundOptions {
            threshold_mb: u64::MAX / (1024 * 1024),
            local_cache: false,
        },
        ArchiveBenchmarkMethod::OnlineCache => ZipWorkaroundOptions {
            threshold_mb: 1,
            local_cache: false,
        },
        ArchiveBenchmarkMethod::TempCopy => ZipWorkaroundOptions {
            threshold_mb: u64::MAX / (1024 * 1024),
            local_cache: false,
        },
    };
    filesystem::set_archive_zip_workaround(workaround);
    let policy = zip_archive_policy_debug(&benchmark_path);

    let started_total = Instant::now();
    let started_scan = Instant::now();
    let mut entries = filesystem::load_zip_entries_unsorted(&benchmark_path)
        .ok_or_else(|| "failed to load archive metadata".to_string())?;
    let metadata_scan = started_scan.elapsed();

    let started_sort = Instant::now();
    filesystem::sort_zip_entries(&mut entries);
    let metadata_sort = started_sort.elapsed();

    let images = entries.len();
    if images == 0 {
        return Err("no readable archive entries".to_string());
    }

    let browser_entries = list_browser_entries(&benchmark_path, NavigationSortOption::OsName);
    let first_entry = browser_entries
        .first()
        .ok_or_else(|| "failed to list archive entries".to_string())?;

    let started_read = Instant::now();
    match open_image_source(first_entry) {
        Some(OpenedImageSource::Bytes { bytes, .. }) => {
            let _ = bytes.len();
        }
        Some(OpenedImageSource::File { path, .. }) => {
            if !path.exists() {
                return Err("failed to read first archive entry".to_string());
            }
        }
        None => {
            let load_path = resolve_start_path(first_entry)
                .ok_or_else(|| "failed to resolve first archive entry".to_string())?;
            if !load_path.exists() {
                return Err("failed to read first archive entry".to_string());
            }
        }
    }
    let archive_read = started_read.elapsed();

    let started_decode = Instant::now();
    for entry in &browser_entries {
        match open_image_source(entry) {
            Some(OpenedImageSource::Bytes {
                bytes, hint_path, ..
            }) => {
                let _ = load_canvas_from_bytes_with_hint(&bytes, Some(&hint_path));
            }
            Some(OpenedImageSource::File { path, .. }) => {
                let _ = load_canvas_from_file(&path);
            }
            None => {
                let Some(load_path) = resolve_start_path(entry) else {
                    continue;
                };
                let _ = load_canvas_from_file(&load_path);
            }
        }
    }
    let decode_total = started_decode.elapsed();
    let total = started_total.elapsed();

    Ok(ArchiveBenchmarkResult {
        method,
        images,
        access_kind: policy.as_ref().map(|policy| match policy.access_kind {
            crate::filesystem::ZipArchiveAccessKind::DirectOriginal => "direct-original",
            crate::filesystem::ZipArchiveAccessKind::Sequential => "sequential",
        }),
        is_network_path: policy
            .as_ref()
            .map(|policy| policy.is_network_path)
            .unwrap_or(false),
        exceeds_size_threshold: policy
            .as_ref()
            .map(|policy| policy.exceeds_size_threshold)
            .unwrap_or(false),
        sampled_supported_entries: policy
            .as_ref()
            .map(|policy| policy.sampled_supported_entries)
            .unwrap_or(0),
        stored_entries: policy
            .as_ref()
            .map(|policy| policy.stored_entries)
            .unwrap_or(0),
        compressed_bytes: policy
            .as_ref()
            .map(|policy| policy.compressed_bytes)
            .unwrap_or(0),
        uncompressed_bytes: policy
            .as_ref()
            .map(|policy| policy.uncompressed_bytes)
            .unwrap_or(0),
        prefers_direct: policy
            .as_ref()
            .map(|policy| policy.prefers_direct)
            .unwrap_or(false),
        metadata_scan,
        metadata_sort,
        archive_read,
        decode_total,
        total,
        average_decode: decode_total / images as u32,
    })
}
