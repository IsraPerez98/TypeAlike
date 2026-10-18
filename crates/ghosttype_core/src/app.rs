//! App lifecycle: whether suggestions are on, and the tray menu and caret badge that control it.

use std::time::Duration;

use crate::desktop::{Desktop, OcrAvailability, ScreenRead, ScreenReadId, ScreenSource};
use crate::engine::{Engine, Reaction};
use crate::inference::{Completion, InferenceError, InferenceRequest};
use crate::key::Key;
use crate::settings::{PromptPreparation, ScreenTextApp, ScreenTextSettings, Settings, SuggestionDisplay};

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

/// What GhostType does when the input settles in a field, a session starts or a read of the
/// screen answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettledWork {
    /// Read the text near the focused field on the screen from `source`, off the thread of the
    /// keys. Give the answer with the id `read` to [`App::on_screen_read`].
    ReadScreen {
        /// The id of the answer.
        read: ScreenReadId,
        /// Where to read the text.
        source: ScreenSource,
    },
    /// Decode the prompt of the focused field before the user types. Submit it as the request of
    /// a [`Reaction::Request`]. Its completion goes to [`App::on_completion`].
    Prepare(InferenceRequest),
}

/// GhostType's state while running: suggestion engine plus on/off status.
///
/// ```
/// use std::time::Duration;
///
/// use ghosttype_core::app::{App, Field};
/// use ghosttype_core::desktop::{CopiedText, Desktop, Surroundings};
/// use ghosttype_core::engine::Reaction;
/// use ghosttype_core::inference::{Completion, TokenCount};
/// use ghosttype_core::key::Key;
///
/// struct Editor;
///
/// impl Desktop for Editor {
///     fn field(&self) -> Field { Field::Open }
///     fn surroundings(&self) -> Surroundings { Surroundings::default() }
///     fn copied_text(&self) -> Option<CopiedText> { None }
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
    screen_text: ScreenTextSettings,
}

impl App {
    /// Creates an active app with no typed context. Until [`use_settings`](Self::use_settings),
    /// the model reads no writer notes and no clipboard text.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Uses the writer notes and the clipboard access of `settings` from the next key in a new
    /// place on, and its prompt preparation, suggestion display, suggestion length and screen
    /// text from now on.
    pub fn use_settings(&mut self, settings: &Settings) {
        self.engine.use_settings(settings);
        self.prompt_preparation = settings.prompt_preparation;
        self.suggestion_display = settings.suggestion_display;
        if self.screen_text != settings.screen_text {
            self.engine.forget_screen();
            self.screen_text = settings.screen_text.clone();
        }
    }

    /// Handles [`INPUT_SETTLE_DELAY`] without keys and clicks. In a text field of an app where
    /// the screen text is on, it starts a read of the screen, unless the session has its final
    /// screen text or waits for a read.
    /// Else, with [`PromptPreparation::OnFocus`] and no session, it prepares the prompt of the
    /// focused field of `desktop`.
    pub fn on_input_settled(&mut self, desktop: &dyn Desktop) -> Option<SettledWork> {
        if self.status == Status::Paused || desktop.field() == Field::Excluded {
            return None;
        }
        if self.reads_screen_in(desktop) {
            return self
                .engine
                .start_screen_read()
                .map(|read| SettledWork::ReadScreen {
                    read,
                    source: ScreenSource::Accessibility,
                });
        }
        if self.engine.has_session() {
            return None;
        }
        self.prepare_on_focus(desktop).map(SettledWork::Prepare)
    }

