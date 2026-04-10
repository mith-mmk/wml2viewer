use std::path::PathBuf;
use std::process::ExitCode;
use wml2viewer::bench::{ArchiveBenchmarkMethod, benchmark_archive_detailed};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!(
            "Error: usage: cargo run --example bench_archive -- <archive-path> [default|direct|online_cache|temp_copy]"
        );
        return ExitCode::FAILURE;
    };
    let method = match args.next().as_deref() {
        Some("direct") => ArchiveBenchmarkMethod::Direct,
        Some("online_cache") => ArchiveBenchmarkMethod::OnlineCache,
        Some("temp_copy") => ArchiveBenchmarkMethod::TempCopy,
        _ => ArchiveBenchmarkMethod::Default,
    };

    let result = match benchmark_archive_detailed(&path, method) {
        Ok(result) => result,
        Err(err) => {
            eprintln!("Error: archive benchmark failed: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "method={:?} access={:?} prefers_direct={} network={} threshold={} sampled={} stored={} compressed_bytes={} uncompressed_bytes={} time_ms={} images={} avg_ms={} metadata_scan_ms={} metadata_sort_ms={} archive_read_ms={} decode_ms={}",
        result.method,
        result.access_kind,
        result.prefers_direct,
        result.is_network_path,
        result.exceeds_size_threshold,
        result.sampled_supported_entries,
        result.stored_entries,
        result.compressed_bytes,
        result.uncompressed_bytes,
        result.total.as_millis(),
        result.images,
        result.average_decode.as_millis(),
        result.metadata_scan.as_millis(),
        result.metadata_sort.as_millis(),
        result.archive_read.as_millis(),
        result.decode_total.as_millis()
    );
    ExitCode::SUCCESS
}
