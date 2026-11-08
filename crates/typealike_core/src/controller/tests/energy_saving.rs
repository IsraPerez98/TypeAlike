//! Tests of the stops that save energy: the idle timeout and the energy saver of the computer.

use std::time::Duration;

use super::harness::{Call, Harness, completed, model_file, suggestion_settings};
use crate::app::INPUT_SETTLE_DELAY;
use crate::controller::Timer;
use crate::desktop::EnergySaver;
use crate::inference::{Backend, InferenceRequest};
use crate::key::Key;
use crate::settings::{
    EnergySaverRule, EnergySavingSettings, IdleTimeout, Settings, SuggestionApp, SuggestionPlaces,
};
use crate::settings_window::energy_saving::{ENERGY_SAVER, IDLE_TIMEOUT};

const THIRTY_MINUTES: Duration = Duration::from_mins(30);

fn load_request() -> InferenceRequest {
    InferenceRequest::Load {
        file: model_file(),
        backend: Backend::Cpu,
    }
}

fn settings_with(energy_saving: EnergySavingSettings) -> Settings {
    Settings {
        energy_saving,
        ..suggestion_settings()
    }
}

fn settings_keeping_suggestions_in_energy_saver() -> Settings {
    settings_with(EnergySavingSettings {
        energy_saver: EnergySaverRule::KeepSuggestions,
        ..EnergySavingSettings::default()
    })
}

fn settings_without_idle_timeout() -> Settings {
    settings_with(EnergySavingSettings {
        idle_timeout: IdleTimeout::Off,
        ..EnergySavingSettings::default()
    })
}

/// Requests that came after the requests of `count` earlier calls.
fn requests_after(harness: &Harness, count: usize) -> Vec<InferenceRequest> {
    harness.requests().split_off(count)
}

fn is_complete(request: &InferenceRequest) -> bool {
    matches!(request, InferenceRequest::Complete { .. })
}

#[test]
fn start_in_energy_saver_does_not_load_the_suggestion_model() {
    let harness = Harness::with_energy_saver(suggestion_settings(), EnergySaver::On);

    assert_eq!(harness.requests(), []);
}

