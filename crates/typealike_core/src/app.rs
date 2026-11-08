//! The suggestion session, the pause state of TypeAlike and its stops that save energy, with the
//! tray menu and the badge at the caret that show and change them.

mod badge;
mod field_place;
mod tray;

pub use badge::{BadgeCommand, BadgeEntry, BadgeItem, BadgeMenu, BadgeSettings, BadgeView};
pub use tray::{TrayCommand, TrayGlyph, TrayItem, TrayOutcome, TrayView};

use std::collections::HashSet;
use std::mem;
use std::time::Duration;

use badge::place_entries;
use field_place::{FieldPlace, ReadField};

use crate::desktop::{
    Desktop, EnergySaver, Field, FullScreenActivity, GameActivity, OcrAvailability, ScreenRead, ScreenReadId,
    ScreenSource,
};
use crate::engine::{Engine, Reaction, ScreenAnswer};
use crate::history::HistoryEntry;
use crate::inference::{Completion, InferenceError, InferenceRequest};
use crate::key::{Key, Press};
use crate::localization::Translations;
use crate::place;
use crate::screen::{self, LineScope};
use crate::settings::{
    EnergySaverRule, FullScreenRule, GameRule, PromptPreparation, ScreenTextApp, ScreenTextSettings,
    Settings, SuggestionDisplay, SuggestionPlaces,
};
use crate::spelling::SpellChecker;

/// The time without keys and clicks after which the caret is where the user types next. A short
/// stop on the way to another field must not start the model.
pub const INPUT_SETTLE_DELAY: Duration = Duration::from_millis(300);

/// Whether TypeAlike is currently offering suggestions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    /// Suggestions are shown while typing.
    #[default]
    Active,
    /// The user paused suggestions from the tray or the badge.
    Paused,
}

impl Status {
    /// The message of the menu entry that pauses or resumes suggestions.
    fn toggle_message(self) -> &'static str {
        match self {
            Self::Active => "menu-pause",
            Self::Paused => "menu-resume",
        }
    }

    /// The message of the name of the app with this status.
    fn name_message(self) -> &'static str {
        match self {
            Self::Active => "status-active",
            Self::Paused => "status-paused",
        }
    }
}

/// The message of the name of the app in a field where the user turned suggestions off.
const OFF_HERE_NAME: &str = "status-off-here";
/// The message of the name of the app while the energy saver of the computer stops suggestions.
const ENERGY_SAVER_NAME: &str = "status-energy-saver";
/// The message of the name of the app while a game in front stops suggestions.
const GAME_NAME: &str = "status-game";
/// The message of the name of the app while an app in full screen stops suggestions.
const FULL_SCREEN_NAME: &str = "status-full-screen";

/// Why TypeAlike keeps the suggestion model unloaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StopReason {
    /// The user paused suggestions from the tray or the badge.
    Paused,
    /// The energy saver of the computer is on, and the Energy saving pane stops suggestions then.
    EnergySaver,
    /// A game is in front, and the Energy saving pane stops suggestions then. After the game, a
    /// key still gets a suggestion: the next request for a suggestion ends this stop.
    Game,
    /// An app in full screen is in front, and the Energy saving pane stops suggestions then. After
    /// the full screen, a key still gets a suggestion: the next request for a suggestion ends this
    /// stop.
    FullScreen,
    /// The user did not type or click for the idle timeout of the Energy saving pane. A key still
    /// gets a suggestion: the next request for a suggestion ends this stop.
    Idle,
}

/// What TypeAlike does when the input settles in a field, a session starts or a read of the
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
    /// Show a suggestion that no key asked for, such as the suggestion for the new line after
    /// Enter: do what the reaction asks, as after a key.
    Suggest(Reaction),
}

