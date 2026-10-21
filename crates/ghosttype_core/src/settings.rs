//! The settings that GhostType keeps between runs, and the seams to the services that keep them.

use std::fmt;

use crate::desktop::Surroundings;
use crate::hub::ModelFile;
use crate::inference::{Backend, LogProbability, TokenCount};

/// What the user writes about themself and how they write, which suggestions use. `Debug` shows
/// only its length, so logs never hold what the user wrote. The default notes are true for every
/// user, so that they show how to write notes in the first person.
///
/// ```
/// use ghosttype_core::settings::WriterNotes;
///
/// assert_eq!(format!("{:?}", WriterNotes("I am a nurse".to_owned())), "WriterNotes(12 chars)");
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct WriterNotes(pub String);

impl Default for WriterNotes {
    fn default() -> Self {
        Self("I write clear and friendly messages. I keep the tone of the conversation.".to_owned())
    }
}

impl fmt::Debug for WriterNotes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "WriterNotes({} chars)", self.0.chars().count())
    }
}

/// The name of the user, which suggestions use. `Debug` shows only its length, so logs never hold
/// it.
///
/// ```
/// use ghosttype_core::settings::WriterName;
///
/// assert_eq!(format!("{:?}", WriterName("Sara Novak".to_owned())), "WriterName(10 chars)");
/// ```
#[derive(Clone, PartialEq, Eq, Default)]
pub struct WriterName(pub String);

impl fmt::Debug for WriterName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "WriterName({} chars)", self.0.chars().count())
    }
}

/// Whether suggestions may use the text on the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClipboardAccess {
    /// GhostType never reads the clipboard.
    #[default]
    Denied,
    /// GhostType reads the clipboard when the user starts to type, and the model reads its text.
    Allowed,
}

/// Whether suggestions may use the text near the text field on the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenTextAccess {
    /// GhostType reads the screen text in each app that is on in the list of the Screen text pane.
    #[default]
    Allowed,
    /// GhostType reads no screen text.
    Denied,
}

/// An app of the Screen text pane, and whether GhostType reads the screen text in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenTextApp {
    /// The id of the app, as [`Surroundings::app_id`] gives it.
    pub app_id: String,
    /// The name of the app, as the user knows it.
    pub app_name: String,
    /// The full path of the program of the app, as [`Surroundings::app_path`] gives it.
    pub app_path: Option<String>,
    /// Whether GhostType reads the screen text in the app.
    pub is_on: bool,
}

impl ScreenTextApp {
    /// The entry of the app of `surroundings`, which is on unless the app is a password manager.
    /// `None` when the app has no id.
    pub(crate) fn of(surroundings: &Surroundings) -> Option<Self> {
        let app_id = surroundings.app_id.clone()?;
        Some(Self {
            app_name: surroundings.app_name.clone().unwrap_or_else(|| app_id.clone()),
            app_path: surroundings.app_path.clone(),
            is_on: !is_password_manager(&app_id),
            app_id,
        })
    }
}

/// How the OCR recognizes the text in a capture of a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OcrLevel {
    /// Fast, for clean text in a few languages.
    #[default]
    Fast,
    /// Slower, with fewer errors and more languages.
    Accurate,
}

/// Where GhostType reads the text near the text field on the screen.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScreenTextSettings {
    /// The main switch: whether GhostType reads screen text in any app.
    pub access: ScreenTextAccess,
    /// The apps where the user typed, in alphabetical order of their names.
    pub apps: Vec<ScreenTextApp>,
    /// How the OCR recognizes the text of a window that gives no text.
    pub ocr_level: OcrLevel,
}

impl ScreenTextSettings {
    /// Whether GhostType reads the screen text in the app with `app_id`. An app that is not in the
    /// list yet is on, unless it is a password manager.
    ///
    /// ```
    /// use ghosttype_core::settings::ScreenTextSettings;
    ///
    /// let settings = ScreenTextSettings::default();
    ///
    /// assert!(settings.allows("com.apple.mail"));
    /// assert!(!settings.allows("com.1password.1password"));
    /// ```
    #[must_use]
    pub fn allows(&self, app_id: &str) -> bool {
        let entry = self.apps.iter().find(|app| app.app_id == app_id);
        self.access == ScreenTextAccess::Allowed
            && entry.map_or_else(|| !is_password_manager(app_id), |app| app.is_on)
    }

