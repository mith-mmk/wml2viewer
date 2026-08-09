#![cfg(target_os = "ios")]

use super::provider::{
    CacheMaterialization, CacheStore, CancelToken, ProviderError, ReadOnlyProvider, RemoteEntry,
    RemoteMetadata, SourceId,
};
use futures_util::StreamExt;
use smb::{
    Client, ClientConfig, DirAccessMask, Directory, FileAccessMask, FileCreateArgs,
    FileDirectoryInformation, GetLen, Resource, UncPath,
};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::mpsc::{self, SyncSender};
use std::{ffi::CString, os::raw::c_char};

const SMB_READ_CHUNK_BYTES: usize = 256 * 1024;

unsafe extern "C" {
    fn wml2viewer_ios_keychain_copy_password(
        reference: *const c_char,
        output: *mut c_char,
        capacity: usize,
    ) -> i32;
}

#[derive(Clone)]
pub struct SmbCredentials {
    pub username: String,
    pub password: String,
}

impl SmbCredentials {
    pub fn from_keychain(
        username: impl Into<String>,
        reference: &str,
    ) -> Result<Self, ProviderError> {
        let reference = CString::new(reference)
            .map_err(|_| ProviderError::InvalidSource("invalid Keychain reference".to_string()))?;
        let mut password = vec![0_i8; 16 * 1024];
        let length = unsafe {
            wml2viewer_ios_keychain_copy_password(
                reference.as_ptr(),
                password.as_mut_ptr(),
                password.len(),
            )
        };
        if length <= 0 {
            return Err(ProviderError::Authentication(
                "SMB credential is missing from the iOS Keychain".to_string(),
            ));
        }
        let length = usize::try_from(length).map_err(|_| {
            ProviderError::Authentication("invalid iOS Keychain credential".to_string())
        })?;
        let password = password.get(..length).ok_or_else(|| {
            ProviderError::Authentication("invalid iOS Keychain credential".to_string())
        })?;
        let password = password
            .iter()
            .map(|value| value.to_ne_bytes()[0])
            .collect::<Vec<_>>();
        let password = String::from_utf8(password).map_err(|_| {
            ProviderError::Authentication("invalid iOS Keychain credential".to_string())
        })?;
        Ok(Self {
            username: username.into(),
            password,
        })
    }
}

pub struct SmbProvider {
    share: String,
    command_tx: SyncSender<SmbCommand>,
}

impl SmbProvider {
    pub fn new(share: impl Into<String>, credentials: SmbCredentials) -> Self {
        let share = share.into();
        let (command_tx, command_rx) = mpsc::sync_channel(16);
        let worker_share = share.clone();
        std::thread::Builder::new()
            .name("wml2viewer-smb".to_string())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let Ok(runtime) = runtime else {
                    drain_with_runtime_error(command_rx, "failed to create SMB runtime");
                    return;
                };
                let mut session = SmbSession::new(worker_share, credentials);
                while let Ok(command) = command_rx.recv() {
                    match command {
                        SmbCommand::List {
                            object,
                            cancel,
                            reply,
                        } => {
                            let _ = reply.send(runtime.block_on(session.list(&object, &cancel)));
                        }
                        SmbCommand::Stat {
                            object,
                            cancel,
                            reply,
                        } => {
                            let _ = reply.send(runtime.block_on(session.stat(&object, &cancel)));
                        }
                        SmbCommand::Materialize {
                            object,
                            version,
                            cache,
                            cancel,
                            reply,
                        } => {
                            let result = runtime.block_on(session.materialize(
                                &object,
                                version.as_deref(),
                                &cache,
                                &cancel,
                            ));
                            let _ = reply.send(result);
                        }
                        SmbCommand::Reconnect { reply } => {
                            let result = runtime.block_on(async {
                                session.disconnect().await;
                                session.ensure_connected().await.map(|_| ())
                            });
                            let _ = reply.send(result);
                        }
                        SmbCommand::Shutdown => {
                            break;
                        }
                    }
                }
                runtime.block_on(session.disconnect());
            })
            .expect("failed to start SMB provider worker");
        Self { share, command_tx }
    }

    pub fn source_id(&self, object: &str, version: Option<&str>) -> SourceId {
        SourceId::smb(
            self.share.clone(),
            normalize_object(object),
            version.map(str::to_owned),
        )
    }

    pub fn reconnect(&self) -> Result<(), ProviderError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(0);
        self.command_tx
            .send(SmbCommand::Reconnect { reply: reply_tx })
            .map_err(|_| worker_stopped())?;
        reply_rx.recv().map_err(|_| worker_stopped())?
    }

    fn request<T>(
        &self,
        make_command: impl FnOnce(SyncSender<Result<T, ProviderError>>) -> SmbCommand,
    ) -> Result<T, ProviderError> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(0);
        self.command_tx
            .send(make_command(reply_tx))
            .map_err(|_| worker_stopped())?;
        reply_rx.recv().map_err(|_| worker_stopped())?
    }
}

