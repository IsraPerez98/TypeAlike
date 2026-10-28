//! The languages that the user prefers, in Language & Region.

use objc2_foundation::NSLocale;
use typealike_core::localization::UiLanguage;

/// The preferred languages of the user, first the most preferred, such as `es-CL`.
pub(crate) fn preferred_languages() -> Vec<String> {
    NSLocale::preferredLanguages()
        .iter()
        .map(|language| language.to_string())
        .collect()
}

/// The language of the texts of TypeAlike: the first preferred language of the user that has
/// texts, else English.
#[must_use]
pub fn ui_language() -> UiLanguage {
    UiLanguage::for_preferred(&preferred_languages())
}
