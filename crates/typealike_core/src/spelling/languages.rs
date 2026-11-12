//! The languages that the user can write, as TypeAlike names them.

use crate::language::{language_of, same_tag};

/// The languages that TypeAlike can check, as a tag and the name of the language in that
/// language. The first entry of a language is its main entry: the language without a region, or
/// its most common region.
pub const WRITING_LANGUAGES: [(&str, &str); 98] = [
    ("en-US", "English (United States)"),
    ("en-GB", "English (United Kingdom)"),
    ("en-AU", "English (Australia)"),
    ("en-CA", "English (Canada)"),
    ("en-IN", "English (India)"),
    ("en-ZA", "English (South Africa)"),
    ("es", "Español"),
    ("es-ES", "Español (España)"),
    ("es-MX", "Español (México)"),
    ("es-AR", "Español (Argentina)"),
    ("es-BO", "Español (Bolivia)"),
    ("es-CL", "Español (Chile)"),
    ("es-CO", "Español (Colombia)"),
    ("es-CR", "Español (Costa Rica)"),
    ("es-CU", "Español (Cuba)"),
    ("es-DO", "Español (República Dominicana)"),
    ("es-EC", "Español (Ecuador)"),
    ("es-GT", "Español (Guatemala)"),
    ("es-HN", "Español (Honduras)"),
    ("es-NI", "Español (Nicaragua)"),
    ("es-PA", "Español (Panamá)"),
    ("es-PE", "Español (Perú)"),
    ("es-PH", "Español (Filipinas)"),
    ("es-PR", "Español (Puerto Rico)"),
    ("es-PY", "Español (Paraguay)"),
    ("es-SV", "Español (El Salvador)"),
    ("es-US", "Español (Estados Unidos)"),
    ("es-UY", "Español (Uruguay)"),
    ("es-VE", "Español (Venezuela)"),
    ("pt-BR", "Português (Brasil)"),
    ("pt-PT", "Português (Portugal)"),
    ("fr", "Français"),
    ("fr-CA", "Français (Canada)"),
    ("de", "Deutsch"),
    ("de-AT", "Deutsch (Österreich)"),
    ("de-CH", "Deutsch (Schweiz)"),
    ("it", "Italiano"),
    ("nl", "Nederlands"),
    ("ca", "Català"),
    ("ca-valencia", "Català (valencià)"),
    ("gl", "Galego"),
    ("eu", "Euskara"),
    ("oc", "Occitan"),
    ("da", "Dansk"),
    ("sv", "Svenska"),
    ("sv-FI", "Svenska (Finland)"),
    ("nb", "Norsk bokmål"),
    ("nn", "Norsk nynorsk"),
    ("fi", "Suomi"),
    ("is", "Íslenska"),
    ("fo", "Føroyskt"),
    ("pl", "Polski"),
    ("cs", "Čeština"),
    ("sk", "Slovenčina"),
    ("sl", "Slovenščina"),
    ("hr", "Hrvatski"),
    ("sr", "Српски"),
    ("sr-Latn", "Srpski (latinica)"),
    ("bg", "Български"),
    ("mk", "Македонски"),
    ("ru", "Русский"),
    ("uk", "Українська"),
    ("ro", "Română"),
    ("hu", "Magyar"),
    ("et", "Eesti"),
    ("lv", "Latviešu"),
    ("lt", "Lietuvių"),
    ("ltg", "Latgalīšu"),
    ("el", "Ελληνικά"),
    ("el-polyton", "Ελληνικά (πολυτονικό)"),
    ("tr", "Türkçe"),
    ("tk", "Türkmençe"),
    ("ka", "ქართული"),
    ("hy", "Հայերեն"),
    ("hyw", "Արեւմտահայերէն"),
    ("he", "עברית"),
    ("fa", "فارسی"),
    ("ar", "العربية"),
    ("hi", "हिन्दी"),
    ("ne", "नेपाली"),
    ("pa", "ਪੰਜਾਬੀ"),
    ("te", "తెలుగు"),
    ("ko", "한국어"),
    ("vi", "Tiếng Việt"),
    ("id", "Bahasa Indonesia"),
    ("mn", "Монгол"),
    ("ga", "Gaeilge"),
    ("gd", "Gàidhlig"),
    ("cy", "Cymraeg"),
    ("br", "Brezhoneg"),
    ("fy", "Frysk"),
    ("lb", "Lëtzebuergesch"),
    ("nds", "Plattdüütsch"),
    ("fur", "Furlan"),
    ("la", "Latina"),
    ("eo", "Esperanto"),
    ("ia", "Interlingua"),
    ("rw", "Ikinyarwanda"),
];

