use std::path::PathBuf;

use crate::options::EndOfFolderOption;

use super::browser::{BrowserEntry, BrowserScanOptions};

#[derive(Clone, Debug)]
pub enum FilesystemCommand {
    Init {
        request_id: u64,
        path: PathBuf,
    },
    SetCurrent {
        request_id: u64,
        path: PathBuf,
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
    OpenBrowserDirectory {
        request_id: u64,
        dir: PathBuf,
        selected: Option<PathBuf>,
        options: Option<BrowserScanOptions>,
    },
    ResolveSourceInput {
        request_id: u64,
        input: PathBuf,
    },
    CancelSourceInput {
        request_id: u64,
    },
}

#[derive(Clone, Debug)]
pub enum FilesystemResult {
    NavigatorReady {
        request_id: u64,
        navigation_path: Option<PathBuf>,
        load_path: Option<PathBuf>,
    },
    CurrentSet,
    PathResolved {
        request_id: u64,
        navigation_path: PathBuf,
        load_path: PathBuf,
    },
    NoPath {
        request_id: u64,
    },
    BrowserReset {
        request_id: u64,
        directory: PathBuf,
        selected: Option<PathBuf>,
    },
    BrowserAppend {
        request_id: u64,
        entries: Vec<BrowserEntry>,
    },
    ThumbnailHint {
        request_id: u64,
        paths: Vec<PathBuf>,
        max_side: u32,
    },
    BrowserFinish {
        request_id: u64,
        directory: PathBuf,
        entries: Vec<BrowserEntry>,
        selected: Option<PathBuf>,
    },
    BrowserFailed {
        request_id: u64,
    },
    InputPathResolved {
        request_id: u64,
        path: PathBuf,
    },
    InputPathFailed {
        request_id: u64,
        input: PathBuf,
    },
    InputPathCancelled {
        request_id: u64,
        input: PathBuf,
    },
}

pub type BrowserQuery = FilesystemCommand;
pub type BrowserQueryResult = FilesystemResult;
