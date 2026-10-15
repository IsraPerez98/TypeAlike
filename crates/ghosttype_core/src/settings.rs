//! The settings that GhostType keeps between runs, and the seams to the services that keep them.

use crate::hub::ModelFile;
use crate::inference::Backend;

/// Everything that GhostType keeps between runs.
///
/// ```
/// use ghosttype_core::inference::Backend;
/// use ghosttype_core::settings::Settings;
///
/// assert_eq!(Settings::default().backend, Backend::Cpu);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    /// The backend that runs models.
    pub backend: Backend,
    /// The model that suggests text as the user types, and that GhostType loads when it starts.
    pub suggestion_model: Option<ModelFile>,
}

/// Keeps the settings between runs.
pub trait SettingsStore {
    /// Keeps `settings` for the next run. The store reports its own failures.
    fn save(&mut self, settings: &Settings);
}

/// Starts GhostType when the user logs in. The OS keeps this setting, not the [`SettingsStore`].
///
/// The service reports its own failures. After a failure, [`is_enabled`](Self::is_enabled) tells
/// the state that the OS kept.
pub trait Autostart {
    /// Whether GhostType starts when the user logs in.
    fn is_enabled(&self) -> bool;

    /// Starts GhostType at each login from now on.
    fn enable(&mut self);

    /// Stops GhostType from starting at login.
    fn disable(&mut self);
}