    /// Handles the answer to a read of the screen. When the accessibility tree gives no lines and
    /// `desktop` can do an OCR, it returns the OCR read. Else the screen text goes into the
    /// session when the session has no final screen text, and into the next session. With
    /// [`PromptPreparation::OnFocus`], it then returns the request that prepares the prompt of the
    /// focused field of `desktop` with the new screen text.
    pub fn on_screen_read(&mut self, read: ScreenRead, desktop: &dyn Desktop) -> Option<SettledWork> {
        let is_active = self.status == Status::Active && desktop.field() == Field::Open;
        let has_lines = read.text.as_ref().is_some_and(|text| !text.lines.is_empty());
        let is_ocr_due = is_active && !has_lines && desktop.ocr_availability() == OcrAvailability::Ready;
        let level = self.screen_text.ocr_level;
        if is_ocr_due && let Some(ocr) = self.engine.start_ocr_after(read.id, level) {
            return Some(SettledWork::ReadScreen {
                read: ocr,
                source: ScreenSource::Ocr(level),
            });
        }
        let is_newest = self.engine.on_screen_read(read, desktop);
        if !is_newest || !is_active {
            return None;
        }
        self.prepare_on_focus(desktop).map(SettledWork::Prepare)
    }

    /// Starts a read of the screen for the session of the last key, when the session is in a text
    /// field of an app where the screen text is on. The session uses the stored screen text until
    /// the answer comes. A session that has its own screen text or waits for a read starts none.
    pub fn start_session_screen_read(&mut self) -> Option<SettledWork> {
        self.engine
            .start_session_screen_read(&self.screen_text)
            .map(|read| SettledWork::ReadScreen {
                read,
                source: ScreenSource::Accessibility,
            })
    }

    /// The app where the user types now, when it has an id. The first key in a field starts the
    /// typing there.
    #[must_use]
    pub fn typed_app(&self) -> Option<&ScreenTextApp> {
        self.engine.session_app()
    }

    fn reads_screen_in(&self, desktop: &dyn Desktop) -> bool {
        let surroundings = desktop.surroundings();
        let is_text_field = surroundings.field_text.is_some();
        is_text_field
            && surroundings
                .app_id
                .is_some_and(|app_id| self.screen_text.allows(&app_id))
    }

