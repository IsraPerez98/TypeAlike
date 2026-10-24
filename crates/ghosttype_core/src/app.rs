//! The suggestion session and the pause state of GhostType, with the tray menu and the badge at
//! the caret that show and change them.

mod badge;
mod field_place;
mod tray;

pub use badge::{BadgeCommand, BadgeEntry, BadgeItem, BadgeMenu, BadgeView};
pub use tray::{TrayCommand, TrayItem, TrayOutcome, TrayView};

use std::time::Duration;

use badge::{TypedPlace, place_entries};
use field_place::{FieldPlace, ReadField};

use crate::desktop::{Desktop, Field, OcrAvailability, ScreenRead, ScreenReadId, ScreenSource};
use crate::engine::{Engine, Reaction};
use crate::history::HistoryEntry;
use crate::inference::{Completion, InferenceError, InferenceRequest};
use crate::key::Key;
use crate::settings::{
    PersonalizationSettings, PromptPreparation, ScreenTextApp, ScreenTextSettings, Settings,
    SuggestionDisplay, SuggestionPlaces,
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

impl Status {
    /// The label of the menu entry that pauses or resumes suggestions.
    fn toggle_label(self) -> &'static str {
        match self {
            Self::Active => "Pause suggestions",
            Self::Paused => "Resume suggestions",
        }
    }

    /// The name of the app with this status.
    fn name(self) -> &'static str {
        match self {
            Self::Active => "GhostType",
            Self::Paused => "GhostType (paused)",
        }
    }
}

/// The name of the app in a field where the user turned suggestions off.
const OFF_HERE_NAME: &str = "GhostType (off here)";

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

/// The suggestion session and the pause state of GhostType, and the views that show them.
///
/// ```
/// use std::time::Duration;
///
/// use ghosttype_core::app::App;
/// use ghosttype_core::desktop::{CopiedText, Desktop, Field, Surroundings};
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
    suggestion_places: SuggestionPlaces,
    /// The place of the field where the user types while suggestions are paused or off there,
    /// which then has no session.
    idle_field_place: Option<FieldPlace>,
}

impl App {
    /// Creates an active app with no typed context. Until [`use_settings`](Self::use_settings),
    /// the model reads no writer notes and no clipboard text.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Uses the writer notes and the clipboard access of `settings` from the next key in a new
    /// place on, and its prompt preparation, suggestion display, suggestion length, screen text
    /// and suggestion places from now on. A session in a place that is off now ends, and the
    /// earlier writing keeps none of its text.
    pub fn use_settings(&mut self, settings: &Settings) {
        self.engine.use_settings(settings);
        self.prompt_preparation = settings.prompt_preparation;
        self.suggestion_display = settings.suggestion_display;
        if self.screen_text != settings.screen_text {
            self.engine.forget_screen();
            self.screen_text = settings.screen_text.clone();
        }
        if self.suggestion_places != settings.suggestion_places {
            self.suggestion_places = settings.suggestion_places.clone();
            let off_session_place = self
                .session_place()
                .filter(|place| !place.is_on_in(&self.suggestion_places));
            if let Some(place) = off_session_place {
                self.engine.discard_session();
                self.idle_field_place = Some(place);
            }
        }
    }

