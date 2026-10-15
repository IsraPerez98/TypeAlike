//! App lifecycle: whether suggestions are on, and the tray menu that controls it and opens settings.

use crate::desktop::Desktop;
use crate::engine::{Engine, Reaction};
use crate::inference::{Completion, InferenceError};
use crate::key::Key;
use crate::settings::Settings;

/// Whether GhostType is currently offering suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    /// Suggestions are shown while typing.
    #[default]
    Active,
    /// The user paused suggestions from the tray.
    Paused,
}

/// Whether the focused field may see suggestions, as classified by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// An ordinary text field.
    Open,
    /// A field GhostType stays out of: passwords, its own windows, apps it cannot type into.
    Excluded,
}

/// An action the user can pick from the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    /// Pauses suggestions when active, resumes them when paused.
    TogglePause,
    /// Shows the settings window.
    OpenSettings,
    /// Exits GhostType.
    Quit,
}

/// One entry of the tray menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayItem {
    /// Text shown for the entry, in sentence case.
    pub label: &'static str,
    /// Command run when the entry is picked.
    pub command: TrayCommand,
}

/// Everything the platform needs to render the tray icon and its menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    /// Status the icon glyph must reflect.
    pub status: Status,
    /// Tooltip naming the app and its status.
    pub tooltip: &'static str,
    /// Menu entries, top to bottom.
    pub items: Vec<TrayItem>,
}

/// What the platform must do after a tray command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayOutcome {
    /// The tray view changed and must be re-rendered; any ghost text must be hidden.
    Updated,
    /// The settings window must show and come to the front.
    OpenSettings,
    /// The app must exit.
    Quit,
}

/// GhostType's state while running: suggestion engine plus on/off status.
///
/// ```
/// use std::time::Duration;
///
/// use ghosttype_core::app::{App, Field};
/// use ghosttype_core::desktop::{Desktop, Surroundings};
/// use ghosttype_core::engine::Reaction;
/// use ghosttype_core::inference::{Completion, TokenCount};
/// use ghosttype_core::key::Key;
///
/// struct Editor;
///
/// impl Desktop for Editor {
///     fn field(&self) -> Field { Field::Open }
///     fn surroundings(&self) -> Surroundings { Surroundings::default() }
///     fn clipboard_text(&self) -> Option<String> { None }
/// }
///
/// let mut app = App::new();
/// app.on_key(Key::Char('B'), &Editor);
/// app.on_completion(Ok(Completion {
///     text: "est regards".to_owned(),
///     tokens: TokenCount(2),
///     duration: Duration::from_millis(80),
/// }));
///
/// assert_eq!(
///     app.on_key(Key::Tab, &Editor),
///     Reaction::Accept { text: "est".to_owned(), rest: Some(" regards".to_owned()) }
/// );
/// assert_eq!(
///     app.on_key(Key::Tab, &Editor),
///     Reaction::Accept { text: " regards".to_owned(), rest: None }
/// );
/// ```
#[derive(Debug, Default)]
pub struct App {
    engine: Engine,
    status: Status,
}

impl App {
    /// Creates an active app with no typed context. Until [`use_settings`](Self::use_settings),
    /// the model reads no system prompt and no clipboard text.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Uses the system prompt and the clipboard access of `settings` from the next key in a new
    /// place on.
    pub fn use_settings(&mut self, settings: &Settings) {
        self.engine.use_settings(settings);
    }

    /// Handles a key pressed in the focused field of `desktop`, before the key reaches the field.
    pub fn on_key(&mut self, key: Key, desktop: &dyn Desktop) -> Reaction {
        match self.status {
            Status::Paused => Reaction::Hide,
            Status::Active => match desktop.field() {
                Field::Excluded => self.engine.reset(),
                Field::Open => self.engine.on_key(key, desktop),
            },
        }
    }

    /// Handles the completion of the last [`Reaction::Request`].
    pub fn on_completion(&mut self, result: Result<Completion, InferenceError>) -> Reaction {
        self.engine.on_completion(result)
    }

    /// Handles the caret moving somewhere unknown: focus change, mouse click.
    pub fn on_context_lost(&mut self) -> Reaction {
        self.engine.reset()
    }

    /// Returns what the tray icon and menu must show now.
    #[must_use]
    pub fn tray_view(&self) -> TrayView {
        let (tooltip, toggle_label) = match self.status {
            Status::Active => ("GhostType", "Pause suggestions"),
            Status::Paused => ("GhostType (paused)", "Resume suggestions"),
        };
        TrayView {
            status: self.status,
            tooltip,
            items: vec![
                TrayItem {
                    label: toggle_label,
                    command: TrayCommand::TogglePause,
                },
                TrayItem {
                    label: "Settings…",
                    command: TrayCommand::OpenSettings,
                },
                TrayItem {
                    label: "Quit GhostType",
                    command: TrayCommand::Quit,
                },
            ],
        }
    }

