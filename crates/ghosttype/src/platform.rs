//! Selects the platform layer that runs GhostType on this OS.

use ghosttype_core::app::App;

/// Runs the app with this OS's tray, hooks and overlay until the user quits.
#[cfg(target_os = "windows")]
pub(crate) fn run(app: App) -> anyhow::Result<()> {
    use anyhow::Context as _;
    ghosttype_windows::run(app).context("ghosttype stopped unexpectedly")
}

/// Runs the app with this OS's menu bar extra, event tap and overlay until the user quits.
#[cfg(target_os = "macos")]
pub(crate) fn run(app: App) -> anyhow::Result<()> {
    use anyhow::Context as _;
    ghosttype_macos::run(app).context("ghosttype stopped unexpectedly")
}

/// Fails: no platform layer exists for this OS yet.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn run(_app: App) -> anyhow::Result<()> {
    anyhow::bail!("this operating system is not supported yet")
}
