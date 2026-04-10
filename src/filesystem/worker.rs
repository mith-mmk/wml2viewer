use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::options::{ArchiveBrowseOption, EndOfFolderOption, NavigationSortOption};

use super::browser::{SharedBrowserWorkerState, preload_browser_directory_for_path};
use super::cache::{FilesystemCache, SharedFilesystemCache};
use super::navigator::{
    FileNavigator, NavigationOutcome, NavigationTarget, PendingDirection, resolve_navigation_path,
};
use super::protocol::{FilesystemCommand, FilesystemResult};
use super::source::{SourceInputResolution, resolve_source_input_path_with_cancel};

pub(crate) fn spawn_filesystem_worker(
    sort: NavigationSortOption,
    archive_mode: ArchiveBrowseOption,
    shared_cache: SharedFilesystemCache,
    shared_browser_state: SharedBrowserWorkerState,
) -> (Sender<FilesystemCommand>, Receiver<FilesystemResult>) {
    let (command_tx, command_rx) = mpsc::channel::<FilesystemCommand>();
    let (result_tx, result_rx) = mpsc::channel::<FilesystemResult>();
    let latest_source_input_request_id = Arc::new(AtomicU64::new(0));

    thread::spawn(move || {
        let mut navigator: Option<FileNavigator> = None;
        let mut active_source_input: Option<(u64, std::path::PathBuf, Arc<AtomicBool>)> = None;

        while let Ok(command) = command_rx.recv() {
            if let FilesystemCommand::ResolveSourceInput { request_id, input } = command {
                latest_source_input_request_id.store(request_id, Ordering::Release);
                if let Some((_, _, cancel)) = active_source_input.take() {
                    cancel.store(true, Ordering::Release);
                }
                let cancel = Arc::new(AtomicBool::new(false));
                active_source_input = Some((request_id, input.clone(), Arc::clone(&cancel)));
                spawn_source_input_resolver(
                    result_tx.clone(),
                    Arc::clone(&latest_source_input_request_id),
                    request_id,
                    input,
                    cancel,
                );
                continue;
            }
            if let FilesystemCommand::CancelSourceInput { request_id } = command {
                if let Some((active_request_id, input, cancel)) = active_source_input.as_ref() {
                    if *active_request_id == request_id {
                        latest_source_input_request_id.store(0, Ordering::Release);
                        cancel.store(true, Ordering::Release);
                        let _ = result_tx.send(FilesystemResult::InputPathCancelled {
                            request_id,
                            input: input.clone(),
                        });
                    }
                }
                continue;
            }

            let Ok(mut cache) = shared_cache.lock() else {
                break;
            };
            cache.ensure_settings(sort, archive_mode);
            match command {
                FilesystemCommand::Init { request_id, path } => {
                    let Some(start_path) = resolve_navigation_path(&path, &mut cache) else {
                        let _ = result_tx.send(FilesystemResult::NoPath { request_id });
                        continue;
                    };

                    navigator = Some(FileNavigator::from_current_path(start_path, &mut cache));
                    if let Some(nav) = navigator.as_ref() {
                        preload_browser_directory_for_path(
                            &shared_browser_state,
                            nav.current(),
                            sort,
                            archive_mode,
                            &mut cache,
                        );
                    }
                    let initial_target = navigator
                        .as_ref()
                        .and_then(|nav| navigation_outcome_to_target(nav.current_target()));
                    let _ = result_tx.send(FilesystemResult::NavigatorReady {
                        request_id,
                        navigation_path: initial_target
                            .as_ref()
                            .map(|target| target.navigation_path.clone()),
                        load_path: initial_target.map(|target| target.load_path),
                    });
                }
                FilesystemCommand::SetCurrent { request_id, path } => {
                    if let Some(nav) = navigator.as_mut() {
                        nav.set_current_input(path, &mut cache);
                        preload_browser_directory_for_path(
                            &shared_browser_state,
                            nav.current(),
                            sort,
                            archive_mode,
                            &mut cache,
                        );
                    } else if let Some(start_path) = resolve_navigation_path(&path, &mut cache) {
                        navigator = Some(FileNavigator::from_current_path(start_path, &mut cache));
                        if let Some(nav) = navigator.as_ref() {
                            preload_browser_directory_for_path(
                                &shared_browser_state,
                                nav.current(),
                                sort,
                                archive_mode,
                                &mut cache,
                            );
                        }
                    }
                    let _ = request_id;
                    let _ = result_tx.send(FilesystemResult::CurrentSet);
                }
                FilesystemCommand::Next { request_id, policy } => {
                    handle_navigation_request(
                        &result_tx,
                        navigator.as_mut(),
                        &mut cache,
                        request_id,
                        policy,
                        PendingDirection::Next,
                    );
                }
                FilesystemCommand::Prev { request_id, policy } => {
                    handle_navigation_request(
                        &result_tx,
                        navigator.as_mut(),
                        &mut cache,
                        request_id,
                        policy,
                        PendingDirection::Prev,
                    );
                }
                FilesystemCommand::First { request_id } => {
                    let outcome = navigator
                        .as_mut()
                        .and_then(|nav| nav.first(&mut cache).map(|_| nav.current_target()))
                        .unwrap_or(NavigationOutcome::NoPath);
                    let _ = send_nav_result(
                        &result_tx,
                        request_id,
                        navigation_outcome_to_target(outcome),
                    );
                }
                FilesystemCommand::Last { request_id } => {
                    let outcome = navigator
                        .as_mut()
                        .and_then(|nav| nav.last(&mut cache).map(|_| nav.current_target()))
                        .unwrap_or(NavigationOutcome::NoPath);
                    let _ = send_nav_result(
                        &result_tx,
                        request_id,
                        navigation_outcome_to_target(outcome),
                    );
                }
                FilesystemCommand::OpenBrowserDirectory { .. } => {}
                FilesystemCommand::ResolveSourceInput { .. }
                | FilesystemCommand::CancelSourceInput { .. } => {}
            }
        }
    });

    (command_tx, result_rx)
}

