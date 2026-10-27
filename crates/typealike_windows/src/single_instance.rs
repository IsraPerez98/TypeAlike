//! Keeps one TypeAlike per user session, so hooks and tray icons are never doubled.

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::{Owned, w};

/// Held while this process is the session's TypeAlike instance.
#[derive(Debug)]
pub(crate) struct InstanceLock {
    /// Held only to keep the mutex open.
    _mutex: Owned<HANDLE>,
}

/// Returns the lock, or `None` when another instance already holds it.
pub(crate) fn acquire() -> windows::core::Result<Option<InstanceLock>> {
    // SAFETY: plain call with a static, nul-terminated name; the handle is owned right after.
    let handle = unsafe { CreateMutexW(None, false, w!("Local\\TypeAlike.SingleInstance")) }?;
    // SAFETY: reads the calling thread's last error, set by CreateMutexW just above.
    let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    // SAFETY: CreateMutexW returned a valid handle that nothing else owns.
    let mutex = unsafe { Owned::new(handle) };
    let lock = InstanceLock { _mutex: mutex };
    Ok((!already_exists).then_some(lock))
}
