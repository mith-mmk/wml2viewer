//! Provider-independent source identity and remote navigation.
//!
//! Local browsing continues to use [`super::FileNavigator`].  This module
//! supplies the same ordering and end-of-folder semantics for providers that
//! cannot be represented by a local `PathBuf`, such as SMB shares.

use super::provider::{
    CacheStore, CancelToken, ProviderError, ReadOnlyProvider, RemoteEntry, SourceId,
};
use crate::options::{EndOfFolderOption, NavigationSortOption};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::thread;
use std::time::SystemTime;

const IMAGE_EXTENSIONS: &[&str] = &[
    "avif", "bmp", "gif", "heic", "heif", "jpeg", "jpg", "jxl", "png", "webp",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceEntryKind {
    File,
    Directory,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEntry {
    pub id: SourceId,
    pub display_name: String,
    pub kind: SourceEntryKind,
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub version: Option<String>,
}

impl SourceEntry {
    pub fn is_directory(&self) -> bool {
        self.kind == SourceEntryKind::Directory
    }

    pub fn is_openable(&self) -> bool {
        if self.is_directory() {
            return false;
        }
        let name = self.display_name.to_ascii_lowercase();
        let extension = name.rsplit_once('.').map(|(_, extension)| extension);
        extension.is_some_and(|extension| {
            IMAGE_EXTENSIONS.contains(&extension)
                || matches!(extension, "zip" | "lha" | "lzh" | "wmltxt")
        })
    }
}

impl RemoteEntry {
    pub fn into_source_entry(self, provider: impl Into<String>) -> SourceEntry {
        let kind = if self.is_directory {
            SourceEntryKind::Directory
        } else {
            SourceEntryKind::File
        };
        SourceEntry {
            id: SourceId::Remote {
                provider: provider.into(),
                object: self.object,
            },
            display_name: self.name,
            kind,
            size: self.size,
            modified: None,
            version: self.version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationTarget {
    pub entry: SourceEntry,
}

#[derive(Debug)]
pub enum NavigationError {
    Provider(ProviderError),
    NoPath,
}

impl From<ProviderError> for NavigationError {
    fn from(error: ProviderError) -> Self {
        Self::Provider(error)
    }
}

/// A remote source navigator backed by a read-only provider.
///
/// Directory listings are cached for the lifetime of this navigator.  The
/// provider remains the source of truth; callers can use [`Self::refresh`] to
/// invalidate the listings after reconnecting or receiving a stale result.
pub struct RemoteNavigator {
    provider: Arc<dyn ReadOnlyProvider>,
    provider_name: String,
    share: Option<String>,
    root_object: String,
    current: SourceEntry,
    sort: NavigationSortOption,
    listings: HashMap<String, Vec<RemoteEntry>>,
}

impl RemoteNavigator {
    pub fn open(
        provider: Arc<dyn ReadOnlyProvider>,
        provider_name: impl Into<String>,
        root_object: impl Into<String>,
        sort: NavigationSortOption,
        cancel: &CancelToken,
    ) -> Result<Self, NavigationError> {
        Self::open_with_share(provider, provider_name, None, root_object, sort, cancel)
    }

    pub fn open_smb(
        provider: Arc<dyn ReadOnlyProvider>,
        share: impl Into<String>,
        root_object: impl Into<String>,
        sort: NavigationSortOption,
        cancel: &CancelToken,
    ) -> Result<Self, NavigationError> {
        Self::open_with_share(
            provider,
            "smb",
            Some(share.into()),
            root_object,
            sort,
            cancel,
        )
    }

    fn open_at(
        provider: Arc<dyn ReadOnlyProvider>,
        provider_name: impl Into<String>,
        share: Option<String>,
        root_object: impl Into<String>,
        selected_object: Option<&str>,
        sort: NavigationSortOption,
        cancel: &CancelToken,
    ) -> Result<Self, NavigationError> {
        let mut navigator =
            Self::open_with_share(provider, provider_name, share, root_object, sort, cancel)?;
        if let Some(selected_object) = selected_object {
            let selected_object = normalize_object(selected_object);
            let target = navigator
                .flatten_files(cancel)?
                .into_iter()
                .find(|entry| remote_object(&entry.id) == selected_object)
                .ok_or(NavigationError::NoPath)?;
            navigator.current = target;
        }
        Ok(navigator)
    }

    fn open_with_share(
        provider: Arc<dyn ReadOnlyProvider>,
        provider_name: impl Into<String>,
        share: Option<String>,
        root_object: impl Into<String>,
        sort: NavigationSortOption,
        cancel: &CancelToken,
    ) -> Result<Self, NavigationError> {
        let provider_name = provider_name.into();
        let root_object = normalize_object(&root_object.into());
        let mut navigator = Self {
            provider,
            provider_name,
            share,
            root_object,
            current: SourceEntry {
                id: SourceId::Remote {
                    provider: String::new(),
                    object: String::new(),
                },
                display_name: String::new(),
                kind: SourceEntryKind::Directory,
                size: None,
                modified: None,
                version: None,
            },
            sort,
            listings: HashMap::new(),
        };
        let first = navigator
            .flatten_files(cancel)?
            .into_iter()
            .next()
            .ok_or(NavigationError::NoPath)?;
        navigator.current = first;
        Ok(navigator)
    }

    pub fn current(&self) -> &SourceEntry {
        &self.current
    }

    pub fn refresh(&mut self) {
        self.listings.clear();
    }

    pub fn first(&mut self, cancel: &CancelToken) -> Result<NavigationTarget, NavigationError> {
        let entry = self
            .current_container_files(cancel)?
            .into_iter()
            .next()
            .ok_or(NavigationError::NoPath)?;
        self.current = entry.clone();
        Ok(NavigationTarget { entry })
    }

    pub fn last(&mut self, cancel: &CancelToken) -> Result<NavigationTarget, NavigationError> {
        let entry = self
            .current_container_files(cancel)?
            .into_iter()
            .last()
            .ok_or(NavigationError::NoPath)?;
        self.current = entry.clone();
        Ok(NavigationTarget { entry })
    }

    pub fn prefetch_candidates(
        &mut self,
        forward_count: usize,
        backward_count: usize,
        cancel: &CancelToken,
    ) -> Result<Vec<SourceEntry>, NavigationError> {
        let files = self.flatten_files(cancel)?;
        let current_index = files
            .iter()
            .position(|entry| entry.id == self.current.id)
            .ok_or(NavigationError::NoPath)?;
        let mut candidates = Vec::with_capacity(forward_count + backward_count);
        for index in (current_index + 1)..=(current_index + forward_count) {
            if let Some(entry) = files.get(index) {
                candidates.push(entry.clone());
            }
        }
        for index in current_index.saturating_sub(backward_count)..current_index {
            if let Some(entry) = files.get(index) {
                candidates.push(entry.clone());
            }
        }
        Ok(candidates)
    }

    pub fn next(
        &mut self,
        policy: EndOfFolderOption,
        cancel: &CancelToken,
    ) -> Result<NavigationTarget, NavigationError> {
        self.move_by(1, policy, cancel)
    }

    pub fn prev(
        &mut self,
        policy: EndOfFolderOption,
        cancel: &CancelToken,
    ) -> Result<NavigationTarget, NavigationError> {
        self.move_by(-1, policy, cancel)
    }

    fn move_by(
        &mut self,
        delta: isize,
        policy: EndOfFolderOption,
        cancel: &CancelToken,
    ) -> Result<NavigationTarget, NavigationError> {
        let files = match policy {
            EndOfFolderOption::Recursive => self.flatten_files(cancel)?,
            EndOfFolderOption::Stop | EndOfFolderOption::Next | EndOfFolderOption::Loop => {
                self.current_container_files(cancel)?
            }
        };
        let current_index = files
            .iter()
            .position(|entry| entry.id == self.current.id)
            .ok_or(NavigationError::NoPath)?;
        let candidate = if delta > 0 {
            files.get(current_index + 1).cloned()
        } else {
            current_index
                .checked_sub(1)
                .and_then(|index| files.get(index).cloned())
        };

        let entry = match candidate {
            Some(entry) => entry,
            None => match policy {
                EndOfFolderOption::Stop => return Err(NavigationError::NoPath),
                EndOfFolderOption::Loop => if delta > 0 {
                    files.first().cloned()
                } else {
                    files.last().cloned()
                }
                .ok_or(NavigationError::NoPath)?,
                // A remote tree is represented as one logical traversal.  A
                // sibling directory therefore has the same observable page
                // order as the existing Recursive local behavior.
                EndOfFolderOption::Next => self
                    .adjacent_directory(delta > 0, cancel)?
                    .ok_or(NavigationError::NoPath)?,
                EndOfFolderOption::Recursive => return Err(NavigationError::NoPath),
            },
        };
        self.current = entry.clone();
        Ok(NavigationTarget { entry })
    }

    fn flatten_files(&mut self, cancel: &CancelToken) -> Result<Vec<SourceEntry>, NavigationError> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        self.flatten_directory(&self.root_object.clone(), &mut visited, &mut result, cancel)?;
        Ok(result)
    }

    fn current_container_files(
        &mut self,
        cancel: &CancelToken,
    ) -> Result<Vec<SourceEntry>, NavigationError> {
        let object = remote_object(&self.current.id);
        let parent = object
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        let entries = self.list_directory(parent, cancel)?;
        Ok(entries
            .into_iter()
            .filter(|entry| !entry.is_directory)
            .map(|entry| self.source_entry(entry))
            .filter(SourceEntry::is_openable)
            .collect())
    }

    fn adjacent_directory(
        &mut self,
        forward: bool,
        cancel: &CancelToken,
    ) -> Result<Option<SourceEntry>, NavigationError> {
        let current_object = remote_object(&self.current.id);
        let current_dir = current_object
            .rsplit_once('/')
            .map(|(parent, _)| parent.to_string())
            .unwrap_or_default();
        if current_dir == self.root_object {
            return Ok(None);
        }
        let parent = current_dir
            .rsplit_once('/')
            .map(|(parent, _)| parent.to_string())
            .unwrap_or_default();
        let directories = self
            .list_directory(&parent, cancel)?
            .into_iter()
            .filter(|entry| entry.is_directory)
            .collect::<Vec<_>>();
        let current_index = directories
            .iter()
            .position(|entry| normalize_object(&entry.object) == current_dir);
        let Some(current_index) = current_index else {
            return Ok(None);
        };
        let candidates = if forward {
            directories
                .into_iter()
                .skip(current_index + 1)
                .collect::<Vec<_>>()
        } else {
            directories
                .into_iter()
                .take(current_index)
                .rev()
                .collect::<Vec<_>>()
        };
        for directory in candidates {
            let mut files = self
                .list_directory(&directory.object, cancel)?
                .into_iter()
                .filter(|entry| !entry.is_directory)
                .map(|entry| self.source_entry(entry))
                .filter(SourceEntry::is_openable);
            if let Some(file) = files.next() {
                return Ok(Some(file));
            }
        }
        Ok(None)
    }

    fn flatten_directory(
        &mut self,
        object: &str,
        visited: &mut HashSet<String>,
        result: &mut Vec<SourceEntry>,
        cancel: &CancelToken,
    ) -> Result<(), NavigationError> {
        if cancel.is_cancelled() {
            return Err(NavigationError::Provider(ProviderError::Cancelled));
        }
        if !visited.insert(object.to_string()) {
            return Ok(());
        }
        let entries = self.list_directory(object, cancel)?;
        for entry in entries.iter().filter(|entry| !entry.is_directory) {
            let entry = self.source_entry(entry.clone());
            if entry.is_openable() {
                result.push(entry);
            }
        }
        for entry in entries.iter().filter(|entry| entry.is_directory) {
            self.flatten_directory(&entry.object, visited, result, cancel)?;
        }
        Ok(())
    }

    fn source_entry(&self, entry: RemoteEntry) -> SourceEntry {
        let kind = if entry.is_directory {
            SourceEntryKind::Directory
        } else {
            SourceEntryKind::File
        };
        let id = match &self.share {
            Some(share) => SourceId::smb(share, entry.object.clone(), entry.version.clone()),
            None => SourceId::Remote {
                provider: self.provider_name.clone(),
                object: entry.object.clone(),
            },
        };
        SourceEntry {
            id,
            display_name: entry.name,
            kind,
            size: entry.size,
            modified: None,
            version: entry.version,
        }
    }

    fn list_directory(
        &mut self,
        object: &str,
        cancel: &CancelToken,
    ) -> Result<Vec<RemoteEntry>, NavigationError> {
        cancel.check().map_err(NavigationError::Provider)?;
        let object = normalize_object(object);
        if let Some(entries) = self.listings.get(&object) {
            return Ok(entries.clone());
        }
        let mut entries = self.provider.list(&object, cancel)?;
        sort_remote_entries(&mut entries, self.sort);
        self.listings.insert(object, entries.clone());
        Ok(entries)
    }
}

fn normalize_object(object: &str) -> String {
    object.trim_matches(['/', '\\']).to_string()
}

fn sort_remote_entries(entries: &mut [RemoteEntry], sort: NavigationSortOption) {
    entries.sort_by(|left, right| {
        let ordering = match sort {
            NavigationSortOption::OsName | NavigationSortOption::Name => {
                left.name.to_lowercase().cmp(&right.name.to_lowercase())
            }
            NavigationSortOption::NameCaseSensitive => left.name.cmp(&right.name),
            NavigationSortOption::NameCaseInsensitive => {
                left.name.to_lowercase().cmp(&right.name.to_lowercase())
            }
            NavigationSortOption::Date => left.version.cmp(&right.version),
            NavigationSortOption::Size => left.size.cmp(&right.size),
        };
        ordering.then_with(|| left.name.cmp(&right.name))
    });
}

#[derive(Clone, Debug)]
pub enum SourceCommand {
    Open {
        request_id: u64,
        root_object: String,
        selected_object: Option<String>,
    },
    List {
        request_id: u64,
        object: String,
    },
    Next {
        request_id: u64,
        policy: EndOfFolderOption,
    },
    Prev {
        request_id: u64,
        policy: EndOfFolderOption,
    },
    First {
        request_id: u64,
    },
    Last {
        request_id: u64,
    },
    Refresh {
        request_id: u64,
    },
    Reconnect {
        request_id: u64,
        object: String,
    },
    Cancel,
}

#[derive(Debug)]
pub enum SourceResult {
    Ready {
        request_id: u64,
        target: SourceEntry,
        local_path: PathBuf,
    },
    NoPath {
        request_id: u64,
    },
    Listed {
        request_id: u64,
        object: String,
        entries: Vec<SourceEntry>,
    },
    Paused {
        request_id: u64,
        message: String,
    },
    Failed {
        request_id: u64,
        message: String,
    },
}

struct PrefetchTask {
    generation: u64,
    entries: Vec<SourceEntry>,
    cancel: CancelToken,
}

/// Starts a provider-backed worker without changing the existing local
/// `FilesystemCommand` API.  The viewer can adopt this worker incrementally;
/// local and Android builds continue to use the established path worker.
pub fn spawn_remote_source_worker(
    provider: Arc<dyn ReadOnlyProvider>,
    provider_name: String,
    share: Option<String>,
    cache: CacheStore,
    sort: NavigationSortOption,
    prefetch_forward: usize,
    prefetch_backward: usize,
) -> (SyncSender<SourceCommand>, Receiver<SourceResult>) {
    const MAX_SOURCE_COMMANDS: usize = 16;
    let (command_tx, command_rx) = mpsc::sync_channel(MAX_SOURCE_COMMANDS);
    let (result_tx, result_rx) = mpsc::channel();
    let (prefetch_tx, prefetch_rx) = mpsc::channel::<PrefetchTask>();
    let prefetch_generation = Arc::new(AtomicU64::new(0));
    let prefetch_control = Arc::new(Mutex::new(None::<CancelToken>));
    let prefetch_control_for_worker = prefetch_control.clone();
    let prefetch_generation_for_worker = prefetch_generation.clone();
    let prefetch_provider = provider.clone();
    let prefetch_cache = cache.clone();

    thread::spawn(move || {
        while let Ok(task) = prefetch_rx.recv() {
            if task.generation != prefetch_generation.load(Ordering::Acquire) {
                continue;
            }
            for entry in task.entries {
                if task.generation != prefetch_generation.load(Ordering::Acquire) {
                    break;
                }
                let _ = prefetch_provider.materialize(
                    &remote_object(&entry.id),
                    entry.version.as_deref(),
                    &prefetch_cache,
                    &task.cancel,
                );
            }
        }
    });

    thread::spawn(move || {
        let mut navigator: Option<RemoteNavigator> = None;
        let mut current_cancel = CancelToken::default();
        while let Ok(command) = command_rx.recv() {
            match command {
                SourceCommand::Cancel => {
                    current_cancel.cancel();
                    cancel_prefetch(&prefetch_control_for_worker);
                    prefetch_generation_for_worker.fetch_add(1, Ordering::AcqRel);
                }
                SourceCommand::Open {
                    request_id,
                    root_object,
                    selected_object,
                } => {
                    current_cancel = CancelToken::default();
                    let result = match share.clone() {
                        Some(share) => RemoteNavigator::open_at(
                            provider.clone(),
                            "smb",
                            Some(share),
                            root_object,
                            selected_object.as_deref(),
                            sort,
                            &current_cancel,
                        ),
                        None => RemoteNavigator::open_at(
                            provider.clone(),
                            provider_name.clone(),
                            None,
                            root_object,
                            selected_object.as_deref(),
                            sort,
                            &current_cancel,
                        ),
                    }
                    .and_then(|mut remote| {
                        materialize_current(&mut remote, &cache, &current_cancel)
                            .map(|(target, path)| (remote, target, path))
                    });
                    match result {
                        Ok((remote, target, path)) => {
                            navigator = Some(remote);
                            let _ = result_tx.send(SourceResult::Ready {
                                request_id,
                                target,
                                local_path: path,
                            });
                            schedule_prefetch(
                                navigator.as_mut().expect("navigator installed"),
                                &prefetch_tx,
                                &prefetch_generation_for_worker,
                                &prefetch_control_for_worker,
                                &current_cancel,
                                prefetch_forward,
                                prefetch_backward,
                            );
                        }
                        Err(error) => send_navigation_error(&result_tx, request_id, error),
                    }
                }
                SourceCommand::List { request_id, object } => {
                    current_cancel = CancelToken::default();
                    match provider.list(&object, &current_cancel) {
                        Ok(entries) => {
                            let entries = entries
                                .into_iter()
                                .map(|entry| {
                                    let kind = if entry.is_directory {
                                        SourceEntryKind::Directory
                                    } else {
                                        SourceEntryKind::File
                                    };
                                    let id = match &share {
                                        Some(share) => SourceId::smb(
                                            share,
                                            entry.object.clone(),
                                            entry.version.clone(),
                                        ),
                                        None => SourceId::Remote {
                                            provider: provider_name.clone(),
                                            object: entry.object.clone(),
                                        },
                                    };
                                    SourceEntry {
                                        id,
                                        display_name: entry.name,
                                        kind,
                                        size: entry.size,
                                        modified: None,
                                        version: entry.version,
                                    }
                                })
                                .collect();
                            let _ = result_tx.send(SourceResult::Listed {
                                request_id,
                                object,
                                entries,
                            });
                        }
                        Err(error) => send_navigation_error(
                            &result_tx,
                            request_id,
                            NavigationError::Provider(error),
                        ),
                    }
                }
                command @ (SourceCommand::Next { .. }
                | SourceCommand::Prev { .. }
                | SourceCommand::First { .. }
                | SourceCommand::Last { .. }) => {
                    let Some(remote) = navigator.as_mut() else {
                        let request_id = source_request_id(&command);
                        let _ = result_tx.send(SourceResult::NoPath { request_id });
                        continue;
                    };
                    current_cancel = CancelToken::default();
                    let request_id = source_request_id(&command);
                    let outcome = match command {
                        SourceCommand::Next { policy, .. } => remote.next(policy, &current_cancel),
                        SourceCommand::Prev { policy, .. } => remote.prev(policy, &current_cancel),
                        SourceCommand::First { .. } => remote.first(&current_cancel),
                        SourceCommand::Last { .. } => remote.last(&current_cancel),
                        _ => unreachable!(),
                    };
                    match outcome.and_then(|target| {
                        materialize_current(remote, &cache, &current_cancel)
                            .map(|(resolved, path)| (target, resolved, path))
                    }) {
                        Ok((target, _, path)) => {
                            let _ = result_tx.send(SourceResult::Ready {
                                request_id,
                                target: target.entry,
                                local_path: path,
                            });
                            schedule_prefetch(
                                remote,
                                &prefetch_tx,
                                &prefetch_generation_for_worker,
                                &prefetch_control_for_worker,
                                &current_cancel,
                                prefetch_forward,
                                prefetch_backward,
                            );
                        }
                        Err(error) => send_navigation_error(&result_tx, request_id, error),
                    }
                }
                SourceCommand::Refresh { request_id } => {
                    if let Some(remote) = navigator.as_mut() {
                        remote.refresh();
                        let _ = result_tx.send(SourceResult::NoPath { request_id });
                    } else {
                        let _ = result_tx.send(SourceResult::NoPath { request_id });
                    }
                }
                SourceCommand::Reconnect { request_id, object } => {
                    current_cancel = CancelToken::default();
                    match provider
                        .reconnect()
                        .and_then(|_| provider.list(&object, &current_cancel))
                    {
                        Ok(entries) => {
                            if let Some(remote) = navigator.as_mut() {
                                remote.refresh();
                            }
                            let entries = entries
                                .into_iter()
                                .map(|entry| {
                                    let kind = if entry.is_directory {
                                        SourceEntryKind::Directory
                                    } else {
                                        SourceEntryKind::File
                                    };
                                    let id = match &share {
                                        Some(share) => SourceId::smb(
                                            share,
                                            entry.object.clone(),
                                            entry.version.clone(),
                                        ),
                                        None => SourceId::Remote {
                                            provider: provider_name.clone(),
                                            object: entry.object.clone(),
                                        },
                                    };
                                    SourceEntry {
                                        id,
                                        display_name: entry.name,
                                        kind,
                                        size: entry.size,
                                        modified: None,
                                        version: entry.version,
                                    }
                                })
                                .collect();
                            let _ = result_tx.send(SourceResult::Listed {
                                request_id,
                                object,
                                entries,
                            });
                        }
                        Err(error) => send_navigation_error(
                            &result_tx,
                            request_id,
                            NavigationError::Provider(error),
                        ),
                    }
                }
            }
        }
        prefetch_generation_for_worker.fetch_add(1, Ordering::AcqRel);
    });

    (command_tx, result_rx)
}

fn source_request_id(command: &SourceCommand) -> u64 {
    match command {
        SourceCommand::Open { request_id, .. }
        | SourceCommand::List { request_id, .. }
        | SourceCommand::Next { request_id, .. }
        | SourceCommand::Prev { request_id, .. }
        | SourceCommand::First { request_id }
        | SourceCommand::Last { request_id }
        | SourceCommand::Refresh { request_id }
        | SourceCommand::Reconnect { request_id, .. } => *request_id,
        SourceCommand::Cancel => 0,
    }
}

fn remote_object(id: &SourceId) -> String {
    match id {
        SourceId::Remote { object, .. } => object.clone(),
        SourceId::Local(path) => path.to_string_lossy().into_owned(),
    }
}

fn materialize_current(
    navigator: &mut RemoteNavigator,
    cache: &CacheStore,
    cancel: &CancelToken,
) -> Result<(SourceEntry, PathBuf), NavigationError> {
    let target = navigator.current().clone();
    let object = remote_object(&target.id);
    let path = navigator
        .provider
        .materialize(&object, target.version.as_deref(), cache, cancel)
        .map_err(NavigationError::Provider)?;
    Ok((target, path))
}

fn schedule_prefetch(
    navigator: &mut RemoteNavigator,
    tx: &Sender<PrefetchTask>,
    generation: &AtomicU64,
    control: &Arc<Mutex<Option<CancelToken>>>,
    current_cancel: &CancelToken,
    forward_count: usize,
    backward_count: usize,
) {
    let cancel = CancelToken::default();
    if let Ok(mut active) = control.lock() {
        if let Some(previous) = active.replace(cancel.clone()) {
            previous.cancel();
        }
    }
    let next_generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
    if let Ok(entries) =
        navigator.prefetch_candidates(forward_count, backward_count, current_cancel)
    {
        let _ = tx.send(PrefetchTask {
            generation: next_generation,
            entries,
            cancel,
        });
    }
}

fn cancel_prefetch(control: &Arc<Mutex<Option<CancelToken>>>) {
    if let Ok(mut active) = control.lock() {
        if let Some(cancel) = active.take() {
            cancel.cancel();
        }
    }
}

fn send_navigation_error(tx: &Sender<SourceResult>, request_id: u64, error: NavigationError) {
    match error {
        NavigationError::Provider(ProviderError::Connection(message)) => {
            let _ = tx.send(SourceResult::Paused {
                request_id,
                message,
            });
        }
        NavigationError::Provider(error) => {
            let _ = tx.send(SourceResult::Failed {
                request_id,
                message: error.to_string(),
            });
        }
        NavigationError::NoPath => {
            let _ = tx.send(SourceResult::NoPath { request_id });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct FakeProvider {
        directories: BTreeMap<String, Vec<RemoteEntry>>,
    }

    impl ReadOnlyProvider for FakeProvider {
        fn list(
            &self,
            object: &str,
            cancel: &CancelToken,
        ) -> Result<Vec<RemoteEntry>, ProviderError> {
            if cancel.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            Ok(self.directories.get(object).cloned().unwrap_or_default())
        }

        fn stat(
            &self,
            _object: &str,
            _cancel: &CancelToken,
        ) -> Result<super::super::provider::RemoteMetadata, ProviderError> {
            Err(ProviderError::Unsupported("not needed in test".to_string()))
        }

        fn materialize(
            &self,
            _object: &str,
            _version: Option<&str>,
            _cache: &super::super::provider::CacheStore,
            _cancel: &CancelToken,
        ) -> Result<PathBuf, ProviderError> {
            Err(ProviderError::Unsupported("not needed in test".to_string()))
        }
    }

    fn entry(name: &str, object: &str, directory: bool) -> RemoteEntry {
        RemoteEntry {
            name: name.to_string(),
            object: object.to_string(),
            is_directory: directory,
            size: None,
            version: None,
        }
    }

    fn navigator() -> RemoteNavigator {
        let mut directories = BTreeMap::new();
        directories.insert(
            String::new(),
            vec![entry("sub", "sub", true), entry("a.jpg", "a.jpg", false)],
        );
        directories.insert(
            "sub".to_string(),
            vec![
                entry("b.png", "sub/b.png", false),
                entry("note.txt", "sub/note.txt", false),
            ],
        );
        RemoteNavigator::open(
            Arc::new(FakeProvider { directories }),
            "smb",
            "",
            NavigationSortOption::OsName,
            &CancelToken::default(),
        )
        .expect("remote navigator")
    }

    #[test]
    fn recursive_remote_order_ignores_display_paths() {
        let mut navigator = navigator();
        assert_eq!(navigator.current().display_name, "a.jpg");
        let next = navigator
            .next(EndOfFolderOption::Recursive, &CancelToken::default())
            .expect("next remote entry");
        assert_eq!(next.entry.display_name, "b.png");
        assert_eq!(
            next.entry.id,
            SourceId::Remote {
                provider: "smb".to_string(),
                object: "sub/b.png".to_string()
            }
        );
    }

    #[test]
    fn loop_wraps_remote_sequence() {
        let mut navigator = navigator();
        let _ = navigator.next(EndOfFolderOption::Recursive, &CancelToken::default());
        let _ = navigator.next(EndOfFolderOption::Recursive, &CancelToken::default());
        assert_eq!(navigator.current().display_name, "b.png");
        let wrapped = navigator
            .next(EndOfFolderOption::Loop, &CancelToken::default())
            .expect("loop target");
        assert_eq!(wrapped.entry.display_name, "b.png");
    }

    #[test]
    fn cancelled_remote_listing_does_not_call_into_navigation() {
        let mut navigator = navigator();
        let cancel = CancelToken::default();
        cancel.cancel();
        assert!(matches!(
            navigator.first(&cancel),
            Err(NavigationError::Provider(ProviderError::Cancelled))
        ));
    }
}
