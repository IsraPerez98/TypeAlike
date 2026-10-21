//! The settings that GhostType keeps between runs, and the seams to the services that keep them.

mod known_apps;
mod personalization;
mod screen_text;

use std::fmt;

use crate::hub::ModelFile;
use crate::inference::{Backend, LogProbability, TokenCount, TypoCheck};

pub use personalization::{
    HistoryAccess, HistoryApp, HistoryScope, KeptTexts, PersonalizationSettings, WordChoice,
};
pub use screen_text::{OcrLevel, ScreenTextAccess, ScreenTextApp, ScreenTextSettings};

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

impl TypoRule {
    /// The least likely that the typed letters of the last word can be before GhostType takes them
    /// for a typo, or `None` when the rule finds no typos. In the prompt eval, this cut catches
    /// almost all typos and hides few other words, each a rare name or word.
    #[must_use]
    pub fn typed_letters_cut(self) -> Option<LogProbability> {
        match self {
            Self::HideSuggestions => Some(LogProbability(-11.0)),
            Self::Suggest => None,
        }
    }
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

    /// The typo check of each suggestion request, from the typo rule and the typo fix.
    pub(crate) fn typo_check(&self) -> TypoCheck {
        let Some(min) = self.typo_rule.typed_letters_cut() else {
            return TypoCheck::Off;
        };
        match self.typo_fix {
            TypoFix::Offer => TypoCheck::Fix(min),
            TypoFix::Off => TypoCheck::Hide(min),
        }
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
