//! The settings that GhostType keeps between runs, and the seams to the services that keep them.

use crate::hub::ModelFile;
use crate::inference::{Backend, SystemPrompt};

/// The system prompt until the user writes one. Base models continue text and do not follow
/// orders, so it describes the text that follows instead of giving orders.
pub const DEFAULT_SYSTEM_PROMPT: &str = "A person types the text below on their computer, for example an \
     email, a chat message, a note or a document. The writing is clear, natural and correct. It keeps \
     the language, tone and style of the person who writes it.";

/// Whether suggestions may use the text on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClipboardAccess {
    /// GhostType never reads the clipboard.
    #[default]
    Denied,
    /// GhostType reads the clipboard when the user starts to type, and the model reads its text.
    Allowed,
}

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
    /// The system prompt that the user wrote, or `None` for [`DEFAULT_SYSTEM_PROMPT`].
    pub custom_system_prompt: Option<SystemPrompt>,
    /// Whether suggestions may use the text on the clipboard.
    pub clipboard_access: ClipboardAccess,
}

impl Settings {
    /// Returns the system prompt that the model reads before the typed text.
    ///
    /// ```
    /// use ghosttype_core::settings::{DEFAULT_SYSTEM_PROMPT, Settings};
    ///
    /// assert_eq!(Settings::default().system_prompt().0, DEFAULT_SYSTEM_PROMPT);
    /// ```
    #[must_use]
    pub fn system_prompt(&self) -> SystemPrompt {
        self.custom_system_prompt
            .clone()
            .unwrap_or_else(|| SystemPrompt(DEFAULT_SYSTEM_PROMPT.to_owned()))
    }
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
