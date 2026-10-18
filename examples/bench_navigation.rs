use std::path::PathBuf;
use std::time::{Duration, Instant};
use wml2viewer::filesystem::{
    FilesystemCommand, FilesystemResult, RecursiveOrder, spawn_filesystem_worker,
};
use wml2viewer::options::{EndOfFolderOption, NavigationSortOption};

fn main() {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: bench_navigation <image>");
    let (tx, rx) = spawn_filesystem_worker(NavigationSortOption::OsName, RecursiveOrder::default());
    let start = Instant::now();
    tx.send(FilesystemCommand::Init {
        request_id: 1,
        path,
    })
    .unwrap();
    loop {
        match rx
            .recv_timeout(Duration::from_secs(120))
            .expect("initialization timed out")
        {
            FilesystemResult::NavigatorReady { .. } => println!(
                "init_ready_ms={:.3}",
                start.elapsed().as_secs_f64() * 1000.0
            ),
            FilesystemResult::NeighborsReady(_) => break,
            _ => {}
        }
    }
    println!(
        "init_neighbors_ms={:.3}",
        start.elapsed().as_secs_f64() * 1000.0
    );
    for request_id in 2..12 {
        let start = Instant::now();
        let command = if request_id % 2 == 0 {
            FilesystemCommand::Next {
                request_id,
                policy: EndOfFolderOption::Recursive,
            }
        } else {
            FilesystemCommand::Prev {
                request_id,
                policy: EndOfFolderOption::Recursive,
            }
        };
        tx.send(command).unwrap();
        loop {
            match rx
                .recv_timeout(Duration::from_secs(120))
                .expect("navigation timed out")
            {
                FilesystemResult::PathResolved { .. } => println!(
                    "navigation_{}_ms={:.3}",
                    request_id - 1,
                    start.elapsed().as_secs_f64() * 1000.0
                ),
                FilesystemResult::NeighborsReady(_) => break,
                FilesystemResult::NoPath { .. } => println!("no_path"),
                _ => {}
            }
        }
    }
}