impl Drop for SmbProvider {
    fn drop(&mut self) {
        // Provider destruction must not block the UI if the bounded worker queue
        // is currently full. Dropping the last sender also terminates the worker.
        let _ = self.command_tx.try_send(SmbCommand::Shutdown);
    }
}

impl ReadOnlyProvider for SmbProvider {
    fn reconnect(&self) -> Result<(), ProviderError> {
        SmbProvider::reconnect(self)
    }

    fn list(&self, object: &str, cancel: &CancelToken) -> Result<Vec<RemoteEntry>, ProviderError> {
        cancel.check()?;
        self.request(|reply| SmbCommand::List {
            object: normalize_object(object),
            cancel: cancel.clone(),
            reply,
        })
    }

    fn stat(&self, object: &str, cancel: &CancelToken) -> Result<RemoteMetadata, ProviderError> {
        cancel.check()?;
        self.request(|reply| SmbCommand::Stat {
            object: normalize_object(object),
            cancel: cancel.clone(),
            reply,
        })
    }

    fn materialize(
        &self,
        object: &str,
        version: Option<&str>,
        cache: &CacheStore,
        cancel: &CancelToken,
    ) -> Result<PathBuf, ProviderError> {
        cancel.check()?;
        self.request(|reply| SmbCommand::Materialize {
            object: normalize_object(object),
            version: version.map(str::to_owned),
            cache: cache.clone(),
            cancel: cancel.clone(),
            reply,
        })
    }
}

enum SmbCommand {
    List {
        object: String,
        cancel: CancelToken,
        reply: SyncSender<Result<Vec<RemoteEntry>, ProviderError>>,
    },
    Stat {
        object: String,
        cancel: CancelToken,
        reply: SyncSender<Result<RemoteMetadata, ProviderError>>,
    },
    Materialize {
        object: String,
        version: Option<String>,
        cache: CacheStore,
        cancel: CancelToken,
        reply: SyncSender<Result<PathBuf, ProviderError>>,
    },
    Reconnect {
        reply: SyncSender<Result<(), ProviderError>>,
    },
    Shutdown,
}

struct SmbSession {
    share: String,
    credentials: SmbCredentials,
    client: Option<Client>,
}

impl SmbSession {
    fn new(share: String, credentials: SmbCredentials) -> Self {
        Self {
            share,
            credentials,
            client: None,
        }
    }

    async fn ensure_connected(&mut self) -> Result<&Client, ProviderError> {
        if self.client.is_none() {
            let share = self.share_path()?;
            let client = Client::new(ClientConfig::default());
            client
                .share_connect(
                    &share,
                    &self.credentials.username,
                    self.credentials.password.clone(),
                )
                .await
                .map_err(|error| ProviderError::Authentication(error.to_string()))?;
            self.client = Some(client);
        }
        self.client
            .as_ref()
            .ok_or_else(|| ProviderError::Connection("SMB client is unavailable".to_string()))
    }

    async fn disconnect(&mut self) {
        if let Some(client) = self.client.take() {
            let _ = client.close().await;
        }
    }

    fn share_path(&self) -> Result<UncPath, ProviderError> {
        let path = UncPath::from_str(&self.share)
            .map_err(|error| ProviderError::InvalidSource(error.to_string()))?;
        if path.share().is_none() {
            return Err(ProviderError::InvalidSource(
                "SMB share must include a server and share name".to_string(),
            ));
        }
        Ok(path.with_no_path())
    }