    fn prepare_on_focus(&mut self, desktop: &dyn Desktop) -> Option<InferenceRequest> {
        match self.prompt_preparation {
            PromptPreparation::OnFocus => self.engine.prepare(desktop),
            PromptPreparation::OnFirstKey => None,
        }
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

    /// Handles the caret moving somewhere unknown: focus change, mouse click. The screen text
    /// of the last field is not used again.
    pub fn on_context_lost(&mut self) -> Reaction {
        self.is_in_open_field = false;
        self.engine.forget_screen();
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
    use crate::desktop::{
        Confidence, CopiedText, FieldText, ScreenLine, ScreenRect, ScreenText, Surroundings,
    };
    use crate::inference::TokenCount;
    use crate::prompt::framed_prompt;
    use crate::settings::{OcrLevel, ScreenTextAccess, WriterNotes};

    struct FakeDesktop(Field);

    impl Desktop for FakeDesktop {
        fn field(&self) -> Field {
            self.0
        }

        fn surroundings(&self) -> Surroundings {
            Surroundings::default()
        }

        fn copied_text(&self) -> Option<CopiedText> {
            None
        }
    }

    /// A text field of the app `app_id`.
    struct TextField {
        app_id: &'static str,
        field: Field,
        ocr: OcrAvailability,
        has_readable_text: bool,
    }

    impl Desktop for TextField {
        fn field(&self) -> Field {
            self.field
        }

        fn surroundings(&self) -> Surroundings {
            Surroundings {
                app_id: Some(self.app_id.to_owned()),
                field_text: self.has_readable_text.then(FieldText::default),
                ..Surroundings::default()
            }
        }

        fn copied_text(&self) -> Option<CopiedText> {
            None
        }

        fn ocr_availability(&self) -> OcrAvailability {
            self.ocr
        }
    }

    const MAIL: TextField = TextField {
        app_id: "com.apple.mail",
        field: Field::Open,
        ocr: OcrAvailability::Unsupported,
        has_readable_text: true,
    };

    const MAIL_WITH_OCR: TextField = TextField {
        ocr: OcrAvailability::Ready,
        ..MAIL
    };

    fn preparation(lines: &[&str], text_before: &str) -> InferenceRequest {
        InferenceRequest::Complete {
            prompt: framed_prompt(lines, text_before),
            max_tokens: TokenCount(0),
        }
    }

    fn request_with(lines: &[&str], text_before: &str) -> Reaction {
        Reaction::Request(InferenceRequest::Complete {
            prompt: framed_prompt(lines, text_before),
            max_tokens: Settings::default().suggestion_length_in_use().max_tokens(),
        })
    }

    fn request(text_before: &str) -> Reaction {
        request_with(&[], text_before)
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
    fn settings_give_the_writer_notes() {
        let mut app = App::new();
        app.use_settings(&Settings {
            writer_notes: WriterNotes("I am a nurse.".to_owned()),
            ..Settings::default()
        });

        let reaction = type_key(&mut app, 'B');

        assert_eq!(
            reaction,
            request_with(&["Notes from the writer: “I am a nurse.”"], "B")
        );
    }

    fn app_preparing_on_focus() -> App {
        let mut app = App::new();
        app.use_settings(&Settings {
            prompt_preparation: PromptPreparation::OnFocus,
            ..Settings::default()
        });
        app
    }

    #[test]
    fn settled_input_prepares_the_open_field() {
        let mut app = app_preparing_on_focus();

        let request = app.on_input_settled(&FakeDesktop(Field::Open));

        assert_eq!(request, Some(SettledWork::Prepare(preparation(&[], ""))));
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
    fn screen_read(app: &mut App) -> ScreenReadId {
        match app.on_input_settled(&MAIL) {
            Some(SettledWork::ReadScreen {
                read,
                source: ScreenSource::Accessibility,
            }) => read,
            work => panic!("a settled text field reads the screen: {work:?}"),
        }
    }

    fn answer(read: ScreenReadId) -> ScreenRead {
        ScreenRead {
            id: read,
            text: Some(ScreenText::default()),
        }
    }

    #[test]
    fn settled_text_field_reads_the_screen_with_each_preparation() {
        let mut first_key = App::new();
        let mut on_focus = app_preparing_on_focus();

        let works = [&mut first_key, &mut on_focus].map(|app| app.on_input_settled(&MAIL));

        assert!(
            works
                .iter()
                .all(|work| matches!(work, Some(SettledWork::ReadScreen { .. }))),
            "{works:?}"
        );
    }

    #[test]
    fn settled_field_without_text_reads_no_screen() {
        let mut app = App::new();

        let work = app.on_input_settled(&FakeDesktop(Field::Open));

        assert_eq!(work, None);
    }

    #[test]
    fn settled_password_field_reads_no_screen() {
        let mut app = App::new();

        let work = app.on_input_settled(&TextField {
            field: Field::Excluded,
            ..MAIL
        });

        assert_eq!(work, None);
    }

    #[test]
    fn settled_field_of_a_password_manager_reads_no_screen() {
        let mut app = App::new();

        let work = app.on_input_settled(&TextField {
            app_id: "com.1password.1password",
            ..MAIL
        });

        assert_eq!(work, None);
    }

    #[test]
    fn screen_text_off_prepares_on_focus_at_once() {
        let mut app = App::new();
        app.use_settings(&Settings {
            prompt_preparation: PromptPreparation::OnFocus,
            screen_text: ScreenTextSettings {
                access: ScreenTextAccess::Denied,
                ..ScreenTextSettings::default()
            },
            ..Settings::default()
        });

        let work = app.on_input_settled(&MAIL);

        assert_eq!(
            work,
            Some(SettledWork::Prepare(preparation(&["Format: email"], "")))
        );
    }

    fn answer_with(read: ScreenReadId, line: &str) -> ScreenRead {
        ScreenRead {
            id: read,
            text: Some(ScreenText {
                lines: vec![ScreenLine {
                    text: line.to_owned(),
                    confidence: Confidence::CERTAIN,
                    frame: ScreenRect {
                        left: 0,
                        top: 100,
                        right: 500,
                        bottom: 120,
                    },
                }],
                caret: None,
                field: ScreenRect {
                    left: 0,
                    top: 200,
                    right: 500,
                    bottom: 240,
                },
            }),
        }
    }

    #[test]
    fn settled_input_during_a_session_without_screen_text_reads_the_screen() {
        let mut app = App::new();
        app.on_key(Key::Char('H'), &MAIL);

        let work = app.on_input_settled(&MAIL);

        assert!(matches!(work, Some(SettledWork::ReadScreen { .. })), "{work:?}");
    }

    #[test]
    fn settled_input_during_a_session_with_its_screen_text_reads_no_screen() {
        let mut app = App::new();
        app.on_key(Key::Char('H'), &MAIL);
        let Some(SettledWork::ReadScreen { read, .. }) = app.start_session_screen_read() else {
            panic!("the first key reads the screen");
        };
        app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL);

        let work = app.on_input_settled(&MAIL);

        assert_eq!(work, None);
    }

    #[test]
    fn first_key_starts_a_read_when_the_session_uses_stored_text() {
        let mut app = App::new();
        let read = screen_read(&mut app);
        app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL);
        app.on_key(Key::Char('H'), &MAIL);

        let work = app.start_session_screen_read();

        assert!(
            matches!(
                work,
                Some(SettledWork::ReadScreen {
                    source: ScreenSource::Accessibility,
                    ..
                })
            ),
            "{work:?}"
        );
    }

    #[test]
    fn session_without_stored_text_reads_at_the_first_key() {
        let mut app = App::new();
        app.on_key(Key::Char('H'), &MAIL);

        let work = app.start_session_screen_read();

        assert!(
            matches!(
                work,
                Some(SettledWork::ReadScreen {
                    source: ScreenSource::Accessibility,
                    ..
                })
            ),
            "{work:?}"
        );
    }

    #[test]
    fn no_read_starts_at_the_first_key_in_an_app_that_is_off() {
        let mut app = App::new();
        app.use_settings(&Settings {
            screen_text: ScreenTextSettings {
                apps: vec![ScreenTextApp {
                    app_id: "com.apple.mail".to_owned(),
                    app_name: "Mail".to_owned(),
                    app_path: None,
                    is_on: false,
                }],
                ..ScreenTextSettings::default()
            },
            ..Settings::default()
        });
        app.on_key(Key::Char('H'), &MAIL);

        let work = app.start_session_screen_read();

        assert_eq!(work, None);
    }

    #[test]
    fn no_read_starts_at_the_first_key_in_a_field_without_readable_text() {
        let mut app = App::new();
        app.on_key(
            Key::Char('H'),
            &TextField {
                has_readable_text: false,
                ..MAIL
            },
        );

        let work = app.start_session_screen_read();

        assert_eq!(work, None);
    }

    #[test]
    fn no_read_starts_without_a_session() {
        let mut app = App::new();

        let work = app.start_session_screen_read();

        assert_eq!(work, None);
    }

    #[test]
    fn settled_input_during_a_session_prepares_nothing() {
        let mut app = App::new();
        app.use_settings(&Settings {
            prompt_preparation: PromptPreparation::OnFocus,
            screen_text: ScreenTextSettings {
                access: ScreenTextAccess::Denied,
                ..ScreenTextSettings::default()
            },
            ..Settings::default()
        });
        app.on_key(Key::Char('H'), &MAIL);
        app.on_completion(completion(""));

        let work = app.on_input_settled(&MAIL);

        assert_eq!(work, None);
    }

    #[test]
    fn answer_during_a_session_prepares_its_prompt_with_the_screen_text() {
        let mut app = app_preparing_on_focus();
        app.on_key(Key::Char('H'), &MAIL);
        app.on_completion(completion(""));
        let read = screen_read(&mut app);

        let request = app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL);

        assert_eq!(
            request,
            Some(SettledWork::Prepare(preparation(
                &["Format: email", "Nearby on screen: “Dinner at 7?”"],
                "H"
            )))
        );
    }

    #[test]
    fn answer_during_a_running_completion_prepares_nothing() {
        let mut app = app_preparing_on_focus();
        app.on_key(Key::Char('H'), &MAIL);
        let read = screen_read(&mut app);

        let request = app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL);

        assert_eq!(request, None);
    }

