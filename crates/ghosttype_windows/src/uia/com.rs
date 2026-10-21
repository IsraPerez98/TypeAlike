//! The COM resources that the UIA connection holds: the membership of its thread in the COM
//! apartment, and the arrays that UIA gives.

use std::marker::PhantomData;
use std::ptr;

use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize, SAFEARRAY};
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayUnaccessData};
use windows::core::Result;

/// Membership of this thread in the multithreaded COM apartment, which ends on drop. It cannot go
/// to another thread, because COM ends only on the thread that joined it.
#[derive(Debug)]
pub(super) struct ComApartment(PhantomData<*const ()>);

impl ComApartment {
    pub(super) fn join() -> Result<Self> {
        // SAFETY: plain call; drop balances it on this thread.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
        Ok(Self(PhantomData))
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: balances the successful CoInitializeEx of `join`, on the same thread.
        unsafe { CoUninitialize() };
    }
}

/// A SAFEARRAY that this thread owns, destroyed on drop.
pub(super) struct SafeArray(pub(super) *mut SAFEARRAY);

impl SafeArray {
    /// Copies the elements of a one-dimensional array of doubles. `None` for another array.
    pub(super) fn doubles(&self) -> Option<Vec<f64>> {
        // SAFETY: a non-null pointer refers to the array that `self` owns.
        let header = unsafe { self.0.as_ref() }?;
        let holds_doubles = header.cDims == 1 && usize::try_from(header.cbElements).ok()? == size_of::<f64>();
        if !holds_doubles {
            return None;
        }
        let count = usize::try_from(header.rgsabound[0].cElements).ok()?;
        if count == 0 {
            return Some(Vec::new());
        }
        let mut data = ptr::null_mut();
        // SAFETY: the array is valid; `data` gets its elements until the access ends.
        unsafe { SafeArrayAccessData(self.0, &raw mut data) }.ok()?;
        // SAFETY: the array holds `count` doubles at `data` while the access lasts.
        let values = unsafe { std::slice::from_raw_parts(data.cast::<f64>(), count) }.to_vec();
        // SAFETY: ends the access above; the array stays valid.
        unsafe { SafeArrayUnaccessData(self.0) }.ok()?;
        Some(values)
    }
}

impl Drop for SafeArray {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }
        // A failed destroy during drop leaves nothing to recover.
        // SAFETY: `self` owns the array and nothing accesses it now.
        let _ = unsafe { SafeArrayDestroy(self.0) };
    }
}
