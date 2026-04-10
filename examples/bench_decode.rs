use std::path::PathBuf;
use wml2viewer::bench::benchmark_decode;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .map(PathBuf::from)
        .expect("usage: cargo run --example bench_decode -- <path> [iterations]");
    let iterations = args
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(5);

    let result = benchmark_decode(&path, iterations).expect("decode benchmark failed");
    println!(
        "{} iterations={} total_ms={} avg_ms={}",
        result.name,
        result.iterations,
        result.total.as_millis(),
        result.average.as_millis()
    );
}
