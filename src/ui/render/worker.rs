use crate::drawers::affine::InterpolationAlgorithm;
use crate::drawers::canvas::Canvas;
use crate::drawers::image::{
    LoadedImage, load_canvas_from_bytes_with_hint, load_canvas_from_file, resize_loaded_image,
};
use crate::filesystem::{OpenedImageSource, open_image_source_with_cancel, resolve_start_path};
use crate::ui::viewer::options::RenderScaleMode;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};

pub(crate) enum RenderCommand {
    LoadPath {
        request_id: u64,
        path: PathBuf,
        zoom: f32,
        method: InterpolationAlgorithm,
        scale_mode: RenderScaleMode,
    },
    LoadSpread {
        request_id: u64,
        path: PathBuf,
        companion_path: PathBuf,
        zoom: f32,
        method: InterpolationAlgorithm,
        scale_mode: RenderScaleMode,
    },
    ResizeCurrent {
        request_id: u64,
        zoom: f32,
        method: InterpolationAlgorithm,
        scale_mode: RenderScaleMode,
    },
    Shutdown,
}

pub(crate) enum RenderResult {
    Loaded {
        request_id: u64,
        path: Option<PathBuf>,
        source: LoadedImage,
        rendered: LoadedImage,
    },
    LoadedSpread {
        request_id: u64,
        path: PathBuf,
        source: LoadedImage,
        rendered: LoadedImage,
        companion: Option<(PathBuf, LoadedImage, LoadedImage)>,
    },
    Failed {
        request_id: u64,
        path: Option<PathBuf>,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveRenderRequest {
    Load(u64),
    Resize(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderWorkerPriority {
    Primary,
    Companion,
    Preload,
}

struct RenderIoCoordinator {
    primary_epoch: AtomicU64,
    primary_active: AtomicU64,
    high_priority_epoch: AtomicU64,
    companion_active: AtomicU64,
}

fn render_io_coordinator() -> &'static RenderIoCoordinator {
    static COORDINATOR: OnceLock<RenderIoCoordinator> = OnceLock::new();
    COORDINATOR.get_or_init(|| RenderIoCoordinator {
        primary_epoch: AtomicU64::new(0),
        primary_active: AtomicU64::new(0),
        high_priority_epoch: AtomicU64::new(0),
        companion_active: AtomicU64::new(0),
    })
}

pub(crate) fn snapshot_primary_io_epoch() -> u64 {
    render_io_coordinator()
        .high_priority_epoch
        .load(Ordering::Acquire)
}

pub(crate) fn should_cancel_low_priority_io(primary_epoch_snapshot: u64) -> bool {
    let coordinator = render_io_coordinator();
    coordinator.primary_active.load(Ordering::Acquire) > 0
        || coordinator.companion_active.load(Ordering::Acquire) > 0
        || coordinator.high_priority_epoch.load(Ordering::Acquire) != primary_epoch_snapshot
}

fn should_abort_background_load(
    priority: RenderWorkerPriority,
    request_id: u64,
    latest_load_request_id: &AtomicU64,
    primary_epoch_snapshot: u64,
    coordinator: &RenderIoCoordinator,
) -> bool {
    if latest_load_request_id.load(Ordering::Acquire) != request_id {
        return true;
    }
    match priority {
        RenderWorkerPriority::Primary => false,
        RenderWorkerPriority::Companion => {
            coordinator.primary_active.load(Ordering::Acquire) > 0
                || coordinator.primary_epoch.load(Ordering::Acquire) != primary_epoch_snapshot
        }
        RenderWorkerPriority::Preload => {
            coordinator.primary_active.load(Ordering::Acquire) > 0
                || coordinator.companion_active.load(Ordering::Acquire) > 0
                || coordinator.high_priority_epoch.load(Ordering::Acquire) != primary_epoch_snapshot
        }
    }
}

pub(crate) struct LowIoPermit;

pub(crate) fn acquire_low_io_permit<F: Fn() -> bool>(
    _priority: RenderWorkerPriority,
    should_cancel: &F,
) -> Option<LowIoPermit> {
    (!should_cancel()).then_some(LowIoPermit)
}

fn blank_loaded_image() -> LoadedImage {
    LoadedImage {
        canvas: Canvas::new(1, 1),
        animation: Vec::new(),
        loop_count: None,
    }
}

fn should_load_spread_companion(source: &LoadedImage, companion_path: Option<&PathBuf>) -> bool {
    companion_path.is_some() && source.canvas.height() >= source.canvas.width()
}

fn load_rendered_image<F: Fn() -> bool>(
    path: &PathBuf,
    zoom: f32,
    method: InterpolationAlgorithm,
    scale_mode: RenderScaleMode,
    should_cancel: &F,
) -> Result<Option<(LoadedImage, LoadedImage, PathBuf)>, Box<dyn Error>> {
    if should_cancel() {
        return Ok(None);
    }

    let load_path = resolve_start_path(path).unwrap_or(path.clone());
    if should_cancel() {
        return Ok(None);
    }

    let source = match open_image_source_with_cancel(&load_path, should_cancel) {
        Some(OpenedImageSource::Bytes {
            bytes, hint_path, ..
        }) => load_canvas_from_bytes_with_hint(&bytes, Some(&hint_path))?,
        Some(OpenedImageSource::File { path, .. }) => load_canvas_from_file(&path)?,
        None => load_canvas_from_file(&load_path)?,
    };
    if should_cancel() {
        return Ok(None);
    }

    let rendered = match scale_mode {
        RenderScaleMode::FastGpu => source.clone(),
        RenderScaleMode::PreciseCpu => resize_loaded_image(&source, zoom, method)?,
    };
    Ok(Some((source, rendered, load_path)))
}

pub(crate) fn spawn_render_worker(
    initial_source: LoadedImage,
    priority: RenderWorkerPriority,
) -> (
    Sender<RenderCommand>,
    Receiver<RenderResult>,
    JoinHandle<()>,
) {
    let (command_tx, command_rx) = mpsc::channel::<RenderCommand>();
    let (result_tx, result_rx) = mpsc::channel::<RenderResult>();
    let current_source = Arc::new(Mutex::new(initial_source));
    let latest_load_request_id = Arc::new(AtomicU64::new(0));

    let join = thread::spawn(move || {
        while let Ok(command) = command_rx.recv() {
            let spread_companion_path = match &command {
                RenderCommand::LoadSpread { companion_path, .. } => Some(companion_path.clone()),
                _ => None,
            };
            let is_spread = spread_companion_path.is_some();
            match command {
                RenderCommand::LoadPath {
                    request_id,
                    path,
                    zoom,
                    method,
                    scale_mode,
                }
                | RenderCommand::LoadSpread {
                    request_id,
                    path,
                    companion_path: _,
                    zoom,
                    method,
                    scale_mode,
                } => {
                    latest_load_request_id.store(request_id, Ordering::Release);
                    let result_tx = result_tx.clone();
                    let current_source = Arc::clone(&current_source);
                    let latest_load_request_id = Arc::clone(&latest_load_request_id);
                    let coordinator = render_io_coordinator();
                    let priority_epoch_snapshot = match priority {
                        RenderWorkerPriority::Primary => {
                            coordinator.primary_active.fetch_add(1, Ordering::AcqRel);
                            coordinator
                                .high_priority_epoch
                                .fetch_add(1, Ordering::AcqRel);
                            coordinator.primary_epoch.fetch_add(1, Ordering::AcqRel) + 1
                        }
                        RenderWorkerPriority::Companion => {
                            coordinator.companion_active.fetch_add(1, Ordering::AcqRel);
                            coordinator
                                .high_priority_epoch
                                .fetch_add(1, Ordering::AcqRel);
                            coordinator.primary_epoch.load(Ordering::Acquire)
                        }
                        RenderWorkerPriority::Preload => {
                            coordinator.high_priority_epoch.load(Ordering::Acquire)
                        }
                    };
                    thread::spawn(move || {
                        struct PriorityLoadGuard<'a> {
                            primary_active: &'a AtomicU64,
                            companion_active: &'a AtomicU64,
                            priority: RenderWorkerPriority,
                        }
                        impl Drop for PriorityLoadGuard<'_> {
                            fn drop(&mut self) {
                                match self.priority {
                                    RenderWorkerPriority::Primary => {
                                        self.primary_active.fetch_sub(1, Ordering::AcqRel);
                                    }
                                    RenderWorkerPriority::Companion => {
                                        self.companion_active.fetch_sub(1, Ordering::AcqRel);
                                    }
                                    RenderWorkerPriority::Preload => {}
                                }
                            }
                        }

                        let _priority_guard = PriorityLoadGuard {
                            primary_active: &coordinator.primary_active,
                            companion_active: &coordinator.companion_active,
                            priority,
                        };
                        let should_cancel = || {
                            should_abort_background_load(
                                priority,
                                request_id,
                                &latest_load_request_id,
                                priority_epoch_snapshot,
                                coordinator,
                            )
                        };
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            (|| -> Result<
                                Option<(
                                    LoadedImage,
                                    LoadedImage,
                                    PathBuf,
                                    Option<(PathBuf, LoadedImage, LoadedImage)>,
                                )>,
                                Box<dyn Error>,
                            > {
                                let Some((source, rendered, load_path)) = load_rendered_image(
                                    &path,
                                    zoom,
                                    method,
                                    scale_mode,
                                    &should_cancel,
                                )? else {
                                    return Ok(None);
                                };

                                let companion = match &spread_companion_path {
                                    Some(companion_path)
                                        if should_load_spread_companion(
                                            &source,
                                            Some(companion_path),
                                        ) =>
                                    {
                                        load_rendered_image(
                                            companion_path,
                                            zoom,
                                            method,
                                            scale_mode,
                                            &should_cancel,
                                        )?
                                        .map(|(source, rendered, path)| (path, source, rendered))
                                    }
                                    _ => None,
                                };

                                Ok(Some((source, rendered, load_path, companion)))
                            })()
                        }))
                        .unwrap_or_else(|_| {
                            Err(Box::new(std::io::Error::other(
                                "decoder panicked while loading image",
                            )))
                        });

                        match result {
                            Ok(Some((source, rendered, load_path, companion))) => {
                                if should_cancel() {
                                    return;
                                }
                                if let Ok(mut current) = current_source.lock() {
                                    *current = source.clone();
                                }
                                if is_spread {
                                    let _ = result_tx.send(RenderResult::LoadedSpread {
                                        request_id,
                                        path: load_path,
                                        source,
                                        rendered,
                                        companion,
                                    });
                                } else {
                                    let _ = result_tx.send(RenderResult::Loaded {
                                        request_id,
                                        path: Some(load_path),
                                        source,
                                        rendered,
                                    });
                                }
                            }
                            Ok(None) => {}
                            Err(err) => {
                                if !should_cancel() {
                                    let _ = result_tx.send(RenderResult::Failed {
                                        request_id,
                                        path: Some(path),
                                        message: err.to_string(),
                                    });
                                }
                            }
                        }
                    });
                }
                RenderCommand::ResizeCurrent {
                    request_id,
                    zoom,
                    method,
                    scale_mode,
                } => {
                    let result_tx = result_tx.clone();
                    let current_source = Arc::clone(&current_source);
                    thread::spawn(move || {
                        let source_snapshot = current_source
                            .lock()
                            .map(|current| current.clone())
                            .unwrap_or_else(|_| blank_loaded_image());
                        match catch_unwind(AssertUnwindSafe(|| match scale_mode {
                            RenderScaleMode::FastGpu => Ok(source_snapshot.clone()),
                            RenderScaleMode::PreciseCpu => {
                                resize_loaded_image(&source_snapshot, zoom, method)
                            }
                        }))
                        .unwrap_or_else(|_| {
                            Err(Box::new(std::io::Error::other(
                                "renderer panicked while resizing image",
                            )))
                        }) {
                            Ok(rendered) => {
                                let _ = result_tx.send(RenderResult::Loaded {
                                    request_id,
                                    path: None,
                                    source: source_snapshot,
                                    rendered,
                                });
                            }
                            Err(err) => {
                                let _ = result_tx.send(RenderResult::Failed {
                                    request_id,
                                    path: None,
                                    message: err.to_string(),
                                });
                            }
                        }
                    });
                }
                RenderCommand::Shutdown => break,
            }
        }
    });

    (command_tx, result_rx, join)
}

