//! App lifecycle: whether suggestions are on, and the tray menu and caret badge that control it.

mod badge;
mod tray;

pub use badge::{BadgeCommand, BadgeEntry, BadgeItem, BadgeMenu, BadgeView};
pub use tray::{TrayCommand, TrayItem, TrayOutcome, TrayView};

use std::time::Duration;

use badge::app_entries;

use crate::desktop::{Desktop, OcrAvailability, ScreenRead, ScreenReadId, ScreenSource};
use crate::engine::{Engine, Reaction};
use crate::history::HistoryEntry;
use crate::inference::{Completion, InferenceError, InferenceRequest};
use crate::key::Key;
use crate::settings::{
    PersonalizationSettings, PromptPreparation, ScreenTextApp, ScreenTextSettings, Settings,
    SuggestionDisplay,
};

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

/// What GhostType does when the input settles in a field, a session starts or a read of the
/// screen answers.
#[derive(Debug, Clone, PartialEq)]
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
    /// Show the suggestion for the new line after Enter: do what the reaction asks, as after a
    /// key.
    Suggest(Reaction),
}

/// GhostType's state while running: suggestion engine plus on/off status.
///
/// ```
/// use std::time::Duration;
///
/// use ghosttype_core::app::{App, Field};
/// use ghosttype_core::desktop::{CopiedText, Desktop, Surroundings};
/// use ghosttype_core::engine::{AfterAccept, Reaction};
/// use ghosttype_core::inference::{Completion, LogProbability, TokenCount};
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
///     word_log_probabilities: Vec::new(),
///     typed_log_probability: LogProbability::default(),
///     correction: None,
/// }));
///
/// assert_eq!(
///     app.on_key(Key::Tab, &Editor),
///     Reaction::Accept { text: "est".to_owned(), then: AfterAccept::Rest(" regards".to_owned()) }
/// );
/// assert!(matches!(
///     app.on_key(Key::Tab, &Editor),
///     Reaction::Accept { text, then: AfterAccept::Next(next) }
///         if text == " regards" && matches!(*next, Reaction::Request(_))
/// ));
/// ```
#[derive(Debug, Default)]
pub struct App {
    engine: Engine,
    status: Status,
    is_in_open_field: bool,
    prompt_preparation: PromptPreparation,
    suggestion_display: SuggestionDisplay,
    screen_text: ScreenTextSettings,
    /// The app of the field where the user types while suggestions are paused, which then has no
    /// session.
    paused_field_owner: Option<ScreenTextApp>,
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

    /// Uses `entries`, the earlier writing of the user, oldest first, from the next key in a new
    /// place on.
    pub fn use_history(&mut self, entries: Vec<HistoryEntry>) {
        self.engine.use_history(entries);
    }

    /// Returns the earlier writing of the user, oldest first, when it changed since the last call:
    /// a finished typing added a text, or [`forget_history_of`](Self::forget_history_of) dropped
    /// texts.
    pub fn history_to_save(&mut self) -> Option<&[HistoryEntry]> {
        self.engine.history_to_save()
    }

    /// Forgets the earlier writing of the app with `app_id`.
    pub fn forget_history_of(&mut self, app_id: &str) {
        self.engine.forget_history_of(app_id);
    }

