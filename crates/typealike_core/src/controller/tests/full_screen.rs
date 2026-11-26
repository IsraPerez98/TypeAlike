//! Tests of the stop while an app in full screen is in front.

use super::harness::{Call, Harness, completed, model_file, suggestion_settings, unload_delays};
use crate::app::{FULL_SCREEN_UNLOAD_DELAY, GAME_UNLOAD_DELAY};
use crate::controller::Timer;
use crate::desktop::{EnergySaver, FullScreenActivity, GameActivity};
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

fn harness_in_full_screen() -> Harness {
    let mut harness = Harness::new(suggestion_settings());
    harness.change_full_screen(FullScreenActivity::InFront);
    harness
}

/// A harness after an app in full screen that stayed in front past its unload delay.
fn harness_after_full_screen() -> Harness {
    let mut harness = harness_in_full_screen();
    harness.run_out_unload_delay();
    harness.change_full_screen(FullScreenActivity::None);
    harness
}

/// A harness after an app in full screen that left the front before its unload delay ran out.
fn harness_after_a_short_full_screen() -> Harness {
    let mut harness = harness_in_full_screen();
    harness.change_full_screen(FullScreenActivity::None);
    harness
}

fn is_complete(request: &InferenceRequest) -> bool {
    matches!(request, InferenceRequest::Complete { .. })
}