    /// The place of the session, if a session runs.
    fn session_place(&self) -> Option<FieldPlace> {
        self.engine.has_session().then(|| FieldPlace {
            app: self.engine.session_app().cloned(),
            host: self.engine.session_domain().map(str::to_owned),
        })
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
    /// [`PromptPreparation::OnFocus`], or in a place where the earlier writing has a text. In a
    /// place where suggestions are off, it does nothing.
    pub fn on_input_settled(&mut self, desktop: &dyn Desktop) -> Option<SettledWork> {
        let is_excluded = desktop.field() == Field::Excluded;
        if self.status == Status::Paused || is_excluded || !self.allows_suggestions_in(desktop) {
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
    /// screen text. In a place where suggestions are off, it only keeps the screen text.
    pub fn on_screen_read(&mut self, read: ScreenRead, desktop: &dyn Desktop) -> Option<SettledWork> {
        let is_active = self.status == Status::Active
            && desktop.field() == Field::Open
            && self.allows_suggestions_in(desktop);
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
    /// typing there, also while suggestions are paused or off there.
    #[must_use]
    pub fn typed_app(&self) -> Option<&ScreenTextApp> {
        self.engine
            .session_app()
            .or_else(|| self.idle_field_place.as_ref()?.app.as_ref())
    }

    /// The host of the website where the user types now, if any. The first key in a field starts
    /// the typing there, also while suggestions are paused or off there.
    #[must_use]
    pub fn typed_host(&self) -> Option<&str> {
        self.engine
            .session_domain()
            .or_else(|| self.idle_field_place.as_ref()?.host.as_deref())
    }

    /// Whether suggestions show in the focused field of `desktop`. A session runs only in a place
    /// where they show.
    fn allows_suggestions_in(&self, desktop: &dyn Desktop) -> bool {
        self.engine.has_session() || FieldPlace::of(&desktop.surroundings()).is_on_in(&self.suggestion_places)
    }

    /// Whether the user types in a place where suggestions are off.
    fn is_in_off_place(&self) -> bool {
        self.idle_field_place
            .as_ref()
            .is_some_and(|place| !place.is_on_in(&self.suggestion_places))
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
    /// While suggestions are paused, or off in the app or on the website of the field, the key
    /// gets no suggestion and starts no session.
    pub fn on_key(&mut self, key: Key, desktop: &dyn Desktop) -> Reaction {
        let field = desktop.field();
        self.is_in_open_field = field == Field::Open;
        if field == Field::Excluded {
            self.idle_field_place = None;
            return self.engine.reset();
        }
        if self.status == Status::Active && !self.engine.starts_session_with(key) {
            return self.engine.on_key(key, desktop);
        }
        let read_field = ReadField {
            desktop,
            surroundings: desktop.surroundings(),
        };
        let place = FieldPlace::of(&read_field.surroundings);
        if self.status == Status::Active && place.is_on_in(&self.suggestion_places) {
            self.idle_field_place = None;
            return self.engine.on_key(key, &read_field);
        }
        self.idle_field_place = Some(place);
        Reaction::Hide
    }

    /// Handles the completion of the last [`Reaction::Request`].
    pub fn on_completion(&mut self, result: Result<Completion, InferenceError>) -> Reaction {
        self.engine.on_completion(result)
    }

    /// Handles `text`, the text that the completion of the last [`Reaction::Request`] added so far.
    /// Returns the reaction when the ghost text changes, or `None`. Only
    /// [`SuggestionDisplay::WordByWord`] changes the ghost text before the completion finishes.
    /// In each display, a typed key that continues `text` keeps the request.
    pub fn on_partial_completion(&mut self, text: &str) -> Option<Reaction> {
        match self.suggestion_display {
            SuggestionDisplay::WordByWord => self.engine.on_partial_completion(text),
            SuggestionDisplay::WhenComplete => {
                self.engine.record_partial_completion(text);
                None
            }
        }
    }

    /// Handles the caret moving somewhere unknown: focus change, mouse click. The screen text
    /// of the last field is not used again.
    pub fn on_context_lost(&mut self) -> Reaction {
        self.is_in_open_field = false;
        self.idle_field_place = None;
        self.engine.forget_screen();
        self.engine.reset()
    }

    /// Returns what the tray icon and menu must show now.
    #[must_use]
    pub fn tray_view(&self) -> TrayView {
        TrayView::of(self.status)
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

    /// Whether the ghost text has a suggestion or a fix, or the completion of the newest
    /// [`Reaction::Request`] still runs and can give one.
    #[must_use]
    pub fn has_offer(&self) -> bool {
        self.engine.has_offer()
    }

    /// Returns the badge to show at the caret, or `None` while the user types in no field that
    /// GhostType watches. In a place where suggestions are off, the badge shows as paused.
    #[must_use]
    pub fn badge_view(&self) -> Option<BadgeView> {
        self.is_in_open_field.then(|| {
            if self.status == Status::Active && self.is_in_off_place() {
                BadgeView {
                    status: Status::Paused,
                    label: OFF_HERE_NAME,
                }
            } else {
                BadgeView {
                    status: self.status,
                    label: self.status.name(),
                }
            }
        })
    }

    /// Returns the menu that a click on the badge at the caret shows, or `None` while no badge
    /// shows. The menu pauses or resumes suggestions. When the lists of `suggestion_places` have
    /// the app or the website where the user types, it also turns their suggestions on or off.
    /// When the lists of `screen_text` and `personalization` have the app, it also switches the
    /// screen text of the app and selects the scope of its earlier writing.
    #[must_use]
    pub fn badge_menu(
        &self,
        suggestion_places: &SuggestionPlaces,
        screen_text: &ScreenTextSettings,
        personalization: &PersonalizationSettings,
    ) -> Option<BadgeMenu> {
        if !self.is_in_open_field {
            return None;
        }
        let pause = BadgeEntry::Item(BadgeItem {
            label: self.status.toggle_label().to_owned(),
            command: BadgeCommand::TogglePause,
            is_checked: false,
            is_enabled: true,
        });
        let place = TypedPlace {
            app: self.typed_app(),
            host: self.typed_host(),
        };
        let place_entries = place_entries(&place, suggestion_places, screen_text, personalization);
        let separator = (!place_entries.is_empty()).then_some(BadgeEntry::Separator);
        Some(BadgeMenu {
            entries: [Some(pause), separator]
                .into_iter()
                .flatten()
                .chain(place_entries)
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
        self.idle_field_place = None;
        let (status, outcome) = match self.status {
            Status::Active => (Status::Paused, TrayOutcome::Paused),
            Status::Paused => (Status::Active, TrayOutcome::Resumed),
        };
        self.status = status;
        outcome
    }
}

#[cfg(test)]
mod tests;
