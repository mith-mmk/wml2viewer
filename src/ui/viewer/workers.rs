use super::*;

impl ViewerApp {
    pub(super) fn alloc_request_id(&mut self) -> u64 {
        self.next_request_id += 1;
        self.next_request_id
    }

    pub(super) fn alloc_fs_request_id(&mut self) -> u64 {
        self.next_fs_request_id += 1;
        self.next_fs_request_id
    }

    pub(super) fn alloc_filer_request_id(&mut self) -> u64 {
        self.next_filer_request_id += 1;
        self.next_filer_request_id
    }

    pub(super) fn alloc_thumbnail_request_id(&mut self) -> u64 {
        self.next_thumbnail_request_id += 1;
        self.next_thumbnail_request_id
    }

    pub(super) fn alloc_preload_request_id(&mut self) -> u64 {
        self.next_preload_request_id += 1;
        self.next_preload_request_id
    }

    pub(super) fn invalidate_preload(&mut self) {
        self.active_preload_request_id = None;
        self.pending_preload_navigation_path = None;
        self.preload_cache.clear();
    }

    pub(super) fn preload_cache_contains(&self, path: &Path) -> bool {
        self.preload_cache
            .iter()
            .any(|entry| entry.navigation_path == path)
    }

    pub(super) fn cached_preloaded_entry(&self, path: &Path) -> Option<PreloadedEntry> {
        self.preload_cache
            .iter()
            .find(|entry| entry.navigation_path == path)
            .cloned()
    }

    pub(super) fn remember_preloaded_entry(&mut self, entry: PreloadedEntry) {
        remember_preloaded_entry_in_cache(&mut self.preload_cache, entry);
    }

    pub(super) fn remember_loaded_page_in_cache(
        &mut self,
        navigation_path: &Path,
        load_path: Option<&Path>,
        source: &LoadedImage,
        rendered: &LoadedImage,
    ) {
        self.remember_preloaded_entry(PreloadedEntry {
            navigation_path: navigation_path.to_path_buf(),
            load_path: load_path.map(Path::to_path_buf),
            display: DisplayedPageState {
                source: source.clone(),
                rendered: rendered.clone(),
                texture: (!self.current_texture_is_default).then(|| self.current_texture.clone()),
                texture_display_scale: self.texture_display_scale,
            },
        });
    }

    pub(super) fn take_preloaded_entry(&mut self, path: &Path) -> Option<PreloadedEntry> {
        let index = self
            .preload_cache
            .iter()
            .position(|cached| cached.navigation_path == path)?;
        self.preload_cache.remove(index)
    }

    pub(super) fn apply_companion_loaded(
        &mut self,
        path: Option<PathBuf>,
        display: DisplayedPageState,
    ) {
        let previous_companion = self.companion_display.clone();
        let layout_changed = path.is_some()
            || previous_companion
                .as_ref()
                .map(|current| {
                    current.source.canvas.width() != display.source.canvas.width()
                        || current.source.canvas.height() != display.source.canvas.height()
                })
                .unwrap_or(true);

        let mut display = display;
        let texture = if let Some(texture) = display.texture.clone() {
            texture
        } else {
            let (canvas, display_scale) = downscale_for_texture_limit(
                display.rendered.frame_canvas(0),
                self.max_texture_side,
                self.render_options.zoom_method,
            );
            let image = self.color_image_from_canvas(&canvas);
            let texture_options = self.texture_options();
            display.texture_display_scale = display_scale;
            if path.is_none() {
                if let Some(existing) = &mut previous_companion.clone() {
                    if let Some(texture) = &mut existing.texture {
                        texture.set(image, texture_options);
                        texture.clone()
                    } else {
                        self.egui_ctx
                            .load_texture("manga_companion", image, texture_options)
                    }
                } else {
                    self.egui_ctx
                        .load_texture("manga_companion", image, texture_options)
                }
            } else {
                self.egui_ctx
                    .load_texture("manga_companion", image, texture_options)
            }
        };

        display.texture = Some(texture);
        if display.texture_display_scale <= 0.0 {
            display.texture_display_scale = 1.0;
        }

        self.companion_display = Some(display);
        if layout_changed {
            self.pending_fit_recalc |= !matches!(self.render_options.zoom_option, ZoomOption::None);
        }
        self.companion_active_request = None;
    }

    pub(super) fn apply_spread_companion_result(&mut self, companion: Option<LoadedRenderPage>) {
        let desired =
            self.desired_manga_companion_path_for_navigation(&self.current_navigation_path);
        match companion {
            Some(companion) if desired.as_ref() == Some(&companion.path) => {
                self.companion_navigation_path = Some(companion.path.clone());
                self.apply_companion_loaded(
                    Some(companion.path),
                    DisplayedPageState {
                        source: companion.source,
                        rendered: companion.rendered,
                        texture: None,
                        texture_display_scale: 1.0,
                    },
                );
            }
            _ => {
                self.clear_manga_companion();
            }
        }
    }

