//! Tests of the looks at the caret, at once and later: the ghost text and the badge show only when
//! the look comes, and the ghost text stays off a caret that did not move after a key.

use super::harness::{Call, Harness, completed, field_around, fixed, ghost_at_caret};
use crate::controller::{CARET_MOVE_CHECK_DELAY, CaretLook, CaretRead, Timer};
use crate::desktop::ScreenRect;
use crate::key::Key;
use crate::settings::Settings;
use crate::test_support::{active_badge, paused_badge};

const CARET_HEIGHT: i32 = 20;
pub(super) const OLD_CARET: i32 = 100;
const NEXT_CARET: i32 = 108;
pub(super) const FAR_CARET: i32 = 900;
const FIELD: ScreenRect = ScreenRect {
    left: 50,
    top: 80,
    right: 650,
    bottom: 300,
};

fn caret_at(left: i32) -> ScreenRect {
    ScreenRect {
        left,
        top: 100,
        right: left,
        bottom: 100 + CARET_HEIGHT,
    }
}

/// A look that finds the caret at `left` in [`FIELD`].
pub(super) fn look_at(left: i32) -> CaretRead {
    CaretRead {
        caret: Some(caret_at(left)),
        field: Some(FIELD),
    }
}

fn harness_with_later_looks() -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.shell.caret_look = CaretLook::Later;
    harness
}

/// A harness whose shell reads the caret at once and finds it at `left` in [`FIELD`].
fn harness_with_looks_at_once(left: i32) -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.shell.caret_look = CaretLook::Now(look_at(left));
    harness
}

/// A harness that waits for the look at the caret to show `suggestion` after "B".
pub(super) fn harness_waiting_for_the_look(suggestion: &str) -> Harness {
    let mut harness = harness_with_later_looks();
    harness.press(Key::Char('B'));
    harness.answer(completed(suggestion));
    harness.settle_caret();
    harness.take_calls();
    harness
}

/// A harness that shows `suggestion` after "B" at the caret at [`OLD_CARET`].
pub(super) fn harness_showing_at_the_old_caret(suggestion: &str) -> Harness {
    let mut harness = harness_waiting_for_the_look(suggestion);
    harness.answer_caret(look_at(OLD_CARET));
    harness.take_calls();
    harness
}

#[test]
fn settled_caret_waits_for_the_look_of_the_shell() {
    let mut harness = harness_with_later_looks();
    harness.press(Key::Char('B'));
    harness.answer(completed("est"));
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(harness.take_calls(), vec![Call::ReadCaret]);
}

#[test]
fn look_shows_the_ghost_and_the_badge_at_its_caret() {
    let mut harness = harness_waiting_for_the_look("est");

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(active_badge()))]
    );
}

#[test]
fn look_after_a_key_is_dropped() {
    let mut harness = harness_waiting_for_the_look("est");
    harness.press(Key::Char('x'));
    harness.take_calls();

    harness.answer_caret(look_at(NEXT_CARET));

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn look_that_nothing_waits_for_is_dropped() {
    let mut harness = harness_with_later_looks();
    harness.press(Key::Char('B'));
    harness.take_calls();

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn caret_that_did_not_move_after_a_key_gets_only_the_badge_and_a_new_look() {
    let mut harness = harness_showing_at_the_old_caret("est regards");
    harness.press(Key::Tab);
    harness.settle_caret();
    harness.take_calls();

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![
            ghost_at_caret(None, Some(active_badge())),
            Call::StartTimer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY),
        ]
    );
}

#[test]
fn caret_that_moved_after_a_key_gets_the_ghost_at_once() {
    let mut harness = harness_showing_at_the_old_caret("est regards");
    harness.press(Key::Tab);
    harness.settle_caret();
    harness.take_calls();

    harness.answer_caret(look_at(NEXT_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some(" regards"), Some(active_badge()))]
    );
}

#[test]
fn ghost_shows_at_the_caret_that_did_not_move_after_the_last_check() {
    let mut harness = harness_showing_at_the_old_caret("est regards");
    harness.press(Key::Tab);
    harness.run_out_caret_checks_at(look_at(OLD_CARET));
    harness.settle_caret();
    harness.take_calls();

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some(" regards"), Some(active_badge()))]
    );
}