/// The suggestion session and the pause state of TypeAlike, and the views that show them.
///
/// ```
/// use std::time::Duration;
///
/// use typealike_core::app::App;
/// use typealike_core::desktop::{CopiedText, Desktop, Field, Surroundings};
/// use typealike_core::engine::{AfterAccept, Reaction};
/// use typealike_core::inference::{Completion, LogProbability, TokenCount};
/// use typealike_core::key::{Key, Press};
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
/// app.on_key(Key::Char('B'), Press::First, &Editor);
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
///     app.on_key(Key::Tab, Press::First, &Editor),
///     Reaction::Accept { text: "est".to_owned(), then: AfterAccept::Rest(" regards".to_owned()) }
/// );
/// assert!(matches!(
///     app.on_key(Key::Tab, Press::First, &Editor),
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
    energy_saver: EnergySaver,
    energy_saver_rule: EnergySaverRule,
    game: GameActivity,
    game_rule: GameRule,
    full_screen: FullScreenActivity,
    full_screen_rule: FullScreenRule,
    /// The stops that apply until the next request for a suggestion: the idle timeout that ran
    /// out, and a game or an app in full screen that stopped suggestions.
    stops_until_request: HashSet<StopReason>,
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
    /// earlier writing keeps none of its text. When the energy saver, a game or an app in full
    /// screen stops suggestions now, the session ends.
    pub fn use_settings(&mut self, settings: &Settings) {
        self.use_energy_saver_state(self.energy_saver, settings.energy_saving.energy_saver);
        self.use_game_state(self.game, settings.energy_saving.game);
        self.use_full_screen_state(self.full_screen, settings.energy_saving.full_screen);
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

    /// Cuts each suggestion of the model before a word that `spell_checker` does not know and that
    /// the user did not write. Where the typo rule took typed letters that `spell_checker` knows
    /// for a typo, asks again without the rule.
    pub fn use_spell_checker(&mut self, spell_checker: Box<dyn SpellChecker>) {
        self.engine.use_spell_checker(spell_checker);
    }

    /// Uses `energy_saver`, the state of the energy saver of the computer, from now on. When the
    /// energy saver stops suggestions now, the session ends.
    pub fn use_energy_saver(&mut self, energy_saver: EnergySaver) {
        self.use_energy_saver_state(energy_saver, self.energy_saver_rule);
    }

    /// Uses the state of the energy saver and the rule of the Energy saving pane. The session ends
    /// when they start to stop suggestions.
    fn use_energy_saver_state(&mut self, energy_saver: EnergySaver, rule: EnergySaverRule) {
        let was_stopped = self.stops_for_energy_saver();
        self.energy_saver = energy_saver;
        self.energy_saver_rule = rule;
        if self.stops_for_energy_saver() && !was_stopped {
            self.end_session_for_a_stop();
        }
    }

    /// Uses `game`, whether a game is in front, from now on. When a game starts to stop
    /// suggestions, the session ends. After the game, [`StopReason::Game`] applies until
    /// [`end_stops_before_request`](Self::end_stops_before_request).
    pub fn use_game(&mut self, game: GameActivity) {
        self.use_game_state(game, self.game_rule);
    }

    /// Uses whether a game is in front and the game rule of the Energy saving pane. The session
    /// ends when they start to stop suggestions. A rule that keeps suggestions ends the stop of an
    /// earlier game.
    fn use_game_state(&mut self, game: GameActivity, rule: GameRule) {
        let was_stopped = self.stops_for_game();
        self.game = game;
        self.game_rule = rule;
        if self.stops_for_game() && !was_stopped {
            self.stops_until_request.insert(StopReason::Game);
            self.end_session_for_a_stop();
        }
        if rule == GameRule::KeepSuggestions {
            self.stops_until_request.remove(&StopReason::Game);
        }
    }

    /// Uses `full_screen`, whether an app in full screen is in front, from now on. When the full
    /// screen starts to stop suggestions, the session ends. After the full screen,
    /// [`StopReason::FullScreen`] applies until
    /// [`end_stops_before_request`](Self::end_stops_before_request).
    pub fn use_full_screen(&mut self, full_screen: FullScreenActivity) {
        self.use_full_screen_state(full_screen, self.full_screen_rule);
    }

    /// Uses whether an app in full screen is in front and the full-screen rule of the Energy
    /// saving pane. The session ends when they start to stop suggestions. A rule that keeps
    /// suggestions ends the stop of an earlier full screen.
    fn use_full_screen_state(&mut self, full_screen: FullScreenActivity, rule: FullScreenRule) {
        let was_stopped = self.stops_for_full_screen();
        self.full_screen = full_screen;
        self.full_screen_rule = rule;
        if self.stops_for_full_screen() && !was_stopped {
            self.stops_until_request.insert(StopReason::FullScreen);
            self.end_session_for_a_stop();
        }
        if rule == FullScreenRule::KeepSuggestions {
            self.stops_until_request.remove(&StopReason::FullScreen);
        }
    }

    fn end_session_for_a_stop(&mut self) {
        self.engine.reset();
        self.idle_field_place = None;
    }

    /// The state of the energy saver of the computer.
    #[must_use]
    pub fn energy_saver(&self) -> EnergySaver {
        self.energy_saver
    }

    fn stops_for_energy_saver(&self) -> bool {
        self.energy_saver == EnergySaver::On && self.energy_saver_rule == EnergySaverRule::StopSuggestions
    }

    /// Whether a game is in front of the other apps.
    #[must_use]
    pub fn game(&self) -> GameActivity {
        self.game
    }

    fn stops_for_game(&self) -> bool {
        self.game == GameActivity::InFront && self.game_rule == GameRule::StopSuggestions
    }

    /// Whether an app in full screen is in front of the other apps.
    #[must_use]
    pub fn full_screen(&self) -> FullScreenActivity {
        self.full_screen
    }

    fn stops_for_full_screen(&self) -> bool {
        self.full_screen == FullScreenActivity::InFront
            && self.full_screen_rule == FullScreenRule::StopSuggestions
    }

    /// Whether a key in a place where suggestions are on can get a suggestion: the user did not
    /// pause suggestions, and neither the energy saver, a game nor an app in full screen stops
    /// them.
    fn offers_suggestions(&self) -> bool {
        self.status == Status::Active
            && !self.stops_for_energy_saver()
            && !self.stops_for_game()
            && !self.stops_for_full_screen()
    }

    /// Why TypeAlike keeps the suggestion model unloaded now, or `None` when it keeps the model
    /// loaded. A pause comes first, then the energy saver, a game, an app in full screen, and the
    /// idle timeout last.
    #[must_use]
    pub fn stop_reason(&self) -> Option<StopReason> {
        [
            (self.status == Status::Paused, StopReason::Paused),
            (self.stops_for_energy_saver(), StopReason::EnergySaver),
            (
                self.stops_for_game() || self.lasts_until_request(StopReason::Game),
                StopReason::Game,
            ),
            (
                self.stops_for_full_screen() || self.lasts_until_request(StopReason::FullScreen),
                StopReason::FullScreen,
            ),
            (self.lasts_until_request(StopReason::Idle), StopReason::Idle),
        ]
        .into_iter()
        .find_map(|(applies, reason)| applies.then_some(reason))
    }

    fn lasts_until_request(&self, reason: StopReason) -> bool {
        self.stops_until_request.contains(&reason)
    }

    /// Handles the end of the idle timeout of the Energy saving pane: the session ends as after a
    /// focus change, and [`StopReason::Idle`] applies until [`end_idle`](Self::end_idle) or
    /// [`end_stops_before_request`](Self::end_stops_before_request).
    pub fn on_idle(&mut self) -> Reaction {
        self.stops_until_request.insert(StopReason::Idle);
        self.on_context_lost()
    }

    /// Ends the stop of the idle timeout. Returns whether the app was idle.
    pub fn end_idle(&mut self) -> bool {
        self.stops_until_request.remove(&StopReason::Idle)
    }

    /// Ends the stops that last until the next request for a suggestion: the idle timeout, and a
    /// game or an app in full screen that left the front. Returns whether one of them applied.
    pub fn end_stops_before_request(&mut self) -> bool {
        !mem::take(&mut self.stops_until_request).is_empty()
    }

    /// The glyph that the tray icon shows, whose robot the badge also shows, and the message of
    /// the name of the app with that glyph.
    fn shown_glyph(&self) -> (TrayGlyph, &'static str) {
        let stop_name = [
            (self.stops_for_energy_saver(), ENERGY_SAVER_NAME),
            (self.stops_for_game(), GAME_NAME),
            (self.stops_for_full_screen(), FULL_SCREEN_NAME),
        ]
        .into_iter()
        .find_map(|(applies, name)| applies.then_some(name));
        match stop_name {
            Some(name) if self.status == Status::Active => (TrayGlyph::Stopped, name),
            _ => (TrayGlyph::from(self.status), self.status.name_message()),
        }
    }

    /// The place of the session, if a session runs. A session never starts in a private window.
    fn session_place(&self) -> Option<FieldPlace> {
        self.engine.has_session().then(|| FieldPlace {
            app: self.engine.session_app().cloned(),
            host: self.engine.session_domain().map(str::to_owned),
            is_private_window: false,
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
    /// Without Enter, the session takes the words that the app changed in the field, as an
    /// autocorrect does. A session that a shortcut ended keeps only the text that the field still
    /// holds. After a key that the OS repeated while the user held it, it then returns
    /// the suggestion for the text of the session.
    /// In a text field that gets suggestions, of an app where the screen text is on, it then starts
    /// a read of the screen, unless the session has its final screen text or waits for a read.
    /// Else, without a session, it prepares the prompt of the focused field of `desktop` with
    /// [`PromptPreparation::OnFocus`], or in a place where the earlier writing has a text. In a
    /// place where suggestions are off, it does nothing.
    pub fn on_input_settled(&mut self, desktop: &dyn Desktop) -> Option<SettledWork> {
        let is_excluded = desktop.field() == Field::Excluded;
        if !self.offers_suggestions() || is_excluded || !self.allows_suggestions_in(desktop) {
            return None;
        }
        self.engine.follow_key_ended_session(desktop);
        if let Some(reaction) = self.engine.suggest_on_new_line(desktop) {
            return Some(SettledWork::Suggest(reaction));
        }
        self.engine.follow_field(desktop);
        let has_new_examples = self.is_preparation_due(desktop) && self.engine.pick_examples_again(desktop);
        if let Some(reaction) = self.engine.suggest_after_hold() {
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
        if self.engine.has_session() && !has_new_examples {
            return None;
        }
        self.prepare_on_focus(desktop).map(SettledWork::Prepare)
    }

    /// Handles the answer to a read of the screen. When the lines of the accessibility tree that
    /// the prompt can keep hold few words and `desktop` can do an OCR, it returns the OCR read. Else the screen text goes into the
    /// session when the session has no final screen text, and into the next session. When the new
    /// text changes the prompt of the suggestion that runs or shows, and the user pressed no key
    /// after its request, it returns the request for that suggestion again. Else, with
    /// [`PromptPreparation::OnFocus`], or in a place where the earlier writing has a text, it then
    /// returns the request that prepares the prompt of the focused field of `desktop` with the new
    /// screen text. In a place where suggestions are off, it only keeps the screen text.
    pub fn on_screen_read(&mut self, read: ScreenRead, desktop: &dyn Desktop) -> Option<SettledWork> {
        let is_active = self.offers_suggestions()
            && desktop.field() == Field::Open
            && self.allows_suggestions_in(desktop);
        let is_ocr_due = is_active
            && desktop.ocr_availability() == OcrAvailability::Ready
            && !read
                .text
                .as_ref()
                .is_some_and(|text| screen::has_enough_words(text, &desktop.surroundings()));
        let level = self.screen_text.ocr_level;
        if is_ocr_due && let Some(ocr) = self.engine.start_ocr_after(read.id, read.text.clone(), level) {
            return Some(SettledWork::ReadScreen {
                read: ocr,
                source: ScreenSource::Ocr(level),
            });
        }
        match self.engine.on_screen_read(read, desktop) {
            ScreenAnswer::Dropped => None,
            _ if !is_active => None,
            ScreenAnswer::ChangedSuggestionPrompt => Some(SettledWork::Suggest(self.engine.request_again())),
            ScreenAnswer::Kept => self.prepare_on_focus(desktop).map(SettledWork::Prepare),
        }
    }

    /// Handles a scroll, which moves the text on the screen: the session reads the screen again at
    /// its next key.
    pub fn on_scroll(&mut self) {
        self.engine.read_screen_again();
    }

    /// Handles the stop of a scroll. In a text field that gets suggestions, of a chat or an email
    /// app where the screen text is on, it returns the read of the accessibility tree, so that the
    /// screen text also gets the lines that the scroll showed. Give the answer to
    /// [`App::on_screen_read`].
    pub fn on_scroll_settled(&mut self, desktop: &dyn Desktop) -> Option<SettledWork> {
        let field = ReadField {
            desktop,
            surroundings: desktop.surroundings(),
        };
        let is_active =
            self.offers_suggestions() && desktop.field() == Field::Open && self.allows_suggestions_in(&field);
        let is_conversation = LineScope::of(&field.surroundings) == LineScope::Column;
        if !is_active || !is_conversation || !self.reads_screen_in(&field) {
            return None;
        }
        self.engine
            .start_screen_read()
            .map(|read| SettledWork::ReadScreen {
                read,
                source: ScreenSource::Accessibility,
            })
    }

    /// Starts a read of the screen for the session of the last key, when the session is in a text
    /// field that gets suggestions, of an app where the screen text is on. The session uses the
    /// stored screen text until
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
        surroundings.holds_text()
            && !place::is_excluded_field(&surroundings)
            && surroundings
                .app_id
                .is_some_and(|app_id| self.screen_text.allows(&app_id))
    }

    fn prepare_on_focus(&mut self, desktop: &dyn Desktop) -> Option<InferenceRequest> {
        if self.is_preparation_due(desktop) {
            self.engine.prepare(desktop)
        } else {
            None
        }
    }

    /// Whether TypeAlike prepares the prompt of the focused field of `desktop` before a key: with
    /// [`PromptPreparation::OnFocus`], or in a place where the earlier writing has a text.
    fn is_preparation_due(&self, desktop: &dyn Desktop) -> bool {
        match self.prompt_preparation {
            PromptPreparation::OnFocus => true,
            PromptPreparation::OnFirstKey => self.engine.has_earlier_writing_in(desktop),
        }
    }

    /// Handles a key pressed in the focused field of `desktop`, before the key reaches the field.
    /// While suggestions are paused, the energy saver, a game or an app in full screen stops them,
    /// or they are off in the app or on the website of the field, the key gets no suggestion and starts no session.
    /// A key that the OS repeats while the user holds it gets no suggestion until the input
    /// settles.
    pub fn on_key(&mut self, key: Key, press: Press, desktop: &dyn Desktop) -> Reaction {
        let field = desktop.field();
        self.is_in_open_field = field == Field::Open;
        if field == Field::Excluded {
            self.idle_field_place = None;
            return self.engine.reset();
        }
        if self.offers_suggestions() && !self.engine.starts_session_with(key) {
            return self.engine.on_key(key, press, desktop);
        }
        let read_field = ReadField {
            desktop,
            surroundings: desktop.surroundings(),
        };
        let place = FieldPlace::of(&read_field.surroundings);
        if self.offers_suggestions() && place.is_on_in(&self.suggestion_places) {
            self.idle_field_place = None;
            return self.engine.on_key(key, press, &read_field);
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
    /// of the last field is not used again, and the pause of the suggestions in the field ends.
    pub fn on_context_lost(&mut self) -> Reaction {
        self.is_in_open_field = false;
        self.idle_field_place = None;
        self.engine.forget_screen();
        self.engine.end_field_pause();
        self.engine.reset()
    }

    /// Ends the pause of the suggestions in the field that [`Reaction::PauseField`] started, so
    /// that the next key there gets a suggestion again.
    pub fn end_field_pause(&mut self) {
        self.engine.end_field_pause();
    }

    /// Returns what the tray icon and menu must show now, with the texts of `translations`. While
    /// the energy saver, a game or an app in full screen stops suggestions, the icon shows as
    /// stopped.
    #[must_use]
    pub fn tray_view(&self, translations: &Translations) -> TrayView {
        let (glyph, name) = self.shown_glyph();
        TrayView {
            glyph,
            tooltip: translations.text(name),
            ..TrayView::of(self.status, translations)
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

    /// Whether the ghost text has a suggestion or a fix, or the completion of the newest
    /// [`Reaction::Request`] still runs and can give one.
    #[must_use]
    pub fn has_offer(&self) -> bool {
        self.engine.has_offer()
    }

    /// Whether the badge shows at the caret: while the user types in a field that TypeAlike
    /// watches.
    #[must_use]
    pub fn has_badge(&self) -> bool {
        self.is_in_open_field
    }

    /// Returns the badge to show at the caret, with the texts of `translations`, or `None` while
    /// the user types in no field that TypeAlike watches. In a place where suggestions are off, and
    /// while the energy saver, a game or an app in full screen stops suggestions, the badge shows
    /// as paused.
    #[must_use]
    pub fn badge_view(&self, translations: &Translations) -> Option<BadgeView> {
        self.is_in_open_field.then(|| {
            let (status, name) = if self.offers_suggestions() && self.is_in_off_place() {
                (Status::Paused, OFF_HERE_NAME)
            } else {
                let (glyph, name) = self.shown_glyph();
                (glyph.face(), name)
            };
            BadgeView {
                status,
                label: translations.text(name),
            }
        })
    }

    /// Returns the menu that a click on the badge at the caret shows, or `None` while no badge
    /// shows. The menu pauses or resumes suggestions. In a private window where suggestions stop,
    /// it also turns off the switch that stops them. When the lists of `suggestion_places` have
    /// the app or the website where the user types, it also turns their suggestions on or off.
    /// When the lists of `screen_text` and `personalization` have the app, it also switches the
    /// screen text of the app and selects the scope of its earlier writing. The entries have the
    /// texts of `translations`.
    #[must_use]
    pub fn badge_menu(&self, settings: &BadgeSettings<'_>, translations: &Translations) -> Option<BadgeMenu> {
        if !self.is_in_open_field {
            return None;
        }
        let pause = BadgeEntry::Item(BadgeItem {
            label: translations.text(self.status.toggle_message()),
            command: BadgeCommand::TogglePause,
            is_checked: false,
            is_enabled: true,
        });
        let place_entries = self
            .session_place()
            .or_else(|| self.idle_field_place.clone())
            .map(|place| place_entries(&place, settings, translations))
            .unwrap_or_default();
        let separator = (!place_entries.is_empty()).then_some(BadgeEntry::Separator);
        Some(BadgeMenu {
            entries: [Some(pause), separator]
                .into_iter()
                .flatten()
                .chain(place_entries)
                .collect(),
            language: translations.language(),
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
            Status::Paused => {
                self.stops_until_request.clear();
                (Status::Active, TrayOutcome::Resumed)
            }
        };
        self.status = status;
        outcome
    }
}

#[cfg(test)]
mod tests;