/// The name of the language of `tag` in that language, when `tag` is one of
/// [`WRITING_LANGUAGES`]. A hyphen and an underscore separate the parts of a tag alike, and the
/// case does not count.
#[must_use]
pub fn language_name(tag: &str) -> Option<&'static str> {
    WRITING_LANGUAGES
        .iter()
        .find(|(language, _)| same_tag(language, tag))
        .map(|(_, name)| *name)
}

/// Whether `tag` is the main entry of its language in [`WRITING_LANGUAGES`]: its first entry.
pub(crate) fn is_main_entry(tag: &str) -> bool {
    let language = language_of(tag);
    WRITING_LANGUAGES
        .iter()
        .find(|(entry, _)| language_of(entry).eq_ignore_ascii_case(language))
        .is_some_and(|(entry, _)| same_tag(entry, tag))
}

/// The entry of [`WRITING_LANGUAGES`] for `tag`, a language of the OS: the entry with the same
/// tag, else the main entry of its language.
pub(crate) fn writing_language_of(tag: &str) -> Option<&'static str> {
    let language = language_of(tag);
    WRITING_LANGUAGES
        .iter()
        .find(|(entry, _)| same_tag(entry, tag))
        .or_else(|| {
            WRITING_LANGUAGES
                .iter()
                .find(|(entry, _)| language_of(entry).eq_ignore_ascii_case(language))
        })
        .map(|(entry, _)| *entry)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn each_writing_language_has_its_own_tag() {
        let tags = WRITING_LANGUAGES
            .iter()
            .map(|(tag, _)| tag.to_ascii_lowercase())
            .collect::<HashSet<_>>();

        assert_eq!(tags.len(), WRITING_LANGUAGES.len());
    }

    #[test]
    fn name_of_a_language_matches_any_form_of_its_tag() {
        let tags = ["es-CL", "es_cl", "pt_BR", "ja"];

        let names = tags.map(language_name);

        assert_eq!(
            names,
            [
                Some("Español (Chile)"),
                Some("Español (Chile)"),
                Some("Português (Brasil)"),
                None
            ]
        );
    }

    #[test]
    fn main_entry_is_the_first_entry_of_its_language() {
        let tags = ["es", "es-CL", "en-US", "en_GB", "pt-BR", "sr-Latn"];

        let answers = tags.map(is_main_entry);

        assert_eq!(answers, [true, false, true, false, true, false]);
    }

    #[test]
    fn language_of_the_os_gives_its_own_entry_else_the_main_entry_of_its_language() {
        let tags = ["es-CL", "es-419", "fr-FR", "en_US", "nb-NO", "ja-JP"];

        let entries = tags.map(writing_language_of);

        assert_eq!(
            entries,
            [
                Some("es-CL"),
                Some("es"),
                Some("fr"),
                Some("en-US"),
                Some("nb"),
                None
            ]
        );
    }

    #[test]
    fn each_language_that_macos_can_check_has_a_writing_language() {
        // "ars" (Najdi Arabic) has a language tag of its own, without a writing language yet.
        let macos_dictionaries = [
            "en_GB", "en", "en_CA", "en_PH", "en_AU", "en_IN", "en_SG", "en_ZA", "en_NZ", "en_JP", "es",
            "ar", "bg", "cs", "da", "de", "el", "fi", "fr", "ga", "he", "hi", "hu", "id", "is", "it", "ko",
            "lt", "nb", "nl", "nn", "pa", "pl", "pt_BR", "pt_PT", "ro", "ru", "sl", "sv", "te", "tr", "uk",
            "vi",
        ];

        let missing = macos_dictionaries
            .into_iter()
            .filter(|tag| writing_language_of(tag).is_none())
            .collect::<Vec<_>>();

        assert_eq!(missing, Vec::<&str>::new());
    }
}