#[test]
fn badge_look_at_the_old_caret_keeps_the_next_ghost_off_that_caret() {
    let mut harness = harness_showing_at_the_old_caret("est regards");
    harness.press(Key::Char('x'));
    harness.settle_caret();
    harness.answer_caret(look_at(OLD_CARET));
    harness.answer(completed("ylophone"));
    harness.take_calls();

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![
            ghost_at_caret(None, Some(active_badge())),
            Call::StartTimer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY),
        ]
    );
}

#[test]
fn earlier_suggestion_after_a_backspace_shows_at_once_at_the_caret_where_it_showed() {
    let mut harness = harness_showing_at_the_old_caret("est regards");
    harness.press(Key::Char('x'));
    harness.settle_caret();
    harness.answer_caret(look_at(NEXT_CARET));
    harness.press(Key::Backspace);
    harness.settle_caret();
    harness.take_calls();

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est regards"), Some(active_badge()))]
    );
}

#[test]
fn rest_after_a_fix_of_the_same_length_shows_at_once_at_the_same_caret() {
    let mut harness = harness_with_later_looks();
    harness.shell.surroundings = field_around("Until te", "");
    harness.press(Key::Char('h'));
    harness.answer(fixed("teh", "the", " end"));
    harness.settle_caret();
    harness.answer_caret(look_at(OLD_CARET));
    harness.press(Key::Tab);
    harness.settle_caret();
    harness.take_calls();

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some(" end"), Some(active_badge()))]
    );
}

#[test]
fn caret_far_from_its_field_hides_the_ghost_and_the_badge_and_gets_a_new_look() {
    let mut harness = harness_waiting_for_the_look("est");

    harness.answer_caret(look_at(FAR_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![
            Call::HideGhost,
            Call::HideBadge,
            Call::StartTimer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY),
        ]
    );
}

#[test]
fn look_without_a_caret_renders_the_ghost_and_the_badge() {
    let mut harness = harness_waiting_for_the_look("est");

    harness.answer_caret(CaretRead {
        caret: None,
        field: Some(FIELD),
    });

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(active_badge()))]
    );
}

#[test]
fn look_without_a_field_frame_shows_a_far_caret() {
    let mut harness = harness_waiting_for_the_look("est");

    harness.answer_caret(CaretRead {
        caret: Some(caret_at(FAR_CARET)),
        field: None,
    });

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some("est"), Some(active_badge()))]
    );
}

#[test]
fn quick_tab_while_the_look_waits_types_the_next_part() {
    let mut harness = harness_showing_at_the_old_caret("est regards to you");
    harness.press(Key::Tab);
    harness.settle_caret();
    harness.take_calls();

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::ReplaceText(0, " regards".to_owned()))
    );
}

#[test]
fn pause_shows_the_paused_badge_when_the_look_comes() {
    let mut harness = harness_with_later_looks();
    harness.press(Key::Char('B'));
    harness.toggle_pause();
    harness.take_calls();

    harness.answer_caret(look_at(OLD_CARET));

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(None, Some(paused_badge()))]
    );
}

#[test]
fn caret_read_at_once_that_did_not_move_after_a_key_gets_only_the_badge_and_a_new_look() {
    let mut harness = harness_with_looks_at_once(OLD_CARET);
    harness.show("est regards");
    harness.press(Key::Tab);
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
fn caret_read_at_once_that_moved_after_a_key_gets_the_ghost() {
    let mut harness = harness_with_looks_at_once(OLD_CARET);
    harness.show("est regards");
    harness.press(Key::Tab);
    harness.shell.caret_look = CaretLook::Now(look_at(NEXT_CARET));
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![ghost_at_caret(Some(" regards"), Some(active_badge()))]
    );
}

#[test]
fn caret_read_at_once_far_from_its_field_hides_the_ghost_and_the_badge() {
    let mut harness = harness_with_looks_at_once(FAR_CARET);
    harness.press(Key::Char('B'));
    harness.answer(completed("est"));
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![
            Call::HideGhost,
            Call::HideBadge,
            Call::StartTimer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY),
        ]
    );
}