#[test]
fn full_screen_keeps_the_suggestion_model_during_its_delay() {
    let harness = harness_in_full_screen();

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn full_screen_starts_its_unload_delay() {
    let mut harness = Harness::new(suggestion_settings());
    harness.take_calls();

    harness.change_full_screen(FullScreenActivity::InFront);

    assert_eq!(unload_delays(&harness.take_calls()), [FULL_SCREEN_UNLOAD_DELAY]);
}

#[test]
fn full_screen_unloads_the_suggestion_model_after_its_delay() {
    let mut harness = harness_in_full_screen();

    harness.run_out_unload_delay();

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn full_screen_keeps_the_suggestion_model_when_the_switch_is_off() {
    let mut harness = Harness::new(settings_keeping_suggestions_in_full_screen());

    harness.change_full_screen(FullScreenActivity::InFront);

    assert_eq!(
        (harness.requests(), unload_delays(&harness.take_calls())),
        (vec![load_request()], vec![])
    );
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
        ("TypeAlike (off while an app fills the screen)", true)
    );
}

#[test]
fn key_in_full_screen_asks_for_no_suggestion() {
    let mut harness = harness_in_full_screen();
    let earlier = harness.requests().len();

    harness.press(Key::Char('B'));

    assert_eq!(harness.requests().split_off(earlier), []);
}

#[test]
fn end_of_full_screen_keeps_the_model_unloaded() {
    let mut harness = harness_in_full_screen();
    harness.run_out_unload_delay();

    harness.change_full_screen(FullScreenActivity::None);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn short_full_screen_keeps_the_model_loaded() {
    let harness = harness_after_a_short_full_screen();

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn end_of_a_short_full_screen_stops_the_unload_delay() {
    let mut harness = harness_in_full_screen();
    harness.take_calls();

    harness.change_full_screen(FullScreenActivity::None);

    assert!(harness.take_calls().contains(&Call::StopTimer(Timer::StopUnload)));
}

#[test]
fn key_after_a_short_full_screen_asks_for_a_suggestion_without_a_load() {
    let mut harness = harness_after_a_short_full_screen();
    let earlier = harness.requests().len();

    harness.press(Key::Char('B'));

    let requests = harness.requests().split_off(earlier);
    assert_eq!(
        (requests.first().is_some_and(is_complete), requests.len()),
        (true, 1)
    );
}

#[test]
fn settled_input_after_a_short_full_screen_prepares_the_prompt() {
    let mut harness = harness_after_a_short_full_screen();
    let earlier = harness.requests().len();

    harness.settle_input();

    let requests = harness.requests().split_off(earlier);
    assert_eq!(
        (requests.first().is_some_and(is_complete), requests.len()),
        (true, 1)
    );
}

#[test]
fn late_unload_delay_after_a_short_full_screen_does_nothing() {
    let mut harness = harness_after_a_short_full_screen();
    harness.take_calls();

    harness.run_out_unload_delay();

    assert_eq!(
        (harness.requests(), harness.take_calls()),
        (vec![load_request()], vec![])
    );
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
fn turning_off_the_switch_during_the_full_screen_delay_keeps_the_model_loaded() {
    let mut harness = harness_in_full_screen();

    harness.toggle(FULL_SCREEN, false);

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn turning_off_the_switch_during_the_full_screen_delay_stops_the_unload_delay() {
    let mut harness = harness_in_full_screen();
    harness.take_calls();

    harness.toggle(FULL_SCREEN, false);

    assert!(harness.take_calls().contains(&Call::StopTimer(Timer::StopUnload)));
}

#[test]
fn turning_on_the_switch_in_full_screen_keeps_the_model_during_the_delay() {
    let mut harness = Harness::new(settings_keeping_suggestions_in_full_screen());
    harness.change_full_screen(FullScreenActivity::InFront);

    harness.toggle(FULL_SCREEN, true);

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn turning_on_the_switch_in_full_screen_unloads_the_model_after_the_delay() {
    let mut harness = Harness::new(settings_keeping_suggestions_in_full_screen());
    harness.change_full_screen(FullScreenActivity::InFront);
    harness.toggle(FULL_SCREEN, true);

    harness.run_out_unload_delay();

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

#[test]
fn energy_saver_during_the_full_screen_delay_unloads_the_model() {
    let mut harness = harness_in_full_screen();

    harness.change_energy_saver(EnergySaver::On);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn end_of_the_energy_saver_during_the_full_screen_delay_keeps_the_model_unloaded() {
    let mut harness = harness_in_full_screen();
    harness.change_energy_saver(EnergySaver::On);

    harness.change_energy_saver(EnergySaver::Off);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn end_of_the_energy_saver_after_a_short_full_screen_loads_the_model() {
    let mut harness = harness_in_full_screen();
    harness.change_energy_saver(EnergySaver::On);
    harness.change_full_screen(FullScreenActivity::None);

    harness.change_energy_saver(EnergySaver::Off);

    assert_eq!(harness.last_request(), Some(load_request()));
}

#[test]
fn resume_during_the_full_screen_delay_keeps_the_model_unloaded() {
    let mut harness = harness_in_full_screen();
    harness.toggle_pause();

    harness.toggle_pause();

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn resume_in_full_screen_after_its_delay_keeps_the_model_unloaded() {
    let mut harness = harness_in_full_screen();
    harness.run_out_unload_delay();
    harness.toggle_pause();

    harness.toggle_pause();

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn idle_timeout_during_the_full_screen_delay_unloads_the_model() {
    let mut harness = harness_in_full_screen();

    harness.run_out_idle_timeout();

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn settled_input_after_the_idle_timeout_and_a_short_full_screen_loads_the_model() {
    let mut harness = Harness::new(suggestion_settings());
    harness.run_out_idle_timeout();
    harness.change_full_screen(FullScreenActivity::InFront);
    harness.change_full_screen(FullScreenActivity::None);
    let earlier = harness.requests().len();

    harness.settle_input();

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
fn game_that_joins_the_full_screen_starts_the_unload_delay_of_the_game() {
    let mut harness = harness_in_full_screen();
    harness.take_calls();

    harness.change_game(GameActivity::InFront);

    assert_eq!(unload_delays(&harness.take_calls()), [GAME_UNLOAD_DELAY]);
}

#[test]
fn full_screen_that_joins_the_game_keeps_the_unload_delay_of_the_game() {
    let mut harness = Harness::new(suggestion_settings());
    harness.take_calls();
    harness.change_game(GameActivity::InFront);

    harness.change_full_screen(FullScreenActivity::InFront);

    assert_eq!(unload_delays(&harness.take_calls()), [GAME_UNLOAD_DELAY]);
}

#[test]
fn settled_input_after_full_screen_keeps_the_model_unloaded() {
    let mut harness = harness_after_full_screen();
    let earlier = harness.requests().len();

    harness.settle_input();

    assert_eq!(harness.requests().split_off(earlier), []);
}

#[test]
fn key_after_settled_input_after_full_screen_loads_the_model_before_its_request() {
    let mut harness = harness_after_full_screen();
    harness.settle_input();
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
fn suggestion_shows_after_settled_input_after_full_screen() {
    let mut harness = harness_after_full_screen();
    harness.settle_input();
    harness.settle_input();
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
