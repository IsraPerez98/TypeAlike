//! The app and the website of the field where the user types, and a desktop whose field the app
//! read already.

use crate::desktop::{CopiedText, Desktop, Field, OcrAvailability, OcrLevels, Surroundings, Timestamp};
use crate::private_window;
use crate::settings::{ScreenTextApp, SuggestionPlaces};

/// The app and the website of a field where the user types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FieldPlace {
    /// The app, if it has an id.
    pub(super) app: Option<ScreenTextApp>,
    /// The host of the website, if any.
    pub(super) host: Option<String>,
    /// Whether the place is a private window of a browser, where suggestions never show.
    pub(super) is_private_window: bool,
}

impl FieldPlace {
    pub(super) fn of(surroundings: &Surroundings) -> Self {
        Self {
            app: ScreenTextApp::of(surroundings),
            host: surroundings.domain.clone(),
            is_private_window: private_window::is_private_window(surroundings),
        }
    }

    /// Whether `places` lets suggestions show in this place.
    pub(super) fn is_on_in(&self, places: &SuggestionPlaces) -> bool {
        let app_id = self.app.as_ref().map(|app| app.app_id.as_str());
        !self.is_private_window && places.allows(app_id, self.host.as_deref())
    }
}

/// A desktop whose surroundings the app read already, so that a key reads them one time.
pub(super) struct ReadField<'desktop> {
    pub(super) desktop: &'desktop dyn Desktop,
    pub(super) surroundings: Surroundings,
}

impl Desktop for ReadField<'_> {
    fn field(&self) -> Field {
        self.desktop.field()
    }

    fn surroundings(&self) -> Surroundings {
        self.surroundings.clone()
    }

    fn copied_text(&self) -> Option<CopiedText> {
        self.desktop.copied_text()
    }

    fn ocr_availability(&self) -> OcrAvailability {
        self.desktop.ocr_availability()
    }

    fn ocr_levels(&self) -> OcrLevels {
        self.desktop.ocr_levels()
    }

    fn now(&self) -> Option<Timestamp> {
        self.desktop.now()
    }

    fn is_ghost_shown(&self) -> bool {
        self.desktop.is_ghost_shown()
    }
}
