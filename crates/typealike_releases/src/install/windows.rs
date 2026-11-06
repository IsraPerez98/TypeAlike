//! The install on Windows: the installer of the release runs without a window. It waits until
//! TypeAlike quits, replaces the installed files and starts the new version
//! (`packaging/windows/TypeAlike.iss`).

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The uninstaller that the installer puts next to the program.
const UNINSTALLER: &str = "unins000.exe";
const SILENT_INSTALL: [&str; 4] = ["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"];

/// This program, when an installer put it in its folder.
pub(crate) fn installed_copy() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?;
    program.with_file_name(UNINSTALLER).is_file().then_some(program)
}

/// The installer needs no preparation.
#[expect(
    clippy::unnecessary_wraps,
    reason = "the preparation of each OS has the same signature"
)]
pub(crate) fn prepare(download: &Path, _folder: &Path) -> io::Result<PathBuf> {
    Ok(download.to_path_buf())
}

/// Starts the `installer` without a window.
pub(crate) fn start_install(installer: &Path, _installed: &Path, _folder: &Path) -> io::Result<()> {
    Command::new(installer)
        .args(SILENT_INSTALL)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}