    pub(super) fn spawn_navigation_workers(&mut self) {
        if self.fs_tx.is_none() || self.fs_rx.is_none() {
            let (tx, rx) = spawn_filesystem_worker(self.navigation_sort);
            self.fs_tx = Some(tx);
            self.fs_rx = Some(rx);
        }
        if self.filer_tx.is_none() || self.filer_rx.is_none() {
            let (tx, rx) = spawn_filer_worker();
            self.filer_tx = Some(tx);
            self.filer_rx = Some(rx);
        }
        if self.thumbnail_tx.is_none() || self.thumbnail_rx.is_none() {
            let (tx, rx) = spawn_thumbnail_worker();
            self.thumbnail_tx = Some(tx);
            self.thumbnail_rx = Some(rx);
        }
    }

    pub(super) fn init_filesystem(&mut self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        self.spawn_navigation_workers();
        self.deferred_filesystem_sync_frame = None;
        if should_queue_filesystem_init(self.active_fs_request_id) {
            self.log_bench_state(
                "viewer.init_filesystem.queued_busy",
                serde_json::json!({
                    "path": path.display().to_string(),
                    "active_fs_request_id": self.active_fs_request_id,
                }),
            );
            queue_filesystem_init_path(&mut self.queued_filesystem_init_path, path);
            return Ok(());
        }
        let Some(fs_tx) = self.fs_tx.clone() else {
            return Ok(());
        };
        let request_id = self.alloc_fs_request_id();
        self.active_fs_request_id = Some(request_id);
        self.log_bench_state(
            "viewer.init_filesystem",
            serde_json::json!({
                "request_id": request_id,
                "path": path.display().to_string(),
            }),
        );
        self.overlay
            .set_loading_message(format!("Scanning {}", path.display()));
        fs_tx
            .send(FilesystemCommand::Init { request_id, path })
            .map_err(filesystem_send_error)?;
        Ok(())
    }

    pub(super) fn request_navigation(
        &mut self,
        mut command: FilesystemCommand,
    ) -> Result<(), Box<dyn Error>> {
        self.sync_navigation_sort_with_filer_sort();
        self.spawn_navigation_workers();
        if !self.navigator_ready {
            self.log_bench_state(
                "viewer.request_navigation.queued_not_ready",
                serde_json::json!({
                    "command": format!("{command:?}"),
                }),
            );
            queue_navigation_command(&mut self.queued_navigation, command);
            return Ok(());
        }
        if self.active_fs_request_id.is_some() {
            self.log_bench_state(
                "viewer.request_navigation.queued_busy",
                serde_json::json!({
                    "command": format!("{command:?}"),
                }),
            );
            queue_navigation_command(&mut self.queued_navigation, command);
            return Ok(());
        }
        let Some(fs_tx) = self.fs_tx.clone() else {
            self.log_bench_state(
                "viewer.request_navigation.queued_no_worker",
                serde_json::json!({
                    "command": format!("{command:?}"),
                }),
            );
            queue_navigation_command(&mut self.queued_navigation, command);
            return Ok(());
        };
        let request_id = self.alloc_fs_request_id();
        self.active_fs_request_id = Some(request_id);
        command = match command {
            FilesystemCommand::Init { path, .. } => FilesystemCommand::Init { request_id, path },
            FilesystemCommand::SetCurrent { path, .. } => {
                FilesystemCommand::SetCurrent { request_id, path }
            }
            FilesystemCommand::Next { policy, .. } => {
                FilesystemCommand::Next { request_id, policy }
            }
            FilesystemCommand::Prev { policy, .. } => {
                FilesystemCommand::Prev { request_id, policy }
            }
            FilesystemCommand::First { .. } => FilesystemCommand::First { request_id },
            FilesystemCommand::Last { .. } => FilesystemCommand::Last { request_id },
        };
        self.log_bench_state(
            "viewer.request_navigation.sent",
            serde_json::json!({
                "request_id": request_id,
                "command": format!("{command:?}"),
            }),
        );
        self.overlay.set_loading_message("Scanning folder...");
        fs_tx.send(command).map_err(filesystem_send_error)?;
        Ok(())
    }

