use crate::drawers::affine::InterpolationAlgorithm;
use crate::drawers::image::{
    load_canvas_from_bytes_with_hint, load_canvas_from_file, resize_loaded_image,
};
use crate::filesystem::{
    OpenedImageSource, open_image_source_with_cancel, source_prefers_low_io, virtual_image_size,
};
use crate::options::ThumbnailWorkaroundOptions;
use crate::ui::render::{
    RenderWorkerPriority, acquire_low_io_permit, canvas_to_color_image,
    should_cancel_low_priority_io, snapshot_primary_io_epoch,
};
use eframe::egui::ColorImage;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;

pub(crate) enum ThumbnailCommand {
    Generate {
        request_id: u64,
        path: PathBuf,
        max_side: u32,
    },
}

pub(crate) enum ThumbnailResult {
    Ready {
        _request_id: u64,
        path: PathBuf,
        max_side: u32,
        image: ColorImage,
    },
    Failed {
        _request_id: u64,
        path: PathBuf,
        _max_side: u32,
        _message: String,
    },
}

pub(crate) fn spawn_thumbnail_worker() -> (Sender<ThumbnailCommand>, Receiver<ThumbnailResult>) {
    let (command_tx, command_rx) = mpsc::channel::<ThumbnailCommand>();
    let (result_tx, result_rx) = mpsc::channel::<ThumbnailResult>();

    thread::spawn(move || {
        while let Ok(command) = command_rx.recv() {
            match command {
                ThumbnailCommand::Generate {
                    request_id,
                    path,
                    max_side,
                } => {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        if should_skip_thumbnail(&path) {
                            return Err("thumbnail suppressed".to_string());
                        }
                        let primary_epoch_snapshot = snapshot_primary_io_epoch();
                        let should_cancel =
                            || should_cancel_low_priority_io(primary_epoch_snapshot);
                        if should_cancel() {
                            return Err("thumbnail cancelled".to_string());
                        }
                        let low_io_permit = source_prefers_low_io(&path)
                            .then(|| {
                                acquire_low_io_permit(RenderWorkerPriority::Preload, &should_cancel)
                            })
                            .flatten();
                        if source_prefers_low_io(&path) && low_io_permit.is_none() {
                            return Err("thumbnail cancelled".to_string());
                        }
                        let loaded = match open_image_source_with_cancel(&path, &should_cancel) {
                            Some(OpenedImageSource::Bytes {
                                bytes, hint_path, ..
                            }) => load_canvas_from_bytes_with_hint(&bytes, Some(&hint_path)),
                            Some(OpenedImageSource::File { path, .. }) => {
                                load_canvas_from_file(&path)
                            }
                            None => load_canvas_from_file(&path),
                        }
                        .map_err(|err| err.to_string())?;
                        drop(low_io_permit);
                        if should_cancel() {
                            return Err("thumbnail cancelled".to_string());
                        }

                        let scale = (max_side as f32
                            / loaded.canvas.width().max(loaded.canvas.height()) as f32)
                            .clamp(0.05, 1.0);
                        let resized =
                            resize_loaded_image(&loaded, scale, InterpolationAlgorithm::Bilinear)
                                .map_err(|err| err.to_string())?;
                        Ok::<_, String>(canvas_to_color_image(&resized.canvas))
                    }));

                    match result {
                        Ok(Ok(image)) => {
                            let _ = result_tx.send(ThumbnailResult::Ready {
                                _request_id: request_id,
                                path,
                                max_side,
                                image,
                            });
                        }
                        Ok(Err(message)) => {
                            let _ = result_tx.send(ThumbnailResult::Failed {
                                _request_id: request_id,
                                path,
                                _max_side: max_side,
                                _message: message,
                            });
                        }
                        Err(_) => {
                            let _ = result_tx.send(ThumbnailResult::Failed {
                                _request_id: request_id,
                                path,
                                _max_side: max_side,
                                _message: "thumbnail worker panicked".to_string(),
                            });
                        }
                    }
                }
            }
        }
    });

    (command_tx, result_rx)
}

pub(crate) fn set_thumbnail_workaround(options: ThumbnailWorkaroundOptions) {
    if let Ok(mut config) = thumbnail_workaround_config().lock() {
        *config = options;
    }
}

fn should_skip_thumbnail(path: &std::path::Path) -> bool {
    let options = thumbnail_workaround_config()
        .lock()
        .map(|config| config.clone())
        .unwrap_or_default();
    if !options.suppress_large_files {
        return false;
    }
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let size = virtual_image_size(path).unwrap_or(0);
    (ext == "bmp" && size > 8 * 1024 * 1024) || size > 128 * 1024 * 1024
}

fn thumbnail_workaround_config() -> &'static Mutex<ThumbnailWorkaroundOptions> {
    static CONFIG: OnceLock<Mutex<ThumbnailWorkaroundOptions>> = OnceLock::new();
    CONFIG.get_or_init(|| Mutex::new(ThumbnailWorkaroundOptions::default()))
}
