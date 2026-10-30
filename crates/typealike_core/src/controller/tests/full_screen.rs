//! Tests of the stop while an app in full screen is in front.

use super::harness::{Call, Harness, model_file, suggestion_settings};
use crate::desktop::FullScreenActivity;
use crate::inference::{Backend, InferenceRequest};
use crate::key::Key;
use crate::settings::{EnergySavingSettings, FullScreenRule, Settings};
use crate::settings_window::energy_saving::FULL_SCREEN;

fn load_request() -> InferenceRequest {
    InferenceRequest::Load {
        file: model_file(),
        backend: Backend::Cpu,
    }
}

fn settings_keeping_suggestions_in_full_screen() -> Settings {
    Settings {
        energy_saving: EnergySavingSettings {
            full_screen: FullScreenRule::KeepSuggestions,
            ..EnergySavingSettings::default()
        },
        ..suggestion_settings()
    }
}

fn harness_after_full_screen() -> Harness {
    let mut harness = Harness::new(suggestion_settings());
    harness.change_full_screen(FullScreenActivity::InFront);
    harness.change_full_screen(FullScreenActivity::None);
    harness
}

fn is_complete(request: &InferenceRequest) -> bool {
    matches!(request, InferenceRequest::Complete { .. })
}

#[test]
fn full_screen_unloads_the_suggestion_model() {
    let mut harness = Harness::new(suggestion_settings());

    harness.change_full_screen(FullScreenActivity::InFront);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn full_screen_keeps_the_suggestion_model_when_the_switch_is_off() {
    let mut harness = Harness::new(settings_keeping_suggestions_in_full_screen());

    harness.change_full_screen(FullScreenActivity::InFront);

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn full_screen_hides_the_ghost_text() {
    let mut harness = Harness::new(suggestion_settings());
    harness.show("est");

    harness.change_full_screen(FullScreenActivity::InFront);

    assert!(harness.take_calls().contains(&Call::HideGhost));
}

#[test]
fn full_screen_shows_the_stop_in_the_tray() {
    let mut harness = Harness::new(suggestion_settings());

    harness.change_full_screen(FullScreenActivity::InFront);

    let tray = harness.controller.tray_view();
    assert_eq!(
        (
            tray.tooltip.as_str(),
            harness.take_calls().contains(&Call::RenderTray(tray.clone()))
        ),
        ("TypeAlike (off in full screen)", true)
    );
}

#[test]
fn end_of_full_screen_keeps_the_model_unloaded() {
    let mut harness = Harness::new(suggestion_settings());
    harness.change_full_screen(FullScreenActivity::InFront);

    harness.change_full_screen(FullScreenActivity::None);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn key_after_full_screen_loads_the_model_before_its_request() {
    let mut harness = harness_after_full_screen();
    let earlier = harness.requests().len();

    harness.press(Key::Char('B'));

    let requests = harness.requests().split_off(earlier);
    assert_eq!(
        (
            requests.first(),
            requests.get(1).is_some_and(is_complete),
            requests.len()
        ),
        (Some(&load_request()), true, 2)
    );
}

#[test]
fn same_full_screen_activity_changes_nothing() {
    let mut harness = Harness::new(suggestion_settings());
    harness.take_calls();

    harness.change_full_screen(FullScreenActivity::None);

    assert_eq!(harness.take_calls(), []);
}

#[test]
fn turning_off_the_switch_after_full_screen_loads_the_model() {
    let mut harness = harness_after_full_screen();

    harness.toggle(FULL_SCREEN, false);

    assert_eq!(harness.last_request(), Some(load_request()));
}

#[test]
fn turning_on_the_switch_in_full_screen_unloads_the_model() {
    let mut harness = Harness::new(settings_keeping_suggestions_in_full_screen());
    harness.change_full_screen(FullScreenActivity::InFront);

    harness.toggle(FULL_SCREEN, true);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn full_screen_switch_is_saved() {
    let mut harness = Harness::new(suggestion_settings());

    harness.toggle(FULL_SCREEN, false);

    assert_eq!(
        harness
            .saved()
            .pop()
            .map(|settings| settings.energy_saving.full_screen),
        Some(FullScreenRule::KeepSuggestions)
    );
}