    pub(super) fn apply_loaded_result(
        &mut self,
        path: Option<PathBuf>,
        source: LoadedImage,
        rendered: LoadedImage,
        companion: Option<LoadedRenderPage>,
    ) {
        self.log_bench_state(
            "viewer.apply_loaded_result.begin",
            serde_json::json!({
                "loaded_path": path.as_ref().map(|path| path.display().to_string()),
                "source_size": [source.canvas.width(), source.canvas.height()],
                "rendered_size": [rendered.canvas.width(), rendered.canvas.height()],
            }),
        );
        let previous_navigation_path = self.current_navigation_path.clone();
        if let Some(pending_navigation_path) = self.pending_navigation_path.take() {
            self.current_navigation_path = if path
                .as_ref()
                .is_some_and(|_| is_browser_container(&pending_navigation_path))
            {
                resolve_navigation_entry_path(&pending_navigation_path)
                    .or_else(|| path.clone())
                    .unwrap_or(pending_navigation_path)
            } else {
                pending_navigation_path
            };
        }
        let loaded_path = path.clone();
        if let Some(path) = path {
            let folder_changed = should_reinitialize_filesystem_after_load(
                &previous_navigation_path,
                &self.current_navigation_path,
            );
            let committed_filer_select = should_cancel_filesystem_request_for_filer_select(
                self.filer.pending_user_request.as_ref(),
                &self.current_navigation_path,
                self.active_fs_request_id,
            );
            self.current_path = path.clone();
            self.save_dialog.file_name = default_save_file_name(&path);
            if folder_changed {
                self.clear_manga_companion();
                self.prev_texture = None;
            }
            if committed_filer_select {
                self.log_bench_state(
                    "viewer.filesystem.restarted_for_filer_select",
                    serde_json::json!({
                        "active_fs_request_id": self.active_fs_request_id,
                        "navigation_path": self.current_navigation_path.display().to_string(),
                    }),
                );
                self.navigator_ready = false;
                self.queued_navigation = None;
                self.respawn_filesystem_worker();
                let _ = self.init_filesystem(self.current_navigation_path.clone());
            } else if folder_changed {
                if let Some(fs_tx) = self.fs_tx.clone() {
                    let request_id = self.alloc_fs_request_id();
                    self.log_bench_state(
                        "viewer.filesystem.set_current_after_branch_change",
                        serde_json::json!({
                            "request_id": request_id,
                            "navigation_path": self.current_navigation_path.display().to_string(),
                        }),
                    );
                    let _ = fs_tx.send(FilesystemCommand::SetCurrent {
                        request_id,
                        path: self.current_navigation_path.clone(),
                    });
                } else {
                    self.navigator_ready = false;
                    self.queued_navigation = None;
                    let _ = self.init_filesystem(self.current_navigation_path.clone());
                }
            } else if let Some(fs_tx) = self.fs_tx.clone() {
                let request_id = self.alloc_fs_request_id();
                let _ = fs_tx.send(FilesystemCommand::SetCurrent {
                    request_id,
                    path: self.current_navigation_path.clone(),
                });
            }
            if self.show_subfiler {
                self.pending_subfiler_focus_path = Some(self.current_navigation_path.clone());
            }
            if should_clear_stale_filer_refresh_request(
                self.filer.pending_user_request.as_ref(),
                self.current_directory().as_deref(),
            ) {
                self.log_bench_state(
                    "viewer.filer.refresh_request_cleared_after_branch_change",
                    serde_json::json!({
                        "current_directory": self.current_directory().map(|path| path.display().to_string()),
                    }),
                );
                self.filer.pending_user_request = None;
            }
            self.clear_committed_filer_user_request();
            self.sync_filer_directory_with_current_path();
            self.apply_spread_companion_result(companion);
        }
        self.source = source;
        self.rendered = rendered;
        self.pending_fit_recalc |= !matches!(self.render_options.zoom_option, ZoomOption::None);
        self.current_frame = self
            .current_frame
            .min(self.rendered.frame_count().saturating_sub(1));
        self.completed_loops = 0;
        self.last_frame_at = Instant::now();
        self.active_request = None;
        self.active_request_started_at = None;

        let source_size = vec2(
            self.source.canvas.width() as f32,
            self.source.canvas.height() as f32,
        );
        let defer_precise_display =
            self.maybe_defer_precise_display(source_size, loaded_path.as_deref());
        if defer_precise_display {
            let _ = self.request_resize_current();
        } else {
            self.rebuild_current_texture();
            if self.active_fs_request_id.is_none() {
                self.overlay.clear_loading_message();
            }
        }
        let cache_navigation_path = self.current_navigation_path.clone();
        let cache_source = self.source.clone();
        let cache_rendered = self.rendered.clone();
        self.remember_loaded_page_in_cache(
            &cache_navigation_path,
            loaded_path.as_deref(),
            &cache_source,
            &cache_rendered,
        );
        if !self.navigator_ready && self.active_fs_request_id.is_none() {
            if self.deferred_filesystem_init_path.is_some() {
                self.deferred_filesystem_init_path = Some(
                    loaded_path
                        .clone()
                        .unwrap_or_else(|| self.current_navigation_path.clone()),
                );
                self.defer_initial_filesystem_sync();
            }
        }
        self.schedule_preload();
        if !self.bench_initial_load_logged {
            self.bench_initial_load_logged = true;
            self.log_bench_state(
                "viewer.initial_load.completed",
                serde_json::json!({
                    "loaded_path": loaded_path.as_ref().map(|path| path.display().to_string()),
                    "frame_counter": self.frame_counter,
                    "startup_phase": format!("{:?}", self.startup_phase),
                }),
            );
        }
        self.log_bench_state(
            "viewer.apply_loaded_result.end",
            serde_json::json!({
                "loaded_path": loaded_path.as_ref().map(|path| path.display().to_string()),
            }),
        );
        if self.pending_resize_after_load {
            self.pending_resize_after_load = false;
            let _ = self.request_resize_current();
        } else if self.pending_resize_after_render {
            self.pending_resize_after_render = false;
            let _ = self.request_resize_current();
        }
        self.flush_pending_viewer_navigation();
    }

