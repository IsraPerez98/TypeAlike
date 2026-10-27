//! The texts of the UI in the language of the user. Each language has a Fluent file in
//! `locales/<tag>/typealike.ftl`, with the same messages as English.

use std::fmt;

use fluent_bundle::types::{FluentNumber, FluentNumberOptions};
use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

use crate::language::closest_languages;

/// A language that the UI has translations for. The default is English.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiLanguage {
    /// American English, the language of the source texts.
    #[default]
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

    /// Whether the titles of this language capitalize each main word, as the menus and the buttons
    /// of English macOS do. Other languages write their titles in sentence case.
    #[must_use]
    pub fn has_title_case(self) -> bool {
        match self {
            Self::English => true,
            Self::Spanish => false,
        }
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

    /// Whether a number of this language has a comma between the whole part and the fraction,
    /// such as 0,42.
    fn has_decimal_comma(self) -> bool {
        match self {
            Self::English => false,
            Self::Spanish => true,
        }
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
    /// A number, such as a count of downloads. A message can pick its plural form from it.
    Number(u32),
    /// A number with a fraction: `value` divided by ten to the power of `digits`, with `digits`
    /// digits after the decimal separator of the language, such as 0.42 for a `value` of 42 and two
    /// `digits`.
    Decimal {
        /// The number without its decimal separator.
        value: u32,
        /// How many digits of `value` come after the decimal separator.
        digits: u8,
    },
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
        if language.has_decimal_comma() {
            bundle.set_formatter(Some(with_decimal_comma));
        }
        bundle.add_resource_overriding(resource);
        Self { language, bundle }
    }

    /// The language of the texts.
    #[must_use]
    pub fn language(&self) -> UiLanguage {
        self.language
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
        MessageArgument::Decimal { value, digits } => {
            let options = FluentNumberOptions {
                minimum_fraction_digits: Some(usize::from(digits)),
                ..FluentNumberOptions::default()
            };
            let number = f64::from(value) / 10_f64.powi(i32::from(digits));
            FluentValue::Number(FluentNumber::new(number, options))
        }
    }
}

/// Writes a number of `value` with a decimal comma. Other values keep the format of Fluent.
fn with_decimal_comma<Memoizer>(value: &FluentValue<'_>, _: &Memoizer) -> Option<String> {
    match value {
        FluentValue::Number(number) => Some(number.as_string().replace('.', ",")),
        _ => None,
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
    use std::collections::BTreeSet;

    use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};

    use super::*;

    fn strings(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| (*text).to_owned()).collect()
    }

    /// The id of each message of `language`, with the names of the arguments that it uses.
    fn message_arguments(language: UiLanguage) -> Vec<(String, BTreeSet<String>)> {
        let resource =
            FluentResource::try_new(language.messages().to_owned()).unwrap_or_else(|(resource, _)| resource);
        let mut arguments = resource
            .entries()
            .filter_map(|entry| match entry {
                Entry::Message(message) => Some((
                    message.id.name.to_owned(),
                    message.value.as_ref().map(pattern_arguments).unwrap_or_default(),
                )),
                _ => None,
            })
            .collect::<Vec<_>>();
        arguments.sort_unstable();
        arguments
    }

    fn pattern_arguments(pattern: &Pattern<&str>) -> BTreeSet<String> {
        pattern
            .elements
            .iter()
            .flat_map(|element| match element {
                PatternElement::Placeable { expression } => expression_arguments(expression),
                PatternElement::TextElement { .. } => BTreeSet::new(),
            })
            .collect()
    }

    fn expression_arguments(expression: &Expression<&str>) -> BTreeSet<String> {
        match expression {
            Expression::Inline(inline) => inline_arguments(inline),
            Expression::Select { selector, variants } => variants
                .iter()
                .flat_map(|variant| pattern_arguments(&variant.value))
                .chain(inline_arguments(selector))
                .collect(),
        }
    }

    fn inline_arguments(inline: &InlineExpression<&str>) -> BTreeSet<String> {
        match inline {
            InlineExpression::VariableReference { id } => BTreeSet::from([id.name.to_owned()]),
            InlineExpression::Placeable { expression } => expression_arguments(expression),
            _ => BTreeSet::new(),
        }
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
    fn a_number_argument_picks_the_plural_form_of_its_message() {
        let translations = Translations::new(UiLanguage::Spanish);
        let counts = [1, 15];

        let details = counts.map(|count| {
            translations.text_with("models-downloads", &[("count", MessageArgument::Number(count))])
        });

        assert_eq!(details, ["1 descarga", "15 descargas"].map(str::to_owned));
    }

    #[test]
    fn a_decimal_argument_has_the_decimal_separator_of_the_language() {
        let languages = [UiLanguage::English, UiLanguage::Spanish];
        let seconds = MessageArgument::Decimal { value: 42, digits: 2 };

        let details = languages.map(|language| {
            Translations::new(language).text_with("playground-time", &[("seconds", seconds)])
        });

        assert_eq!(
            details,
            ["Written in 0.42 seconds", "Escrita en 0,42 segundos"].map(str::to_owned)
        );
    }

    #[test]
    fn a_decimal_argument_keeps_its_zeros_after_the_separator() {
        let translations = Translations::new(UiLanguage::English);

        let size = translations.text_with(
            "models-size-gigabytes",
            &[("size", MessageArgument::Decimal { value: 20, digits: 1 })],
        );

        assert_eq!(size, "2.0 GB");
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
    fn each_language_uses_the_arguments_of_each_english_message() {
        let english = message_arguments(UiLanguage::English);

        let arguments = UiLanguage::ALL.map(message_arguments);

        assert_eq!(arguments, UiLanguage::ALL.map(|_| english.clone()));
    }

    #[test]
    fn each_language_has_each_message_of_english_one_time() {
        let mut english = sorted_message_ids(UiLanguage::English);
        english.dedup();

        let ids = UiLanguage::ALL.map(sorted_message_ids);

        assert_eq!(ids, UiLanguage::ALL.map(|_| english.clone()));
    }
}
