use std::path::PathBuf;

use wml2viewer::bench::benchmark_filer_scan;
use wml2viewer::filesystem::BrowserSortField;
use wml2viewer::options::{ArchiveBrowseOption, NavigationSortOption};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .map(PathBuf::from)
        .expect("usage: cargo run --example bench_filer -- <path> [iterations] [name|modified|size] [folder|skip|archiver]");
    let iterations = args
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3);
    let sort_field = match args.next().as_deref() {
        Some("modified") => BrowserSortField::Modified,
        Some("size") => BrowserSortField::Size,
        _ => BrowserSortField::Name,
    };
    let archive_mode = match args.next().as_deref() {
        Some("skip") => ArchiveBrowseOption::Skip,
        Some("archiver") => ArchiveBrowseOption::Archiver,
        _ => ArchiveBrowseOption::Folder,
    };

    let result = benchmark_filer_scan(
        &path,
        iterations,
        NavigationSortOption::OsName,
        archive_mode,
        sort_field,
        sort_field != BrowserSortField::Name,
    )
    .expect("filer benchmark failed");

    println!(
        "filer iterations={} entries={} filtered={} listing_ms={} preview_ms={} metadata_ms={} finalize_ms={} total_ms={}",
        result.iterations,
        result.entry_count,
        result.filtered_count,
        result.listing.as_millis(),
        result.preview_filter.as_millis(),
        result.metadata.as_millis(),
        result.finalize.as_millis(),
        result.total.as_millis()
    );
}
