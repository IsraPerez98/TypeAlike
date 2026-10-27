//! The texts of the UI in the language of the user. Each language has a Fluent file in
//! `locales/<tag>/typealike.ftl`, with the same messages as English.

use std::fmt;

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

use crate::language::closest_languages;

/// A language that the UI has translations for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiLanguage {
    /// American English, the language of the source texts.
    English,
    /// Spanish.
    Spanish,
}

impl UiLanguage {
    const ALL: [Self; 2] = [Self::English, Self::Spanish];

    /// Returns the language of the UI for the `preferred` languages of the user, such as `es-CL`:
    /// the first one that the UI has translations for, else English.
    #[must_use]
    pub fn for_preferred(preferred: &[String]) -> Self {
        let tags = Self::ALL.map(|language| language.tag().to_owned());
        closest_languages(preferred, &tags)
            .first()
            .and_then(|tag| Self::ALL.into_iter().find(|language| language.tag() == tag))
            .unwrap_or(Self::English)
    }

    fn tag(self) -> &'static str {
        match self {
            Self::English => "en-US",
            Self::Spanish => "es",
        }
    }

    fn identifier(self) -> LanguageIdentifier {
        self.tag()
            .parse()
            .expect("the tag of each UI language is a valid language tag")
    }

    fn messages(self) -> &'static str {
        match self {
            Self::English => include_str!("../locales/en-US/typealike.ftl"),
            Self::Spanish => include_str!("../locales/es/typealike.ftl"),
        }
    }
}

/// A value that a message of the UI puts into its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageArgument<'text> {
    /// A text, such as the name of an app or the host of a website.
    Text(&'text str),
    /// A number, such as a count of minutes. A message can pick its plural form from it.
    Number(u32),
}

/// The texts of the UI in one language.
pub struct Translations {
    language: UiLanguage,
    bundle: FluentBundle<FluentResource>,
}

impl Translations {
    /// Loads the texts of `language`.
    #[must_use]
    pub fn new(language: UiLanguage) -> Self {
        // A test makes sure that each file parses without errors.
        let resource =
            FluentResource::try_new(language.messages().to_owned()).unwrap_or_else(|(resource, _)| resource);
        let mut bundle = FluentBundle::new(vec![language.identifier()]);
        // The isolation marks are for text that goes right to left, and no UI language does.
        bundle.set_use_isolating(false);
        bundle.add_resource_overriding(resource);
        Self { language, bundle }
    }

    /// Returns the text of the message `id`.
    ///
    /// # Panics
    ///
    /// In a debug build, panics when the language has no message `id`, or when the message needs
    /// an argument. A release build then gives `id`, or the message with the name of the argument.
    #[must_use]
    pub fn text(&self, id: &str) -> String {
        self.text_with(id, &[])
    }

    /// Returns the text of the message `id`, with the `arguments` that it puts into its text,
    /// each with its name, such as `app`.
    ///
    /// # Panics
    ///
    /// In a debug build, panics when the language has no message `id`, or when the message needs
    /// an argument that `arguments` does not give. A release build then gives `id`, or the message
    /// with the name of the missing argument.
    #[must_use]
    pub fn text_with(&self, id: &str, arguments: &[(&str, MessageArgument<'_>)]) -> String {
        let pattern = self.bundle.get_message(id).and_then(|message| message.value());
        debug_assert!(pattern.is_some(), "no message {id} in {:?}", self.language);
        let Some(pattern) = pattern else {
            return id.to_owned();
        };
        let arguments = arguments
            .iter()
            .map(|&(name, argument)| (name, fluent_value(argument)))
            .collect::<FluentArgs<'_>>();
        let mut errors = Vec::new();
        let text = self
            .bundle
            .format_pattern(pattern, Some(&arguments), &mut errors)
            .into_owned();
        debug_assert!(errors.is_empty(), "the message {id} has errors: {errors:?}");
        text
    }
}

fn fluent_value(argument: MessageArgument<'_>) -> FluentValue<'_> {
    match argument {
        MessageArgument::Text(text) => FluentValue::from(text),
        MessageArgument::Number(number) => FluentValue::from(number),
    }
}

impl fmt::Debug for Translations {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Translations")
            .field("language", &self.language)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use fluent_syntax::ast::Entry;

    use super::*;

    fn strings(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| (*text).to_owned()).collect()
    }

    fn sorted_message_ids(language: UiLanguage) -> Vec<String> {
        let resource =
            FluentResource::try_new(language.messages().to_owned()).unwrap_or_else(|(resource, _)| resource);
        let mut ids = resource
            .entries()
            .filter_map(|entry| match entry {
                Entry::Message(message) => Some(message.id.name.to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    }

    #[test]
    fn ui_language_is_the_first_preferred_language_with_translations() {
        let cases = [
            (&["es-CL"][..], UiLanguage::Spanish),
            (&["en-GB"], UiLanguage::English),
            (&["fr-FR", "es-MX", "en-US"], UiLanguage::Spanish),
            (&["fr-FR"], UiLanguage::English),
            (&[], UiLanguage::English),
        ];

        let languages = cases.map(|(preferred, _)| UiLanguage::for_preferred(&strings(preferred)));

        assert_eq!(languages, cases.map(|(_, expected)| expected));
    }

    #[test]
    fn each_language_shows_its_own_text() {
        let languages = [UiLanguage::English, UiLanguage::Spanish];

        let titles = languages.map(|language| Translations::new(language).text("energy-saving-title"));

        assert_eq!(titles, ["Energy saving", "Ahorro de energía"].map(str::to_owned));
    }

    #[test]
    fn a_text_argument_goes_into_its_message_as_it_is() {
        let translations = Translations::new(UiLanguage::English);

        let label = translations.text_with(
            "badge-app-suggestions",
            &[("app", MessageArgument::Text("Notepad"))],
        );

        assert_eq!(label, "Show suggestions in Notepad");
    }

    #[test]
    fn a_number_argument_goes_into_its_message() {
        let translations = Translations::new(UiLanguage::Spanish);

        let choice = translations.text_with(
            "energy-saving-idle-timeout-minutes",
            &[("minutes", MessageArgument::Number(15))],
        );

        assert_eq!(choice, "15 minutos");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "no message no-such-message")]
    fn a_message_that_the_language_does_not_have_is_a_bug() {
        let translations = Translations::new(UiLanguage::English);

        // The call panics before it gives a text.
        let _ = translations.text("no-such-message");
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "badge-app-suggestions")]
    fn a_missing_argument_is_a_bug() {
        let translations = Translations::new(UiLanguage::English);

        // The call panics before it gives a text.
        let _ = translations.text("badge-app-suggestions");
    }

    #[test]
    fn each_language_parses_without_errors() {
        let is_parsed =
            UiLanguage::ALL.map(|language| FluentResource::try_new(language.messages().to_owned()).is_ok());

        assert_eq!(is_parsed, UiLanguage::ALL.map(|_| true));
    }

    #[test]
    fn each_language_has_each_message_of_english_one_time() {
        let mut english = sorted_message_ids(UiLanguage::English);
        english.dedup();

        let ids = UiLanguage::ALL.map(sorted_message_ids);

        assert_eq!(ids, UiLanguage::ALL.map(|_| english.clone()));
    }
}
