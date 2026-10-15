//! Selects the platform layer that runs GhostType on this OS.

use ghosttype_core::app::App;

/// Runs the app with this OS's tray, hooks, overlay and settings window until the user quits.
#[cfg(target_os = "windows")]
pub(crate) fn run(app: App) -> anyhow::Result<()> {
    use anyhow::Context as _;
    let models_dir = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .context("no local app data directory")?
        .join("GhostType")
        .join("Models");
    let hub = ghosttype_models::HuggingFace::new(models_dir.clone());
    let runner = ghosttype_llm::LlamaCpp::new(move |file| ghosttype_models::stored_path(&models_dir, file));
    ghosttype_windows::run(app, Box::new(hub), Box::new(runner)).context("ghosttype stopped unexpectedly")
}

/// Runs the app with this OS's menu bar extra, event tap, overlay and settings window until the
/// user quits.
#[cfg(target_os = "macos")]
pub(crate) fn run(app: App) -> anyhow::Result<()> {
    use anyhow::Context as _;
    const MODELS_DIR: &str = "Library/Application Support/GhostType/Models";
    let models_dir = std::env::home_dir()
        .context("no home directory")?
        .join(MODELS_DIR);
    let hub = ghosttype_models::HuggingFace::new(models_dir.clone());
    let runner = ghosttype_llm::LlamaCpp::new(move |file| ghosttype_models::stored_path(&models_dir, file));
    ghosttype_macos::run(app, Box::new(hub), Box::new(runner)).context("ghosttype stopped unexpectedly")
}

/// Fails: no platform layer exists for this OS yet.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn run(_app: App) -> anyhow::Result<()> {
    anyhow::bail!("ghosttype does not support this operating system yet")
}