fn send_nav_result(
    tx: &Sender<FilesystemResult>,
    request_id: u64,
    target: Option<NavigationTarget>,
) -> Result<(), mpsc::SendError<FilesystemResult>> {
    match target {
        Some(target) => tx.send(FilesystemResult::PathResolved {
            request_id,
            navigation_path: target.navigation_path,
            load_path: target.load_path,
        }),
        None => tx.send(FilesystemResult::NoPath { request_id }),
    }
}

fn handle_navigation_request(
    tx: &Sender<FilesystemResult>,
    navigator: Option<&mut FileNavigator>,
    cache: &mut FilesystemCache,
    request_id: u64,
    policy: EndOfFolderOption,
    direction: PendingDirection,
) {
    let outcome = match navigator {
        Some(nav) => match direction {
            PendingDirection::Next => nav.next_with_policy(policy, cache),
            PendingDirection::Prev => nav.prev_with_policy(policy, cache),
        },
        None => NavigationOutcome::NoPath,
    };

    let _ = send_nav_result(tx, request_id, navigation_outcome_to_target(outcome));
}

fn navigation_outcome_to_target(outcome: NavigationOutcome) -> Option<NavigationTarget> {
    match outcome {
        NavigationOutcome::Resolved(target) => Some(target),
        NavigationOutcome::NoPath => None,
    }
}

fn spawn_source_input_resolver(
    result_tx: Sender<FilesystemResult>,
    latest_request_id: Arc<AtomicU64>,
    request_id: u64,
    input: std::path::PathBuf,
    cancel: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let result = match resolve_source_input_path_with_cancel(&input, Some(&cancel)) {
            SourceInputResolution::Resolved(path) => {
                FilesystemResult::InputPathResolved { request_id, path }
            }
            SourceInputResolution::Cancelled => {
                FilesystemResult::InputPathCancelled { request_id, input }
            }
            SourceInputResolution::Failed => {
                FilesystemResult::InputPathFailed { request_id, input }
            }
        };
        if latest_request_id.load(Ordering::Acquire) != request_id || cancel.load(Ordering::Acquire)
        {
            return;
        }
        let _ = result_tx.send(result);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{ArchiveBrowseOption, NavigationSortOption};
    use std::fs;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("wml2viewer-worker-{name}-{unique}.png"))
    }

    #[test]
    fn resolve_source_input_returns_local_path() {
        let path = temp_path("resolve");
        fs::write(&path, b"png").unwrap();

        let (tx, rx) = spawn_filesystem_worker(
            NavigationSortOption::OsName,
            ArchiveBrowseOption::Folder,
            super::super::new_shared_filesystem_cache(
                NavigationSortOption::OsName,
                ArchiveBrowseOption::Folder,
            ),
            super::super::new_shared_browser_worker_state(),
        );

        tx.send(FilesystemCommand::ResolveSourceInput {
            request_id: 7,
            input: path.clone(),
        })
        .unwrap();

        match rx.recv().unwrap() {
            FilesystemResult::InputPathResolved {
                request_id,
                path: resolved,
            } => {
                assert_eq!(request_id, 7);
                assert_eq!(resolved, path);
            }
            other => panic!("unexpected result: {other:?}"),
        }

        let _ = fs::remove_file(path);
    }

    #[test]
    fn stale_source_input_result_is_not_sent() {
        let path = temp_path("stale");
        fs::write(&path, b"png").unwrap();
        let (tx, rx) = mpsc::channel();
        let latest = Arc::new(AtomicU64::new(9));

        spawn_source_input_resolver(
            tx,
            latest,
            8,
            path.clone(),
            Arc::new(AtomicBool::new(false)),
        );

        assert!(
            rx.recv_timeout(std::time::Duration::from_millis(100))
                .is_err()
        );

        let _ = fs::remove_file(path);
    }
}
