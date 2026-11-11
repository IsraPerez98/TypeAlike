//! Tests of the ghost text after the caret that stays inside its field: the controller wraps the
//! rest that does not fit on the line of the caret onto the lines below. When the field has no
//! room for those lines or text on them, only the words that fit on the line of the caret show,
//! and the whole ghost text shows below the caret only when not even its first word fits there.

use super::harness::{Call, Harness, field_around};
use crate::controller::{CaretLook, CaretRead, Ghost, GhostPlacement, GhostView, WrappedLines};
use crate::desktop::{AfterCaret, ScreenRect};
use crate::key::{Disposition, Key};
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
fn ghost_text_that_does_not_fit_in_a_single_line_field_shows_the_words_that_fit_after_the_caret() {
    let mut harness = harness_at(100, Some(SINGLE_LINE_FIELD));

    harness.show(SUGGESTION);

    let view = shown_view(&harness);
    assert_eq!(
        (view.ghost, view.wrapped, view.placement),
        (text("est regards"), None, GhostPlacement::AfterCaret)
    );
}

#[test]
fn ghost_text_whose_first_word_does_not_fit_in_a_single_line_field_shows_whole_below_the_caret() {
    let mut harness = harness_at(210, Some(SINGLE_LINE_FIELD));

    harness.show(SUGGESTION);

    let view = shown_view(&harness);
    assert_eq!(
        (view.ghost, view.wrapped, view.placement),
        (text(SUGGESTION), None, GhostPlacement::BelowCaret)
    );
}

#[test]
fn ghost_text_that_does_not_fit_above_a_line_with_text_shows_the_words_that_fit_after_the_caret() {
    let mut harness = harness_at(100, Some(FIELD));
    harness.shell.surroundings = field_around("", "\nSee you on Friday.");

    harness.show(SUGGESTION);

    let view = shown_view(&harness);
    assert_eq!(
        (view.ghost, view.wrapped, view.placement),
        (text("est regards"), None, GhostPlacement::AfterCaret)
    );
}

#[test]
fn ghost_text_does_not_wrap_over_text_that_an_app_gives_without_a_line_break_after_the_caret() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        caret: Some(caret_at(100)),
        field: Some(FIELD),
        after: AfterCaret::Nothing,
        ..CaretRead::default()
    });
    harness.shell.surroundings = field_around("", "See you on Friday.");

    harness.show(SUGGESTION);

    let view = shown_view(&harness);
    assert_eq!(
        (view.ghost, view.wrapped, view.placement),
        (text("est regards"), None, GhostPlacement::AfterCaret)
    );
}

#[test]
fn ghost_text_does_not_wrap_over_the_text_below_that_a_look_sees() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        caret: Some(caret_at(100)),
        field: Some(FIELD),
        after: AfterCaret::TextBelow,
        ..CaretRead::default()
    });

    harness.show(SUGGESTION);

    let view = shown_view(&harness);
    assert_eq!(
        (view.ghost, view.wrapped, view.placement),
        (text("est regards"), None, GhostPlacement::AfterCaret)
    );
}

#[test]
fn ghost_text_wraps_onto_lines_below_that_have_only_whitespace() {
    let mut harness = harness_at(100, Some(FIELD));
    harness.shell.surroundings = field_around("", "\n  \n");

    harness.show(SUGGESTION);

    assert_eq!(
        shown_lines(&harness),
        (text("est regards "), Some(wrapped("to you")))
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
fn quick_tab_after_a_part_types_the_next_word_that_showed_on_the_line_of_the_caret() {
    let mut harness = harness_at(100, Some(SINGLE_LINE_FIELD));
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
fn quick_tab_after_the_last_word_that_showed_waits_for_the_rest() {
    let mut harness = harness_at(100, Some(SINGLE_LINE_FIELD));
    harness.show(SUGGESTION);
    harness.press(Key::Tab);
    harness.press(Key::Tab);
    harness.shell.is_ghost_shown = false;
    harness.take_calls();

    let disposition = harness.press(Key::Tab);

    let calls = harness.take_calls();
    assert_eq!(
        (
            disposition,
            calls.contains(&Call::ReplaceText(0, " to".to_owned()))
        ),
        (Disposition::Swallow, false)
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
