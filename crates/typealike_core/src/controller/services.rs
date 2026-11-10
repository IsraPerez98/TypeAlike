//! The services that the controller uses, and the way their answers come back to it.

use std::fmt;
use std::sync::Arc;

use crate::desktop::{EnergySaver, ScreenRead};
use crate::history::WritingHistory;
use crate::hub::{HubEvent, ModelHub};
use crate::inference::{Backend, InferenceEvent, ModelRunner};
use crate::localization::UiLanguage;
use crate::release::{DownloadedRelease, ReleaseDownloadFailed, ReleaseFeed, ReleaseReadFailed, ReleaseTag};
use crate::settings::{Autostart, SettingsStore};
use crate::settings_window::about::Library;
use crate::setup::TrayPlace;
use crate::spelling::{DictionaryEvent, DictionaryHub, OsDictionaries};

/// Who picks the backend that runs each model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendChoice {
    /// The user, in the Models pane.
    User,
    /// The platform, which runs each model on this backend.
    Platform(Backend),
}

/// What the platform tells the controller when TypeAlike starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Startup {
    /// Who picks the backend that runs each model.
    pub backend_choice: BackendChoice,
    /// Whether the energy saver of the computer is on. The platform reports each later change to
    /// [`Controller::on_energy_saver`](super::Controller::on_energy_saver).
    pub energy_saver: EnergySaver,
    /// The language of the texts that TypeAlike shows while the user picked no language in the
    /// settings, from the preferred languages of the user ([`UiLanguage::for_preferred`]).
    pub ui_language: UiLanguage,
    /// Whether the settings window shows the Playground pane ([`PlaygroundPage::of_build`]).
    pub playground: PlaygroundPage,
    /// Where the platform shows the icon of TypeAlike, which the setup tells the user.
    pub tray: TrayPlace,
}

/// Whether the settings window shows the Playground pane, where a developer tries the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaygroundPage {
    /// The window has no Playground pane.
    Hidden,
    /// The Playground pane is the last pane of the window.
    Shown,
}

impl PlaygroundPage {
    /// The Playground pane of this build: shown only with the `playground` feature.
    #[must_use]
    pub const fn of_build() -> Self {
        if cfg!(feature = "playground") {
            Self::Shown
        } else {
            Self::Hidden
        }
    }
}

/// The answer of a service. The platform gives it back to the controller on the thread that runs
/// the controller.
#[derive(Debug, Clone, PartialEq)]
pub enum Delivery {
    /// For the Models pane.
    Hub(HubEvent),
    /// For the Languages and typos pane.
    Dictionary(DictionaryEvent),
    /// For the Playground pane, which also loads the suggestion model.
    Playground(InferenceEvent),
    /// For the ghost text at the caret.
    Suggestion(InferenceEvent),
    /// For the model that TypeAlike recommends before the first download.
    Recommendation(InferenceEvent),
    /// The answer to a read of the screen that [`Shell::read_screen`](super::Shell::read_screen) started.
    Screen(ScreenRead),
    /// The newest release, for the check for updates of the About pane.
    Release(Result<ReleaseTag, ReleaseReadFailed>),
    /// The downloaded release, which the About pane installs.
    ReleaseDownload(Result<DownloadedRelease, ReleaseDownloadFailed>),
}

/// Takes a delivery from any thread to the thread that runs the controller, and gives it to
/// [`Controller::on_delivery`](super::Controller::on_delivery) there.
pub type DeliverySink = Arc<dyn Fn(Delivery) + Send + Sync>;

/// The services that the controller uses.
pub struct Services {
    /// Finds, downloads and deletes models for the Models pane.
    pub hub: Box<dyn ModelHub>,
    /// Loads the suggestion model, and completes the text that the user types.
    pub runner: Box<dyn ModelRunner>,
    /// Keeps the settings that the user changes in the settings window.
    pub settings_store: Box<dyn SettingsStore>,
    /// Starts TypeAlike at login when the user turns it on in the General pane.
    pub autostart: Box<dyn Autostart>,
    /// Keeps the earlier writing of the user while the Personalization pane allows it.
    pub history: Box<dyn WritingHistory>,
    /// The spelling dictionaries of the OS, which know the words of the languages that the user
    /// writes, so that a suggestion stops before a word that the model made up.
    pub os_dictionaries: Box<dyn OsDictionaries>,
    /// Downloads the dictionaries of the languages that the OS has no dictionary for, on an OS that
    /// has few of them.
    pub dictionary_hub: Option<Box<dyn DictionaryHub>>,
    /// Finds, downloads and installs the newest release of TypeAlike for the About pane.
    pub releases: Box<dyn ReleaseFeed>,
    /// The third-party libraries of the build, which the About pane lists with their licenses.
    pub libraries: &'static [Library],
}

impl fmt::Debug for Services {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Services").finish_non_exhaustive()
    }
}

/// Returns a sink that gives each event to `deliver`, wrapped by `wrap`.
pub(super) fn delivering_sink<Event: 'static>(
    deliver: &DeliverySink,
    wrap: fn(Event) -> Delivery,
) -> Arc<dyn Fn(Event) + Send + Sync> {
    let deliver = Arc::clone(deliver);
    Arc::new(move |event| deliver(wrap(event)))
}