    pub(super) fn next_preload_candidate(&self) -> Option<PathBuf> {
        if let Some(companion) = self.desired_manga_companion_path() {
            let companion_ready = self.visible_companion().is_some();
            if should_prioritize_companion_preload(
                Some(companion.as_path()),
                self.companion_navigation_path.as_deref(),
                companion_ready,
            ) {
                return Some(companion);
            }
        }
        let step =
            if self.manga_spread_active() { 2 } else { 1 } * self.navigation_direction_sign();
        adjacent_entry(&self.current_navigation_path, self.navigation_sort, step)
    }

    pub(super) fn schedule_preload(&mut self) {
        if self.empty_mode || self.active_request.is_some() {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_busy",
                serde_json::json!({}),
            );
            return;
        }
        if !self.navigator_ready {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_not_ready",
                serde_json::json!({}),
            );
            return;
        }
        if archive_prefers_low_io(&self.current_navigation_path) {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_low_io_current",
                serde_json::json!({
                    "path": self.current_navigation_path.display().to_string(),
                }),
            );
            return;
        }
        let Some(path) = self.next_preload_candidate() else {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_no_candidate",
                serde_json::json!({}),
            );
            return;
        };
        if archive_prefers_low_io(&path) {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_low_io_candidate",
                serde_json::json!({
                    "path": path.display().to_string(),
                }),
            );
            return;
        }
        if self.preload_cache_contains(&path)
            || self.pending_preload_navigation_path.as_ref() == Some(&path)
        {
            self.log_bench_state(
                "viewer.schedule_preload.skipped_duplicate",
                serde_json::json!({
                    "path": path.display().to_string(),
                }),
            );
            return;
        }
        let request_id = self.alloc_preload_request_id();
        self.active_preload_request_id = Some(request_id);
        self.pending_preload_navigation_path = Some(path.clone());
        self.log_bench_state(
            "viewer.schedule_preload.sent",
            serde_json::json!({
                "request_id": request_id,
                "path": path.display().to_string(),
            }),
        );
        let _ = self.preload_tx.send(RenderCommand::LoadPath {
            request_id,
            path,
            companion_path: None,
            zoom: self.zoom,
            method: self.render_options.zoom_method,
            scale_mode: self.render_options.scale_mode,
        });
    }

    pub(super) fn try_take_preloaded(&mut self, path: &std::path::Path) -> bool {
        let Some(entry) = self.take_preloaded_entry(path) else {
            return false;
        };

        self.log_bench_state(
            "viewer.try_take_preloaded.hit",
            serde_json::json!({
                "path": path.display().to_string(),
                "load_path": entry.load_path.as_ref().map(|path| path.display().to_string()),
            }),
        );
        if let Some(texture) = entry.display.texture {
            self.current_texture = texture;
            self.current_texture_is_default = false;
            self.texture_display_scale = entry.display.texture_display_scale;
        }
        self.pending_navigation_path = Some(path.to_path_buf());
        self.overlay.clear_loading_message();
        self.apply_loaded_result(
            entry.load_path,
            entry.display.source,
            entry.display.rendered,
            None,
        );
        true
    }

    pub(super) fn respawn_render_worker(&mut self) {
        let (worker_tx, worker_rx, worker_join) = spawn_render_worker(self.source.clone());
        self.worker_tx = worker_tx;
        self.worker_rx = worker_rx;
        self.worker_join = Some(worker_join);
        self.active_request = None;
        self.active_request_started_at = None;
    }

    pub(super) fn respawn_companion_worker(&mut self) {
        let seed = self
            .companion_display
            .as_ref()
            .map(|display| display.source.clone())
            .unwrap_or_else(|| self.source.clone());
        let (tx, rx, join) = spawn_render_worker(seed);
        self.companion_tx = tx;
        self.companion_rx = rx;
        self.companion_join = Some(join);
        self.companion_active_request = None;
    }

    pub(super) fn respawn_preload_worker(&mut self) {
        let (tx, rx, join) = spawn_render_worker(self.source.clone());
        self.preload_tx = tx;
        self.preload_rx = rx;
        self.preload_join = Some(join);
        self.invalidate_preload();
    }

    pub(crate) fn respawn_filesystem_worker(&mut self) {
        let (tx, rx) = spawn_filesystem_worker(self.navigation_sort);
        self.fs_tx = Some(tx);
        self.fs_rx = Some(rx);
        self.navigator_ready = false;
        self.active_fs_request_id = None;
        let _ = self.init_filesystem(self.current_navigation_path.clone());
    }

    pub(super) fn respawn_filer_worker(&mut self) {
        let (tx, rx) = spawn_filer_worker();
        self.filer_tx = Some(tx);
        self.filer_rx = Some(rx);
        self.filer.pending_request_id = None;
        if let Some(dir) = self
            .filer
            .directory
            .clone()
            .or_else(|| self.current_directory())
        {
            self.request_filer_directory(dir, self.filer.selected.clone());
        }
    }

    pub(super) fn respawn_thumbnail_worker(&mut self) {
        let (tx, rx) = spawn_thumbnail_worker();
        self.thumbnail_tx = Some(tx);
        self.thumbnail_rx = Some(rx);
        self.thumbnail_pending.clear();
    }

    pub(super) fn poll_worker(&mut self) {
        loop {
            match self.worker_rx.try_recv() {
                Ok(RenderResult::Loaded {
                    request_id,
                    path,
                    source,
                    rendered,
                    companion,
                    metrics,
                }) => {
                    let Some(active_request) = self.active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if !request_matches {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_worker.loaded",
                        serde_json::json!({
                            "request_id": request_id,
                            "path": path.as_ref().map(|path| path.display().to_string()),
                            "companion_path": companion.as_ref().map(|page| page.path.display().to_string()),
                            "metrics": Self::bench_metrics_payload(&metrics),
                        }),
                    );
                    self.apply_loaded_result(path, source, rendered, companion);
                }
                Ok(RenderResult::Failed {
                    request_id,
                    path,
                    message,
                    metrics,
                }) => {
                    let Some(active_request) = self.active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if !request_matches {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_worker.failed",
                        serde_json::json!({
                            "request_id": request_id,
                            "path": path.as_ref().map(|path| path.display().to_string()),
                            "message": message,
                            "metrics": Self::bench_metrics_payload(&metrics),
                        }),
                    );
                    let failed_during_load = matches!(active_request, ActiveRenderRequest::Load(_));
                    let failed_navigation_path = self.pending_navigation_path.take();
                    let should_advance = failed_during_load
                        && should_advance_after_load_failure(
                            &self.current_navigation_path,
                            failed_navigation_path.as_deref(),
                        );
                    let label = path
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .and_then(|name| name.to_str())
                        .unwrap_or("image");
                    self.save_dialog.message = Some(format!("Load failed: {label}: {message}"));
                    self.clear_current_image_display();
                    self.show_loading_texture(true);
                    self.overlay.clear_loading_message();
                    self.active_request = None;
                    self.active_request_started_at = None;
                    if matches!(
                        self.filer.pending_user_request,
                        Some(FilerUserRequest::SelectFile { .. })
                    ) {
                        self.log_bench_state(
                            "viewer.filer.select_request_cleared_after_load_failure",
                            serde_json::json!({
                                "failed_navigation_path": failed_navigation_path
                                    .as_ref()
                                    .map(|path| path.display().to_string()),
                            }),
                        );
                        self.filer.pending_user_request = None;
                    }
                    self.flush_pending_viewer_navigation();
                    if !self.navigator_ready && self.active_fs_request_id.is_none() {
                        if self.deferred_filesystem_init_path.is_some() {
                            self.deferred_filesystem_init_path =
                                Some(self.current_navigation_path.clone());
                            self.defer_initial_filesystem_sync();
                        }
                    }
                    if should_advance {
                        let _ = self.next_image();
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.log_bench_state("viewer.poll_worker.disconnected", serde_json::json!({}));
                    self.open_dialog_with_title_key(
                        UiTextKey::AlertTitle,
                        self.text(UiTextKey::RenderWorkerDisconnected).to_string(),
                    );
                    self.overlay.clear_loading_message();
                    self.respawn_render_worker();
                    if !self.empty_mode {
                        let _ = self.request_load_path(self.current_navigation_path.clone());
                    }
                    break;
                }
            }
        }
    }

    pub(super) fn poll_preload_worker(&mut self) {
        loop {
            match self.preload_rx.try_recv() {
                Ok(RenderResult::Loaded {
                    request_id,
                    path,
                    source,
                    rendered,
                    companion: _,
                    metrics,
                }) => {
                    if self.active_preload_request_id != Some(request_id) {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_preload_worker.loaded",
                        serde_json::json!({
                            "request_id": request_id,
                            "path": path.as_ref().map(|path| path.display().to_string()),
                            "navigation_path": self.pending_preload_navigation_path.as_ref().map(|path| path.display().to_string()),
                            "metrics": Self::bench_metrics_payload(&metrics),
                        }),
                    );
                    self.active_preload_request_id = None;
                    let Some(navigation_path) = self.pending_preload_navigation_path.take() else {
                        continue;
                    };
                    let texture_name = self.texture_name_for_path(path.as_deref());
                    let (texture, display_scale) =
                        self.build_texture_from_canvas(&texture_name, rendered.frame_canvas(0));
                    self.remember_preloaded_entry(PreloadedEntry {
                        navigation_path,
                        load_path: path,
                        display: DisplayedPageState {
                            source,
                            rendered,
                            texture: Some(texture),
                            texture_display_scale: display_scale,
                        },
                    });
                }
                Ok(RenderResult::Failed {
                    request_id,
                    metrics,
                    ..
                }) => {
                    if self.active_preload_request_id == Some(request_id) {
                        self.log_bench_state(
                            "viewer.poll_preload_worker.failed",
                            serde_json::json!({
                                "request_id": request_id,
                                "navigation_path": self.pending_preload_navigation_path.as_ref().map(|path| path.display().to_string()),
                                "metrics": Self::bench_metrics_payload(&metrics),
                            }),
                        );
                        self.active_preload_request_id = None;
                        self.pending_preload_navigation_path = None;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.log_bench_state(
                        "viewer.poll_preload_worker.disconnected",
                        serde_json::json!({}),
                    );
                    self.respawn_preload_worker();
                    break;
                }
            }
        }
    }

    pub(super) fn poll_companion_worker(&mut self) {
        loop {
            match self.companion_rx.try_recv() {
                Ok(RenderResult::Loaded {
                    request_id,
                    path,
                    source,
                    rendered,
                    companion: _,
                    metrics,
                }) => {
                    let Some(active_request) = self.companion_active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if !request_matches {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_companion_worker.loaded",
                        serde_json::json!({
                            "request_id": request_id,
                            "path": path.as_ref().map(|path| path.display().to_string()),
                            "companion_navigation_path": self.companion_navigation_path.as_ref().map(|path| path.display().to_string()),
                            "metrics": Self::bench_metrics_payload(&metrics),
                        }),
                    );
                    let (canvas, display_scale) = downscale_for_texture_limit(
                        rendered.frame_canvas(0),
                        self.max_texture_side,
                        self.render_options.zoom_method,
                    );
                    let image = self.color_image_from_canvas(&canvas);
                    let texture_options = self.texture_options();
                    let texture = if path.is_none() {
                        if let Some(texture) = self
                            .companion_display
                            .as_mut()
                            .and_then(|display| display.texture.as_mut())
                        {
                            texture.set(image, texture_options);
                            texture.clone()
                        } else {
                            self.egui_ctx
                                .load_texture("manga_companion", image, texture_options)
                        }
                    } else {
                        self.egui_ctx
                            .load_texture("manga_companion", image, texture_options)
                    };
                    if let Some(navigation_path) = self.companion_navigation_path.clone() {
                        self.remember_preloaded_entry(PreloadedEntry {
                            navigation_path,
                            load_path: path.clone(),
                            display: DisplayedPageState {
                                source: source.clone(),
                                rendered: rendered.clone(),
                                texture: Some(texture.clone()),
                                texture_display_scale: display_scale,
                            },
                        });
                    }
                    self.apply_companion_loaded(
                        path,
                        DisplayedPageState {
                            source,
                            rendered,
                            texture: Some(texture),
                            texture_display_scale: display_scale,
                        },
                    );
                }
                Ok(RenderResult::Failed {
                    request_id,
                    metrics,
                    ..
                }) => {
                    let Some(active_request) = self.companion_active_request else {
                        continue;
                    };
                    let request_matches = match active_request {
                        ActiveRenderRequest::Load(active_id)
                        | ActiveRenderRequest::Resize(active_id) => active_id == request_id,
                    };
                    if request_matches {
                        self.log_bench_state(
                            "viewer.poll_companion_worker.failed",
                            serde_json::json!({
                                "request_id": request_id,
                                "companion_navigation_path": self.companion_navigation_path.as_ref().map(|path| path.display().to_string()),
                                "metrics": Self::bench_metrics_payload(&metrics),
                            }),
                        );
                        self.clear_manga_companion();
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.log_bench_state(
                        "viewer.poll_companion_worker.disconnected",
                        serde_json::json!({}),
                    );
                    self.clear_manga_companion();
                    self.respawn_companion_worker();
                    if let Some(path) = self.desired_manga_companion_path() {
                        let _ = self.request_companion_load(path);
                    }
                    break;
                }
            }
        }
    }

    pub(super) fn poll_filesystem(&mut self) {
        loop {
            let result = match self.fs_rx.as_ref() {
                Some(rx) => rx.try_recv(),
                None => return,
            };
            match result {
                Ok(FilesystemResult::NavigatorReady {
                    request_id,
                    navigation_path,
                    load_path,
                }) => {
                    if self.active_fs_request_id == Some(request_id) {
                        self.log_bench_state(
                            "viewer.poll_filesystem.navigator_ready",
                            serde_json::json!({
                                "request_id": request_id,
                                "navigation_path": navigation_path.as_ref().map(|path| path.display().to_string()),
                                "load_path": load_path.as_ref().map(|path| path.display().to_string()),
                            }),
                        );
                        self.navigator_ready = true;
                        self.active_fs_request_id = None;
                        self.startup_phase = StartupPhase::MultiViewer;
                        self.log_bench_startup_sync_once("navigator_ready");
                        match (navigation_path, load_path) {
                            (Some(navigation_path), Some(load_path)) => {
                                self.empty_mode = false;
                                if self.current_navigation_path != navigation_path
                                    || self.current_path != load_path
                                {
                                    let _ = self.request_load_target(navigation_path, load_path);
                                }
                            }
                            (Some(navigation_path), None) => {
                                self.current_navigation_path = navigation_path;
                            }
                            _ => {
                                self.empty_mode = true;
                                self.show_filer = true;
                                self.overlay
                                    .set_loading_message("No displayable file found");
                            }
                        }
                        if self.active_request.is_none() && !self.empty_mode {
                            self.overlay.clear_loading_message();
                        }
                    }
                }
                Ok(FilesystemResult::CurrentSet) => {}
                Ok(FilesystemResult::PathResolved {
                    request_id,
                    navigation_path,
                    load_path,
                }) => {
                    if self.active_fs_request_id == Some(request_id) {
                        self.log_bench_state(
                            "viewer.poll_filesystem.path_resolved",
                            serde_json::json!({
                                "request_id": request_id,
                                "navigation_path": navigation_path.display().to_string(),
                                "load_path": load_path.display().to_string(),
                            }),
                        );
                        self.empty_mode = false;
                        self.startup_phase = StartupPhase::MultiViewer;
                        self.log_bench_startup_sync_once("path_resolved");
                        if self.current_navigation_path != navigation_path
                            || self.current_path != load_path
                        {
                            let _ = self.request_load_target(navigation_path, load_path);
                        }
                        self.active_fs_request_id = None;
                    }
                }
                Ok(FilesystemResult::NoPath { request_id }) => {
                    if self.active_fs_request_id == Some(request_id) {
                        self.log_bench_state(
                            "viewer.poll_filesystem.no_path",
                            serde_json::json!({
                                "request_id": request_id,
                            }),
                        );
                        self.startup_phase = StartupPhase::MultiViewer;
                        self.log_bench_startup_sync_once("no_path");
                        self.overlay
                            .set_loading_message("No displayable file found");
                        self.show_filer = true;
                        self.active_fs_request_id = None;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.log_bench_state(
                        "viewer.poll_filesystem.disconnected",
                        serde_json::json!({}),
                    );
                    self.overlay
                        .set_loading_message("filesystem worker disconnected");
                    self.respawn_filesystem_worker();
                    break;
                }
            }
        }
        if self.active_fs_request_id.is_none() {
            match take_next_queued_filesystem_work(
                &mut self.queued_filesystem_init_path,
                &mut self.queued_navigation,
            ) {
                Some(PendingFilesystemWork::Init(path)) => {
                    let _ = self.init_filesystem(path);
                }
                Some(PendingFilesystemWork::Command(command)) => {
                    let _ = self.request_navigation(command);
                }
                None => {}
            }
        }
    }

    pub(super) fn poll_filer_worker(&mut self) {
        loop {
            let result = match self.filer_rx.as_ref() {
                Some(rx) => rx.try_recv(),
                None => return,
            };
            match result {
                Ok(FilerResult::Reset {
                    request_id,
                    directory,
                    selected,
                }) => {
                    if self.filer.pending_request_id != Some(request_id) {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_filer_worker.reset",
                        serde_json::json!({
                            "request_id": request_id,
                            "directory": directory.display().to_string(),
                            "selected": selected.as_ref().map(|path| path.display().to_string()),
                        }),
                    );
                    self.filer.directory = Some(directory);
                    self.filer.entries.clear();
                    self.filer.selected = self.filer_selected_for_directory(
                        self.filer.directory.as_deref().unwrap(),
                        selected,
                    );
                }
                Ok(FilerResult::Append {
                    request_id,
                    entries,
                }) => {
                    if self.filer.pending_request_id != Some(request_id) {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_filer_worker.append",
                        serde_json::json!({
                            "request_id": request_id,
                            "entry_count": entries.len(),
                        }),
                    );
                    self.filer.entries.extend(entries);
                    self.sync_filer_selected_with_current_when_aligned();
                }
                Ok(FilerResult::Snapshot {
                    request_id,
                    directory,
                    entries,
                    selected,
                }) => {
                    if self.filer.pending_request_id != Some(request_id) {
                        continue;
                    }
                    self.log_bench_state(
                        "viewer.poll_filer_worker.snapshot",
                        serde_json::json!({
                            "request_id": request_id,
                            "directory": directory.display().to_string(),
                            "entry_count": entries.len(),
                            "selected": selected.as_ref().map(|path| path.display().to_string()),
                        }),
                    );
                    if matches!(
                        self.filer.pending_user_request.as_ref(),
                        Some(FilerUserRequest::BrowseDirectory { directory: browse_dir }) if browse_dir == &directory
                    ) {
                        self.filer.committed_browse_directory = Some(directory.clone());
                        self.log_bench_state(
                            "viewer.filer.browse_committed",
                            serde_json::json!({
                                "request_id": request_id,
                                "directory": directory.display().to_string(),
                            }),
                        );
                        self.filer.pending_user_request = None;
                    }
                    self.filer.pending_request_id = None;
                    self.filer.directory = Some(directory);
                    self.filer.entries = entries;
                    let snapshot_signature = filer_entries_signature(&self.filer.entries);
                    let snapshot_changed_in_same_directory =
                        filer_snapshot_changed_in_same_directory(
                            self.last_filer_snapshot_signature
                                .as_ref()
                                .map(|(directory, signature)| (directory.as_path(), *signature)),
                            self.filer.directory.as_deref().unwrap(),
                            snapshot_signature,
                        );
                    self.last_filer_snapshot_signature = Some((
                        self.filer.directory.as_deref().unwrap().to_path_buf(),
                        snapshot_signature,
                    ));
                    self.filer.selected = self.filer_selected_for_directory(
                        self.filer.directory.as_deref().unwrap(),
                        selected,
                    );
                    self.sync_filer_selected_with_current_when_aligned();
                    if snapshot_changed_in_same_directory
                        && self.navigator_ready
                        && self.active_fs_request_id.is_none()
                        && should_reinitialize_filesystem_from_filer_snapshot(
                            &self.current_navigation_path,
                            self.current_directory().as_deref(),
                            self.filer.directory.as_deref(),
                            &self.filer.entries,
                            self.filer.selected.as_deref(),
                        )
                    {
                        self.log_bench_state(
                            "viewer.filesystem.reinit_from_filer_snapshot",
                            serde_json::json!({
                                "directory": self.filer.directory.as_ref().map(|path| path.display().to_string()),
                                "entry_count": self.filer.entries.len(),
                                "current_navigation_path": self.current_navigation_path.display().to_string(),
                            }),
                        );
                        self.navigator_ready = false;
                        self.queued_navigation = None;
                        let _ = self.init_filesystem(self.current_navigation_path.clone());
                    }
                    if should_clear_filer_user_request_after_snapshot(
                        self.filer.pending_user_request.as_ref(),
                    ) {
                        self.filer.pending_user_request = None;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.respawn_filer_worker();
                    break;
                }
            }
        }
    }

    pub(super) fn poll_thumbnail_worker(&mut self) {
        loop {
            let result = match self.thumbnail_rx.as_ref() {
                Some(rx) => rx.try_recv(),
                None => return,
            };
            match result {
                Ok(ThumbnailResult::Ready {
                    _request_id: _,
                    path,
                    image,
                }) => {
                    self.thumbnail_pending.remove(&path);
                    let texture = self.egui_ctx.load_texture(
                        format!("thumb:{}", path.display()),
                        image,
                        TextureOptions::LINEAR,
                    );
                    self.thumbnail_cache.insert(path, texture);
                }
                Ok(ThumbnailResult::Failed {
                    _request_id: _,
                    path,
                    ..
                }) => {
                    self.thumbnail_pending.remove(&path);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.respawn_thumbnail_worker();
                    break;
                }
            }
        }
    }

    pub(crate) fn ensure_thumbnail(&mut self, path: &std::path::Path, max_side: u32) {
        self.spawn_navigation_workers();
        let Some(thumbnail_tx) = self.thumbnail_tx.clone() else {
            return;
        };
        if self.thumbnail_cache.contains_key(path) || self.thumbnail_pending.contains(path) {
            return;
        }
        let request_id = self.alloc_thumbnail_request_id();
        let path = path.to_path_buf();
        self.thumbnail_pending.insert(path.clone());
        let _ = thumbnail_tx.send(ThumbnailCommand::Generate {
            request_id,
            path,
            max_side,
        });
    }

    pub(super) fn sync_window_state(&mut self, ctx: &egui::Context) {
        let viewport = ctx.input(|i| i.viewport().clone());
        self.startup_window_sync_frames += 1;

        if let Some(fullscreen) = viewport.fullscreen {
            self.window_options.fullscreen = fullscreen;
        }

        if self.window_options.fullscreen || self.startup_window_sync_frames < 20 {
            return;
        }

        if self.window_options.remember_size {
            if let Some(inner_rect) = viewport.inner_rect {
                self.window_options.size = crate::ui::viewer::options::WindowSize::Exact {
                    width: inner_rect.width(),
                    height: inner_rect.height(),
                };
            }
        }

        if self.window_options.remember_position {
            if let Some(outer_rect) = viewport.outer_rect {
                self.window_options.start_position = WindowStartPosition::Exact {
                    x: outer_rect.min.x,
                    y: outer_rect.min.y,
                };
            }
        }
    }

    pub(super) fn poll_render_request_timeout(&mut self) {
        let Some(active_request) = self.active_request else {
            self.active_request_started_at = None;
            return;
        };
        let Some(started_at) = self.active_request_started_at else {
            self.active_request_started_at = Some(Instant::now());
            return;
        };
        if started_at.elapsed() < RENDER_REQUEST_TIMEOUT {
            return;
        }

        let timed_out_navigation_path = self.pending_navigation_path.take();
        self.log_bench_state(
            "viewer.poll_worker.timeout",
            serde_json::json!({
                "active_request": format!("{active_request:?}"),
                "elapsed_ms": started_at.elapsed().as_millis() as u64,
                "timeout_ms": RENDER_REQUEST_TIMEOUT.as_millis() as u64,
                "timed_out_navigation_path": timed_out_navigation_path.as_ref().map(|path| path.display().to_string()),
            }),
        );

        let timed_out_during_load = matches!(active_request, ActiveRenderRequest::Load(_));
        let should_advance = timed_out_during_load
            && should_advance_after_load_failure(
                &self.current_navigation_path,
                timed_out_navigation_path.as_deref(),
            );

        self.save_dialog.message = Some("Load timeout: request skipped".to_string());
        self.clear_current_image_display();
        self.show_loading_texture(true);
        self.overlay.clear_loading_message();
        self.active_request = None;
        self.active_request_started_at = None;
        self.respawn_render_worker();

        if matches!(
            self.filer.pending_user_request,
            Some(FilerUserRequest::SelectFile { .. })
        ) {
            self.filer.pending_user_request = None;
        }

        self.flush_pending_viewer_navigation();
        if should_advance {
            let _ = self.next_image();
        }
    }
}
