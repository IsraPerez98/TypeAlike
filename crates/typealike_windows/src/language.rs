//! The languages that the user prefers, in Time & language > Language & region.

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
