//! Tests of a scroll: the ghost text and the badge hide, and show again at the caret when the
//! scroll stops, with the same suggestion.

use super::caret_look::{
    FAR_CARET, OLD_CARET, harness_showing_at_the_old_caret, harness_waiting_for_the_look, look_at,
};
use super::harness::{Call, Harness, completed, ghost_at_caret};
use crate::controller::{CARET_SETTLE_DELAY, Timer};
use crate::key::Key;
use crate::settings::Settings;
use crate::test_support::ACTIVE_BADGE;

#[test]
fn scroll_while_the_ghost_shows_hides_the_ghost_and_the_badge_until_the_caret_settles() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");

    harness.scroll();

    assert_eq!(
        harness.take_calls(),
        vec![
            Call::HideGhost,
            Call::HideBadge,
            Call::StartTimer(Timer::CaretSettle, CARET_SETTLE_DELAY),
        ]
    );
}

#[test]
fn ghost_and_badge_show_again_when_the_caret_settles_after_a_scroll() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");
    harness.scroll();
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(ACTIVE_BADGE))]
    );
}

#[test]
fn ghost_shows_again_at_a_caret_that_did_not_move_after_a_scroll() {
    let mut harness = harness_showing_at_the_old_caret("est");
    harness.scroll();
    harness.settle_caret();
    harness.take_calls();

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(ACTIVE_BADGE))]
    );
}

#[test]
fn scroll_that_moves_the_caret_far_from_its_field_keeps_the_ghost_and_the_badge_hidden() {
    let mut harness = harness_showing_at_the_old_caret("est");
    harness.scroll();
    harness.settle_caret();
    harness.take_calls();

    harness.answer_caret(look_at(FAR_CARET));

    assert_eq!(harness.take_calls(), vec![Call::HideGhost, Call::HideBadge]);
}

#[test]
fn look_that_started_before_a_scroll_is_dropped() {
    let mut harness = harness_waiting_for_the_look("est");
    harness.scroll();
    harness.take_calls();

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn completion_during_a_scroll_waits_for_the_caret_to_settle() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));
    harness.settle_caret();
    harness.scroll();
    harness.take_calls();

    harness.answer(completed("est"));

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn tab_after_a_scroll_types_the_kept_suggestion() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est regards");
    harness.scroll();
    harness.settle_caret();
    harness.take_calls();

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::ReplaceText(0, "est".to_owned()))
    );
}

#[test]
fn scroll_outside_a_watched_field_does_nothing() {
    let mut harness = Harness::new(Settings::default());

    harness.scroll();

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn scroll_after_a_click_does_nothing() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");
    harness.controller.on_context_lost(&mut harness.shell);
    harness.take_calls();

    harness.scroll();

    assert_eq!(harness.take_calls(), Vec::new());
}
