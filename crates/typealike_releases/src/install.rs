//! The install of a downloaded release, which replaces this copy of TypeAlike and then starts the
//! new version. Each OS has its own installer, so each OS has its own steps.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub(crate) use macos::{installed_copy, prepare, start_install};
#[cfg(target_os = "windows")]
pub(crate) use windows::{installed_copy, prepare, start_install};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) use other::{installed_copy, prepare, start_install};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod other {
    use std::io;
    use std::path::{Path, PathBuf};

    /// No release of TypeAlike installs on this OS.
    pub(crate) fn installed_copy() -> Option<PathBuf> {
        None
    }

    pub(crate) fn prepare(_download: &Path, _folder: &Path) -> io::Result<PathBuf> {
        Err(io::ErrorKind::Unsupported.into())
    }

    pub(crate) fn start_install(_prepared: &Path, _installed: &Path, _folder: &Path) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
