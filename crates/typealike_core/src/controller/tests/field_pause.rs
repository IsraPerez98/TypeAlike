//! Tests the pause of the suggestions in a field after Escape rejects the suggestion that shows.

use super::harness::{Call, Harness};
use crate::controller::{FIELD_PAUSE_DELAY, Timer};
use crate::key::{Disposition, Key};
use crate::settings::Settings;

/// Returns a harness where Escape rejected a suggestion that showed.
fn harness_in_a_paused_field() -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");
    harness.press(Key::Escape);
    harness.take_calls();
    harness
}

#[test]
fn escape_on_a_shown_suggestion_starts_the_field_pause_timer() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");

    harness.press(Key::Escape);

    assert!(
        harness
            .take_calls()
            .contains(&Call::StartTimer(Timer::FieldPause, FIELD_PAUSE_DELAY))
    );
}

#[test]
fn escape_on_a_shown_suggestion_reaches_the_app() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");

    let disposition = harness.press(Key::Escape);

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn key_in_a_paused_field_requests_no_suggestion() {
    let mut harness = harness_in_a_paused_field();
    let requests = harness.requests().len();

    harness.press(Key::Char('e'));

    assert_eq!(harness.requests().len(), requests);
}

#[test]
fn key_after_the_field_pause_timer_requests_a_suggestion() {
    let mut harness = harness_in_a_paused_field();
    harness.controller.on_timer(Timer::FieldPause, &mut harness.shell);
    let requests = harness.requests().len();

    harness.press(Key::Char('e'));

    assert_eq!(harness.requests().len(), requests + 1);
}

#[test]
fn key_after_a_click_requests_a_suggestion_in_the_pause() {
    let mut harness = harness_in_a_paused_field();
    harness.controller.on_context_lost(&mut harness.shell);
    let requests = harness.requests().len();

    harness.press(Key::Char('e'));

    assert_eq!(harness.requests().len(), requests + 1);
}
