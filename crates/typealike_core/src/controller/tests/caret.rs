//! Tests of the ghost text, the badge and the fixes at the caret.

use super::harness::{
    Call, Harness, MADE_UP_WORD, completed, field_around, fixed, ghost_at_caret, hidden_ghost_and_badge,
    loaded,
};
use crate::controller::{
    CARET_MOVE_CHECK_DELAY, CARET_SETTLE_DELAY, CaretLook, CaretRead, Delivery, FocusChange, Ghost,
    GhostPlacement, Timer,
};
use crate::desktop::{AfterCaret, Field};
use crate::engine::WordFix;
use crate::inference::{InferenceError, InferenceEvent, InferenceRequest};
use crate::key::{Disposition, Key, Press};
use crate::settings::{Settings, SuggestionDisplay};
use crate::test_support::{active_badge, suggestion_request};

#[test]
fn key_in_open_field_requests_suggestion() {
    let mut harness = Harness::new(Settings::default());

    harness.press(Key::Char('B'));

    assert!(matches!(
        harness.last_request(),
        Some(InferenceRequest::Complete { .. })
    ));
}

#[test]
fn typed_key_passes() {
    let mut harness = Harness::new(Settings::default());

    let disposition = harness.press(Key::Char('B'));

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn key_in_open_field_hides_ghost_and_waits_for_caret() {
    let mut harness = Harness::new(Settings::default());

    harness.press(Key::Char('B'));

    assert_eq!(
        harness.take_calls(),
        vec![
            Call::StopTimer(Timer::CaretSettle),
            Call::HideGhost,
            Call::StartTimer(Timer::CaretSettle, CARET_SETTLE_DELAY),
        ]
    );
}

#[test]
fn settled_caret_shows_badge() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(None, Some(active_badge()))]
    );
}

#[test]
fn completion_during_caret_wait_keeps_timer() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.take_calls();

    harness.answer(completed("est"));

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn completion_of_a_replaced_request_keeps_timer_for_the_newest_completion() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.press(Key::Char('e'));
    harness.answer(InferenceEvent::Completed(Err(InferenceError::Superseded)));
    harness.take_calls();

    harness.answer(completed("st"));

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn completion_without_suggestion_during_caret_wait_keeps_timer_for_the_badge() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.take_calls();

    harness.answer(completed(""));

    assert_eq!(harness.take_calls(), vec![Call::HideGhost]);
}

/// Returns a harness whose completion ended before "e", the key that the user typed after "B" while
/// it ran, so that the completion asked again during the caret wait.
fn harness_that_asked_again_during_caret_wait() -> Harness {
    let mut harness = Harness::new(Settings {
        suggestion_display: SuggestionDisplay::WhenComplete,
        ..Settings::default()
    });
    harness.press(Key::Char('B'));
    harness.answer(InferenceEvent::Partial("est regards".to_owned()));
    harness.press(Key::Char('e'));
    harness.take_calls();
    harness.answer(completed("e"));
    harness
}

#[test]
fn completion_that_asks_again_during_caret_wait_keeps_timer() {
    let mut harness = harness_that_asked_again_during_caret_wait();

    let calls = harness.take_calls();

    assert_eq!(calls, vec![Call::HideGhost]);
}

#[test]
fn completion_that_asks_again_during_caret_wait_submits_the_new_request() {
    let harness = harness_that_asked_again_during_caret_wait();

    let request = harness.last_request();

    assert_eq!(request, Some(suggestion_request(&[], "Be")));
}

#[test]
fn answer_of_a_request_that_a_completion_asked_again_keeps_timer() {
    let mut harness = harness_that_asked_again_during_caret_wait();
    harness.take_calls();

    harness.answer(completed("st"));

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn completion_shows_when_caret_settles() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.answer(completed("est"));
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(active_badge()))]
    );
}

#[test]
fn completion_shows_up_to_a_word_that_the_dictionary_does_not_know() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.answer(completed(&format!("est {MADE_UP_WORD} today")));
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(active_badge()))]
    );
}

#[test]
fn completion_after_caret_settled_shows_at_once() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.settle_caret();
    harness.take_calls();

    harness.answer(completed("est"));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(active_badge()))]
    );
}

#[test]
fn completion_that_does_not_show_after_caret_settled_checks_the_caret_again() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.settle_caret();
    harness.shell.is_ghost_shown = false;
    harness.take_calls();

    harness.answer(completed("est"));

    assert_eq!(
        harness.take_calls(),
        vec![
            ghost_at_caret(Some("est"), Some(active_badge())),
            Call::StartTimer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY),
        ]
    );
}

