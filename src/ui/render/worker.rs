use crate::drawers::affine::InterpolationAlgorithm;
use crate::drawers::canvas::Canvas;
use crate::drawers::image::{
    LoadedImage, load_canvas_from_bytes_with_hint, load_canvas_from_file, resize_loaded_image,
};
use crate::filesystem::{load_virtual_image_bytes, resolve_start_path};
use crate::ui::render::{canvas_to_color_image, downscale_for_texture_limit};
use crate::ui::viewer::options::RenderScaleMode;
use std::error::Error;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Instant;

pub(crate) enum RenderCommand {
    LoadPath {
        request_id: u64,
        path: PathBuf,
        companion_path: Option<PathBuf>,
        zoom: f32,
        method: InterpolationAlgorithm,
        scale_mode: RenderScaleMode,
        max_texture_side: usize,
    },
    ResizeCurrent {
        request_id: u64,
        source: LoadedImage,
        zoom: f32,
        method: InterpolationAlgorithm,
        scale_mode: RenderScaleMode,
        max_texture_side: usize,
    },
    Shutdown,
}

pub(crate) enum RenderResult {
    Loaded {
        request_id: u64,
        path: Option<PathBuf>,
        source: LoadedImage,
        rendered: LoadedImage,
        companion: Option<LoadedRenderPage>,
        metrics: RenderLoadMetrics,
        prepared_texture: Option<PreparedTexture>,
    },
    Failed {
        request_id: u64,
        path: Option<PathBuf>,
        message: String,
        metrics: RenderLoadMetrics,
    },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RenderLoadMetrics {
    pub(crate) resolved_path: Option<PathBuf>,
    pub(crate) used_virtual_bytes: bool,
    pub(crate) decoded_from_bytes: bool,
    pub(crate) source_bytes_len: Option<usize>,
    pub(crate) resolve_ms: u128,
    pub(crate) read_ms: u128,
    pub(crate) decode_ms: u128,
    pub(crate) resize_ms: u128,
}

pub(crate) struct LoadedRenderPage {
    pub(crate) path: PathBuf,
    pub(crate) source: LoadedImage,
    pub(crate) rendered: LoadedImage,
    pub(crate) metrics: RenderLoadMetrics,
    pub(crate) prepared_texture: Option<PreparedTexture>,
}

#[derive(Clone)]
pub(crate) struct PreparedTexture {
    pub(crate) image: Arc<eframe::egui::ColorImage>,
    pub(crate) display_scale: f32,
    pub(crate) max_texture_side: usize,
    pub(crate) method: InterpolationAlgorithm,
    pub(crate) downscale_ms: f64,
    pub(crate) color_conversion_ms: f64,
}

fn prepare_texture(
    canvas: &Canvas,
    max_texture_side: usize,
    method: InterpolationAlgorithm,
) -> PreparedTexture {
    let started = Instant::now();
    let (canvas, display_scale) = downscale_for_texture_limit(canvas, max_texture_side, method);
    let downscale_done = Instant::now();
    let image = canvas_to_color_image(&canvas);
    PreparedTexture {
        image: Arc::new(image),
        display_scale,
        max_texture_side,
        method,
        downscale_ms: downscale_done.duration_since(started).as_secs_f64() * 1000.0,
        color_conversion_ms: downscale_done.elapsed().as_secs_f64() * 1000.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActiveRenderRequest {
    Load(u64),
    Resize(u64),
}

pub(crate) fn spawn_render_worker(
    _initial_source: LoadedImage,
) -> (
    Sender<RenderCommand>,
    Receiver<RenderResult>,
    JoinHandle<()>,
) {
    let (command_tx, command_rx) = mpsc::channel::<RenderCommand>();
    let (result_tx, result_rx) = mpsc::channel::<RenderResult>();
    let latest_load_request_id = Arc::new(AtomicU64::new(0));

    let join = thread::spawn(move || {
        while let Ok(command) = command_rx.recv() {
            match command {
                RenderCommand::LoadPath {
                    request_id,
                    path,
                    companion_path,
                    zoom,
                    method,
                    scale_mode,
                    max_texture_side,
                } => {
                    latest_load_request_id.store(request_id, Ordering::Release);
                    let result_tx = result_tx.clone();
                    let latest_load_request_id = Arc::clone(&latest_load_request_id);
                    thread::spawn(move || {
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            (|| -> Result<Option<(LoadedRenderPage, Option<LoadedRenderPage>)>, Box<dyn Error>> {
                                let Some(primary) = load_render_page(
                                    &path,
                                    request_id,
                                    &latest_load_request_id,
                                    zoom,
                                    method,
                                    scale_mode,
                                    max_texture_side,
                                )? else {
                                    return Ok(None);
                                };

                                let companion = if let Some(companion_path) = companion_path {
                                    load_render_page(
                                        &companion_path,
                                        request_id,
                                        &latest_load_request_id,
                                        zoom,
                                        method,
                                        scale_mode,
                                        max_texture_side,
                                    )
                                    .ok()
                                    .flatten()
                                } else {
                                    None
                                };

                                Ok(Some((primary, companion)))
                            })()
                        }))
                        .unwrap_or_else(|_| {
                            Err(Box::new(std::io::Error::other(
                                "decoder panicked while loading image",
                            )))
                        });

                        match result {
                            Ok(Some((primary, companion))) => {
                                if latest_load_request_id.load(Ordering::Acquire) != request_id {
                                    return;
                                }
                                let _ = result_tx.send(RenderResult::Loaded {
                                    request_id,
                                    path: Some(primary.path.clone()),
                                    source: primary.source,
                                    rendered: primary.rendered,
                                    companion,
                                    metrics: primary.metrics,
                                    prepared_texture: primary.prepared_texture,
                                });
                            }
                            Ok(None) => {}
                            Err(err) => {
                                let _ = result_tx.send(RenderResult::Failed {
                                    request_id,
                                    path: Some(path),
                                    message: err.to_string(),
                                    metrics: RenderLoadMetrics::default(),
                                });
                            }
                        }
                    });
                }
                RenderCommand::ResizeCurrent {
                    request_id,
                    source,
                    zoom,
                    method,
                    scale_mode,
                    max_texture_side,
                } => {
                    let result_tx = result_tx.clone();
                    thread::spawn(move || {
                        let source_snapshot = source;
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
                                let prepared_texture = Some(prepare_texture(
                                    rendered.frame_canvas(0),
                                    max_texture_side,
                                    method,
                                ));
                                let _ = result_tx.send(RenderResult::Loaded {
                                    request_id,
                                    path: None,
                                    source: source_snapshot,
                                    rendered,
                                    companion: None,
                                    metrics: RenderLoadMetrics::default(),
                                    prepared_texture,
                                });
                            }
                            Err(err) => {
                                let _ = result_tx.send(RenderResult::Failed {
                                    request_id,
                                    path: None,
                                    message: err.to_string(),
                                    metrics: RenderLoadMetrics::default(),
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

fn load_render_page(
    path: &PathBuf,
    request_id: u64,
    latest_load_request_id: &AtomicU64,
    zoom: f32,
    method: InterpolationAlgorithm,
    scale_mode: RenderScaleMode,
    max_texture_side: usize,
) -> Result<Option<LoadedRenderPage>, Box<dyn Error>> {
    let mut metrics = RenderLoadMetrics::default();
    if latest_load_request_id.load(Ordering::Acquire) != request_id {
        return Ok(None);
    }

    let resolve_started = Instant::now();
    let load_path = resolve_start_path(path).unwrap_or(path.clone());
    metrics.resolve_ms = resolve_started.elapsed().as_millis();
    metrics.resolved_path = Some(load_path.clone());
    if latest_load_request_id.load(Ordering::Acquire) != request_id {
        return Ok(None);
    }

    let read_started = Instant::now();
    let virtual_bytes = load_virtual_image_bytes(&load_path);
    metrics.read_ms = read_started.elapsed().as_millis();
    metrics.used_virtual_bytes = virtual_bytes.is_some();
    metrics.source_bytes_len = virtual_bytes.as_ref().map(Vec::len);

    let decode_started = Instant::now();
    let source = if let Some(bytes) = virtual_bytes {
        metrics.decoded_from_bytes = true;
        load_canvas_from_bytes_with_hint(&bytes, Some(&load_path))?
    } else {
        metrics.decoded_from_bytes = false;
        load_canvas_from_file(&load_path)?
    };
    metrics.decode_ms = decode_started.elapsed().as_millis();
    if latest_load_request_id.load(Ordering::Acquire) != request_id {
        return Ok(None);
    }

    let resize_started = Instant::now();
    let rendered = match scale_mode {
        RenderScaleMode::FastGpu => source.clone(),
        RenderScaleMode::PreciseCpu => resize_loaded_image(&source, zoom, method)?,
    };
    metrics.resize_ms = resize_started.elapsed().as_millis();

    if latest_load_request_id.load(Ordering::Acquire) != request_id {
        return Ok(None);
    }
    let prepared_texture = Some(prepare_texture(
        rendered.frame_canvas(0),
        max_texture_side,
        method,
    ));
    if latest_load_request_id.load(Ordering::Acquire) != request_id {
        return Ok(None);
    }

    Ok(Some(LoadedRenderPage {
        path: load_path,
        source,
        rendered,
        metrics,
        prepared_texture,
    }))
}

pub(crate) fn worker_send_error(err: mpsc::SendError<RenderCommand>) -> Box<dyn Error> {
    Box::new(std::io::Error::other(err.to_string()))
}

#[cfg(test)]
#[path = "../../../tests/support/src/ui/render/worker_tests.rs"]
mod tests;