    /// Runs a command picked from the tray menu.
    pub fn on_tray_command(&mut self, command: TrayCommand) -> TrayOutcome {
        match command {
            TrayCommand::TogglePause => {
                self.status = match self.status {
                    Status::Active => Status::Paused,
                    Status::Paused => Status::Active,
                };
                self.engine.reset();
                TrayOutcome::Updated
            }
            TrayCommand::OpenSettings => TrayOutcome::OpenSettings,
            TrayCommand::Quit => TrayOutcome::Quit,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::desktop::Surroundings;
    use crate::engine::MAX_SUGGESTION_TOKENS;
    use crate::inference::{InferenceRequest, Prompt, TokenCount};

    struct FakeDesktop(Field);

    impl Desktop for FakeDesktop {
        fn field(&self) -> Field {
            self.0
        }

        fn surroundings(&self) -> Surroundings {
            Surroundings::default()
        }

        fn clipboard_text(&self) -> Option<String> {
            None
        }
    }

    fn request(context: &str) -> Reaction {
        Reaction::Request(InferenceRequest::Complete {
            prompt: Prompt(context.to_owned()),
            max_tokens: MAX_SUGGESTION_TOKENS,
        })
    }

    #[expect(clippy::unnecessary_wraps, reason = "on_completion takes a Result")]
    fn completion(text: &str) -> Result<Completion, InferenceError> {
        Ok(Completion {
            text: text.to_owned(),
            tokens: TokenCount(1),
            duration: Duration::ZERO,
        })
    }

    fn type_key(app: &mut App, character: char) -> Reaction {
        app.on_key(Key::Char(character), &FakeDesktop(Field::Open))
    }

    fn app_showing(suggestion: &str) -> App {
        let mut app = App::new();
        type_key(&mut app, 'B');
        app.on_completion(completion(suggestion));
        app
    }

    #[test]
    fn active_app_requests_completion() {
        let mut app = App::new();

        let reaction = type_key(&mut app, 'B');

        assert_eq!(reaction, request("B"));
    }

    #[test]
    fn completion_shows_in_active_app() {
        let mut app = App::new();
        type_key(&mut app, 'B');

        let reaction = app.on_completion(completion("est regards"));

        assert_eq!(reaction, Reaction::Show("est regards".to_owned()));
    }

    #[test]
    fn excluded_field_gets_no_suggestion() {
        let mut app = App::new();

        let reaction = app.on_key(Key::Char('B'), &FakeDesktop(Field::Excluded));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn excluded_field_forgets_typed_context() {
        let mut app = app_showing("est");
        app.on_key(Key::Char('x'), &FakeDesktop(Field::Excluded));

        let reaction = type_key(&mut app, 'r');

        assert_eq!(reaction, request("r"));
    }

    #[test]
    fn settings_give_the_system_prompt() {
        let mut app = App::new();
        app.use_settings(&Settings::default());

        let reaction = type_key(&mut app, 'B');

        assert_eq!(
            reaction,
            request(&format!("{}\n\nB", crate::settings::DEFAULT_SYSTEM_PROMPT))
        );
    }

    #[test]
    fn context_lost_hides_suggestion() {
        let mut app = app_showing("est");

        let reaction = app.on_context_lost();

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn new_app_is_active_with_pause_entry() {
        let app = App::new();

        let view = app.tray_view();

        assert_eq!(
            view,
            TrayView {
                status: Status::Active,
                tooltip: "GhostType",
                items: vec![
                    TrayItem {
                        label: "Pause suggestions",
                        command: TrayCommand::TogglePause
                    },
                    TrayItem {
                        label: "Settings…",
                        command: TrayCommand::OpenSettings
                    },
                    TrayItem {
                        label: "Quit GhostType",
                        command: TrayCommand::Quit
                    },
                ],
            }
        );
    }

    #[test]
    fn toggle_pause_updates_tray() {
        let mut app = App::new();

        let outcome = app.on_tray_command(TrayCommand::TogglePause);

        assert_eq!(outcome, TrayOutcome::Updated);
    }

    #[test]
    fn paused_app_offers_resume_entry() {
        let mut app = App::new();
        app.on_tray_command(TrayCommand::TogglePause);

        let view = app.tray_view();

        assert_eq!(
            view,
            TrayView {
                status: Status::Paused,
                tooltip: "GhostType (paused)",
                items: vec![
                    TrayItem {
                        label: "Resume suggestions",
                        command: TrayCommand::TogglePause
                    },
                    TrayItem {
                        label: "Settings…",
                        command: TrayCommand::OpenSettings
                    },
                    TrayItem {
                        label: "Quit GhostType",
                        command: TrayCommand::Quit
                    },
                ],
            }
        );
    }

    #[test]
    fn paused_app_does_not_suggest() {
        let mut app = App::new();
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = type_key(&mut app, 'B');

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn paused_app_lets_tab_through() {
        let mut app = app_showing("est");
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = app.on_key(Key::Tab, &FakeDesktop(Field::Open));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn paused_app_drops_completion() {
        let mut app = App::new();
        type_key(&mut app, 'B');
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = app.on_completion(completion("est"));

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn resumed_app_suggests_again() {
        let mut app = App::new();
        app.on_tray_command(TrayCommand::TogglePause);
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = type_key(&mut app, 'B');

        assert_eq!(reaction, request("B"));
    }

    #[test]
    fn pausing_forgets_typed_context() {
        let mut app = app_showing("est");
        app.on_tray_command(TrayCommand::TogglePause);
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = type_key(&mut app, 'r');

        assert_eq!(reaction, request("r"));
    }

    #[test]
    fn settings_command_opens_settings() {
        let mut app = App::new();

        let outcome = app.on_tray_command(TrayCommand::OpenSettings);

        assert_eq!(outcome, TrayOutcome::OpenSettings);
    }

    #[test]
    fn quit_command_quits() {
        let mut app = App::new();

        let outcome = app.on_tray_command(TrayCommand::Quit);

        assert_eq!(outcome, TrayOutcome::Quit);
    }
}
