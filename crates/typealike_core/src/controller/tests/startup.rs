//! Tests of the start of the controller: the suggestion model, the backend and the saved settings.

use super::harness::{Harness, model_file, suggestion_settings};
use crate::controller::BackendChoice;
use crate::inference::{Backend, InferenceRequest};
use crate::settings::Settings;
use crate::settings_window::general::BACKEND;
use crate::settings_window::view::{Page, SettingsView};

fn models_page(view: &SettingsView) -> &Page {
    view.pages
        .iter()
        .find(|page| page.title == "Models")
        .expect("the settings window shows the Models pane")
}

#[test]
fn new_controller_loads_saved_suggestion_model() {
    let harness = Harness::new(suggestion_settings());

    assert_eq!(
        harness.requests(),
        vec![InferenceRequest::Load {
            file: model_file(),
            backend: Backend::Cpu,
        }]
    );
}

#[test]
fn user_backend_choice_loads_suggestion_model_on_saved_backend() {
    let settings = Settings {
        backend: Backend::Vulkan,
        ..suggestion_settings()
    };

    let harness = Harness::with_backend_choice(settings, BackendChoice::User);

    assert_eq!(
        harness.requests(),
        vec![InferenceRequest::Load {
            file: model_file(),
            backend: Backend::Vulkan,
        }]
    );
}

#[test]
fn user_backend_choice_shows_backend_menu() {
    let mut harness = Harness::with_backend_choice(Settings::default(), BackendChoice::User);

    let view = harness.shown_settings();

    assert!(models_page(&view).row(BACKEND).is_some());
}

#[test]
fn platform_backend_choice_shows_no_backend_menu() {
    let mut harness =
        Harness::with_backend_choice(Settings::default(), BackendChoice::Platform(Backend::Metal));

    let view = harness.shown_settings();

    assert_eq!(models_page(&view).row(BACKEND), None);
}

#[test]
fn platform_backend_choice_loads_suggestion_model_on_its_backend() {
    let harness =
        Harness::with_backend_choice(suggestion_settings(), BackendChoice::Platform(Backend::Metal));

    assert_eq!(
        harness.requests(),
        vec![InferenceRequest::Load {
            file: model_file(),
            backend: Backend::Metal,
        }]
    );
}

#[test]
fn platform_backend_choice_saves_its_backend_in_place_of_the_saved_one() {
    let harness = Harness::with_backend_choice(Settings::default(), BackendChoice::Platform(Backend::Metal));

    assert_eq!(
        harness.saved(),
        vec![Settings {
            backend: Backend::Metal,
            ..Settings::default()
        }]
    );
}

#[test]
fn user_backend_choice_saves_the_default_in_place_of_a_backend_without_a_menu_entry() {
    let settings = Settings {
        backend: Backend::Metal,
        ..Settings::default()
    };

    let harness = Harness::with_backend_choice(settings, BackendChoice::User);

    assert_eq!(harness.saved(), vec![Settings::default()]);
}

#[test]
fn settings_that_start_unchanged_are_not_saved() {
    let settings = Settings {
        backend: Backend::Metal,
        ..Settings::default()
    };

    let harness = Harness::with_backend_choice(settings, BackendChoice::Platform(Backend::Metal));

    assert_eq!(harness.saved(), Vec::new());
}
