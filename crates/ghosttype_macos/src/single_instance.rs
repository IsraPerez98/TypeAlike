//! Keeps one GhostType per user, so event taps and menu bar extras are never doubled.

use std::fs::{File, TryLockError};
use std::io;

const LOCK_FILE_NAME: &str = "GhostType.lock";

/// Held while this process is the user's GhostType instance.
#[derive(Debug)]
pub(crate) struct InstanceLock(#[expect(dead_code, reason = "held only to keep the file locked")] File);

/// Returns the lock, or `None` when another instance already holds it.
pub(crate) fn acquire() -> io::Result<Option<InstanceLock>> {
    let file = File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(std::env::temp_dir().join(LOCK_FILE_NAME))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(InstanceLock(file))),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}