    fn object_path(&self, object: &str) -> Result<UncPath, ProviderError> {
        validate_object(object)?;
        Ok(self.share_path()?.with_path(&normalize_object(object)))
    }

    async fn open_file(&mut self, object: &str) -> Result<smb::File, ProviderError> {
        let target = self.object_path(object)?;
        let args =
            FileCreateArgs::make_open_existing(FileAccessMask::new().with_generic_read(true));
        let result = self
            .ensure_connected()
            .await?
            .create_file(&target, &args)
            .await;
        let resource = match result {
            Ok(resource) => resource,
            Err(error) => {
                self.disconnect().await;
                return Err(ProviderError::Connection(error.to_string()));
            }
        };
        match resource {
            Resource::File(file) => Ok(file),
            Resource::Directory(_) => Err(ProviderError::InvalidSource(
                "SMB object is a directory".to_string(),
            )),
            Resource::Pipe(_) => Err(ProviderError::InvalidSource(
                "SMB object is not a regular file".to_string(),
            )),
        }
    }

    async fn open_directory(&mut self, object: &str) -> Result<Arc<Directory>, ProviderError> {
        let target = self.object_path(object)?;
        let args = FileCreateArgs::make_open_existing(
            DirAccessMask::new().with_list_directory(true).into(),
        );
        let result = self
            .ensure_connected()
            .await?
            .create_file(&target, &args)
            .await;
        let resource = match result {
            Ok(resource) => resource,
            Err(error) => {
                self.disconnect().await;
                return Err(ProviderError::Connection(error.to_string()));
            }
        };
        match resource {
            Resource::Directory(directory) => Ok(Arc::new(directory)),
            Resource::File(_) | Resource::Pipe(_) => Err(ProviderError::InvalidSource(
                "SMB object is not a directory".to_string(),
            )),
        }
    }

    async fn list(
        &mut self,
        object: &str,
        cancel: &CancelToken,
    ) -> Result<Vec<RemoteEntry>, ProviderError> {
        cancel.check()?;
        let object = normalize_object(object);
        let directory = self.open_directory(&object).await?;
        let query = Directory::query::<FileDirectoryInformation>(&directory, "*").await;
        let mut stream = match query {
            Ok(stream) => stream,
            Err(error) => {
                self.disconnect().await;
                return Err(ProviderError::Connection(error.to_string()));
            }
        };
        let mut entries = Vec::new();
        let result = loop {
            if let Err(error) = cancel.check() {
                break Err(error);
            }
            let Some(entry) = stream.next().await else {
                break Ok(entries);
            };
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => break Err(ProviderError::Connection(error.to_string())),
            };
            let name = entry.file_name.to_string();
            if name == "." || name == ".." {
                continue;
            }
            let child = if object.is_empty() {
                name.clone()
            } else {
                format!("{object}/{name}")
            };
            let is_directory = entry.file_attributes.directory();
            let version = format!(
                "{:016x}:{:016x}:{:016x}",
                *entry.last_write_time, *entry.change_time, entry.end_of_file
            );
            entries.push(RemoteEntry {
                name,
                object: child,
                is_directory,
                size: (!is_directory).then_some(entry.end_of_file),
                version: Some(version),
            });
        };
        drop(stream);
        let _ = directory.close().await;
        if matches!(result, Err(ProviderError::Connection(_))) {
            self.disconnect().await;
        }
        result
    }

    async fn stat(
        &mut self,
        object: &str,
        cancel: &CancelToken,
    ) -> Result<RemoteMetadata, ProviderError> {
        cancel.check()?;
        let file = self.open_file(object).await?;
        let size = match file.get_len().await {
            Ok(size) => size,
            Err(error) => {
                self.disconnect().await;
                return Err(ProviderError::Connection(error.to_string()));
            }
        };
        cancel.check()?;
        let modified = file.modified();
        let version = Some(format!("{modified}:{size:016x}"));
        let modified = modified.as_utc().try_into().ok();
        let result = cancel.check().map(|()| RemoteMetadata {
            size: Some(size),
            version,
            modified,
        });
        let _ = file.close().await;
        result
    }

    async fn materialize(
        &mut self,
        object: &str,
        version: Option<&str>,
        cache: &CacheStore,
        cancel: &CancelToken,
    ) -> Result<PathBuf, ProviderError> {
        cancel.check()?;
        validate_object(object)?;
        let file = self.open_file(object).await?;
        let size = match file.get_len().await {
            Ok(size) => size,
            Err(error) => {
                self.disconnect().await;
                return Err(ProviderError::Connection(error.to_string()));
            }
        };
        let discovered_version = format!("{}:{size:016x}", file.modified());
        let version = version.unwrap_or(&discovered_version);
        let extension = Path::new(object)
            .extension()
            .and_then(|value| value.to_str());
        let key = smb_cache_key(&self.share, object, version);
        let materialization = cache.begin_materialization(&key, extension, Some(size), cancel);
        let materialization = match materialization {
            Ok(materialization) => materialization,
            Err(error) => {
                let _ = file.close().await;
                return Err(error);
            }
        };
        let mut writer = match materialization {
            CacheMaterialization::Hit(path) => {
                let _ = file.close().await;
                return Ok(path);
            }
            CacheMaterialization::Write(writer) => writer,
        };

        let mut offset = 0_u64;
        let mut buffer = vec![0_u8; SMB_READ_CHUNK_BYTES];
        let result = loop {
            if let Err(error) = cancel.check() {
                break Err(error);
            }
            let read = match file.read_block(&mut buffer, offset, None, false).await {
                Ok(read) => read,
                Err(error) => break Err(ProviderError::Connection(error.to_string())),
            };
            if read == 0 {
                break writer.commit(cancel);
            }
            if let Err(error) = writer.write_chunk(&buffer[..read], cancel) {
                break Err(error);
            }
            offset = offset.saturating_add(read as u64);
        };
        let _ = file.close().await;
        if matches!(result, Err(ProviderError::Connection(_))) {
            self.disconnect().await;
        }
        result
    }
}