    /// Adds `app` to the list in its alphabetical place, unless the list has an app with its id.
    /// Returns whether the list changed.
    pub(crate) fn add(&mut self, app: &ScreenTextApp) -> bool {
        if self.apps.iter().any(|known| known.app_id == app.app_id) {
            return false;
        }
        let name = app.app_name.to_lowercase();
        let place = self
            .apps
            .partition_point(|known| known.app_name.to_lowercase() <= name);
        self.apps.insert(place, app.clone());
        true
    }
}

/// Whether the app with `app_id` is a password manager, which starts off in the list of the
/// Screen text pane. Its id matches by its start in lowercase.
fn is_password_manager(app_id: &str) -> bool {
    let app_id = app_id.to_lowercase();
    PASSWORD_MANAGERS.iter().any(|start| app_id.starts_with(start))
}

/// The password managers that GhostType knows, by the start of their app id in lowercase: the
/// bundle identifier on macOS and the program file name on Windows.
const PASSWORD_MANAGERS: [&str; 24] = [
    "com.1password.",
    "com.agilebits.onepassword",
    "com.bitwarden.desktop",
    "org.keepassxc.keepassxc",
    "com.lastpass.",
    "com.dashlane.",
    "com.apple.keychainaccess",
    "com.apple.passwords",
    "in.sinew.enpass",
    "com.nordpass.",
    "me.proton.pass",
    "com.callpod.keepermac",
    "com.siber.roboform",
    "1password.exe",
    "bitwarden.exe",
    "keepassxc.exe",
    "keepass.exe",
    "enpass.exe",
    "nordpass.exe",
    "protonpass.exe",
    "dashlane.exe",
    "lastpass.exe",
    "keeperpasswordmanager.exe",
    "roboform",
];

/// Whether GhostType saves the text that the user writes, for the suggestions to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HistoryAccess {
    /// GhostType saves no new text, and suggestions use no earlier writing.
    #[default]
    Off,
    /// GhostType saves the text that the user writes in each app that is on in the list of the
    /// Personalization pane, and suggestions use it.
    On,
}

/// Which texts GhostType saves while the history is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeptTexts {
    /// Only a text where the user accepted a suggestion.
    #[default]
    AcceptedOnly,
    /// Each text.
    All,
}

/// How much suggestions prefer the words that the user often writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordChoice {
    /// Suggestions do not prefer the words of the user.
    Off,
    /// A small preference.
    Light,
    /// A medium preference.
    #[default]
    Medium,
    /// A large preference, which can make a suggestion match the text less well.
    Strong,
}

/// Which earlier writing an app gives to suggestions and gets from them. An app gives its text
/// only to the apps that it gets text from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryScope {
    /// GhostType saves no text of the app, and suggestions in the app use no earlier writing.
    Off,
    /// GhostType saves the text of the app, and suggestions in the app use only the text of the
    /// app.
    ThisApp,
    /// GhostType saves the text of the app, and suggestions in each app with this scope use the
    /// text of all these apps, the text of the same app or web page first.
    AllApps,
}

/// An app of the Personalization pane, and which earlier writing it gives and gets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryApp {
    /// The id of the app, as [`Surroundings::app_id`] gives it.
    pub app_id: String,
    /// The name of the app, as the user knows it.
    pub app_name: String,
    /// The full path of the program of the app, as [`Surroundings::app_path`] gives it.
    pub app_path: Option<String>,
    /// Which earlier writing the app gives and gets.
    pub scope: HistoryScope,
}

impl HistoryApp {
    /// The entry of `app`, where the user types, which shares its text with all apps unless the
    /// app is a password manager or a terminal.
    pub(crate) fn of(app: &ScreenTextApp) -> Self {
        Self {
            app_id: app.app_id.clone(),
            app_name: app.app_name.clone(),
            app_path: app.app_path.clone(),
            scope: default_scope(&app.app_id),
        }
    }
}

