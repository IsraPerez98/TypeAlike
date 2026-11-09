//! Tests of the work that the controller does when the input settles.

use super::harness::{Call, Harness, completed, field_around, ghost_at_caret, hidden_ghost_and_badge};
use crate::app::INPUT_SETTLE_DELAY;
use crate::controller::{CARET_SETTLE_DELAY, FocusChange, Timer};
use crate::desktop::{Field, Surroundings};
use crate::inference::{InferenceRequest, TokenCount};
use crate::key::Key;
use crate::settings::{PromptPreparation, Settings, SuggestionPlaces};
use crate::test_support::{MAIL_APP, active_badge, suggestion_app, suggestion_request};

#[test]
fn input_waits_for_input_to_settle() {
    let mut harness = Harness::new(Settings::default());

    harness.controller.on_input(&mut harness.shell);

    assert!(
        harness
            .take_calls()
            .contains(&Call::StartTimer(Timer::InputSettle, INPUT_SETTLE_DELAY))
    );
}

#[test]
fn settled_input_prepares_prompt_when_preparing_on_focus() {
    let settings = Settings {
        prompt_preparation: PromptPreparation::OnFocus,
        ..Settings::default()
    };
    let mut harness = Harness::new(settings);

    harness.settle_input();

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
    let mut harness = Harness::new(Settings {
        prompt_preparation: PromptPreparation::OnFirstKey,
        ..Settings::default()
    });
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
            Some(suggestion_request(&[], "Best regards,\n")),
            vec![ghost_at_caret(Some("Sara Novak"), Some(active_badge()))]
        )
    );
}

#[test]
fn suggestion_after_enter_asks_when_the_caret_settles() {
    let mut harness = harness_after_enter();
    harness.shell.surroundings = field_with_text("Best regards,\n");

    harness.settle_caret();

    assert_eq!(
        harness.last_request(),
        Some(suggestion_request(&[], "Best regards,\n"))
    );
}

#[test]
fn settled_input_hides_the_new_line_suggestion_when_the_app_emptied_the_field() {
    let mut harness = harness_after_enter();
    harness.shell.surroundings = field_with_text("Best regards,\n");
    harness.settle_caret();
    harness.answer(completed("Sara Novak"));
    harness.shell.surroundings = field_with_text("");
    harness.take_calls();

    harness.settle_input();

    assert!(harness.take_calls().contains(&Call::HideGhost));
}

#[test]
fn suggestion_after_enter_and_a_pause_shows_at_once_when_the_caret_settled() {
    let mut harness = harness_after_enter();
    harness.settle_caret();
    harness.shell.surroundings = field_with_text("Best regards,\n");
    harness.settle_input();
    harness.take_calls();

    harness.answer(completed("Sara Novak"));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("Sara Novak"), Some(active_badge()))]
    );
}

#[test]
fn suggestion_after_enter_and_a_pause_waits_for_the_caret_to_settle_after_enter() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = field_with_text("Best regards");
    harness.press(Key::Char(','));
    harness.answer(completed(""));
    harness.settle_caret();
    harness.press(Key::Enter);
    harness.shell.surroundings = field_with_text("Best regards,\n");
    harness.settle_input();
    harness.take_calls();

    harness.answer(completed("Sara Novak"));

    assert_eq!(
        harness.take_calls(),
        vec![Call::StartTimer(Timer::CaretSettle, CARET_SETTLE_DELAY)]
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
        Some(suggestion_request(&[], "Best regards,\nS"))
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

    harness.settle_input();

    assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
}

#[test]
fn settled_input_in_a_field_without_text_wakes_the_field_text() {
    let mut harness = Harness::new(Settings::default());

    harness.settle_input();

    assert_eq!(harness.shell.field_text_wakes, 1);
}

#[test]
fn settled_input_in_a_field_with_text_does_not_wake_the_field_text() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = field_with_text("Best regards");

    harness.settle_input();

    assert_eq!(harness.shell.field_text_wakes, 0);
}

#[test]
fn settled_input_in_an_app_where_suggestions_are_off_does_not_wake_the_field_text() {
    let mut harness = Harness::new(Settings {
        suggestion_places: SuggestionPlaces {
            apps: vec![suggestion_app(MAIL_APP, "Mail", false)],
            ..SuggestionPlaces::default()
        },
        ..Settings::default()
    });
    harness.shell.surroundings = Surroundings {
        app_id: Some(MAIL_APP.to_owned()),
        ..Surroundings::default()
    };

    harness.settle_input();

    assert_eq!(harness.shell.field_text_wakes, 0);
}

#[test]
fn settled_input_in_an_excluded_field_does_not_wake_the_field_text() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.field = Field::Excluded;

    harness.settle_input();

    assert_eq!(harness.shell.field_text_wakes, 0);
}

#[test]
fn settled_input_while_suggestions_pause_does_not_wake_the_field_text() {
    let mut harness = Harness::new(Settings::default());
    harness.toggle_pause();

    harness.settle_input();

    assert_eq!(harness.shell.field_text_wakes, 0);
}

/// A harness after the user typed a space after "I recieve", and the model suggested "the letter".
fn harness_with_suggestion_after_a_typo() -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = field_with_text("I recieve");
    harness.press(Key::Char(' '));
    harness.answer(completed("the letter"));
    harness
}

#[test]
fn settled_input_asks_again_for_the_word_that_the_app_corrected() {
    let mut harness = harness_with_suggestion_after_a_typo();
    harness.shell.surroundings = field_with_text("I receive ");

    harness.settle_input();

    assert_eq!(
        harness.last_request(),
        Some(suggestion_request(&[], "I receive "))
    );
}

#[test]
fn settled_input_asks_again_when_the_session_takes_a_field_with_text() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('H'));
    harness.answer(completed("ello there"));
    harness.shell.surroundings = field_with_text("Dear Ana,\nH");

    harness.settle_input();

    assert_eq!(
        harness.last_request(),
        Some(suggestion_request(&[], "Dear Ana,\nH"))
    );
}

#[test]
fn settled_input_hides_the_suggestion_for_the_word_that_the_app_corrected() {
    let mut harness = harness_with_suggestion_after_a_typo();
    harness.shell.surroundings = field_with_text("I receive ");
    harness.take_calls();

    harness.settle_input();

    assert!(harness.take_calls().contains(&Call::HideGhost));
}

#[test]
fn settled_input_asks_for_no_suggestion_when_the_field_shows_the_typed_text() {
    let mut harness = harness_with_suggestion_after_a_typo();
    harness.shell.surroundings = field_with_text("I recieve ");
    let requests_before_settle = harness.requests().len();

    harness.settle_input();

    assert_eq!(harness.requests().len(), requests_before_settle);
}

#[test]
fn settled_input_after_a_key_that_continues_the_suggestion_does_not_ask_again() {
    let mut harness = harness_with_suggestion_after_a_typo();
    harness.press(Key::Char('t'));
    harness.shell.surroundings = field_with_text("I receive t");
    let requests_before_settle = harness.requests().len();

    harness.settle_input();

    assert_eq!(harness.requests().len(), requests_before_settle);
}

#[test]
fn settled_input_after_a_completion_without_a_suggestion_does_not_ask_again() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = field_with_text("I recieve");
    harness.press(Key::Char(' '));
    harness.answer(completed(""));
    harness.shell.surroundings = field_with_text("I receive ");
    let requests_before_settle = harness.requests().len();

    harness.settle_input();

    assert_eq!(harness.requests().len(), requests_before_settle);
}