#[test]
fn partial_completion_after_caret_settled_shows_at_once_when_words_show_early() {
    let settings = Settings {
        suggestion_display: SuggestionDisplay::WordByWord,
        ..Settings::default()
    };
    let mut harness = Harness::new(settings);
    harness.press(Key::Char('B'));
    harness.settle_caret();
    harness.take_calls();

    harness.answer(InferenceEvent::Partial("est regards".to_owned()));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(active_badge()))]
    );
}

#[test]
fn load_answer_for_suggestion_changes_nothing() {
    let mut harness = Harness::new(Settings::default());

    harness
        .controller
        .on_delivery(Delivery::Suggestion(loaded()), &mut harness.shell);

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn accept_swallows_key() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards");

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Swallow);
}

#[test]
fn accept_types_text() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards");

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::ReplaceText(0, "est".to_owned()))
    );
}

#[test]
fn accept_shows_rest_when_caret_settles() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards");
    harness.press(Key::Tab);
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some(" regards"), Some(active_badge()))]
    );
}

#[test]
fn tab_on_the_last_part_submits_the_request_for_the_next_suggestion() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");

    harness.press(Key::Tab);

    assert_eq!(harness.last_request(), Some(suggestion_request(&[], "Best")));
}

#[test]
fn tab_while_the_rest_waits_for_the_caret_types_its_next_part() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards to you");
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;
    harness.take_calls();

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::ReplaceText(0, " regards".to_owned()))
    );
}

#[test]
fn tab_while_the_rest_waits_for_the_caret_swallows_the_key() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards to you");
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Swallow);
}

#[test]
fn tab_after_the_rest_did_not_show_at_the_caret_passes_to_the_app() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards to you");
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;
    harness.run_out_caret_checks();
    harness.settle_caret();

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn tab_while_a_new_suggestion_waits_for_the_caret_passes_to_the_app() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.answer(completed("est regards"));
    harness.shell.is_ghost_shown = false;

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn ghost_that_does_not_show_after_a_key_checks_the_caret_again() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards");
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![
            ghost_at_caret(Some(" regards"), Some(active_badge())),
            Call::StartTimer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY),
        ]
    );
}

#[test]
fn ghost_shows_at_any_caret_after_the_last_check() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards");
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;
    harness.run_out_caret_checks();
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some(" regards"), Some(active_badge()))]
    );
}

#[test]
fn ghost_text_goes_below_the_caret_while_text_follows_the_caret_on_its_line() {
    let texts_after = ["", "\nSee you", " on Friday."];

    let placements = texts_after.map(|text_after| {
        let mut harness = Harness::new(Settings::default());
        harness.shell.surroundings = field_around("Let's meet", text_after);
        harness.press(Key::Char(' '));
        harness.answer(completed("tomorrow"));
        harness.settle_caret();
        harness.shell.ghost_view.map(|view| view.placement)
    });

    assert_eq!(
        placements,
        [
            Some(GhostPlacement::AfterCaret),
            Some(GhostPlacement::AfterCaret),
            Some(GhostPlacement::BelowCaret),
        ]
    );
}

/// A harness whose session read `text_after` after "Let's meet" at its start, and whose look at
/// the caret sees `after`. It waits for the look to show "tomorrow".
fn harness_whose_look_sees(text_after: &str, after: AfterCaret) -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = field_around("Let's meet", text_after);
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        after,
        ..CaretRead::default()
    });
    harness.press(Key::Char(' '));
    harness.answer(completed("tomorrow"));
    harness
}

#[test]
fn ghost_text_goes_after_the_caret_when_the_look_sees_that_the_text_after_it_is_gone() {
    let mut harness = harness_whose_look_sees("Try \"create a util\"", AfterCaret::Nothing);

    harness.settle_caret();

    assert_eq!(
        harness.shell.ghost_view.map(|view| view.placement),
        Some(GhostPlacement::AfterCaret)
    );
}

#[test]
fn ghost_text_goes_below_the_caret_when_the_look_sees_text_that_the_session_did_not_read() {
    let mut harness = harness_whose_look_sees("", AfterCaret::Text);

    harness.settle_caret();

    assert_eq!(
        harness.shell.ghost_view.map(|view| view.placement),
        Some(GhostPlacement::BelowCaret)
    );
}

#[test]
fn ghost_text_hides_while_the_look_sees_selected_text_after_the_caret() {
    let mut harness = harness_whose_look_sees("", AfterCaret::Selection);
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![
            ghost_at_caret(None, Some(active_badge())),
            Call::StartTimer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY),
        ]
    );
}

