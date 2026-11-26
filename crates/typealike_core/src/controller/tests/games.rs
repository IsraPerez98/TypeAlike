//! Tests of the stop while a game is in front.

use super::harness::{Call, Harness, model_file, suggestion_settings, unload_delays};
use crate::app::GAME_UNLOAD_DELAY;
use crate::controller::Timer;
use crate::desktop::GameActivity;
use crate::inference::{Backend, InferenceRequest};
use crate::key::Key;
use crate::settings::{EnergySavingSettings, GameRule, Settings};
use crate::settings_window::energy_saving::GAME;

fn load_request() -> InferenceRequest {
    InferenceRequest::Load {
        file: model_file(),
        backend: Backend::Cpu,
    }
}

fn settings_keeping_suggestions_during_games() -> Settings {
    Settings {
        energy_saving: EnergySavingSettings {
            game: GameRule::KeepSuggestions,
            ..EnergySavingSettings::default()
        },
        ..suggestion_settings()
    }
}

fn harness_during_a_game() -> Harness {
    let mut harness = Harness::new(suggestion_settings());
    harness.change_game(GameActivity::InFront);
    harness
}

/// A harness after a game that stayed in front past its unload delay.
fn harness_after_a_game() -> Harness {
    let mut harness = harness_during_a_game();
    harness.run_out_unload_delay();
    harness.change_game(GameActivity::None);
    harness
}

/// A harness after a game that left the front before its unload delay ran out.
fn harness_after_a_short_game() -> Harness {
    let mut harness = harness_during_a_game();
    harness.change_game(GameActivity::None);
    harness
}

fn is_complete(request: &InferenceRequest) -> bool {
    matches!(request, InferenceRequest::Complete { .. })
}

