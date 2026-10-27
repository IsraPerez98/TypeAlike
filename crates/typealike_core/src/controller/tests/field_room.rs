//! Tests of the ghost text after the caret that stays inside its field: the controller cuts it
//! after the last whole Tab part that fits before the right edge of the field.

use super::harness::{Call, Harness};
use crate::controller::tab_hold::TAB_HOLD_DELAY;
use crate::controller::{CaretLook, CaretRead, Ghost, GhostPlacement, Timer};
use crate::desktop::ScreenRect;
use crate::key::{Disposition, Key};
use crate::settings::Settings;

/// A field with room for 11 characters of ghost text after a caret at 100, less the gap of 4
/// pixels before its right edge.
const FIELD: ScreenRect = ScreenRect {
    left: 50,
    top: 80,
    right: 220,
    bottom: 140,
};
/// A suggestion after "B" with the Tab parts "est", " regards", " to" and " you".
const SUGGESTION: &str = "est regards to you";

fn caret_at(left: i32) -> ScreenRect {
    ScreenRect {
        left,
        top: 100,
        right: left,
        bottom: 120,
    }
}

/// A harness whose shell reads the caret at once and finds it at `left` in `field`.
fn harness_at(left: i32, field: Option<ScreenRect>) -> Harness {
    let mut harness = Harness::new(Settings::default());
    move_caret_to(&mut harness, left, field);
    harness
}

fn move_caret_to(harness: &mut Harness, left: i32, field: Option<ScreenRect>) {
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        caret: Some(caret_at(left)),
        field,
    });
}

fn shown_ghost(harness: &Harness) -> Option<Ghost> {
    harness.shell.ghost_view.as_ref().map(|view| view.ghost.clone())
}

fn text(ghost: &str) -> Ghost {
    Ghost::Text(ghost.to_owned())
}

/// A harness after a Tab on the first part of [`SUGGESTION`], shown in [`FIELD`] up to " regards",
/// and a quick Tab on " regards", which leaves " to you" that did not show.
fn harness_after_the_last_shown_part() -> Harness {
    let mut harness = harness_at(100, Some(FIELD));
    harness.show(SUGGESTION);
    harness.press(Key::Tab);
    harness.press(Key::Tab);
    harness.take_calls();
    harness
}

#[test]
fn ghost_text_that_runs_past_its_field_shows_its_parts_that_fit() {
    let mut harness = harness_at(100, Some(FIELD));

    harness.show(SUGGESTION);

    assert_eq!(shown_ghost(&harness), Some(text("est regards")));
}

#[test]
fn ghost_text_stays_whole_in_a_field_without_a_known_frame() {
    let fields = [None, Some(ScreenRect { right: 50, ..FIELD })];

    let ghosts = fields.map(|field| {
        let mut harness = harness_at(100, field);
        harness.show(SUGGESTION);
        shown_ghost(&harness)
    });

    assert_eq!(ghosts, [Some(text(SUGGESTION)), Some(text(SUGGESTION))]);
}

#[test]
fn ghost_text_whose_first_part_does_not_fit_shows_whole_below_the_caret() {
    let mut harness = harness_at(200, Some(FIELD));

    harness.show(SUGGESTION);

    let view = harness.shell.ghost_view.expect("a shown ghost text");
    assert_eq!(
        (view.ghost, view.placement),
        (Ghost::Text(SUGGESTION.to_owned()), GhostPlacement::BelowCaret)
    );
}

#[test]
fn rest_after_tab_shows_its_parts_that_fit_at_the_new_caret() {
    let mut harness = harness_at(100, Some(FIELD));
    harness.show(SUGGESTION);
    harness.press(Key::Tab);
    move_caret_to(&mut harness, 130, Some(FIELD));

    harness.settle_caret();

    assert_eq!(shown_ghost(&harness), Some(text(" regards")));
}

#[test]
fn quick_tab_after_a_part_types_the_next_part_that_showed() {
    let mut harness = harness_at(100, Some(FIELD));
    harness.show(SUGGESTION);
    harness.press(Key::Tab);
    harness.take_calls();

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::ReplaceText(0, " regards".to_owned()))
    );
}

#[test]
fn quick_tab_after_the_last_shown_part_starts_the_tab_hold_timer() {
    let mut harness = harness_at(100, Some(FIELD));
    harness.show(SUGGESTION);
    harness.press(Key::Tab);

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::StartTimer(Timer::TabHold, TAB_HOLD_DELAY))
    );
}

#[test]
fn quick_tab_on_a_rest_that_did_not_show_is_held() {
    let mut harness = harness_after_the_last_shown_part();

    let disposition = harness.press(Key::Tab);

    assert_eq!(
        (disposition, harness.take_calls()),
        (Disposition::Swallow, Vec::new())
    );
}

#[test]
fn tab_after_the_rest_shows_types_its_next_part() {
    let mut harness = harness_after_the_last_shown_part();
    move_caret_to(&mut harness, 160, Some(FIELD));
    harness.settle_caret();

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::ReplaceText(0, " to".to_owned()))
    );
}
