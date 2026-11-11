//! Tests of the place of the badge at the caret, for a shell that gives the side of its badge: the
//! badge shows at the end of the line of the caret, inside its field, and the ghost text after the
//! caret ends before it. The badge then covers no text of the field and no ghost text.

use super::harness::{Call, Harness, field_around, ghost_at_caret};
use crate::app::BadgeView;
use crate::controller::{
    CARET_MOVE_CHECK_DELAY, CaretLook, CaretRead, Ghost, GhostPlacement, Timer, WrappedLines,
};
use crate::desktop::{AfterCaret, ScreenRect, Surroundings};
use crate::key::Key;
use crate::settings::Settings;
use crate::test_support::active_badge;

/// The text field of Notepad at a scale of 100 %, in physical pixels, from a log of a real app
/// test. Its first line starts at its top edge, just below the menu bar.
const NOTEPAD: ScreenRect = ScreenRect {
    left: 208,
    top: 201,
    right: 1075,
    bottom: 639,
};
/// The top of the first line of [`NOTEPAD`], and of its third line.
const FIRST_LINE: i32 = 201;
const THIRD_LINE: i32 = 237;
/// The side of the badge on a line of 18 pixels at a scale of 100 %.
const NOTEPAD_BADGE_SIDE: i32 = 18;

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
    ScreenRect {
        left,
        top,
        right,
        bottom,
    }
}

/// A caret of 18 pixels at `left` on the line that starts at `top`.
fn caret(left: i32, top: i32) -> ScreenRect {
    rect(left, top, left, top + 18)
}

/// A look that finds `caret` in [`NOTEPAD`] and sees `after` after it.
fn notepad_look(caret: ScreenRect, after: AfterCaret) -> CaretRead {
    CaretRead {
        caret: Some(caret),
        field: Some(NOTEPAD),
        after,
    }
}

/// A harness whose shell gives badges of `side`, reads the caret at once and sees `look`.
fn harness_with_badge_side(side: i32, look: CaretRead) -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.shell.badge_side = Some(side);
    harness.shell.caret_look = CaretLook::Now(look);
    harness
}

fn badge_in(frame: ScreenRect) -> BadgeView {
    BadgeView {
        frame: Some(frame),
        ..active_badge()
    }
}

#[test]
fn badge_shows_at_the_end_of_the_line_of_the_caret_inside_its_field() {
    let carets = [caret(280, FIRST_LINE), caret(480, THIRD_LINE)];

    let badges = carets.map(|caret| {
        let mut harness =
            harness_with_badge_side(NOTEPAD_BADGE_SIDE, notepad_look(caret, AfterCaret::Nothing));
        harness.show("est regards");
        harness.shell.shown_badge
    });

    assert_eq!(
        badges,
        [
            Some(badge_in(rect(1053, 201, 1071, 219))),
            Some(badge_in(rect(1053, 237, 1071, 255))),
        ]
    );
}

#[test]
fn badge_stays_in_place_while_the_user_types_at_the_end_of_the_line() {
    let mut harness = harness_with_badge_side(
        NOTEPAD_BADGE_SIDE,
        notepad_look(caret(280, FIRST_LINE), AfterCaret::Nothing),
    );
    harness.show("est regards");
    let badge_before = harness.shell.shown_badge.clone();
    harness.shell.caret_look = CaretLook::Now(notepad_look(caret(288, FIRST_LINE), AfterCaret::Nothing));

    harness.press(Key::Char('e'));
    harness.settle_caret();

    let badge = badge_in(rect(1053, 201, 1071, 219));
    assert_eq!(
        (badge_before, harness.shell.shown_badge.clone()),
        (Some(badge.clone()), Some(badge))
    );
}

#[test]
fn caret_that_did_not_move_after_a_key_gets_only_the_badge_at_the_end_of_its_line() {
    let mut harness = harness_with_badge_side(
        NOTEPAD_BADGE_SIDE,
        notepad_look(caret(280, FIRST_LINE), AfterCaret::Nothing),
    );
    harness.show("est regards");
    harness.press(Key::Tab);
    harness.take_calls();

    harness.settle_caret();

    assert_eq!(
        harness.take_calls(),
        vec![
            ghost_at_caret(None, Some(badge_in(rect(1053, 201, 1071, 219)))),
            Call::StartTimer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY),
        ]
    );
}

