//! App lifecycle: whether suggestions are on, and the tray menu and caret badge that control it.

use std::time::Duration;

use crate::desktop::Desktop;
use crate::engine::{Engine, Reaction};
use crate::inference::{Completion, InferenceError, InferenceRequest};
use crate::key::Key;
use crate::settings::{PromptPreparation, Settings, SuggestionDisplay};

/// The time without keys and clicks after which the caret is where the user types next. A short
/// stop on the way to another field must not start the model.
pub const INPUT_SETTLE_DELAY: Duration = Duration::from_millis(300);

/// Whether GhostType is currently offering suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    /// Suggestions are shown while typing.
    #[default]
    Active,
    /// The user paused suggestions from the tray or the badge.
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

/// An action the user can pick from the tray menu or the badge at the caret.
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

/// The badge at the caret of the field where the user types: a button that pauses or resumes
/// suggestions. It shows above the caret, on the side opposite to the ghost text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadgeView {
    /// Status the glyph must reflect.
    pub status: Status,
    /// What a click does, in sentence case, for assistive technology.
    pub label: &'static str,
    /// Command run on click.
    pub command: TrayCommand,
}

/// What the platform must do after a tray command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayOutcome {
    /// Suggestions stopped: hide any ghost text, re-render the tray and the badge, and unload
    /// the suggestion model to free its memory.
    Paused,
    /// Suggestions started again: re-render the tray and the badge, and load the suggestion
    /// model again.
    Resumed,
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
    is_in_open_field: bool,
    prompt_preparation: PromptPreparation,
    suggestion_display: SuggestionDisplay,
}

impl App {
    /// Creates an active app with no typed context. Until [`use_settings`](Self::use_settings),
    /// the model reads no system prompt and no clipboard text.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Uses the system prompt and the clipboard access of `settings` from the next key in a new
    /// place on, and its prompt preparation and suggestion display from now on.
    pub fn use_settings(&mut self, settings: &Settings) {
        self.engine.use_settings(settings);
        self.prompt_preparation = settings.prompt_preparation;
        self.suggestion_display = settings.suggestion_display;
    }

    /// Handles [`INPUT_SETTLE_DELAY`] without keys and clicks. With
    /// [`PromptPreparation::OnFocus`], it returns the request that decodes the prompt of the
    /// focused field of `desktop` before the user types. Submit it as the request of a
    /// [`Reaction::Request`]. Its completion goes to [`on_completion`](Self::on_completion).
    pub fn on_input_settled(&mut self, desktop: &dyn Desktop) -> Option<InferenceRequest> {
        let prepares_on_focus =
            self.status == Status::Active && self.prompt_preparation == PromptPreparation::OnFocus;
        if !prepares_on_focus || desktop.field() == Field::Excluded {
            return None;
        }
        self.engine.prepare(desktop)
    }

    /// Handles a key pressed in the focused field of `desktop`, before the key reaches the field.
    pub fn on_key(&mut self, key: Key, desktop: &dyn Desktop) -> Reaction {
        let field = desktop.field();
        self.is_in_open_field = field == Field::Open;
        match (self.status, field) {
            (Status::Paused, _) => Reaction::Hide,
            (Status::Active, Field::Excluded) => self.engine.reset(),
            (Status::Active, Field::Open) => self.engine.on_key(key, desktop),
        }
    }

    /// Handles the completion of the last [`Reaction::Request`].
    pub fn on_completion(&mut self, result: Result<Completion, InferenceError>) -> Reaction {
        self.engine.on_completion(result)
    }

    /// Handles `text`, the text that the completion of the last [`Reaction::Request`] added so far.
    /// Returns the reaction when the ghost text changes, or `None`. Only
    /// [`SuggestionDisplay::WordByWord`] changes the ghost text before the completion finishes.
    pub fn on_partial_completion(&mut self, text: &str) -> Option<Reaction> {
        if self.suggestion_display != SuggestionDisplay::WordByWord {
            return None;
        }
        self.engine.on_partial_completion(text)
    }

    /// Handles the caret moving somewhere unknown: focus change, mouse click.
    pub fn on_context_lost(&mut self) -> Reaction {
        self.is_in_open_field = false;
        self.engine.reset()
    }