/// What suggestions learn from the earlier writing of the user.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersonalizationSettings {
    /// The main switch: whether GhostType saves and uses the text of any app.
    pub history: HistoryAccess,
    /// Which texts GhostType saves.
    pub kept_texts: KeptTexts,
    /// The apps where the user typed, in alphabetical order of their names.
    pub apps: Vec<HistoryApp>,
    /// How much suggestions prefer the words that the user often writes.
    pub word_choice: WordChoice,
}

impl PersonalizationSettings {
    /// Which earlier writing the app with `app_id` gives and gets. Each app is off while the main
    /// switch is off. An app that is not in the list yet shares with all apps, unless it is a
    /// password manager or a terminal.
    ///
    /// ```
    /// use ghosttype_core::settings::{HistoryAccess, HistoryScope, PersonalizationSettings};
    ///
    /// let settings = PersonalizationSettings {
    ///     history: HistoryAccess::On,
    ///     ..PersonalizationSettings::default()
    /// };
    ///
    /// assert_eq!(settings.scope_of("com.apple.mail"), HistoryScope::AllApps);
    /// assert_eq!(settings.scope_of("com.apple.Terminal"), HistoryScope::Off);
    /// ```
    #[must_use]
    pub fn scope_of(&self, app_id: &str) -> HistoryScope {
        if self.history == HistoryAccess::Off {
            return HistoryScope::Off;
        }
        self.apps
            .iter()
            .find(|app| app.app_id == app_id)
            .map_or_else(|| default_scope(app_id), |app| app.scope)
    }

    /// Adds `app` to the list in its alphabetical place, unless the list has an app with its id.
    /// Returns whether the list changed.
    pub(crate) fn add(&mut self, app: &HistoryApp) -> bool {
        if self.apps.iter().any(|known| known.app_id == app.app_id) {
            return false;
        }
        let name = app.app_name.to_lowercase();
        let place = self
            .apps
            .partition_point(|known| known.app_name.to_lowercase() <= name);
        self.apps.insert(place, app.clone());
        true
    }
}

fn default_scope(app_id: &str) -> HistoryScope {
    if is_password_manager(app_id) || is_terminal(app_id) {
        HistoryScope::Off
    } else {
        HistoryScope::AllApps
    }
}

/// Whether the app with `app_id` is a terminal, whose text GhostType does not save unless the
/// user turns it on. Its id matches by its start in lowercase.
fn is_terminal(app_id: &str) -> bool {
    let app_id = app_id.to_lowercase();
    TERMINALS.iter().any(|start| app_id.starts_with(start))
}

/// The terminals that GhostType knows, by the start of their app id in lowercase: the bundle
/// identifier on macOS and the program file name on Windows.
const TERMINALS: [&str; 25] = [
    "com.apple.terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "dev.warp.",
    "net.kovidgoyal.kitty",
    "io.alacritty",
    "org.alacritty",
    "com.github.wez.wezterm",
    "co.zeit.hyper",
    "com.raphaelamorim.rio",
    "org.tabby",
    "windowsterminal.exe",
    "openconsole.exe",
    "conhost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "warp.exe",
    "mintty.exe",
    "tabby.exe",
    "hyper.exe",
    "putty.exe",
    "rio.exe",
];

/// When GhostType gives the text around the caret to the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PromptPreparation {
    /// At the first key that the user types in a field. In a place where the earlier writing has
    /// a text, the user often types, so GhostType prepares there as with [`Self::OnFocus`].
    #[default]
    OnFirstKey,
    /// When the keys and clicks stop in a field, before the user types. The first suggestion then
    /// shows sooner, but the model also works when the user does not type.
    OnFocus,
}

/// When the ghost text shows the words that the model writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SuggestionDisplay {
    /// After the model writes the full suggestion.
    #[default]
    WhenComplete,
    /// As soon as the model writes each word. The first word then shows sooner, but the ghost text
    /// grows while the user reads it.
    WordByWord,
}

/// What suggestions do while the word before the caret looks misspelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TypoRule {
    /// No suggestion finishes the word, so that Tab never keeps a typo. A rare name or term can also
    /// look misspelled.
    #[default]
    HideSuggestions,
    /// Suggestions show as for any other word.
    Suggest,
}

