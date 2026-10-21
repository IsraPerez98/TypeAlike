//! Tests of the ghost text, the badge and the fixes at the caret.

use super::harness::{
    ACTIVE_BADGE, Call, Harness, completed, field_around, ghost_at_caret, hidden_ghost_and_badge, loaded,
    suggestion_request,
};
use crate::controller::{
    CARET_MOVE_CHECK_DELAY, CARET_SETTLE_DELAY, Delivery, FocusChange, Ghost, GhostPlacement, Timer,
};
use crate::desktop::Field;
use crate::engine::WordFix;
use crate::inference::{Completion, Correction, InferenceEvent, InferenceRequest};
use crate::key::{Disposition, Key};
use crate::settings::{Settings, SuggestionDisplay};

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
        vec![ghost_at_caret(None, Some(ACTIVE_BADGE))]
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
fn completion_shows_when_caret_settles() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.answer(completed("est"));
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(ACTIVE_BADGE))]
    );
}

#[test]
fn completion_after_caret_settled_waits_for_caret_again() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.settle_caret();
    harness.take_calls();

    harness.answer(completed("est"));

    assert_eq!(
        harness.take_calls(),
        vec![Call::StartTimer(Timer::CaretSettle, CARET_SETTLE_DELAY)]
    );
}

#[test]
fn partial_completion_waits_for_caret_when_words_show_early() {
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
        vec![Call::StartTimer(Timer::CaretSettle, CARET_SETTLE_DELAY)]
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

    assert!(harness.take_calls().contains(&Call::TypeText("est".to_owned())));
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
        vec![ghost_at_caret(Some(" regards"), Some(ACTIVE_BADGE))]
    );
}

#[test]
fn tab_on_the_last_part_submits_the_request_for_the_next_suggestion() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");

    harness.press(Key::Tab);

    assert_eq!(harness.last_request(), Some(suggestion_request("Best")));
}

#[test]
fn tab_while_the_rest_waits_for_the_caret_types_its_next_part() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards to you");
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;
    harness.take_calls();

    let disposition = harness.press(Key::Tab);

    let calls = harness.take_calls();
    assert!(
        disposition == Disposition::Swallow && calls.contains(&Call::TypeText(" regards".to_owned())),
        "unexpected {disposition:?} {calls:?}"
    );
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
fn ghost_after_a_key_shows_only_at_a_caret_that_moved() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards");
    harness.press(Key::Tab);

    harness.settle_caret();

    assert!(harness.shell.ghost_view.expect("a ghost view").is_after_edit);
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
            ghost_at_caret(Some(" regards"), Some(ACTIVE_BADGE)),
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
        vec![ghost_at_caret(Some(" regards"), Some(ACTIVE_BADGE))]
    );
    assert!(!harness.shell.ghost_view.expect("a ghost view").is_after_edit);
}

fn fixed(typed: &str, fixed: &str, text: &str) -> InferenceEvent {
    let InferenceEvent::Completed(Ok(completion)) = completed(text) else {
        unreachable!("completed gives a completion");
    };
    InferenceEvent::Completed(Ok(Completion {
        correction: Some(Correction {
            typed: typed.to_owned(),
            fixed: fixed.to_owned(),
        }),
        ..completion
    }))
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
            badge: Some(ACTIVE_BADGE),
        }]
    );
}

#[test]
fn tab_on_a_fix_replaces_the_typo_and_swallows_the_key() {
    let mut harness = harness_showing_a_fix();
    harness.settle_caret();
    harness.take_calls();

    let disposition = harness.press(Key::Tab);

    let calls = harness.take_calls();
    assert!(
        disposition == Disposition::Swallow && calls.contains(&Call::ReplaceText(1, "Best".to_owned())),
        "unexpected {disposition:?} {calls:?}"
    );
}

#[test]
fn untranslated_key_passes() {
    let mut harness = Harness::new(Settings::default());

    let disposition = harness.controller.on_key_down(None, &mut harness.shell);

    assert_eq!(disposition, Disposition::Pass);
}

#[test]
fn untranslated_key_requests_nothing() {
    let mut harness = Harness::new(Settings::default());

    harness.controller.on_key_down(None, &mut harness.shell);

    assert_eq!(harness.requests(), Vec::new());
}

#[test]
fn focus_change_before_key_hides_ghost_and_badge() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.focus_change = FocusChange::Changed;

    harness.controller.on_key_down(None, &mut harness.shell);

    assert_eq!(harness.take_calls(), hidden_ghost_and_badge());
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
