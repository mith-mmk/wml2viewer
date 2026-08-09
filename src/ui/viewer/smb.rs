use super::*;
use crate::filesystem::provider::CacheStore;
use crate::filesystem::smb::{SmbCredentials, SmbProvider};
use crate::filesystem::source::{
    SourceCommand, SourceEntry, SourceEntryKind, SourceResult, spawn_remote_source_worker,
};

#[derive(Default)]
pub(crate) struct SmbBrowserState {
    pub(crate) share: String,
    pub(crate) username: String,
    pub(crate) credential_reference: String,
    pub(crate) password: String,
    pub(crate) object: String,
    pub(crate) entries: Vec<SourceEntry>,
    pub(crate) status: String,
    pub(crate) connected: bool,
}

impl ViewerApp {
    pub(crate) fn smb_browser_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.collapsing("SMB共有", |ui| {
            ui.label("SMBはアプリ内ブラウザから接続します。パスワードはKeychainに保存されます。");
            ui.horizontal(|ui| {
                ui.label("共有");
                ui.text_edit_singleline(&mut self.smb_browser.share);
            });
            ui.horizontal(|ui| {
                ui.label("ユーザー");
                ui.text_edit_singleline(&mut self.smb_browser.username);
            });
            ui.horizontal(|ui| {
                ui.label("Keychain参照");
                ui.text_edit_singleline(&mut self.smb_browser.credential_reference);
            });
            ui.horizontal(|ui| {
                ui.label("パスワード");
                ui.add(egui::TextEdit::singleline(&mut self.smb_browser.password).password(true));
            });
            ui.horizontal(|ui| {
                if ui.button("接続").clicked() {
                    self.start_smb_connection();
                }
                if self.smb_browser.connected && ui.button("再読み込み").clicked() {
                    self.request_smb_reconnect(self.smb_browser.object.clone());
                }
            });
            if !self.smb_browser.status.is_empty() {
                ui.small(&self.smb_browser.status);
            }
            if self.smb_browser.connected {
                ui.separator();
                ui.label(format!("/{}", self.smb_browser.object));
                let entries = self.smb_browser.entries.clone();
                for entry in entries {
                    let label = if entry.kind == SourceEntryKind::Directory {
                        format!("📁 {}", entry.display_name)
                    } else {
                        entry.display_name.clone()
                    };
                    if ui.button(label).clicked() {
                        match entry.kind {
                            SourceEntryKind::Directory => {
                                self.smb_browser.object = source_object(&entry);
                                self.request_smb_list(self.smb_browser.object.clone());
                            }
                            SourceEntryKind::File if entry.is_openable() => {
                                let object = source_object(&entry);
                                let root = parent_object(&object);
                                self.start_smb_view(root, object);
                            }
                            SourceEntryKind::File => {}
                        }
                    }
                }
            }
        });
    }

    fn start_smb_connection(&mut self) {
        let share = self.smb_browser.share.trim().to_string();
        let username = self.smb_browser.username.trim().to_string();
        let reference = self.smb_browser.credential_reference.trim().to_string();
        if share.is_empty() || username.is_empty() || reference.is_empty() {
            self.smb_browser.status = "共有、ユーザー、Keychain参照を入力してください".to_string();
            return;
        }
        if !self.smb_browser.password.is_empty() {
            if !crate::dependent::save_smb_password(&reference, &self.smb_browser.password) {
                self.smb_browser.status = "Keychainへの保存に失敗しました".to_string();
                self.smb_browser.password.clear();
                return;
            }
            self.smb_browser.password.clear();
        }
        let credentials = match SmbCredentials::from_keychain(username, &reference) {
            Ok(credentials) => credentials,
            Err(error) => {
                self.smb_browser.status = format!("認証情報を取得できません: {error}");
                return;
            }
        };
        let cache_root = crate::dependent::default_temp_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("remote");
        let cache = match CacheStore::with_limits(
            cache_root,
            self.network.max_materialization_bytes,
            self.network.cache_capacity_bytes,
        ) {
            Ok(cache) => cache,
            Err(error) => {
                self.smb_browser.status = format!("キャッシュを準備できません: {error}");
                return;
            }
        };
        let provider = Arc::new(SmbProvider::new(share.clone(), credentials));
        let (tx, rx) = spawn_remote_source_worker(
            provider,
            "smb".to_string(),
            Some(share),
            cache,
            self.navigation_sort,
            self.network.prefetch_forward,
            self.network.prefetch_backward,
        );
        self.remote_tx = Some(tx);
        self.remote_rx = Some(rx);
        self.remote_mode = true;
        self.navigator_ready = false;
        self.smb_browser.connected = true;
        self.smb_browser.object.clear();
        self.smb_browser.status = "共有へ接続中…".to_string();
        self.request_smb_list(String::new());
    }

    fn request_smb_list(&mut self, object: String) {
        let Some(tx) = self.remote_tx.clone() else {
            return;
        };
        self.next_remote_request_id += 1;
        let request_id = self.next_remote_request_id;
        self.remote_active_request_id = Some(request_id);
        if tx
            .try_send(SourceCommand::List { request_id, object })
            .is_err()
        {
            self.remote_active_request_id = None;
            self.smb_browser.status =
                "SMB操作キューが満杯です。少し待って再試行してください".to_string();
        }
    }

    fn request_smb_reconnect(&mut self, object: String) {
        let Some(tx) = self.remote_tx.clone() else {
            return;
        };
        self.next_remote_request_id += 1;
        let request_id = self.next_remote_request_id;
        self.remote_active_request_id = Some(request_id);
        self.smb_browser.status = "SMBへ再接続中…".to_string();
        if tx
            .try_send(SourceCommand::Reconnect { request_id, object })
            .is_err()
        {
            self.remote_active_request_id = None;
            self.smb_browser.status =
                "SMB操作キューが満杯です。少し待って再試行してください".to_string();
        }
    }

    fn start_smb_view(&mut self, root_object: String, selected_object: String) {
        let Some(tx) = self.remote_tx.clone() else {
            return;
        };
        self.next_remote_request_id += 1;
        let request_id = self.next_remote_request_id;
        self.remote_active_request_id = Some(request_id);
        self.smb_browser.status = "ページを取得中…".to_string();
        self.set_show_filer(false);
        if tx
            .try_send(SourceCommand::Open {
                request_id,
                root_object,
                selected_object: Some(selected_object),
            })
            .is_err()
        {
            self.remote_active_request_id = None;
            self.smb_browser.status =
                "SMB操作キューが満杯です。少し待って再試行してください".to_string();
        }
    }

    pub(crate) fn request_remote_navigation(
        &mut self,
        command: SourceCommand,
        direction: Option<ImageTransitionDirection>,
    ) -> Result<(), Box<dyn Error>> {
        let Some(tx) = self.remote_tx.clone() else {
            return Ok(());
        };
        self.next_remote_request_id += 1;
        let request_id = self.next_remote_request_id;
        self.remote_active_request_id = Some(request_id);
        self.active_navigation_transition_direction = direction;
        let command = with_source_request_id(command, request_id);
        tx.try_send(command)
            .map_err(|error| -> Box<dyn Error> { Box::new(error) })?;
        Ok(())
    }

    pub(crate) fn poll_remote_source(&mut self) {
        loop {
            let result = match self.remote_rx.as_ref() {
                Some(rx) => rx.try_recv(),
                None => return,
            };
            match result {
                Ok(SourceResult::Listed {
                    request_id,
                    object,
                    entries,
                }) => {
                    if self.remote_active_request_id == Some(request_id) {
                        self.smb_browser.object = object;
                        self.smb_browser.entries = entries;
                        self.smb_browser.status = "接続済み".to_string();
                        self.remote_active_request_id = None;
                    }
                }
                Ok(SourceResult::Ready {
                    request_id,
                    target,
                    local_path,
                }) => {
                    if self.remote_active_request_id == Some(request_id) {
                        self.remote_active_request_id = None;
                        self.navigator_ready = true;
                        self.empty_mode = false;
                        self.smb_browser.status = format!("表示中: {}", target.display_name);
                        self.current_navigation_path = remote_display_path(&target);
                        self.current_path = local_path.clone();
                        let direction = self.active_navigation_transition_direction.take();
                        let _ = self.request_load_target_with_transition_direction(
                            self.current_navigation_path.clone(),
                            local_path,
                            direction,
                        );
                    }
                }
                Ok(SourceResult::NoPath { request_id }) => {
                    if self.remote_active_request_id == Some(request_id) {
                        self.remote_active_request_id = None;
                        self.smb_browser.status =
                            "これ以上表示できるファイルがありません".to_string();
                    }
                }
                Ok(SourceResult::Paused {
                    request_id,
                    message,
                }) => {
                    if self.remote_active_request_id == Some(request_id) {
                        self.smb_browser.status = format!("接続待ち: {message}");
                    }
                }
                Ok(SourceResult::Failed {
                    request_id,
                    message,
                }) => {
                    if self.remote_active_request_id == Some(request_id) {
                        self.remote_active_request_id = None;
                        self.smb_browser.status = format!("SMBエラー: {message}");
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.smb_browser.status = "SMB worker disconnected".to_string();
                    self.remote_tx = None;
                    self.remote_rx = None;
                    self.remote_mode = false;
                    break;
                }
            }
        }
    }
}

fn with_source_request_id(command: SourceCommand, request_id: u64) -> SourceCommand {
    match command {
        SourceCommand::Next { policy, .. } => SourceCommand::Next { request_id, policy },
        SourceCommand::Prev { policy, .. } => SourceCommand::Prev { request_id, policy },
        SourceCommand::First { .. } => SourceCommand::First { request_id },
        SourceCommand::Last { .. } => SourceCommand::Last { request_id },
        SourceCommand::Reconnect { object, .. } => SourceCommand::Reconnect { request_id, object },
        other => other,
    }
}

fn source_object(entry: &SourceEntry) -> String {
    match &entry.id {
        crate::filesystem::provider::SourceId::Local(path) => path.to_string_lossy().into_owned(),
        crate::filesystem::provider::SourceId::Remote { object, .. } => object.clone(),
    }
}

fn parent_object(object: &str) -> String {
    object
        .rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default()
}

fn remote_display_path(entry: &SourceEntry) -> PathBuf {
    let mut path = PathBuf::from(".remote");
    match &entry.id {
        crate::filesystem::provider::SourceId::Local(local) => path.push(local),
        crate::filesystem::provider::SourceId::Remote { provider, object } => {
            path.push(provider);
            path.push(object);
        }
    }
    path
}
