//! Bounded local-file reads shared by mobile bridge adapters.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

const READ_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Debug)]
pub enum BoundedReadError {
    Io(io::Error),
    Limit { dimension: &'static str },
}

pub fn read_file_bounded(
    path: &Path,
    maximum_bytes: u64,
    dimension: &'static str,
) -> Result<Vec<u8>, BoundedReadError> {
    let file = File::open(path).map_err(BoundedReadError::Io)?;
    let metadata = file.metadata().map_err(BoundedReadError::Io)?;
    if metadata.len() > maximum_bytes {
        return Err(BoundedReadError::Limit { dimension });
    }

    read_bounded(file, metadata.len(), maximum_bytes, dimension)
}

fn read_bounded(
    reader: impl Read,
    declared_bytes: u64,
    maximum_bytes: u64,
    dimension: &'static str,
) -> Result<Vec<u8>, BoundedReadError> {
    if declared_bytes > maximum_bytes {
        return Err(BoundedReadError::Limit { dimension });
    }

    let maximum =
        usize::try_from(maximum_bytes).map_err(|_| BoundedReadError::Limit { dimension })?;
    let declared =
        usize::try_from(declared_bytes).map_err(|_| BoundedReadError::Limit { dimension })?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(declared)
        .map_err(|_| BoundedReadError::Limit { dimension })?;

    let mut reader = reader.take(maximum_bytes.saturating_add(1));
    let mut chunk = [0_u8; READ_CHUNK_BYTES];
    loop {
        let read = match reader.read(&mut chunk) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(BoundedReadError::Io(error)),
        };
        if read == 0 {
            return Ok(bytes);
        }
        let new_length = bytes
            .len()
            .checked_add(read)
            .ok_or(BoundedReadError::Limit { dimension })?;
        if new_length > maximum {
            return Err(BoundedReadError::Limit { dimension });
        }
        bytes
            .try_reserve(read)
            .map_err(|_| BoundedReadError::Limit { dimension })?;
        bytes.extend_from_slice(&chunk[..read]);
    }
}

#[cfg(test)]
pub fn read_bounded_for_test(
    reader: impl Read,
    declared_bytes: u64,
    maximum_bytes: u64,
) -> Result<Vec<u8>, BoundedReadError> {
    read_bounded(reader, declared_bytes, maximum_bytes, "test_bytes")
}
