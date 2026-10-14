//! App lifecycle: whether suggestions are on, and the tray menu that controls it.

use crate::engine::{Engine, Reaction};
use crate::key::Key;
use crate::suggestion::SuggestionProvider;

/// Whether GhostType is currently offering suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Suggestions are shown while typing.
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
    /// The app must exit.
    Quit,
}

/// GhostType's state while running: suggestion engine plus on/off status.
///
/// ```
/// use ghosttype_core::app::{App, Field};
/// use ghosttype_core::engine::Reaction;
/// use ghosttype_core::key::Key;
/// use ghosttype_core::phrases::PhraseList;
///
/// let mut app = App::new(Box::new(PhraseList::new(["Best regards"])));
/// for character in "Best r".chars() {
///     app.on_key(Key::Char(character), Field::Open);
/// }
///
/// assert_eq!(app.on_key(Key::Tab, Field::Open), Reaction::Accept("egards".to_owned()));
/// ```
#[derive(Debug)]
pub struct App {
    engine: Engine,
    status: Status,
}

impl App {
    /// Creates an active app suggesting completions from `provider`.
    #[must_use]
    pub fn new(provider: Box<dyn SuggestionProvider>) -> Self {
        Self {
            engine: Engine::new(provider),
            status: Status::Active,
        }
    }

    /// Handles a key pressed in the focused `field`.
    pub fn on_key(&mut self, key: Key, field: Field) -> Reaction {
        match (self.status, field) {
            (Status::Paused, _) => Reaction::Hide,
            (Status::Active, Field::Excluded) => self.engine.reset(),
            (Status::Active, Field::Open) => self.engine.on_key(key),
        }
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
            TrayCommand::Quit => TrayOutcome::Quit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::phrases::PhraseList;

    fn app() -> App {
        App::new(Box::new(PhraseList::new(["Best regards"])))
    }

    fn type_text(app: &mut App, text: &str) -> Reaction {
        text.chars()
            .map(|character| app.on_key(Key::Char(character), Field::Open))
            .last()
            .expect("text is not empty")
    }

    #[test]
    fn active_app_suggests() {
        let mut app = app();

        let reaction = type_text(&mut app, "Best r");

        assert_eq!(reaction, Reaction::Show("egards".to_owned()));
    }

    #[test]
    fn excluded_field_gets_no_suggestion() {
        let mut app = app();
        type_text(&mut app, "Best ");

        let reaction = app.on_key(Key::Char('r'), Field::Excluded);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn excluded_field_forgets_typed_context() {
        let mut app = app();
        type_text(&mut app, "Best ");
        app.on_key(Key::Char('x'), Field::Excluded);

        let reaction = type_text(&mut app, "r");

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn context_lost_hides_suggestion() {
        let mut app = app();
        type_text(&mut app, "Best r");

        let reaction = app.on_context_lost();

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn new_app_is_active_with_pause_entry() {
        let app = app();

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
                        label: "Quit GhostType",
                        command: TrayCommand::Quit
                    },
                ],
            }
        );
    }

    #[test]
    fn toggle_pause_updates_tray() {
        let mut app = app();

        let outcome = app.on_tray_command(TrayCommand::TogglePause);

        assert_eq!(outcome, TrayOutcome::Updated);
    }

    #[test]
    fn paused_app_offers_resume_entry() {
        let mut app = app();
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
                        label: "Quit GhostType",
                        command: TrayCommand::Quit
                    },
                ],
            }
        );
    }

    #[test]
    fn paused_app_does_not_suggest() {
        let mut app = app();
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = type_text(&mut app, "Best r");

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn paused_app_lets_tab_through() {
        let mut app = app();
        type_text(&mut app, "Best r");
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = app.on_key(Key::Tab, Field::Open);

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn resumed_app_suggests_again() {
        let mut app = app();
        app.on_tray_command(TrayCommand::TogglePause);
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = type_text(&mut app, "Best r");

        assert_eq!(reaction, Reaction::Show("egards".to_owned()));
    }

    #[test]
    fn pausing_forgets_typed_context() {
        let mut app = app();
        type_text(&mut app, "Best ");
        app.on_tray_command(TrayCommand::TogglePause);
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = type_text(&mut app, "r");

        assert_eq!(reaction, Reaction::Hide);
    }

    #[test]
    fn quit_command_quits() {
        let mut app = app();

        let outcome = app.on_tray_command(TrayCommand::Quit);

        assert_eq!(outcome, TrayOutcome::Quit);
    }
}