#[test]
fn game_keeps_the_suggestion_model_during_its_delay() {
    let harness = harness_during_a_game();

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn game_starts_its_unload_delay() {
    let mut harness = Harness::new(suggestion_settings());
    harness.take_calls();

    harness.change_game(GameActivity::InFront);

    assert_eq!(unload_delays(&harness.take_calls()), [GAME_UNLOAD_DELAY]);
}

#[test]
fn game_unloads_the_suggestion_model_after_its_delay() {
    let mut harness = harness_during_a_game();

    harness.run_out_unload_delay();

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn game_keeps_the_suggestion_model_when_the_switch_is_off() {
    let mut harness = Harness::new(settings_keeping_suggestions_during_games());

    harness.change_game(GameActivity::InFront);

    assert_eq!(
        (harness.requests(), unload_delays(&harness.take_calls())),
        (vec![load_request()], vec![])
    );
}

#[test]
fn game_hides_the_ghost_text() {
    let mut harness = Harness::new(suggestion_settings());
    harness.show("est");

    harness.change_game(GameActivity::InFront);

    assert!(harness.take_calls().contains(&Call::HideGhost));
}

#[test]
fn game_shows_the_stop_in_the_tray() {
    let mut harness = Harness::new(suggestion_settings());

    harness.change_game(GameActivity::InFront);

    let tray = harness.controller.tray_view();
    assert_eq!(
        (
            tray.tooltip.as_str(),
            harness.take_calls().contains(&Call::RenderTray(tray.clone()))
        ),
        ("TypeAlike (off while you play a game)", true)
    );
}

#[test]
fn key_during_a_game_asks_for_no_suggestion() {
    let mut harness = harness_during_a_game();
    let earlier = harness.requests().len();

    harness.press(Key::Char('B'));

    assert_eq!(harness.requests().split_off(earlier), []);
}

#[test]
fn end_of_the_game_keeps_the_model_unloaded() {
    let mut harness = harness_during_a_game();
    harness.run_out_unload_delay();

    harness.change_game(GameActivity::None);

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn short_game_keeps_the_model_loaded() {
    let harness = harness_after_a_short_game();

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn end_of_a_short_game_stops_the_unload_delay() {
    let mut harness = harness_during_a_game();
    harness.take_calls();

    harness.change_game(GameActivity::None);

    assert!(harness.take_calls().contains(&Call::StopTimer(Timer::StopUnload)));
}

#[test]
fn key_after_a_short_game_asks_for_a_suggestion_without_a_load() {
    let mut harness = harness_after_a_short_game();
    let earlier = harness.requests().len();

    harness.press(Key::Char('B'));

    let requests = harness.requests().split_off(earlier);
    assert_eq!(
        (requests.first().is_some_and(is_complete), requests.len()),
        (true, 1)
    );
}

#[test]
fn late_unload_delay_after_a_short_game_does_nothing() {
    let mut harness = harness_after_a_short_game();
    harness.take_calls();

    harness.run_out_unload_delay();

    assert_eq!(
        (harness.requests(), harness.take_calls()),
        (vec![load_request()], vec![])
    );
}

#[test]
fn key_after_the_game_loads_the_model_before_its_request() {
    let mut harness = harness_after_a_game();
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
fn same_game_activity_changes_nothing() {
    let mut harness = Harness::new(suggestion_settings());
    harness.take_calls();

    harness.change_game(GameActivity::None);

    assert_eq!(harness.take_calls(), []);
}

#[test]
fn turning_off_the_switch_after_a_game_loads_the_model() {
    let mut harness = harness_after_a_game();

    harness.toggle(GAME, false);

    assert_eq!(harness.last_request(), Some(load_request()));
}

#[test]
fn turning_off_the_switch_during_the_game_delay_keeps_the_model_loaded() {
    let mut harness = harness_during_a_game();

    harness.toggle(GAME, false);

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn turning_on_the_switch_during_a_game_unloads_the_model_after_the_delay() {
    let mut harness = Harness::new(settings_keeping_suggestions_during_games());
    harness.change_game(GameActivity::InFront);
    harness.toggle(GAME, true);

    harness.run_out_unload_delay();

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn game_switch_is_saved() {
    let mut harness = Harness::new(suggestion_settings());

    harness.toggle(GAME, false);

    assert_eq!(
        harness.saved().pop().map(|settings| settings.energy_saving.game),
        Some(GameRule::KeepSuggestions)
    );
}

#[test]
fn resume_after_a_game_loads_the_model() {
    let mut harness = harness_after_a_game();
    harness.toggle_pause();

    harness.toggle_pause();

    assert_eq!(harness.last_request(), Some(load_request()));
}

#[test]
fn resume_during_a_game_after_its_delay_keeps_the_model_unloaded() {
    let mut harness = harness_during_a_game();
    harness.run_out_unload_delay();
    harness.toggle_pause();

    harness.toggle_pause();

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn settled_input_after_the_game_keeps_the_model_unloaded() {
    let mut harness = harness_after_a_game();
    let earlier = harness.requests().len();

    harness.settle_input();

    assert_eq!(harness.requests().split_off(earlier), []);
}

#[test]
fn key_after_settled_input_after_the_game_loads_the_model_before_its_request() {
    let mut harness = harness_after_a_game();
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
fn start_with_a_game_does_not_load_the_suggestion_model() {
    let harness = Harness::with_game(suggestion_settings(), GameActivity::InFront);

    assert_eq!(harness.requests(), []);
}

#[test]
fn start_with_a_game_loads_the_suggestion_model_when_the_switch_is_off() {
    let harness = Harness::with_game(settings_keeping_suggestions_during_games(), GameActivity::InFront);

    assert_eq!(harness.requests(), [load_request()]);
}

#[test]
fn key_after_the_game_of_the_start_loads_the_model_before_its_request() {
    let mut harness = Harness::with_game(suggestion_settings(), GameActivity::InFront);
    harness.change_game(GameActivity::None);

    harness.press(Key::Char('B'));

    let requests = harness.requests();
    assert_eq!(
        (
            requests.first(),
            requests.get(1).is_some_and(is_complete),
            requests.len()
        ),
        (Some(&load_request()), true, 2)
    );
}
