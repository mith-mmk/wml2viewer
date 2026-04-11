use crate::drawers::affine::InterpolationAlgorithm;
use crate::drawers::canvas::Canvas;
use crate::drawers::image::{
    LoadedImage, load_canvas_from_bytes_with_hint, load_canvas_from_file, resize_loaded_image,
};
use crate::filesystem::{load_virtual_image_bytes, resolve_start_path};
use crate::ui::viewer::options::RenderScaleMode;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

pub(crate) enum RenderCommand {
    LoadPath {
        request_id: u64,
        path: PathBuf,
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

fn blank_loaded_image() -> LoadedImage {
    LoadedImage {
        canvas: Canvas::new(1, 1),
        animation: Vec::new(),
        loop_count: None,
    }
}

pub(crate) fn spawn_render_worker(
    initial_source: LoadedImage,
) -> (Sender<RenderCommand>, Receiver<RenderResult>, JoinHandle<()>) {
    let (command_tx, command_rx) = mpsc::channel::<RenderCommand>();
    let (result_tx, result_rx) = mpsc::channel::<RenderResult>();
    let current_source = Arc::new(Mutex::new(initial_source));
    let latest_load_request_id = Arc::new(AtomicU64::new(0));

    let join = thread::spawn(move || {
        while let Ok(command) = command_rx.recv() {
            match command {
                RenderCommand::LoadPath {
                    request_id,
                    path,
                    zoom,
                    method,
                    scale_mode,
                } => {
                    latest_load_request_id.store(request_id, Ordering::Release);
                    let result_tx = result_tx.clone();
                    let current_source = Arc::clone(&current_source);
                    let latest_load_request_id = Arc::clone(&latest_load_request_id);
                    thread::spawn(move || {
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            (|| -> Result<Option<(LoadedImage, LoadedImage, PathBuf)>, Box<dyn Error>> {
                                if latest_load_request_id.load(Ordering::Acquire) != request_id {
                                    return Ok(None);
                                }

                                let load_path = resolve_start_path(&path).unwrap_or(path.clone());
                                if latest_load_request_id.load(Ordering::Acquire) != request_id {
                                    return Ok(None);
                                }

                                let source = if let Some(bytes) = load_virtual_image_bytes(&load_path) {
                                    load_canvas_from_bytes_with_hint(&bytes, Some(&load_path))?
                                } else {
                                    load_canvas_from_file(&load_path)?
                                };
                                if latest_load_request_id.load(Ordering::Acquire) != request_id {
                                    return Ok(None);
                                }

                                let rendered = match scale_mode {
                                    RenderScaleMode::FastGpu => source.clone(),
                                    RenderScaleMode::PreciseCpu => {
                                        resize_loaded_image(&source, zoom, method)?
                                    }
                                };
                                Ok(Some((source, rendered, load_path)))
                            })()
                        }))
                        .unwrap_or_else(|_| {
                            Err(Box::new(std::io::Error::other(
                                "decoder panicked while loading image",
                            )))
                        });

                        match result {
                            Ok(Some((source, rendered, load_path))) => {
                                if latest_load_request_id.load(Ordering::Acquire) == request_id {
                                    if let Ok(mut current) = current_source.lock() {
                                        *current = source.clone();
                                    }
                                }
                                let _ = result_tx.send(RenderResult::Loaded {
                                    request_id,
                                    path: Some(load_path),
                                    source,
                                    rendered,
                                });
                            }
                            Ok(None) => {}
                            Err(err) => {
                                let _ = result_tx.send(RenderResult::Failed {
                                    request_id,
                                    path: Some(path),
                                    message: err.to_string(),
                                });
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
                        match catch_unwind(AssertUnwindSafe(|| {
                            match scale_mode {
                                RenderScaleMode::FastGpu => Ok(source_snapshot.clone()),
                                RenderScaleMode::PreciseCpu => {
                                    resize_loaded_image(&source_snapshot, zoom, method)
                                }
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
