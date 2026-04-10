use std::path::PathBuf;
use wml2viewer::bench::benchmark_browser_scan;
use wml2viewer::options::NavigationSortOption;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .map(PathBuf::from)
        .expect("usage: cargo run --example bench_browser -- <path> [iterations]");
    let iterations = args
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(3);

    let result = benchmark_browser_scan(&path, iterations, NavigationSortOption::OsName)
        .expect("browser benchmark failed");
    println!(
        "{} iterations={} total_ms={} avg_ms={}",
        result.name,
        result.iterations,
        result.total.as_millis(),
        result.average.as_millis()
    );
}
