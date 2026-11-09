//! Tests of the ghost text after the caret that stays inside its field: the controller wraps the
//! rest that does not fit on the line of the caret onto the lines below, or shows the whole ghost
//! text below the caret when the field has no room for those lines.

use super::harness::{Call, Harness};
use crate::controller::{CaretLook, CaretRead, Ghost, GhostPlacement, GhostView, WrappedLines};
use crate::desktop::ScreenRect;
use crate::key::Key;
use crate::settings::Settings;

/// A field with one line of 20 pixels below the line of the caret. A caret at 100 has room for
/// 11 characters of ghost text, and the line below has room for 16, between the gaps of 4 pixels
/// at the edges of the field.
const FIELD: ScreenRect = ScreenRect {
    left: 50,
    top: 80,
    right: 220,
    bottom: 140,
};
/// A field with no room below the line of the caret.
const SINGLE_LINE_FIELD: ScreenRect = ScreenRect { bottom: 120, ..FIELD };
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
        ..CaretRead::default()
    });
}

fn shown_view(harness: &Harness) -> GhostView {
    harness.shell.ghost_view.clone().expect("a shown ghost text")
}

fn shown_lines(harness: &Harness) -> (Ghost, Option<WrappedLines>) {
    let view = shown_view(harness);
    (view.ghost, view.wrapped)
}

fn text(ghost: &str) -> Ghost {
    Ghost::Text(ghost.to_owned())
}

fn wrapped(line: &str) -> WrappedLines {
    WrappedLines {
        left: 54,
        lines: vec![line.to_owned()],
    }
}

#[test]
fn ghost_text_that_runs_past_its_field_wraps_onto_the_line_below() {
    let mut harness = harness_at(100, Some(FIELD));

    harness.show(SUGGESTION);

    assert_eq!(
        shown_lines(&harness),
        (text("est regards "), Some(wrapped("to you")))
    );
}

#[test]
fn ghost_text_stays_whole_in_a_field_without_a_known_frame() {
    let fields = [None, Some(ScreenRect { right: 50, ..FIELD })];

    let ghosts = fields.map(|field| {
        let mut harness = harness_at(100, field);
        harness.show(SUGGESTION);
        shown_lines(&harness)
    });

    assert_eq!(ghosts, [(text(SUGGESTION), None), (text(SUGGESTION), None)]);
}

#[test]
fn ghost_text_that_does_not_fit_in_a_single_line_field_shows_whole_below_the_caret() {
    let mut harness = harness_at(100, Some(SINGLE_LINE_FIELD));

    harness.show(SUGGESTION);

    let view = shown_view(&harness);
    assert_eq!(
        (view.ghost, view.wrapped, view.placement),
        (text(SUGGESTION), None, GhostPlacement::BelowCaret)
    );
}

#[test]
fn rest_after_tab_wraps_again_at_the_new_caret() {
    let mut harness = harness_at(100, Some(FIELD));
    harness.show(SUGGESTION);
    harness.press(Key::Tab);
    move_caret_to(&mut harness, 130, Some(FIELD));

    harness.settle_caret();

    assert_eq!(
        shown_lines(&harness),
        (text(" regards "), Some(wrapped("to you")))
    );
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
fn quick_tab_types_the_part_that_showed_on_a_wrapped_line() {
    let mut harness = harness_at(100, Some(FIELD));
    harness.show(SUGGESTION);
    harness.press(Key::Tab);
    harness.press(Key::Tab);
    harness.take_calls();

    harness.press(Key::Tab);

    assert!(
        harness
            .take_calls()
            .contains(&Call::ReplaceText(0, " to".to_owned()))
    );
}