    /// Handles [`INPUT_SETTLE_DELAY`] without keys and clicks. After Enter, it reads the field of
    /// `desktop` again. When the field shows the text of the session and a new line, the session
    /// continues there, and it returns the suggestion for the new line. Else the session ends.
    /// In a text field of an app where the screen text is on, it then starts a read of the screen,
    /// unless the session has its final screen text or waits for a read.
    /// Else, without a session, it prepares the prompt of the focused field of `desktop` with
    /// [`PromptPreparation::OnFocus`], or in a place where the earlier writing has a text.
    pub fn on_input_settled(&mut self, desktop: &dyn Desktop) -> Option<SettledWork> {
        if self.status == Status::Paused || desktop.field() == Field::Excluded {
            return None;
        }
        if let Some(reaction) = self.engine.suggest_on_new_line(desktop) {
            return Some(SettledWork::Suggest(reaction));
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
    /// [`PromptPreparation::OnFocus`], or in a place where the earlier writing has a text, it then
    /// returns the request that prepares the prompt of the focused field of `desktop` with the new
    /// screen text.
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
    /// typing there, also while suggestions are paused.
    #[must_use]
    pub fn typed_app(&self) -> Option<&ScreenTextApp> {
        self.engine.session_app().or(self.paused_field_owner.as_ref())
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
        let is_due = match self.prompt_preparation {
            PromptPreparation::OnFocus => true,
            PromptPreparation::OnFirstKey => self.engine.has_earlier_writing_in(desktop),
        };
        if is_due {
            self.engine.prepare(desktop)
        } else {
            None
        }
    }

    /// Handles a key pressed in the focused field of `desktop`, before the key reaches the field.
    pub fn on_key(&mut self, key: Key, desktop: &dyn Desktop) -> Reaction {
        let field = desktop.field();
        self.is_in_open_field = field == Field::Open;
        match (self.status, field) {
            (Status::Paused, Field::Open) => {
                self.paused_field_owner = ScreenTextApp::of(&desktop.surroundings());
                Reaction::Hide
            }
            (Status::Paused, Field::Excluded) => {
                self.paused_field_owner = None;
                Reaction::Hide
            }
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
        self.paused_field_owner = None;
        self.engine.forget_screen();
        self.engine.reset()
    }

    /// Returns what the tray icon and menu must show now.
    #[must_use]
    pub fn tray_view(&self) -> TrayView {
        TrayView {
            status: self.status,
            tooltip: self.status_name(),
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

    /// Returns the text before the caret in the session, empty without a session.
    #[must_use]
    pub fn text_before_caret(&self) -> &str {
        self.engine.text_before_caret()
    }

    /// Whether the line of the caret has no text after the caret, also without a session. `false`
    /// in a field that hides that line, because text can follow the caret there.
    #[must_use]
    pub fn is_caret_at_line_end(&self) -> bool {
        self.engine.is_caret_at_line_end()
    }

    /// Returns the badge to show at the caret, or `None` while the user types in no field that
    /// GhostType watches.
    #[must_use]
    pub fn badge_view(&self) -> Option<BadgeView> {
        self.is_in_open_field.then(|| BadgeView {
            status: self.status,
            label: self.status_name(),
        })
    }

    /// Returns the menu that a click on the badge at the caret shows, or `None` while no badge
    /// shows. The menu pauses or resumes suggestions. When the lists of `screen_text` and
    /// `personalization` have the app where the user types, it also switches the screen text of
    /// the app and selects the scope of its earlier writing.
    #[must_use]
    pub fn badge_menu(
        &self,
        screen_text: &ScreenTextSettings,
        personalization: &PersonalizationSettings,
    ) -> Option<BadgeMenu> {
        if !self.is_in_open_field {
            return None;
        }
        let pause = BadgeEntry::Item(BadgeItem {
            label: self.toggle_label().to_owned(),
            command: BadgeCommand::TogglePause,
            is_checked: false,
            is_enabled: true,
        });
        let app_entries = self
            .typed_app()
            .map(|app| app_entries(&app.app_id, screen_text, personalization))
            .unwrap_or_default();
        let separator = (!app_entries.is_empty()).then_some(BadgeEntry::Separator);
        Some(BadgeMenu {
            entries: [Some(pause), separator]
                .into_iter()
                .flatten()
                .chain(app_entries)
                .collect(),
        })
    }

    /// Runs a command picked from the tray menu or the menu of the badge.
    pub fn on_tray_command(&mut self, command: TrayCommand) -> TrayOutcome {
        match command {
            TrayCommand::TogglePause => self.toggle_pause(),
            TrayCommand::OpenSettings => TrayOutcome::OpenSettings,
            TrayCommand::Quit => TrayOutcome::Quit,
        }
    }

    fn toggle_pause(&mut self) -> TrayOutcome {
        self.engine.reset();
        self.paused_field_owner = None;
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

    fn status_name(&self) -> &'static str {
        match self.status {
            Status::Active => "GhostType",
            Status::Paused => "GhostType (paused)",
        }
    }
}

#[cfg(test)]
mod tests;
