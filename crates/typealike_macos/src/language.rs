//! The languages that the user prefers, in Language & Region.

use objc2_foundation::NSLocale;

/// The preferred languages of the user, first the most preferred, such as `es-CL`.
pub(crate) fn preferred_languages() -> Vec<String> {
    NSLocale::preferredLanguages()
        .iter()
        .map(|language| language.to_string())
        .collect()
}