#[test]
fn start_in_energy_saver_loads_the_suggestion_model_when_the_switch_is_off() {
    let harness = Harness::with_energy_saver(settings_keeping_suggestions_in_energy_saver(), EnergySaver::On);

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn energy_saver_unloads_the_suggestion_model() {
    let mut harness = Harness::new(suggestion_settings());

    harness.change_energy_saver(EnergySaver::On);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn energy_saver_keeps_the_suggestion_model_when_the_switch_is_off() {
    let mut harness = Harness::new(settings_keeping_suggestions_in_energy_saver());

    harness.change_energy_saver(EnergySaver::On);

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn energy_saver_hides_the_ghost_text() {
    let mut harness = Harness::new(suggestion_settings());
    harness.show("est");

    harness.change_energy_saver(EnergySaver::On);

    assert!(harness.take_calls().contains(&Call::HideGhost));
}

#[test]
fn energy_saver_shows_the_stop_in_the_tray() {
    let mut harness = Harness::new(suggestion_settings());

    harness.change_energy_saver(EnergySaver::On);

    let tray = harness.controller.tray_view();
    assert_eq!(
        (
            tray.tooltip.as_str(),
            harness.take_calls().contains(&Call::RenderTray(tray.clone()))
        ),
        ("TypeAlike (off while the computer saves energy)", true)
    );
}

#[test]
fn key_in_energy_saver_asks_for_no_suggestion() {
    let mut harness = Harness::with_energy_saver(suggestion_settings(), EnergySaver::On);

    harness.press(Key::Char('B'));

    assert_eq!(harness.requests(), []);
}

#[test]
fn end_of_the_energy_saver_loads_the_suggestion_model_again() {
    let mut harness = Harness::with_energy_saver(suggestion_settings(), EnergySaver::On);

    harness.change_energy_saver(EnergySaver::Off);

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn end_of_the_energy_saver_after_the_idle_timeout_keeps_the_model_unloaded() {
    let mut harness = Harness::with_energy_saver(suggestion_settings(), EnergySaver::On);
    harness.run_out_idle_timeout();

    harness.change_energy_saver(EnergySaver::Off);

    assert_eq!(harness.requests(), []);
}

#[test]
fn resume_in_energy_saver_keeps_the_model_unloaded() {
    let mut harness = Harness::with_energy_saver(suggestion_settings(), EnergySaver::On);
    harness.toggle_pause();

    harness.toggle_pause();

    assert_eq!(harness.requests(), []);
}

#[test]
fn turning_off_the_switch_in_energy_saver_loads_the_model() {
    let mut harness = Harness::with_energy_saver(suggestion_settings(), EnergySaver::On);

    harness.toggle(ENERGY_SAVER, false);

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn turning_on_the_switch_in_energy_saver_unloads_the_model() {
    let mut harness =
        Harness::with_energy_saver(settings_keeping_suggestions_in_energy_saver(), EnergySaver::On);

    harness.toggle(ENERGY_SAVER, true);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn input_starts_the_idle_timer() {
    let mut harness = Harness::new(suggestion_settings());

    harness.controller.on_input(&mut harness.shell);

    assert_eq!(
        harness.take_calls(),
        [
            Call::StartTimer(Timer::InputSettle, INPUT_SETTLE_DELAY),
            Call::StartTimer(Timer::Idle, THIRTY_MINUTES),
        ]
    );
}

#[test]
fn input_starts_no_idle_timer_when_the_timeout_is_off() {
    let mut harness = Harness::new(settings_without_idle_timeout());

    harness.controller.on_input(&mut harness.shell);

    assert_eq!(
        harness.take_calls(),
        [Call::StartTimer(Timer::InputSettle, INPUT_SETTLE_DELAY)]
    );
}

#[test]
fn idle_timeout_unloads_the_suggestion_model() {
    let mut harness = Harness::new(suggestion_settings());

    harness.run_out_idle_timeout();

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn idle_timeout_hides_the_ghost_text_and_the_badge() {
    let mut harness = Harness::new(suggestion_settings());
    harness.show("est");

    harness.run_out_idle_timeout();

    let calls = harness.take_calls();
    assert_eq!(
        (calls.contains(&Call::HideGhost), calls.contains(&Call::HideBadge)),
        (true, true)
    );
}

#[test]
fn key_after_the_idle_timeout_loads_the_model_before_its_request() {
    let mut harness = Harness::new(suggestion_settings());
    harness.run_out_idle_timeout();
    let earlier = harness.requests().len();

    harness.press(Key::Char('B'));

    let requests = requests_after(&harness, earlier);
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
fn second_key_after_the_idle_timeout_loads_the_model_no_more() {
    let mut harness = Harness::new(suggestion_settings());
    harness.run_out_idle_timeout();
    harness.press(Key::Char('B'));
    let earlier = harness.requests().len();

    harness.press(Key::Char('e'));

    let requests = requests_after(&harness, earlier);
    assert_eq!(
        (requests.len(), requests.first().is_some_and(is_complete)),
        (1, true)
    );
}

#[test]
fn completion_after_the_reload_shows_its_suggestion() {
    let mut harness = Harness::new(suggestion_settings());
    harness.run_out_idle_timeout();
    harness.press(Key::Char('B'));

    harness.answer(completed("est"));
    harness.settle_caret();

    assert!(
        harness
            .take_calls()
            .iter()
            .any(|call| matches!(call, Call::RenderAtCaret { ghost: Some(_), .. }))
    );
}

#[test]
fn key_in_an_off_app_after_the_idle_timeout_keeps_the_model_unloaded() {
    let settings = Settings {
        suggestion_places: SuggestionPlaces {
            apps: vec![SuggestionApp {
                app_id: "com.apple.mail".to_owned(),
                app_name: "Mail".to_owned(),
                app_path: None,
                is_on: false,
            }],
            ..SuggestionPlaces::default()
        },
        ..suggestion_settings()
    };
    let mut harness = Harness::new(settings);
    harness.focus_text_field_of("com.apple.mail", "Mail");
    harness.run_out_idle_timeout();

    harness.press(Key::Char('B'));

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn picking_another_idle_timeout_starts_the_idle_timer_with_it() {
    let mut harness = Harness::new(suggestion_settings());

    harness.pick(IDLE_TIMEOUT, 1);

    assert!(
        harness
            .take_calls()
            .contains(&Call::StartTimer(Timer::Idle, Duration::from_mins(15)))
    );
}

#[test]
fn turning_off_the_idle_timeout_stops_the_idle_timer() {
    let mut harness = Harness::new(suggestion_settings());

    harness.pick(IDLE_TIMEOUT, 0);

    assert!(harness.take_calls().contains(&Call::StopTimer(Timer::Idle)));
}

#[test]
fn turning_off_the_idle_timeout_loads_the_model_that_it_unloaded() {
    let mut harness = Harness::new(suggestion_settings());
    harness.run_out_idle_timeout();

    harness.pick(IDLE_TIMEOUT, 0);

    assert_eq!(harness.last_request(), Some(load_request()));
}

#[test]
fn picked_idle_timeout_is_saved() {
    let mut harness = Harness::new(suggestion_settings());

    harness.pick(IDLE_TIMEOUT, 3);

    assert_eq!(
        harness
            .saved()
            .pop()
            .map(|settings| settings.energy_saving.idle_timeout),
        Some(IdleTimeout::SixtyMinutes)
    );
}

#[test]
fn resume_ends_the_idle_stop_and_loads_the_model() {
    let mut harness = Harness::new(suggestion_settings());
    harness.toggle_pause();
    harness.run_out_idle_timeout();

    harness.toggle_pause();

    assert_eq!(harness.last_request(), Some(load_request()));
}
