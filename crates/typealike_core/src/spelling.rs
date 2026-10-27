//! The spelling dictionary of the OS, which tells which words a suggestion may write.

/// The spelling dictionary of the OS for the languages that the user writes. The platform
/// implements it, and core asks it about each word that a suggestion writes, so that a suggestion
/// never writes a word that the model made up.
pub trait SpellChecker {
    /// Whether the dictionary knows `word`, a run of letters with no whitespace, in any language
    /// that the user writes. A platform that cannot check `word` answers `true`.
    fn is_known_word(&self, word: &str) -> bool;
}

/// Returns the dictionary of each of the `preferred` languages of the user, such as `en-CL`, among
/// the `available` dictionaries of the OS, such as `en`, `en_GB` or `en-US`: the same language and
/// region, else the language alone, else the first one with the same language. A hyphen and an
/// underscore separate the parts of a tag alike, and the case does not count. Each dictionary comes
/// one time, in the order of `preferred`.
#[must_use]
pub fn dictionary_languages(preferred: &[String], available: &[String]) -> Vec<String> {
    let mut languages = Vec::<String>::new();
    for tag in preferred {
        let language = language_of(tag);
        let found = available
            .iter()
            .find(|dictionary| same_tag(dictionary, tag))
            .or_else(|| available.iter().find(|dictionary| same_tag(dictionary, language)))
            .or_else(|| {
                available
                    .iter()
                    .find(|dictionary| language_of(dictionary).eq_ignore_ascii_case(language))
            });
        if let Some(dictionary) = found.filter(|dictionary| !languages.contains(dictionary)) {
            languages.push(dictionary.clone());
        }
    }
    languages
}

fn language_of(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or_default()
}

fn same_tag(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left.chars().zip(right.chars()).all(|(left, right)| {
            left.eq_ignore_ascii_case(&right) || matches!((left, right), ('-', '_') | ('_', '-'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(texts: &[&str]) -> Vec<String> {
        texts.iter().map(|text| (*text).to_owned()).collect()
    }

    #[test]
    fn each_preferred_language_gets_its_closest_dictionary_one_time() {
        let cases = [
            (
                &["en-AU"][..],
                &["en_GB", "en", "en_AU", "es"][..],
                &["en_AU"][..],
            ),
            (
                &["en-CL", "en-US", "es-419"],
                &["en_GB", "en", "es"],
                &["en", "es"],
            ),
            (&["pt-PT"], &["pt_BR", "de"], &["pt_BR"]),
            (&["ja-JP"], &["en", "es"], &[]),
            (
                &["es-419", "en-CL"],
                &["en-US", "es-ES", "es-MX"],
                &["es-ES", "en-US"],
            ),
            (&["es-MX"], &["es-ES", "es-mx"], &["es-mx"]),
        ];

        let languages = cases
            .map(|(preferred, available, _)| dictionary_languages(&strings(preferred), &strings(available)));

        assert_eq!(languages, cases.map(|(_, _, expected)| strings(expected)));
    }
}
