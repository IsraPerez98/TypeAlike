//! Selects the platform layer that runs GhostType on this OS.

#[cfg(target_os = "macos")]
mod launch_agent;
#[cfg(any(target_os = "windows", target_os = "macos"))]
mod settings_file;

use ghosttype_core::app::App;

/// Runs the app with this OS's tray, hooks, overlay and settings window until the user quits.
#[cfg(target_os = "windows")]
pub(crate) fn run(app: App) -> anyhow::Result<()> {
    use anyhow::Context as _;
    let data_dir = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .context("no local app data directory")?
        .join("GhostType");
    let models_dir = data_dir.join("Models");
    let settings_file = settings_file::SettingsFile::new(data_dir.join("settings.json"));
    let settings = settings_file.load();
    let program = std::env::current_exe().context("no path to the running program")?;
    let hub = ghosttype_models::HuggingFace::new(models_dir.clone());
    let runner = ghosttype_llm::LlamaCpp::new(move |file| ghosttype_models::stored_path(&models_dir, file));
    let services = ghosttype_windows::Services {
        hub: Box::new(hub),
        runner: Box::new(runner),
        settings_store: Box::new(settings_file),
        autostart: Box::new(ghosttype_windows::StartupEntry::new(&program)),
    };
    ghosttype_windows::run(app, settings, services).context("ghosttype stopped unexpectedly")
}

/// Runs the app with this OS's menu bar extra, event tap, overlay and settings window until the
/// user quits.
#[cfg(target_os = "macos")]
pub(crate) fn run(app: App) -> anyhow::Result<()> {
    use anyhow::Context as _;
    const DATA_DIR: &str = "Library/Application Support/GhostType";
    const LAUNCH_AGENTS_DIR: &str = "Library/LaunchAgents";
    let home_dir = std::env::home_dir().context("no home directory")?;
    let data_dir = home_dir.join(DATA_DIR);
    let models_dir = data_dir.join("Models");
    let settings_file = settings_file::SettingsFile::new(data_dir.join("settings.json"));
    let settings = settings_file.load();
    let program = std::env::current_exe().context("no path to the running program")?;
    let launch_agent = launch_agent::LaunchAgent::new(&home_dir.join(LAUNCH_AGENTS_DIR), program);
    let hub = ghosttype_models::HuggingFace::new(models_dir.clone());
    let runner = ghosttype_llm::LlamaCpp::new(move |file| ghosttype_models::stored_path(&models_dir, file));
    let services = ghosttype_macos::Services {
        hub: Box::new(hub),
        runner: Box::new(runner),
        settings_store: Box::new(settings_file),
        autostart: Box::new(launch_agent),
    };
    ghosttype_macos::run(app, settings, services).context("ghosttype stopped unexpectedly")
}

/// Fails: no platform layer exists for this OS yet.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn run(_app: App) -> anyhow::Result<()> {
    anyhow::bail!("ghosttype does not support this operating system yet")
}
