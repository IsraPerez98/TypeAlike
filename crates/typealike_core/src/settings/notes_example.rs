//! Examples of the notes about the writer, one for each common kind of writing.

use crate::localization::{Translations, UiLanguage};
use crate::settings::WriterNotes;

/// An example of the notes about the writer, which the user can pick and then change. The text of
/// each example is in the language of the UI.
///
/// ```
/// use typealike_core::localization::{Translations, UiLanguage};
/// use typealike_core::settings::{NotesExample, WriterNotes};
///
/// let translations = Translations::new(UiLanguage::English);
///
/// assert_eq!(NotesExample::General.notes(&translations), WriterNotes::default());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesExample {
    /// Clear and friendly messages of any kind. These are the default notes.
    General,
    /// Emails and messages at work.
    WorkEmails,
    /// Answers to customers.
    CustomerSupport,
    /// Casual chats with friends and family.
    Chats,
    /// Essays, notes and emails of a student.
    Study,
    /// Code reviews, bug reports and technical documents.
    SoftwareDevelopment,
}

impl NotesExample {
    /// Each example, in the order of the menu.
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::WorkEmails,
        Self::CustomerSupport,
        Self::Chats,
        Self::Study,
        Self::SoftwareDevelopment,
    ];

    /// The notes of the example, in the language of `translations`.
    #[must_use]
    pub fn notes(self, translations: &Translations) -> WriterNotes {
        WriterNotes(translations.text(self.text_message()))
    }

    /// The example whose notes in the language of `translations` are `notes`, if any.
    #[must_use]
    pub fn of(notes: &WriterNotes, translations: &Translations) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|example| example.notes(translations) == *notes)
    }

    /// The message of the name of the example in a menu.
    pub(crate) const fn name_message(self) -> &'static str {
        match self {
            Self::General => "notes-example-general",
            Self::WorkEmails => "notes-example-work",
            Self::CustomerSupport => "notes-example-support",
            Self::Chats => "notes-example-chats",
            Self::Study => "notes-example-study",
            Self::SoftwareDevelopment => "notes-example-software",
        }
    }

    const fn text_message(self) -> &'static str {
        match self {
            Self::General => "notes-example-general-text",
            Self::WorkEmails => "notes-example-work-text",
            Self::CustomerSupport => "notes-example-support-text",
            Self::Chats => "notes-example-chats-text",
            Self::Study => "notes-example-study-text",
            Self::SoftwareDevelopment => "notes-example-software-text",
        }
    }
}

impl WriterNotes {
    /// The default notes in `language`: the notes of [`NotesExample::General`].
    #[must_use]
    pub fn default_for(language: UiLanguage) -> Self {
        NotesExample::General.notes(&Translations::new(language))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_notes_follow_the_language() {
        let notes = [UiLanguage::English, UiLanguage::Spanish].map(WriterNotes::default_for);

        assert_eq!(
            notes,
            [
                WriterNotes(
                    "I write clear and friendly messages. I keep the tone of the conversation.".to_owned()
                ),
                WriterNotes(
                    "Escribo mensajes claros y amables. Mantengo el tono de la conversación.".to_owned()
                ),
            ]
        );
    }

    #[test]
    fn each_example_has_notes_of_its_own() {
        let translations = Translations::new(UiLanguage::English);

        let found =
            NotesExample::ALL.map(|example| NotesExample::of(&example.notes(&translations), &translations));

        assert_eq!(found, NotesExample::ALL.map(Some));
    }

    #[test]
    fn notes_that_the_user_wrote_are_no_example() {
        let notes = WriterNotes("I am a nurse.".to_owned());

        let example = NotesExample::of(&notes, &Translations::new(UiLanguage::English));

        assert_eq!(example, None);
    }
}