    #[test]
    fn answer_to_the_screen_read_prepares_on_focus() {
        let mut app = app_preparing_on_focus();
        let read = screen_read(&mut app);

        let request = app.on_screen_read(answer(read), &MAIL);

        assert_eq!(
            request,
            Some(SettledWork::Prepare(preparation(&["Format: email"], "")))
        );
    }

    #[test]
    fn answer_to_the_screen_read_prepares_nothing_before_the_first_key() {
        let mut app = App::new();
        let read = screen_read(&mut app);

        let request = app.on_screen_read(answer(read), &MAIL);

        assert_eq!(request, None);
    }

    #[test]
    fn answer_after_a_lost_context_is_dropped() {
        let mut app = app_preparing_on_focus();
        let read = screen_read(&mut app);
        app.on_context_lost();

        let request = app.on_screen_read(answer(read), &MAIL);

        assert_eq!(request, None);
    }

    #[test]
    fn answer_without_lines_starts_an_ocr_read() {
        let mut app = App::new();
        let read = screen_read(&mut app);

        let work = app.on_screen_read(answer(read), &MAIL_WITH_OCR);

        assert!(
            matches!(
                work,
                Some(SettledWork::ReadScreen {
                    source: ScreenSource::Ocr(_),
                    ..
                })
            ),
            "{work:?}"
        );
    }

