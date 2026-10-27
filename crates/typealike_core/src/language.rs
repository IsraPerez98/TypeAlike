//! The language tags of the user, such as `es-CL`, and how they match the languages that a part of
//! the OS or of TypeAlike has.

/// Returns the closest of the `available` languages, such as `en`, `en_GB` or `en-US`, for each of
/// the `preferred` languages of the user, such as `en-CL`: the same language and region, else the
/// language alone, else the first one with the same language. A hyphen and an underscore separate
/// the parts of a tag alike, and the case does not count. Each available language comes one time,
/// in the order of `preferred`.
#[must_use]
pub fn closest_languages(preferred: &[String], available: &[String]) -> Vec<String> {
    let mut languages = Vec::<String>::new();
    for tag in preferred {
        let language = language_of(tag);
        let found = available
            .iter()
            .find(|candidate| same_tag(candidate, tag))
            .or_else(|| available.iter().find(|candidate| same_tag(candidate, language)))
            .or_else(|| {
                available
                    .iter()
                    .find(|candidate| language_of(candidate).eq_ignore_ascii_case(language))
            });
        if let Some(candidate) = found.filter(|candidate| !languages.contains(candidate)) {
            languages.push(candidate.clone());
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
    fn each_preferred_language_gets_its_closest_available_language_one_time() {
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
            .map(|(preferred, available, _)| closest_languages(&strings(preferred), &strings(available)));

        assert_eq!(languages, cases.map(|(_, _, expected)| strings(expected)));
    }
}