    /// Returns what the tray icon and menu must show now.
    #[must_use]
    pub fn tray_view(&self) -> TrayView {
        let tooltip = match self.status {
            Status::Active => "GhostType",
            Status::Paused => "GhostType (paused)",
        };
        TrayView {
            status: self.status,
            tooltip,
            items: vec![
                TrayItem {
                    label: self.toggle_label(),
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

    /// Returns the badge to show at the caret, or `None` while the user types in no field that
    /// GhostType watches.
    #[must_use]
    pub fn badge_view(&self) -> Option<BadgeView> {
        self.is_in_open_field.then(|| BadgeView {
            status: self.status,
            label: self.toggle_label(),
            command: TrayCommand::TogglePause,
        })
    }

    /// Runs a command picked from the tray menu or the badge.
    pub fn on_tray_command(&mut self, command: TrayCommand) -> TrayOutcome {
        match command {
            TrayCommand::TogglePause => self.toggle_pause(),
            TrayCommand::OpenSettings => TrayOutcome::OpenSettings,
            TrayCommand::Quit => TrayOutcome::Quit,
        }
    }

    fn toggle_pause(&mut self) -> TrayOutcome {
        self.engine.reset();
        let (status, outcome) = match self.status {
            Status::Active => (Status::Paused, TrayOutcome::Paused),
            Status::Paused => (Status::Active, TrayOutcome::Resumed),
        };
        self.status = status;
        outcome
    }

    fn toggle_label(&self) -> &'static str {
        match self.status {
            Status::Active => "Pause suggestions",
            Status::Paused => "Resume suggestions",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::desktop::Surroundings;
    use crate::engine::MAX_SUGGESTION_TOKENS;
    use crate::inference::{Prompt, SystemPrompt, TokenCount};

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

    fn app_showing_words_early() -> App {
        let mut app = App::new();
        app.use_settings(&Settings {
            suggestion_display: SuggestionDisplay::WordByWord,
            ..Settings::default()
        });
        app
    }

    #[test]
    fn partial_completion_shows_when_words_show_early() {
        let mut app = app_showing_words_early();
        type_key(&mut app, 'B');

        let reaction = app.on_partial_completion("est regards");

        assert_eq!(reaction, Some(Reaction::Show("est".to_owned())));
    }

    #[test]
    fn partial_completion_shows_nothing_when_suggestions_show_complete() {
        let mut app = App::new();
        type_key(&mut app, 'B');

        let reaction = app.on_partial_completion("est regards");

        assert_eq!(reaction, None);
    }

    #[test]
    fn completion_after_a_partial_completion_that_shows_nothing_shows_all_its_text() {
        let mut app = App::new();
        type_key(&mut app, 'B');
        app.on_partial_completion("est regards");

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

    fn app_preparing_on_focus() -> App {
        let mut app = App::new();
        app.use_settings(&Settings {
            custom_system_prompt: Some(SystemPrompt(String::new())),
            prompt_preparation: PromptPreparation::OnFocus,
            ..Settings::default()
        });
        app
    }

    #[test]
    fn settled_input_prepares_the_open_field() {
        let mut app = app_preparing_on_focus();

        let request = app.on_input_settled(&FakeDesktop(Field::Open));

        assert_eq!(
            request,
            Some(InferenceRequest::Complete {
                prompt: Prompt(String::new()),
                max_tokens: TokenCount(0),
            })
        );
    }

    #[test]
    fn settled_input_prepares_nothing_when_preparation_waits_for_the_first_key() {
        let mut app = App::new();

        let request = app.on_input_settled(&FakeDesktop(Field::Open));

        assert_eq!(request, None);
    }

    #[test]
    fn settled_input_prepares_no_excluded_field() {
        let mut app = app_preparing_on_focus();

        let request = app.on_input_settled(&FakeDesktop(Field::Excluded));

        assert_eq!(request, None);
    }

    #[test]
    fn paused_app_prepares_nothing() {
        let mut app = app_preparing_on_focus();
        app.on_tray_command(TrayCommand::TogglePause);

        let request = app.on_input_settled(&FakeDesktop(Field::Open));

        assert_eq!(request, None);
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
    fn pausing_reports_paused() {
        let mut app = App::new();

        let outcome = app.on_tray_command(TrayCommand::TogglePause);

        assert_eq!(outcome, TrayOutcome::Paused);
    }

    #[test]
    fn resuming_reports_resumed() {
        let mut app = App::new();
        app.on_tray_command(TrayCommand::TogglePause);

        let outcome = app.on_tray_command(TrayCommand::TogglePause);

        assert_eq!(outcome, TrayOutcome::Resumed);
    }

    const PAUSE_BADGE: BadgeView = BadgeView {
        status: Status::Active,
        label: "Pause suggestions",
        command: TrayCommand::TogglePause,
    };

    const RESUME_BADGE: BadgeView = BadgeView {
        status: Status::Paused,
        label: "Resume suggestions",
        command: TrayCommand::TogglePause,
    };

    #[test]
    fn new_app_shows_no_badge() {
        let app = App::new();

        assert_eq!(app.badge_view(), None);
    }

    #[test]
    fn typing_in_open_field_shows_pause_badge() {
        let mut app = App::new();

        type_key(&mut app, 'B');

        assert_eq!(app.badge_view(), Some(PAUSE_BADGE));
    }

    #[test]
    fn typing_in_excluded_field_hides_badge() {
        let mut app = App::new();
        type_key(&mut app, 'B');

        app.on_key(Key::Char('x'), &FakeDesktop(Field::Excluded));

        assert_eq!(app.badge_view(), None);
    }

    #[test]
    fn context_lost_hides_badge() {
        let mut app = App::new();
        type_key(&mut app, 'B');

        app.on_context_lost();

        assert_eq!(app.badge_view(), None);
    }

    #[test]
    fn pausing_turns_badge_into_resume_badge() {
        let mut app = App::new();
        type_key(&mut app, 'B');

        app.on_tray_command(TrayCommand::TogglePause);

        assert_eq!(app.badge_view(), Some(RESUME_BADGE));
    }

    #[test]
    fn paused_app_shows_resume_badge_in_open_field() {
        let mut app = App::new();
        app.on_tray_command(TrayCommand::TogglePause);

        type_key(&mut app, 'B');

        assert_eq!(app.badge_view(), Some(RESUME_BADGE));
    }

    #[test]
    fn paused_app_shows_no_badge_in_excluded_field() {
        let mut app = App::new();
        app.on_tray_command(TrayCommand::TogglePause);

        app.on_key(Key::Char('B'), &FakeDesktop(Field::Excluded));

        assert_eq!(app.badge_view(), None);
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
    fn paused_app_drops_partial_completion() {
        let mut app = app_showing_words_early();
        type_key(&mut app, 'B');
        app.on_tray_command(TrayCommand::TogglePause);

        let reaction = app.on_partial_completion("est regards");

        assert_eq!(reaction, None);
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
