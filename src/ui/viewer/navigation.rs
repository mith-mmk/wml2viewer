use super::*;

impl ViewerApp {
    pub(super) fn handoff_filer_control_to_viewer_navigation(&mut self) {
        if should_cancel_filer_request_for_viewer_navigation(
            self.filer.pending_user_request.as_ref(),
        ) {
            self.log_bench_state(
                "viewer.filer.cancelled_for_viewer_navigation",
                serde_json::json!({
                    "pending_user_request": self.filer.pending_user_request.as_ref().map(|request| format!("{request:?}")),
                }),
            );
            self.filer.pending_user_request = None;
            self.filer.committed_browse_directory = None;
            return;
        }
        if !should_handoff_filer_control_to_viewer_navigation(
            self.filer.pending_user_request.as_ref(),
            self.filer.committed_browse_directory.as_deref(),
        ) {
            return;
        }
        self.log_bench_state(
            "viewer.filer.handoff_to_viewer_navigation",
            serde_json::json!({
                "committed_browse_directory": self.filer.committed_browse_directory.as_ref().map(|path| path.display().to_string()),
            }),
        );
        self.filer.committed_browse_directory = None;
    }

    pub(crate) fn next_image(&mut self) -> Result<(), Box<dyn Error>> {
        self.cancel_pending_single_click_navigation();
        if !self.can_trigger_navigation() {
            return Ok(());
        }
        if self.navigation_blocked_by_active_load() {
            self.queue_viewer_navigation(PendingViewerNavigation::Next);
            return Ok(());
        }
        self.handoff_filer_control_to_viewer_navigation();
        if let Some(target) = self.manga_navigation_target(true) {
            self.request_load_path(target)?;
            self.last_navigation_at = Some(Instant::now());
            return Ok(());
        }
        let command = if self.filer.ascending {
            FilesystemCommand::Next {
                request_id: 0,
                policy: self.end_of_folder,
            }
        } else {
            FilesystemCommand::Prev {
                request_id: 0,
                policy: self.end_of_folder,
            }
        };
        self.request_navigation(command)?;
        self.last_navigation_at = Some(Instant::now());
        Ok(())
    }

    pub(crate) fn prev_image(&mut self) -> Result<(), Box<dyn Error>> {
        self.cancel_pending_single_click_navigation();
        if !self.can_trigger_navigation() {
            return Ok(());
        }
        if self.navigation_blocked_by_active_load() {
            self.queue_viewer_navigation(PendingViewerNavigation::Prev);
            return Ok(());
        }
        self.handoff_filer_control_to_viewer_navigation();
        if let Some(target) = self.manga_navigation_target(false) {
            self.request_load_path(target)?;
            self.last_navigation_at = Some(Instant::now());
            return Ok(());
        }
        let command = if self.filer.ascending {
            FilesystemCommand::Prev {
                request_id: 0,
                policy: self.end_of_folder,
            }
        } else {
            FilesystemCommand::Next {
                request_id: 0,
                policy: self.end_of_folder,
            }
        };
        self.request_navigation(command)?;
        self.last_navigation_at = Some(Instant::now());
        Ok(())
    }

    pub(crate) fn first_image(&mut self) -> Result<(), Box<dyn Error>> {
        self.cancel_pending_single_click_navigation();
        if !self.can_trigger_navigation() {
            return Ok(());
        }
        if self.navigation_blocked_by_active_load() {
            self.queue_viewer_navigation(PendingViewerNavigation::First);
            return Ok(());
        }
        self.handoff_filer_control_to_viewer_navigation();
        if self.should_apply_edge_noop(PendingViewerNavigation::First)
            && self.navigation_edge_reached(PendingViewerNavigation::First)
        {
            self.log_bench_state(
                "viewer.navigation.edge_noop",
                serde_json::json!({
                    "navigation": "First",
                    "path": self.current_navigation_path.display().to_string(),
                }),
            );
            return Ok(());
        }
        if let Some((target, is_container)) =
            self.filer_edge_navigation_target(PendingViewerNavigation::First)
        {
            if should_skip_edge_navigation_for_same_target(
                &self.current_navigation_path,
                &target,
                PendingViewerNavigation::First,
            ) {
                self.log_bench_state(
                    "viewer.navigation.edge_noop_same_target",
                    serde_json::json!({
                        "navigation": "First",
                        "path": self.current_navigation_path.display().to_string(),
                    }),
                );
                return Ok(());
            }
            self.request_filer_edge_target_navigation(
                target,
                is_container,
                PendingViewerNavigation::First,
            )?;
            self.last_navigation_at = Some(Instant::now());
            return Ok(());
        }
        let command = if self.filer.ascending {
            FilesystemCommand::First { request_id: 0 }
        } else {
            FilesystemCommand::Last { request_id: 0 }
        };
        self.request_navigation(command)?;
        self.last_navigation_at = Some(Instant::now());
        Ok(())
    }

