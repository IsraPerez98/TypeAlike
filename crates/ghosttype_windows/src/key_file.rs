//! Keeps the key of the earlier writing in a file that the Data Protection API (DPAPI) locks to
//! the Windows account of the user.

use std::path::PathBuf;
use std::{fs, io, slice};

use ghosttype_core::history::{HistoryKey, KeyVault};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows::core::PCWSTR;

/// The file that keeps the key of the earlier writing. Only the same Windows account can open it.
#[derive(Debug)]
pub struct ProtectedKeyFile {
    path: PathBuf,
}

impl ProtectedKeyFile {
    /// Keeps the key in the file at `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl KeyVault for ProtectedKeyFile {
    fn load(&mut self) -> io::Result<Option<HistoryKey>> {
        let sealed = match fs::read(&self.path) {
            Ok(sealed) => sealed,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let key = unprotect(&sealed)?
            .try_into()
            .map_err(|_bytes| io::Error::other("the key file has a key of a wrong length"))?;
        Ok(Some(HistoryKey(key)))
    }

    fn store(&mut self, key: &HistoryKey) -> io::Result<()> {
        if let Some(folder) = self.path.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(&self.path, protect(&key.0)?)
    }
}

fn protect(plain: &[u8]) -> io::Result<Vec<u8>> {
    let input = blob_of(plain)?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` points at `plain`, which outlives the call, and `output` is a valid place
    // for the result. The call shows no prompt.
    unsafe {
        CryptProtectData(
            &raw const input,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    }
    .map_err(io::Error::other)?;
    // SAFETY: on success, `output` holds a buffer that the call allocated with `LocalAlloc`.
    Ok(unsafe { take_blob(output) })
}

fn unprotect(sealed: &[u8]) -> io::Result<Vec<u8>> {
    let input = blob_of(sealed)?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` points at `sealed`, which outlives the call, and `output` is a valid place
    // for the result. The call shows no prompt.
    unsafe {
        CryptUnprotectData(
            &raw const input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &raw mut output,
        )
    }
    .map_err(io::Error::other)?;
    // SAFETY: on success, `output` holds a buffer that the call allocated with `LocalAlloc`.
    Ok(unsafe { take_blob(output) })
}

/// A blob that points at `bytes`, for input only.
fn blob_of(bytes: &[u8]) -> io::Result<CRYPT_INTEGER_BLOB> {
    Ok(CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(io::Error::other)?,
        pbData: bytes.as_ptr().cast_mut(),
    })
}

/// Copies the bytes of `blob` and frees its buffer.
///
/// # Safety
///
/// `blob` must hold a buffer of `cbData` bytes that `LocalAlloc` allocated, or a null buffer.
unsafe fn take_blob(blob: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    if blob.pbData.is_null() {
        return Vec::new();
    }
    let length = usize::try_from(blob.cbData).expect("a u32 fits in a usize on Windows");
    // SAFETY: the caller guarantees that the buffer holds `cbData` bytes.
    let bytes = unsafe { slice::from_raw_parts(blob.pbData, length) }.to_vec();
    // SAFETY: the caller guarantees that `LocalAlloc` allocated the buffer, and nothing uses it
    // after this call.
    unsafe { LocalFree(Some(HLOCAL(blob.pbData.cast()))) };
    bytes
}