#[test]
fn ghost_text_shows_when_the_selected_text_after_the_caret_goes_away() {
    let mut harness = harness_whose_look_sees("", AfterCaret::Selection);
    harness.shell.caret_look = CaretLook::Later;
    harness.settle_caret();
    harness.answer_caret(CaretRead {
        after: AfterCaret::Selection,
        ..CaretRead::default()
    });
    harness.settle_caret();
    harness.take_calls();

    harness.answer_caret(CaretRead {
        after: AfterCaret::Nothing,
        ..CaretRead::default()
    });

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("tomorrow"), Some(active_badge()))]
    );
}

#[test]
fn ghost_view_gets_the_word_before_the_caret() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('H'));
    harness.answer(completed("ello there"));

    harness.settle_caret();

    assert_eq!(
        harness.shell.ghost_view.map(|view| view.typed_word).as_deref(),
        Some("H")
    );
}

fn harness_showing_a_fix() -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.answer(fixed("B", "Be", "st regards"));
    harness
}

#[test]
fn fix_shows_when_caret_settles() {
    let mut harness = harness_showing_a_fix();
    harness.take_calls();

    harness.settle_caret();

    let fix = WordFix {
        typo: "B".to_owned(),
        word: "Best".to_owned(),
        rest: " regards".to_owned(),
    };
    assert_eq!(
        harness.take_calls(),
        vec![Call::RenderAtCaret {
            ghost: Some(Ghost::Fix(fix)),
            badge: Some(active_badge()),
        }]
    );
}

#[test]
fn fix_after_caret_settled_shows_at_once() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.settle_caret();
    harness.take_calls();

    harness.answer(fixed("B", "Be", "st regards"));

    let fix = WordFix {
        typo: "B".to_owned(),
        word: "Best".to_owned(),
        rest: " regards".to_owned(),
    };
    assert_eq!(
        harness.take_calls(),
        vec![Call::RenderAtCaret {
            ghost: Some(Ghost::Fix(fix)),
            badge: Some(active_badge()),
        }]
    );
}

#[test]
fn tab_on_a_fix_replaces_the_typo() {
    let mut harness = harness_showing_a_fix();
    harness.settle_caret();
    harness.take_calls();

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::ReplaceText(1, "Best".to_owned()))
    );
}

#[test]
fn tab_on_a_fix_swallows_the_key() {
    let mut harness = harness_showing_a_fix();
    harness.settle_caret();

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Swallow);
}

/// Returns a harness that shows the fix of "B" kept through a typed space, in a field where the app
/// changed the text before the caret to `before_caret`.
fn harness_showing_a_kept_fix_in_a_field_with(before_caret: &str) -> Harness {
    let mut harness = harness_showing_a_fix();
    harness.settle_caret();
    harness.press(Key::Char(' '));
    harness.settle_caret();
    harness.shell.surroundings = field_around(before_caret, "");
    harness.take_calls();
    harness
}

#[test]
fn tab_on_a_kept_fix_that_the_app_changed_swallows_the_key() {
    let mut harness = harness_showing_a_kept_fix_in_a_field_with("Be ");

    let disposition = harness.press(Key::Tab);

    assert_eq!(disposition, Disposition::Swallow);
}

#[test]
fn tab_on_a_kept_fix_that_the_app_changed_types_nothing() {
    let mut harness = harness_showing_a_kept_fix_in_a_field_with("Be ");

    harness.press(Key::Tab);

    assert!(
        !harness
            .take_calls()
            .iter()
            .any(|call| matches!(call, Call::ReplaceText(..)))
    );
}

#[test]
fn untranslated_key_passes() {
    let mut harness = Harness::new(Settings::default());

    let disposition = harness
        .controller
        .on_key_down(None, Press::First, &mut harness.shell);

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn untranslated_key_requests_nothing() {
    let mut harness = Harness::new(Settings::default());

    harness
        .controller
        .on_key_down(None, Press::First, &mut harness.shell);

    assert_eq!(harness.requests(), Vec::new());
}

#[test]
fn focus_change_before_key_hides_ghost_and_badge() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.focus_change = FocusChange::Changed;

    harness
        .controller
        .on_key_down(None, Press::First, &mut harness.shell);

    assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
}

#[test]
fn focus_move_to_another_window_without_a_key_hides_ghost_and_badge() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");
    harness.shell.focus_change = FocusChange::Changed;

    harness.controller.on_focus_moved(&mut harness.shell);

    assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
}

#[test]
fn focus_move_inside_the_same_window_keeps_ghost_and_badge() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");
    harness.take_calls();

    harness.controller.on_focus_moved(&mut harness.shell);

    assert_eq!(harness.take_calls(), []);
}

#[test]
fn key_in_excluded_field_hides_ghost_and_badge() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.field = Field::Excluded;

    harness.press(Key::Char('B'));

    assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
}

#[test]
fn lost_context_hides_ghost_and_badge() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");

    harness.controller.on_context_lost(&mut harness.shell);

    assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
}