    pub(crate) fn last_image(&mut self) -> Result<(), Box<dyn Error>> {
        self.cancel_pending_single_click_navigation();
        if !self.can_trigger_navigation() {
            return Ok(());
        }
        if self.navigation_blocked_by_active_load() {
            self.queue_viewer_navigation(PendingViewerNavigation::Last);
            return Ok(());
        }
        self.handoff_filer_control_to_viewer_navigation();
        if self.should_apply_edge_noop(PendingViewerNavigation::Last)
            && self.navigation_edge_reached(PendingViewerNavigation::Last)
        {
            self.log_bench_state(
                "viewer.navigation.edge_noop",
                serde_json::json!({
                    "navigation": "Last",
                    "path": self.current_navigation_path.display().to_string(),
                }),
            );
            return Ok(());
        }
        if let Some((target, is_container)) =
            self.filer_edge_navigation_target(PendingViewerNavigation::Last)
        {
            if should_skip_edge_navigation_for_same_target(
                &self.current_navigation_path,
                &target,
                PendingViewerNavigation::Last,
            ) {
                self.log_bench_state(
                    "viewer.navigation.edge_noop_same_target",
                    serde_json::json!({
                        "navigation": "Last",
                        "path": self.current_navigation_path.display().to_string(),
                    }),
                );
                return Ok(());
            }
            self.request_filer_edge_target_navigation(
                target,
                is_container,
                PendingViewerNavigation::Last,
            )?;
            self.last_navigation_at = Some(Instant::now());
            return Ok(());
        }
        let command = if self.filer.ascending {
            FilesystemCommand::Last { request_id: 0 }
        } else {
            FilesystemCommand::First { request_id: 0 }
        };
        self.request_navigation(command)?;
        self.last_navigation_at = Some(Instant::now());
        Ok(())
    }

    pub(super) fn can_trigger_navigation(&self) -> bool {
        self.last_navigation_at
            .map(|last| last.elapsed() >= NAVIGATION_REPEAT_INTERVAL)
            .unwrap_or(true)
    }

    pub(super) fn request_filer_edge_target_navigation(
        &mut self,
        target: PathBuf,
        is_container: bool,
        navigation: PendingViewerNavigation,
    ) -> Result<(), Box<dyn Error>> {
        if is_container {
            self.browse_filer_directory(target.clone());
            let resolved = match navigation {
                PendingViewerNavigation::First => resolve_start_path(&target),
                PendingViewerNavigation::Last => resolve_end_path(&target),
                PendingViewerNavigation::Next | PendingViewerNavigation::Prev => None,
            };
            if let Some(path) = resolved {
                return self.request_load_path(path);
            }
            return Ok(());
        }
        self.request_load_path(target)
    }

    pub(crate) fn request_load_path(&mut self, path: PathBuf) -> Result<(), Box<dyn Error>> {
        self.request_load_target(path.clone(), path)
    }

    pub(crate) fn set_show_filer(&mut self, show: bool) {
        if self.show_filer == show {
            return;
        }
        self.show_filer = show;
        if show {
            self.filer.committed_browse_directory = None;
            self.pending_filer_focus_path = Some(self.current_navigation_path.clone());
            self.sync_filer_directory_with_current_path();
            return;
        }

        self.pending_filer_focus_path = None;
        self.filer.committed_browse_directory = None;
        if should_clear_filer_request_on_hide(self.filer.pending_user_request.as_ref()) {
            self.log_bench_state(
                "viewer.filer.pending_request_cleared_on_hide",
                serde_json::json!({
                    "pending_user_request": self.filer.pending_user_request.as_ref().map(|request| format!("{request:?}")),
                }),
            );
            self.filer.pending_user_request = None;
        }
    }

    pub(crate) fn set_show_subfiler(&mut self, show: bool) {
        self.show_subfiler = show;
        if show {
            self.pending_subfiler_focus_path = Some(self.current_navigation_path.clone());
        } else {
            self.pending_subfiler_focus_path = None;
        }
    }

    pub(crate) fn sync_navigation_sort_with_filer_sort(&mut self) {
        let desired = navigation_sort_for_filer(self.filer.sort_field, self.filer.name_sort_mode);
        if self.navigation_sort == desired {
            return;
        }
        self.navigation_sort = desired;
        self.log_bench_state(
            "viewer.navigation_sort.synced_from_filer",
            serde_json::json!({
                "navigation_sort": format!("{:?}", self.navigation_sort),
                "filer_sort_field": format!("{:?}", self.filer.sort_field),
                "filer_name_sort_mode": format!("{:?}", self.filer.name_sort_mode),
            }),
        );
        self.respawn_filesystem_worker();
    }

}