pub(crate) fn worker_send_error(err: mpsc::SendError<RenderCommand>) -> Box<dyn Error> {
    Box::new(std::io::Error::other(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        RenderIoCoordinator, RenderWorkerPriority, blank_loaded_image,
        should_abort_background_load, should_load_spread_companion,
    };
    use crate::drawers::canvas::Canvas;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn preload_is_aborted_while_primary_is_active() {
        let latest = AtomicU64::new(7);
        let coordinator = RenderIoCoordinator {
            primary_epoch: AtomicU64::new(3),
            primary_active: AtomicU64::new(1),
            high_priority_epoch: AtomicU64::new(3),
            companion_active: AtomicU64::new(0),
        };

        assert!(should_abort_background_load(
            RenderWorkerPriority::Preload,
            7,
            &latest,
            3,
            &coordinator,
        ));
    }

    #[test]
    fn preload_is_aborted_after_primary_epoch_changes() {
        let latest = AtomicU64::new(9);
        let coordinator = RenderIoCoordinator {
            primary_epoch: AtomicU64::new(4),
            primary_active: AtomicU64::new(0),
            high_priority_epoch: AtomicU64::new(4),
            companion_active: AtomicU64::new(0),
        };

        assert!(should_abort_background_load(
            RenderWorkerPriority::Preload,
            9,
            &latest,
            3,
            &coordinator,
        ));
    }

    #[test]
    fn primary_load_is_not_aborted_by_primary_activity() {
        let latest = AtomicU64::new(11);
        let coordinator = RenderIoCoordinator {
            primary_epoch: AtomicU64::new(5),
            primary_active: AtomicU64::new(1),
            high_priority_epoch: AtomicU64::new(5),
            companion_active: AtomicU64::new(0),
        };

        assert!(!should_abort_background_load(
            RenderWorkerPriority::Primary,
            11,
            &latest,
            5,
            &coordinator,
        ));
        coordinator.primary_active.store(0, Ordering::Release);
    }

    #[test]
    fn preload_is_aborted_while_companion_is_active() {
        let latest = AtomicU64::new(13);
        let coordinator = RenderIoCoordinator {
            primary_epoch: AtomicU64::new(5),
            primary_active: AtomicU64::new(0),
            high_priority_epoch: AtomicU64::new(6),
            companion_active: AtomicU64::new(1),
        };

        assert!(should_abort_background_load(
            RenderWorkerPriority::Preload,
            13,
            &latest,
            5,
            &coordinator,
        ));
    }

    #[test]
    fn companion_load_is_not_aborted_by_companion_activity() {
        let latest = AtomicU64::new(15);
        let coordinator = RenderIoCoordinator {
            primary_epoch: AtomicU64::new(8),
            primary_active: AtomicU64::new(0),
            high_priority_epoch: AtomicU64::new(9),
            companion_active: AtomicU64::new(1),
        };

        assert!(!should_abort_background_load(
            RenderWorkerPriority::Companion,
            15,
            &latest,
            8,
            &coordinator,
        ));
    }

    #[test]
    fn spread_companion_requires_portrait_primary() {
        let mut source = blank_loaded_image();
        source.canvas = Canvas::new(800, 1200);
        assert!(should_load_spread_companion(
            &source,
            Some(&PathBuf::from("next.png"))
        ));
    }

    #[test]
    fn spread_companion_is_skipped_for_landscape_primary() {
        let mut source = blank_loaded_image();
        source.canvas = Canvas::new(1200, 800);
        assert!(!should_load_spread_companion(
            &source,
            Some(&PathBuf::from("next.png"))
        ));
    }
}