/// Whether a suggestion fixes the word before the caret when it looks misspelled. It works only
/// while [`TypoRule::HideSuggestions`] finds typos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TypoFix {
    /// The suggestion shows the typo struck through and the fixed word after it. Tab replaces the
    /// word.
    #[default]
    Offer,
    /// No suggestion shows for a word with a typo.
    Off,
}

/// The most text that one suggestion shows, from the shortest to the longest. A shorter
/// suggestion shows sooner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum SuggestionLength {
    /// About 2 words.
    VeryShort,
    /// About 4 words.
    Short,
    /// About 7 words.
    #[default]
    Medium,
    /// About 10 words.
    Long,
    /// About 15 words.
    VeryLong,
}

impl SuggestionLength {
    /// The most tokens that the model adds for one suggestion: the median tokens of the words in
    /// Spanish, which need more tokens than English, and one token for the healed word.
    pub(crate) fn max_tokens(self) -> TokenCount {
        match self {
            Self::VeryShort => TokenCount(3),
            Self::Short => TokenCount(6),
            Self::Medium => TokenCount(10),
            Self::Long => TokenCount(15),
            Self::VeryLong => TokenCount(22),
        }
    }

    /// The least likely that a suggestion can get after its first word. Up to medium, about 2
    /// percent: in the prompt eval, this cut leaves out more than half of the wrong words and
    /// almost no right word. A longer length keeps the cut of each token at medium (about -0.39),
    /// because a fixed cut ends most suggestions after 4 or 5 words.
    pub(crate) fn tail_cut(self) -> LogProbability {
        match self {
            Self::VeryShort | Self::Short | Self::Medium => LogProbability(-3.9),
            Self::Long => LogProbability(-5.9),
            Self::VeryLong => LogProbability(-8.6),
        }
    }
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
    /// The name of the user.
    pub writer_name: WriterName,
    /// What the user writes about themself and how they write.
    pub writer_notes: WriterNotes,
    /// Whether suggestions may use the text on the clipboard.
    pub clipboard_access: ClipboardAccess,
    /// Where suggestions may use the text near the text field on the screen.
    pub screen_text: ScreenTextSettings,
    /// What suggestions learn from the earlier writing of the user.
    pub personalization: PersonalizationSettings,
    /// When GhostType gives the text around the caret to the model.
    pub prompt_preparation: PromptPreparation,
    /// When the ghost text shows the words that the model writes.
    pub suggestion_display: SuggestionDisplay,
    /// The most text that one suggestion shows.
    pub suggestion_length: SuggestionLength,
    /// What suggestions do while the word before the caret looks misspelled.
    pub typo_rule: TypoRule,
    /// Whether a suggestion fixes the word before the caret when it looks misspelled.
    pub typo_fix: TypoFix,
}

impl Settings {
    /// The longest suggestion that the backend allows: short on the CPU, which adds each token
    /// slowly.
    pub(crate) fn longest_suggestion_length(&self) -> SuggestionLength {
        if self.backend == Backend::Cpu {
            SuggestionLength::Short
        } else {
            SuggestionLength::VeryLong
        }
    }

    /// The most texts of a place that match the topic of the field, which a prompt shows as
    /// examples. The CPU shows one, because each example adds to the time of the first suggestion.
    pub(crate) fn matching_examples(&self) -> usize {
        if self.backend == Backend::Cpu { 1 } else { 2 }
    }

