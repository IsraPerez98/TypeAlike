//! Tests of the work that the controller does when the input settles.

use super::harness::{
    ACTIVE_BADGE, Call, Harness, completed, field_around, ghost_at_caret, hidden_ghost_and_badge,
    suggestion_request,
};
use crate::app::INPUT_SETTLE_DELAY;
use crate::controller::{FocusChange, Timer};
use crate::desktop::Surroundings;
use crate::inference::{InferenceRequest, TokenCount};
use crate::key::Key;
use crate::settings::{PromptPreparation, Settings};

#[test]
fn input_waits_for_input_to_settle() {
    let mut harness = Harness::new(Settings::default());

    harness.controller.on_input(&mut harness.shell);

    assert_eq!(
        harness.take_calls(),
        vec![Call::StartTimer(Timer::InputSettle, INPUT_SETTLE_DELAY)]
    );
}

#[test]
fn settled_input_prepares_prompt_when_preparing_on_focus() {
    let settings = Settings {
        prompt_preparation: PromptPreparation::OnFocus,
        ..Settings::default()
    };
    let mut harness = Harness::new(settings);

    harness
        .controller
        .on_timer(Timer::InputSettle, &mut harness.shell);

    assert!(matches!(
        harness.last_request(),
        Some(InferenceRequest::Complete {
            max_tokens: TokenCount(0),
            ..
        })
    ));
}

fn field_with_text(before_caret: &str) -> Surroundings {
    field_around(before_caret, "")
}

/// A harness after the user typed "Best regards," and pressed Enter.
fn harness_after_enter() -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = field_with_text("Best regards");
    harness.press(Key::Char(','));
    harness.answer(completed(""));
    harness.press(Key::Enter);
    harness
}

#[test]
fn suggestion_shows_after_enter_and_a_pause() {
    let mut harness = harness_after_enter();
    harness.shell.surroundings = field_with_text("Best regards,\n");
    harness.settle_input();
    harness.answer(completed("Sara Novak"));
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        (harness.last_request(), harness.take_calls()),
        (
            Some(suggestion_request("Best regards,\n")),
            vec![ghost_at_caret(Some("Sara Novak"), Some(ACTIVE_BADGE))]
        )
    );
}

#[test]
fn key_before_the_pause_after_enter_continues_the_new_line() {
    let mut harness = harness_after_enter();
    harness.shell.surroundings = field_with_text("Best regards,\n");
    harness.press(Key::Char('S'));

    harness.settle_input();

    assert_eq!(
        harness.last_request(),
        Some(suggestion_request("Best regards,\nS"))
    );
}

#[test]
fn enter_that_sends_the_text_requests_no_suggestion() {
    let mut harness = harness_after_enter();
    harness.shell.surroundings = field_with_text("");
    let requests_before_settle = harness.requests().len();

    harness.settle_input();

    assert_eq!(harness.requests().len(), requests_before_settle);
}

#[test]
fn settled_input_after_focus_change_hides_ghost_and_badge() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.focus_change = FocusChange::Changed;

    harness
        .controller
        .on_timer(Timer::InputSettle, &mut harness.shell);

    assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
}