fn validate_object(object: &str) -> Result<(), ProviderError> {
    if object.split(['/', '\\']).any(|component| component == "..") {
        return Err(ProviderError::InvalidSource(
            "SMB object path cannot contain parent traversal".to_string(),
        ));
    }
    Ok(())
}

fn normalize_object(object: &str) -> String {
    object.trim_matches(['/', '\\']).replace('\\', "/")
}

fn smb_cache_key(share: &str, object: &str, version: &str) -> String {
    format!(
        "smb:{}:{share}:{}:{object}:{}:{version}",
        share.len(),
        object.len(),
        version.len()
    )
}

fn worker_stopped() -> ProviderError {
    ProviderError::Connection("SMB provider worker stopped".to_string())
}

fn drain_with_runtime_error(command_rx: mpsc::Receiver<SmbCommand>, message: &str) {
    for command in command_rx {
        let error = || ProviderError::Connection(message.to_string());
        match command {
            SmbCommand::List { reply, .. } => {
                let _ = reply.send(Err(error()));
            }
            SmbCommand::Stat { reply, .. } => {
                let _ = reply.send(Err(error()));
            }
            SmbCommand::Materialize { reply, .. } => {
                let _ = reply.send(Err(error()));
            }
            SmbCommand::Reconnect { reply } => {
                let _ = reply.send(Err(error()));
            }
            SmbCommand::Shutdown => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_object, smb_cache_key, validate_object};
    use crate::filesystem::provider::ProviderError;

    #[test]
    fn cache_identity_distinguishes_share_object_and_version() {
        let base = smb_cache_key("//server/one", "folder/page.jpg", "v1");
        assert_ne!(base, smb_cache_key("//server/two", "folder/page.jpg", "v1"));
        assert_ne!(base, smb_cache_key("//server/one", "other/page.jpg", "v1"));
        assert_ne!(base, smb_cache_key("//server/one", "folder/page.jpg", "v2"));
    }

    #[test]
    fn object_normalization_does_not_accept_parent_traversal() {
        assert_eq!(normalize_object("/folder\\page.jpg/"), "folder/page.jpg");
        assert!(matches!(
            validate_object("folder/../secret"),
            Err(ProviderError::InvalidSource(_))
        ));
    }
}
