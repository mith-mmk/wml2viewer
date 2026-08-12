//! Byte-based ZIP, LHA/LZH, and WML listed-file virtual entry contracts.

use crate::reading::SourceId;
use crate::{CoreError, CoreResult};
use oxiarc_archive::LzhReader;
use std::io::{Cursor, Read};
use std::panic::{AssertUnwindSafe, catch_unwind};
use zip::ZipArchive;

const LISTED_FILE_HEADER: &str = "#!WMLViewer2 ListedFile";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VirtualSourceFormat {
    Zip,
    Lha,
    ListedFile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArchiveLimits {
    pub maximum_entries: usize,
    pub maximum_entry_bytes: u64,
    pub maximum_total_uncompressed_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl ArchiveLimits {
    pub const DEFAULT: Self = Self {
        maximum_entries: 20_000,
        maximum_entry_bytes: 512 * 1024 * 1024,
        maximum_total_uncompressed_bytes: 4 * 1024 * 1024 * 1024,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualEntry {
    pub index: usize,
    pub name: String,
    pub uncompressed_size: Option<u64>,
    pub source_id: SourceId,
}

#[derive(Clone, Debug)]
enum EntryLocator {
    ArchiveIndex(usize),
    ListedRelativePath(String),
}

#[derive(Clone, Debug)]
pub struct VirtualContainer {
    format: VirtualSourceFormat,
    source_id: SourceId,
    bytes: Vec<u8>,
    entries: Vec<VirtualEntry>,
    locators: Vec<EntryLocator>,
    limits: ArchiveLimits,
}

impl VirtualContainer {
    pub fn open(
        format: VirtualSourceFormat,
        source_id: SourceId,
        bytes: Vec<u8>,
        limits: ArchiveLimits,
    ) -> CoreResult<Self> {
        let (entries, locators) = match format {
            VirtualSourceFormat::Zip => list_zip(&bytes, source_id, limits)?,
            VirtualSourceFormat::Lha => list_lha(&bytes, source_id, limits)?,
            VirtualSourceFormat::ListedFile => list_wmltxt(&bytes, source_id, limits)?,
        };
        Ok(Self {
            format,
            source_id,
            bytes,
            entries,
            locators,
            limits,
        })
    }

    pub fn format(&self) -> VirtualSourceFormat {
        self.format
    }

    pub fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub fn entries(&self) -> &[VirtualEntry] {
        &self.entries
    }

    pub fn read_entry(&self, index: usize) -> CoreResult<Vec<u8>> {
        match self.locators.get(index) {
            Some(EntryLocator::ArchiveIndex(raw_index)) => match self.format {
                VirtualSourceFormat::Zip => read_zip(&self.bytes, *raw_index, self.limits),
                VirtualSourceFormat::Lha => read_lha(&self.bytes, *raw_index, self.limits),
                VirtualSourceFormat::ListedFile => unreachable!("listed entries use paths"),
            },
            Some(EntryLocator::ListedRelativePath(_)) => Err(CoreError::invalid_input(
                "listed-file entries require a platform materializer",
            )),
            None => Err(CoreError::invalid_input(
                "virtual entry index is out of range",
            )),
        }
    }

    pub fn materialize_entry<F>(&self, index: usize, mut read_relative: F) -> CoreResult<Vec<u8>>
    where
        F: FnMut(&str) -> CoreResult<Vec<u8>>,
    {
        match self.locators.get(index) {
            Some(EntryLocator::ListedRelativePath(path)) => {
                let bytes = read_relative(path)?;
                enforce_actual_size(bytes, self.limits.maximum_entry_bytes)
            }
            Some(EntryLocator::ArchiveIndex(_)) => self.read_entry(index),
            None => Err(CoreError::invalid_input(
                "virtual entry index is out of range",
            )),
        }
    }
}

pub fn normalize_relative_entry_path(raw: &str) -> CoreResult<String> {
    if raw.contains('\0') {
        return Err(CoreError::archive("entry path contains NUL"));
    }
    let replaced = raw.trim().replace('\\', "/");
    if replaced.is_empty()
        || replaced.starts_with('/')
        || replaced.starts_with("//")
        || replaced
            .split('/')
            .next()
            .is_some_and(|part| part.contains(':'))
    {
        return Err(CoreError::archive("entry path must be relative"));
    }
    let mut safe = Vec::new();
    for component in replaced.split('/') {
        match component {
            "" | "." => {}
            ".." => return Err(CoreError::archive("entry path traversal is not allowed")),
            value => safe.push(value),
        }
    }
    if safe.is_empty() {
        return Err(CoreError::archive("entry path is empty"));
    }
    Ok(safe.join("/"))
}

fn list_zip(
    bytes: &[u8],
    source_id: SourceId,
    limits: ArchiveLimits,
) -> CoreResult<(Vec<VirtualEntry>, Vec<EntryLocator>)> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| CoreError::archive(error.to_string()))?;
    enforce_entry_count(archive.len(), limits)?;
    let mut entries = Vec::new();
    let mut locators = Vec::new();
    let mut total = 0_u64;
    for raw_index in 0..archive.len() {
        let file = archive
            .by_index(raw_index)
            .map_err(|error| CoreError::archive(error.to_string()))?;
        if file.is_dir() {
            continue;
        }
        let name = normalize_relative_entry_path(&String::from_utf8_lossy(file.name_raw()))?;
        enforce_declared_size(file.size(), &mut total, limits)?;
        let index = entries.len();
        entries.push(VirtualEntry {
            index,
            name,
            uncompressed_size: Some(file.size()),
            source_id,
        });
        locators.push(EntryLocator::ArchiveIndex(raw_index));
    }
    Ok((entries, locators))
}

fn read_zip(bytes: &[u8], raw_index: usize, limits: ArchiveLimits) -> CoreResult<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| CoreError::archive(error.to_string()))?;
    let mut file = archive
        .by_index(raw_index)
        .map_err(|error| CoreError::archive(error.to_string()))?;
    let _ = normalize_relative_entry_path(&String::from_utf8_lossy(file.name_raw()))?;
    if file.size() > limits.maximum_entry_bytes {
        return Err(CoreError::limit("archive entry exceeds byte limit"));
    }
    read_limited(&mut file, limits.maximum_entry_bytes)
}

