//! Selects the platform layer that runs TypeAlike on this OS.

/// Runs the app with this OS's tray, hooks, overlay and settings window until the user quits.
#[cfg(target_os = "windows")]
pub(crate) fn run() -> anyhow::Result<()> {
    use anyhow::Context as _;
    use typealike_core::settings::{Settings, Setup, WriterNotes};
    let data_dir = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .context("no local app data directory")?
        .join("TypeAlike");
    let models_dir = data_dir.join("Models");
    let first_start = Settings {
        backend: typealike_llm::DEFAULT_BACKEND,
        writer_name: typealike_windows::account_name(),
        writer_notes: WriterNotes::default_for(typealike_windows::ui_language()),
        setup: Setup::Needed,
        ..Settings::default()
    };
    let settings_file = typealike_settings::SettingsFile::new(data_dir.join("settings.json"), first_start);
    let settings = settings_file.load();
    let program = std::env::current_exe().context("no path to the running program")?;
    let hub = typealike_models::HuggingFace::new(models_dir.clone());
    let runner = typealike_llm::LlamaCpp::new(
        move |file| typealike_models::stored_path(&models_dir, file),
        typealike_windows::cpu_cores(),
    );
    let services = typealike_core::controller::Services {
        hub: Box::new(hub),
        runner: Box::new(runner),
        settings_store: Box::new(settings_file),
        autostart: Box::new(typealike_windows::StartupEntry::new(&program)),
        history: Box::new(typealike_history::HistoryFile::new(
            data_dir.join("history.bin"),
            Box::new(typealike_windows::ProtectedKeyFile::new(
                data_dir.join("history.key"),
            )),
        )),
        spell_checker: Box::new(typealike_windows::SpellingDictionary::new()),
    };
    typealike_windows::run(settings, services).context("typealike stopped unexpectedly")
}

/// Runs the app with this OS's menu bar extra, event tap, overlay and settings window until the
/// user quits.
#[cfg(target_os = "macos")]
pub(crate) fn run() -> anyhow::Result<()> {
    use anyhow::Context as _;
    use typealike_core::settings::{Settings, Setup, WriterNotes};
    const DATA_DIR: &str = "Library/Application Support/TypeAlike";
    const LAUNCH_AGENTS_DIR: &str = "Library/LaunchAgents";
    let home_dir = std::env::home_dir().context("no home directory")?;
    let data_dir = home_dir.join(DATA_DIR);
    let models_dir = data_dir.join("Models");
    let first_start = Settings {
        writer_name: typealike_macos::account_name(),
        writer_notes: WriterNotes::default_for(typealike_macos::ui_language()),
        setup: Setup::Needed,
        ..Settings::default()
    };
    let settings_file = typealike_settings::SettingsFile::new(data_dir.join("settings.json"), first_start);
    let settings = settings_file.load();
    let program = std::env::current_exe().context("no path to the running program")?;
    let launch_agent = typealike_macos::LaunchAgent::new(&home_dir.join(LAUNCH_AGENTS_DIR), &program);
    let hub = typealike_models::HuggingFace::new(models_dir.clone());
    let runner =
        typealike_llm::LlamaCpp::new(move |file| typealike_models::stored_path(&models_dir, file), None);
    let services = typealike_core::controller::Services {
        hub: Box::new(hub),
        runner: Box::new(runner),
        settings_store: Box::new(settings_file),
        autostart: Box::new(launch_agent),
        history: Box::new(typealike_history::HistoryFile::new(
            data_dir.join("history.bin"),
            Box::new(typealike_macos::Keychain),
        )),
        spell_checker: Box::new(typealike_macos::SpellingDictionary::new()),
    };
    typealike_macos::run(settings, services).context("typealike stopped unexpectedly")
}

/// Fails: no platform layer exists for this OS yet.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn run() -> anyhow::Result<()> {
    anyhow::bail!("typealike does not support this operating system yet")
}