    /// The length of each suggestion: the selected length, at most the longest length that the
    /// backend allows.
    pub(crate) fn suggestion_length_in_use(&self) -> SuggestionLength {
        self.suggestion_length.min(self.longest_suggestion_length())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app(app_id: &str, app_name: &str, is_on: bool) -> ScreenTextApp {
        ScreenTextApp {
            app_id: app_id.to_owned(),
            app_name: app_name.to_owned(),
            app_path: None,
            is_on,
        }
    }

    fn surroundings_of(app_id: Option<&str>, app_name: Option<&str>) -> Surroundings {
        Surroundings {
            app_id: app_id.map(str::to_owned),
            app_name: app_name.map(str::to_owned),
            app_path: Some("C:\\Program Files\\App\\app.exe".to_owned()),
            ..Surroundings::default()
        }
    }

    #[test]
    fn new_app_entry_is_on_unless_the_app_is_a_password_manager() {
        let cases = [
            ("com.apple.mail", true),
            ("slack.exe", true),
            ("com.1password.1password", false),
            ("KeePassXC.exe", false),
        ];

        let switches = cases.map(|(app_id, _)| {
            ScreenTextApp::of(&surroundings_of(Some(app_id), Some("App"))).map(|entry| entry.is_on)
        });

        assert_eq!(switches, cases.map(|(_, is_on)| Some(is_on)));
    }

    #[test]
    fn app_entry_keeps_the_path_and_names_an_app_without_a_name_by_its_id() {
        let entry = ScreenTextApp::of(&surroundings_of(Some("notepad.exe"), None));

        assert_eq!(
            entry,
            Some(ScreenTextApp {
                app_path: Some("C:\\Program Files\\App\\app.exe".to_owned()),
                ..app("notepad.exe", "notepad.exe", true)
            })
        );
    }

    #[test]
    fn word_choice_is_medium_by_default() {
        let settings = PersonalizationSettings::default();

        assert_eq!(settings.word_choice, WordChoice::Medium);
    }

    #[test]
    fn app_without_an_id_has_no_entry() {
        let entry = ScreenTextApp::of(&surroundings_of(None, Some("Mail")));

        assert_eq!(entry, None);
    }

    #[test]
    fn app_switch_decides_for_its_app() {
        let settings = ScreenTextSettings {
            access: ScreenTextAccess::Allowed,
            apps: vec![
                app("com.apple.mail", "Mail", false),
                app("com.1password.1password", "1Password", true),
            ],
            ..ScreenTextSettings::default()
        };

        let allowed = ["com.apple.mail", "com.1password.1password", "com.apple.notes"]
            .map(|app_id| settings.allows(app_id));

        assert_eq!(allowed, [false, true, true]);
    }

    #[test]
    fn main_switch_off_allows_no_app() {
        let settings = ScreenTextSettings {
            access: ScreenTextAccess::Denied,
            apps: vec![app("com.apple.mail", "Mail", true)],
            ..ScreenTextSettings::default()
        };

        let allowed = ["com.apple.mail", "com.apple.notes"].map(|app_id| settings.allows(app_id));

        assert_eq!(allowed, [false, false]);
    }

    #[test]
    fn apps_go_in_alphabetical_order_of_their_names() {
        let mut settings = ScreenTextSettings::default();

        let added = [
            app("slack.exe", "Slack", true),
            app("com.apple.mail", "mail", true),
            app("b.exe", "Bear", true),
        ]
        .map(|entry| settings.add(&entry));

        assert_eq!(added, [true; 3]);
        assert_eq!(
            settings
                .apps
                .iter()
                .map(|entry| entry.app_name.as_str())
                .collect::<Vec<_>>(),
            ["Bear", "mail", "Slack"]
        );
    }

    #[test]
    fn known_app_is_not_added_again() {
        let mut settings = ScreenTextSettings {
            access: ScreenTextAccess::Allowed,
            apps: vec![app("com.apple.mail", "Mail", false)],
            ..ScreenTextSettings::default()
        };

        let added = settings.add(&app("com.apple.mail", "Mail", true));

        assert!(!added);
        assert_eq!(settings.apps, [app("com.apple.mail", "Mail", false)]);
    }

    fn history_app(app_id: &str, app_name: &str, scope: HistoryScope) -> HistoryApp {
        HistoryApp {
            app_id: app_id.to_owned(),
            app_name: app_name.to_owned(),
            app_path: None,
            scope,
        }
    }

    fn personalization(history: HistoryAccess, apps: Vec<HistoryApp>) -> PersonalizationSettings {
        PersonalizationSettings {
            history,
            apps,
            ..PersonalizationSettings::default()
        }
    }

    #[test]
    fn new_history_app_shares_with_all_apps_unless_it_is_a_password_manager_or_a_terminal() {
        let cases = [
            ("com.apple.mail", HistoryScope::AllApps),
            ("com.1password.1password", HistoryScope::Off),
            ("com.apple.Terminal", HistoryScope::Off),
            ("WindowsTerminal.exe", HistoryScope::Off),
            ("pwsh.exe", HistoryScope::Off),
        ];

        let scopes = cases.map(|(app_id, _)| HistoryApp::of(&app(app_id, "App", true)).scope);

        assert_eq!(scopes, cases.map(|(_, scope)| scope));
    }

    #[test]
    fn scope_of_an_app_in_the_list_is_its_own() {
        let settings = personalization(
            HistoryAccess::On,
            vec![
                history_app("com.apple.mail", "Mail", HistoryScope::Off),
                history_app("com.apple.Notes", "Notes", HistoryScope::ThisApp),
                history_app("com.apple.Terminal", "Terminal", HistoryScope::AllApps),
            ],
        );

        let scopes = ["com.apple.mail", "com.apple.Notes", "com.apple.Terminal"]
            .map(|app_id| settings.scope_of(app_id));

        assert_eq!(
            scopes,
            [HistoryScope::Off, HistoryScope::ThisApp, HistoryScope::AllApps]
        );
    }

    #[test]
    fn scope_of_an_app_that_is_not_in_the_list_is_all_apps_unless_it_is_a_password_manager_or_a_terminal() {
        let settings = personalization(HistoryAccess::On, Vec::new());

        let scopes =
            ["com.apple.notes", "com.1password.1password", "cmd.exe"].map(|app_id| settings.scope_of(app_id));

        assert_eq!(
            scopes,
            [HistoryScope::AllApps, HistoryScope::Off, HistoryScope::Off]
        );
    }

    #[test]
    fn history_off_turns_off_each_app() {
        let settings = personalization(
            HistoryAccess::Off,
            vec![history_app("com.apple.mail", "Mail", HistoryScope::AllApps)],
        );

        let scopes = ["com.apple.mail", "com.apple.notes"].map(|app_id| settings.scope_of(app_id));

        assert_eq!(scopes, [HistoryScope::Off, HistoryScope::Off]);
    }

    #[test]
    fn history_apps_go_in_alphabetical_order_and_once() {
        let mut settings = personalization(HistoryAccess::On, Vec::new());

        let added = [
            history_app("slack.exe", "Slack", HistoryScope::AllApps),
            history_app("com.apple.mail", "mail", HistoryScope::AllApps),
            history_app("slack.exe", "Slack", HistoryScope::Off),
        ]
        .map(|entry| settings.add(&entry));

        assert_eq!(added, [true, true, false]);
        assert_eq!(
            settings.apps,
            [
                history_app("com.apple.mail", "mail", HistoryScope::AllApps),
                history_app("slack.exe", "Slack", HistoryScope::AllApps)
            ]
        );
    }

    #[test]
    fn each_suggestion_length_allows_its_own_number_of_tokens() {
        let lengths = [
            SuggestionLength::VeryShort,
            SuggestionLength::Short,
            SuggestionLength::Medium,
            SuggestionLength::Long,
            SuggestionLength::VeryLong,
        ];

        let max_tokens = lengths.map(SuggestionLength::max_tokens);

        assert_eq!(max_tokens, [3, 6, 10, 15, 22].map(TokenCount));
    }

    #[test]
    fn only_the_cpu_backend_shows_one_matching_example() {
        let backends = [Backend::Cpu, Backend::Vulkan, Backend::Cuda, Backend::Metal];

        let counts = backends.map(|backend| {
            Settings {
                backend,
                ..Settings::default()
            }
            .matching_examples()
        });

        assert_eq!(counts, [1, 2, 2, 2]);
    }

    #[test]
    fn cpu_backend_uses_at_most_short_suggestions() {
        let cases = [
            (Backend::Cpu, SuggestionLength::VeryLong, SuggestionLength::Short),
            (
                Backend::Cpu,
                SuggestionLength::VeryShort,
                SuggestionLength::VeryShort,
            ),
            (
                Backend::Metal,
                SuggestionLength::VeryLong,
                SuggestionLength::VeryLong,
            ),
        ];

        let lengths = cases.map(|(backend, suggestion_length, _)| {
            Settings {
                backend,
                suggestion_length,
                ..Settings::default()
            }
            .suggestion_length_in_use()
        });

        assert_eq!(lengths, cases.map(|(_, _, in_use)| in_use));
    }
}