fn list_lha(
    bytes: &[u8],
    source_id: SourceId,
    limits: ArchiveLimits,
) -> CoreResult<(Vec<VirtualEntry>, Vec<EntryLocator>)> {
    let reader = catch_unwind(AssertUnwindSafe(|| LzhReader::new(Cursor::new(bytes))))
        .map_err(|_| CoreError::archive("LHA parser panicked"))?
        .map_err(|error| CoreError::archive(error.to_string()))?;
    enforce_entry_count(reader.entries().len(), limits)?;
    let mut entries = Vec::new();
    let mut locators = Vec::new();
    let mut total = 0_u64;
    for (raw_index, entry) in reader.entries().iter().enumerate() {
        let name = normalize_relative_entry_path(&entry.name)?;
        enforce_declared_size(entry.size, &mut total, limits)?;
        let index = entries.len();
        entries.push(VirtualEntry {
            index,
            name,
            uncompressed_size: Some(entry.size),
            source_id,
        });
        locators.push(EntryLocator::ArchiveIndex(raw_index));
    }
    Ok((entries, locators))
}

fn read_lha(bytes: &[u8], raw_index: usize, limits: ArchiveLimits) -> CoreResult<Vec<u8>> {
    let mut reader = catch_unwind(AssertUnwindSafe(|| LzhReader::new(Cursor::new(bytes))))
        .map_err(|_| CoreError::archive("LHA parser panicked"))?
        .map_err(|error| CoreError::archive(error.to_string()))?;
    let entry = reader
        .entries()
        .get(raw_index)
        .cloned()
        .ok_or_else(|| CoreError::invalid_input("virtual entry index is out of range"))?;
    let _ = normalize_relative_entry_path(&entry.name)?;
    if entry.size > limits.maximum_entry_bytes {
        return Err(CoreError::limit("archive entry exceeds byte limit"));
    }
    let bytes = catch_unwind(AssertUnwindSafe(|| reader.extract_to_vec(&entry)))
        .map_err(|_| CoreError::archive("LHA extractor panicked"))?
        .map_err(|error| CoreError::archive(error.to_string()))?;
    enforce_actual_size(bytes, limits.maximum_entry_bytes)
}

fn list_wmltxt(
    bytes: &[u8],
    source_id: SourceId,
    limits: ArchiveLimits,
) -> CoreResult<(Vec<VirtualEntry>, Vec<EntryLocator>)> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| CoreError::archive("listed file must be UTF-8"))?;
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    let header = lines
        .next()
        .ok_or_else(|| CoreError::archive("listed file is empty"))?
        .trim();
    if !header.starts_with(LISTED_FILE_HEADER) {
        return Err(CoreError::archive("listed file header is invalid"));
    }
    let mut entries = Vec::new();
    let mut locators = Vec::new();
    for raw_line in lines {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('@') {
            continue;
        }
        if entries.len() >= limits.maximum_entries {
            return Err(CoreError::limit("listed file exceeds entry limit"));
        }
        let path = normalize_relative_entry_path(line)?;
        let index = entries.len();
        entries.push(VirtualEntry {
            index,
            name: path.clone(),
            uncompressed_size: None,
            source_id,
        });
        locators.push(EntryLocator::ListedRelativePath(path));
    }
    Ok((entries, locators))
}

fn enforce_entry_count(count: usize, limits: ArchiveLimits) -> CoreResult<()> {
    if count > limits.maximum_entries {
        return Err(CoreError::limit("archive exceeds entry limit"));
    }
    Ok(())
}

fn enforce_declared_size(size: u64, total: &mut u64, limits: ArchiveLimits) -> CoreResult<()> {
    if size > limits.maximum_entry_bytes {
        return Err(CoreError::limit("archive entry exceeds byte limit"));
    }
    *total = total
        .checked_add(size)
        .ok_or_else(|| CoreError::limit("archive total size overflow"))?;
    if *total > limits.maximum_total_uncompressed_bytes {
        return Err(CoreError::limit("archive exceeds total byte limit"));
    }
    Ok(())
}

fn enforce_actual_size(bytes: Vec<u8>, maximum: u64) -> CoreResult<Vec<u8>> {
    if bytes.len() as u64 > maximum {
        return Err(CoreError::limit("materialized entry exceeds byte limit"));
    }
    Ok(bytes)
}

fn read_limited(reader: &mut impl Read, maximum: u64) -> CoreResult<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| CoreError::io(error.to_string()))?;
    enforce_actual_size(bytes, maximum)
}
