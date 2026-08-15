//! Process-global, never-reused typed handle registries.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

const FIRST_HANDLE: u64 = 1;
const MAX_HANDLE: u64 = i64::MAX as u64;
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(FIRST_HANDLE);

/// Thread-safe storage backed by process-global monotonically allocated IDs.
///
/// Handles never wrap or reuse an identifier across registry types, so a
/// released or wrong-kind handle cannot refer to another allocation.
pub struct HandleRegistry<T> {
    entries: RwLock<HashMap<u64, Arc<T>>>,
}

impl<T> Default for HandleRegistry<T> {
    fn default() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }
}

impl<T> HandleRegistry<T> {
    pub fn insert(&self, value: T) -> Option<u64> {
        let handle = NEXT_HANDLE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                (current < MAX_HANDLE).then_some(current + 1)
            })
            .ok()?;
        self.write_entries().insert(handle, Arc::new(value));
        Some(handle)
    }

    pub fn get(&self, handle: u64) -> Option<Arc<T>> {
        if handle == 0 {
            return None;
        }
        self.read_entries().get(&handle).cloned()
    }

    pub fn remove(&self, handle: u64) -> Option<Arc<T>> {
        if handle == 0 {
            return None;
        }
        self.write_entries().remove(&handle)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.read_entries().len()
    }

    fn read_entries(&self) -> std::sync::RwLockReadGuard<'_, HashMap<u64, Arc<T>>> {
        self.entries
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_entries(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<u64, Arc<T>>> {
        self.entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
