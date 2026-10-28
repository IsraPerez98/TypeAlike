//! The languages that the user prefers, in Time & language > Language & region.

use typealike_core::localization::UiLanguage;
use windows::System::UserProfile::GlobalizationPreferences;

/// The preferred languages of the user, first the most preferred, such as `es-CL`. Empty when
/// Windows does not give them.
pub(crate) fn preferred_languages() -> Vec<String> {
    match GlobalizationPreferences::Languages() {
        Ok(languages) => languages
            .into_iter()
            .map(|language| language.to_string())
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "failed to read the preferred languages");
            Vec::new()
        }
    }
}

/// The language of the texts of TypeAlike: the first preferred language of the user that has
/// texts, else English.
#[must_use]
pub fn ui_language() -> UiLanguage {
    UiLanguage::for_preferred(&preferred_languages())
}
