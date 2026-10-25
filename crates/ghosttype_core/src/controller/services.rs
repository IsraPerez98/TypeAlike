//! The services that the controller uses, and the way their answers come back to it.

use std::fmt;
use std::sync::Arc;

use crate::desktop::{EnergySaver, ScreenRead};
use crate::history::WritingHistory;
use crate::hub::{HubEvent, ModelHub};
use crate::inference::{Backend, InferenceEvent, ModelRunner};
use crate::settings::{Autostart, SettingsStore};

/// Who picks the backend that runs each model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendChoice {
    /// The user, in the General pane.
    User,
    /// The platform, which runs each model on this backend.
    Platform(Backend),
}

/// What the platform tells the controller when GhostType starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Startup {
    /// Who picks the backend that runs each model.
    pub backend_choice: BackendChoice,
    /// Whether the energy saver of the computer is on. The platform reports each later change to
    /// [`Controller::on_energy_saver`](super::Controller::on_energy_saver).
    pub energy_saver: EnergySaver,
}

/// The answer of a service. The platform gives it back to the controller on the thread that runs
/// the controller.
#[derive(Debug, Clone, PartialEq)]
pub enum Delivery {
    /// For the Models pane.
    Hub(HubEvent),
    /// For the Playground pane, which also loads the suggestion model.
    Playground(InferenceEvent),
    /// For the ghost text at the caret.
    Suggestion(InferenceEvent),
    /// The answer to a read of the screen that [`Shell::read_screen`](super::Shell::read_screen) started.
    Screen(ScreenRead),
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
    /// Keeps the settings that the user changes in the General pane.
    pub settings_store: Box<dyn SettingsStore>,
    /// Starts GhostType at login when the user turns it on in the General pane.
    pub autostart: Box<dyn Autostart>,
    /// Keeps the earlier writing of the user while the Personalization pane allows it.
    pub history: Box<dyn WritingHistory>,
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