#[test]
fn badge_hides_while_text_can_follow_the_caret_on_its_line() {
    let line_with_text = field_around("", " see you");
    let hidden_line = Surroundings {
        is_caret_line_hidden: true,
        ..Surroundings::default()
    };
    let cases = [
        (AfterCaret::Text, Surroundings::default()),
        (AfterCaret::Selection, Surroundings::default()),
        (AfterCaret::Unknown, line_with_text),
        (AfterCaret::Nothing, hidden_line),
    ];

    let badges = cases.map(|(after, surroundings)| {
        let mut harness =
            harness_with_badge_side(NOTEPAD_BADGE_SIDE, notepad_look(caret(280, FIRST_LINE), after));
        harness.shell.surroundings = surroundings;
        harness.show("est regards");
        harness.shell.shown_badge
    });

    assert_eq!(badges, [None, None, None, None]);
}

#[test]
fn badge_hides_when_the_line_of_the_caret_has_no_room_for_it() {
    let carets = [caret(1050, FIRST_LINE), caret(1075, FIRST_LINE)];

    let badges = carets.map(|caret| {
        let mut harness =
            harness_with_badge_side(NOTEPAD_BADGE_SIDE, notepad_look(caret, AfterCaret::Nothing));
        harness.show("est regards");
        harness.shell.shown_badge
    });

    assert_eq!(badges, [None, None]);
}

#[test]
fn badge_hides_without_a_field_frame() {
    let look = CaretRead {
        field: None,
        ..notepad_look(caret(280, FIRST_LINE), AfterCaret::Nothing)
    };
    let mut harness = harness_with_badge_side(NOTEPAD_BADGE_SIDE, look);

    harness.show("est regards");

    assert_eq!(harness.shell.shown_badge, None);
}

/// A field with one line of 20 pixels below the line of the caret, and a caret at 100 on its
/// first line. Without a badge, the line of the caret has room for 11 characters of ghost text.
/// A badge of 20 pixels at the end of the line leaves room for 9.
const SMALL_FIELD: ScreenRect = ScreenRect {
    left: 50,
    top: 80,
    right: 220,
    bottom: 140,
};

fn small_field_look(caret_left: i32, field: ScreenRect) -> CaretRead {
    CaretRead {
        caret: Some(rect(caret_left, 100, caret_left, 120)),
        field: Some(field),
        after: AfterCaret::Nothing,
    }
}

#[test]
fn ghost_text_ends_before_the_badge_and_wraps_its_rest_onto_the_line_below() {
    let mut harness = harness_with_badge_side(20, small_field_look(100, SMALL_FIELD));

    harness.show("est regards to you");

    let view = harness.shell.ghost_view.clone().expect("a shown ghost text");
    assert_eq!(
        (view.ghost, view.wrapped, harness.shell.shown_badge),
        (
            Ghost::Text("est ".to_owned()),
            Some(WrappedLines {
                left: 54,
                lines: vec!["regards to you".to_owned()],
            }),
            Some(badge_in(rect(196, 100, 216, 120))),
        )
    );
}

#[test]
fn ghost_text_whose_first_word_does_not_fit_before_the_badge_shows_below_the_caret_with_the_badge() {
    let single_line_field = ScreenRect {
        bottom: 120,
        ..SMALL_FIELD
    };
    let mut harness = harness_with_badge_side(20, small_field_look(170, single_line_field));

    harness.show("est regards to you");

    let view = harness.shell.ghost_view.clone().expect("a shown ghost text");
    assert_eq!(
        (view.placement, harness.shell.shown_badge),
        (
            GhostPlacement::BelowCaret,
            Some(badge_in(rect(196, 100, 216, 120)))
        )
    );
}

#[test]
fn shell_without_a_badge_side_puts_the_badge_at_the_caret_by_itself() {
    let afters = [AfterCaret::Nothing, AfterCaret::Text];

    let badges = afters.map(|after| {
        let mut harness = Harness::new(Settings::default());
        harness.shell.caret_look = CaretLook::Now(notepad_look(caret(280, FIRST_LINE), after));
        harness.show("est regards");
        harness.shell.shown_badge
    });

    assert_eq!(badges, [Some(active_badge()), Some(active_badge())]);
}