    #[test]
    fn ocr_read_uses_the_level_of_the_settings() {
        let mut app = App::new();
        app.use_settings(&Settings {
            screen_text: ScreenTextSettings {
                ocr_level: OcrLevel::Accurate,
                ..ScreenTextSettings::default()
            },
            ..Settings::default()
        });
        let read = screen_read(&mut app);

        let work = app.on_screen_read(answer(read), &MAIL_WITH_OCR);

        assert!(
            matches!(
                work,
                Some(SettledWork::ReadScreen {
                    source: ScreenSource::Ocr(OcrLevel::Accurate),
                    ..
                })
            ),
            "{work:?}"
        );
    }

    #[test]
    fn answer_with_lines_starts_no_ocr_read() {
        let mut app = App::new();
        let read = screen_read(&mut app);

        let work = app.on_screen_read(answer_with(read, "Dinner at 7?"), &MAIL_WITH_OCR);

        assert_eq!(work, None);
    }

    #[test]
    fn answer_without_lines_starts_no_ocr_read_when_the_ocr_cannot_run() {
        let mut app = App::new();
        let read = screen_read(&mut app);

        let work = app.on_screen_read(
            answer(read),
            &TextField {
                ocr: OcrAvailability::NotPermitted,
                ..MAIL
            },
        );

        assert_eq!(work, None);
    }

    #[test]
    fn answer_of_the_ocr_without_lines_starts_no_other_read() {
        let mut app = App::new();
        let read = screen_read(&mut app);
        let Some(SettledWork::ReadScreen { read: ocr, .. }) =
            app.on_screen_read(answer(read), &MAIL_WITH_OCR)
        else {
            panic!("an answer without lines starts an OCR read");
        };

        let work = app.on_screen_read(answer(ocr), &MAIL_WITH_OCR);

        assert_eq!(work, None);
    }

    #[test]
    fn answer_of_the_ocr_goes_into_the_prompt() {
        let mut app = app_preparing_on_focus();
        let read = screen_read(&mut app);
        let Some(SettledWork::ReadScreen { read: ocr, .. }) =
            app.on_screen_read(answer(read), &MAIL_WITH_OCR)
        else {
            panic!("an answer without lines starts an OCR read");
        };

        let work = app.on_screen_read(answer_with(ocr, "Dinner at 7?"), &MAIL_WITH_OCR);

        assert_eq!(
            work,
            Some(SettledWork::Prepare(preparation(
                &["Format: email", "Nearby on screen: “Dinner at 7?”"],
                ""
            )))
        );
    }

    #[test]
    fn typing_names_the_typed_app() {
        let mut app = App::new();

        app.on_key(Key::Char('H'), &MAIL);

        assert_eq!(
            app.typed_app().map(|typed| typed.app_id.as_str()),
            Some("com.apple.mail")
        );
    }
}
