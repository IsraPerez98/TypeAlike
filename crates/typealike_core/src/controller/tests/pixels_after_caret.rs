//! Tests of the ghost text in a field whose text after the caret the platform cannot read, as the
//! editor of VS Code and the input box of a terminal of xterm.js: the pixels after the caret tell
//! what follows the caret. The ghost text shows after the caret where the pixels show no text, with
//! the rules of the room in the field, and below the caret where they show text. The edge of a box
//! of the app on the line of the caret ends the field.

use super::harness::{Harness, field_around};
use crate::controller::{CaretLook, CaretRead, Ghost, GhostPlacement, WrappedLines};
use crate::desktop::{AfterCaret, PixelColor, ScreenPixels, ScreenRect, Surroundings};
use crate::settings::Settings;

/// A field with 4 lines of 20 pixels below the line of the caret. A caret at 100 has room for 11
/// characters of ghost text, and each line below has room for 16.
const FIELD: ScreenRect = ScreenRect {
    left: 50,
    top: 80,
    right: 220,
    bottom: 200,
};
/// A caret 20 pixels high at 100, on the line from 100 to 120.
const CARET: ScreenRect = ScreenRect {
    left: 100,
    top: 100,
    right: 100,
    bottom: 120,
};
const BACKGROUND: PixelColor = PixelColor {
    red: 30,
    green: 30,
    blue: 30,
};
const TEXT: PixelColor = PixelColor {
    red: 220,
    green: 220,
    blue: 220,
};
/// A suggestion after "B" with the Tab parts "est", " regards", " to" and " you".
const SUGGESTION: &str = "est regards to you";

/// The input box of a terminal of xterm.js, which hides the line of its caret.
fn terminal_input() -> Surroundings {
    Surroundings {
        is_caret_line_hidden: true,
        ..field_around("", "")
    }
}

/// The editor of VS Code, whose text the platform cannot read.
fn code_editor() -> Surroundings {
    Surroundings {
        has_unreadable_text: true,
        ..Surroundings::default()
    }
}

/// The background of a terminal around the box of a terminal app, darker than the box.
const TERMINAL_BACKGROUND: PixelColor = PixelColor {
    red: 10,
    green: 10,
    blue: 10,
};
/// The part of the terminal right of a box of a terminal app that has its own background and no
/// border on the right, as the input box of opencode in the integrated terminal of VS Code. A caret
/// at 100 has room for 7 characters of ghost text before the edge of the box, and each line below
/// has room for 12 inside the box.
const OUTSIDE_THE_BOX: ScreenRect = ScreenRect { left: 180, ..FIELD };
/// The status row of a terminal app inside its box, below the line of the caret.
const STATUS_ROW: ScreenRect = ScreenRect {
    left: 54,
    top: 146,
    right: 140,
    bottom: 154,
};

/// The pixels after the caret, on the background, with each rectangle of `areas` painted in its
/// color. A later rectangle paints over an earlier one.
fn pixels_painted(areas: &[(ScreenRect, PixelColor)]) -> ScreenPixels {
    let area = ScreenPixels::area_after_caret(CARET, FIELD).expect("an area after the caret");
    let width = area.right - area.left;
    let height = area.bottom - area.top;
    let colors = (0..height)
        .flat_map(|row| (0..width).map(move |column| (area.left + column, area.top + row)))
        .map(|(x, y)| {
            areas
                .iter()
                .rev()
                .find(|(rect, _)| {
                    (rect.left..rect.right).contains(&x) && (rect.top..rect.bottom).contains(&y)
                })
                .map_or(BACKGROUND, |&(_, color)| color)
        })
        .collect();
    ScreenPixels::new(area, colors).expect("a color for each pixel")
}

/// The pixels after the caret, on the background, with `text` painted in each of its rectangles.
fn pixels_with(text: &[ScreenRect]) -> ScreenPixels {
    let areas: Vec<_> = text.iter().map(|&rect| (rect, TEXT)).collect();
    pixels_painted(&areas)
}

/// A harness in `field`, whose look at the caret sees `after` and `pixels`, after the controller
/// showed the suggestion.
fn harness_showing(field: Surroundings, after: AfterCaret, pixels: Option<ScreenPixels>) -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = field;
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        caret: Some(CARET),
        field: Some(FIELD),
        after,
        pixels,
    });
    harness.show(SUGGESTION);
    harness
}

fn shown(harness: &Harness) -> (Ghost, Option<WrappedLines>, GhostPlacement) {
    let view = harness.shell.ghost_view.clone().expect("a shown ghost text");
    (view.ghost, view.wrapped, view.placement)
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
fn ghost_text_in_a_terminal_wraps_after_the_caret_where_the_pixels_show_no_text() {
    let harness = harness_showing(terminal_input(), AfterCaret::Text, Some(pixels_with(&[])));

    assert_eq!(
        shown(&harness),
        (
            text("est regards "),
            Some(wrapped("to you")),
            GhostPlacement::AfterCaret
        )
    );
}

#[test]
fn ghost_text_in_a_terminal_shows_below_the_caret_where_the_pixels_show_text_after_it() {
    let text_after = ScreenRect {
        left: 150,
        top: 104,
        right: 170,
        bottom: 116,
    };

    let harness = harness_showing(
        terminal_input(),
        AfterCaret::Nothing,
        Some(pixels_with(&[text_after])),
    );

    assert_eq!(
        shown(&harness),
        (text(SUGGESTION), None, GhostPlacement::BelowCaret)
    );
}

#[test]
fn ghost_text_in_a_terminal_shows_the_words_that_fit_above_the_status_line_that_the_pixels_show() {
    let status_line = ScreenRect {
        left: 54,
        top: 146,
        right: 200,
        bottom: 154,
    };

    let harness = harness_showing(
        terminal_input(),
        AfterCaret::Text,
        Some(pixels_with(&[status_line])),
    );

    assert_eq!(
        shown(&harness),
        (text("est regards"), None, GhostPlacement::AfterCaret)
    );
}

#[test]
fn ghost_text_in_a_terminal_shows_the_words_that_fit_before_the_edge_of_the_box_of_the_app() {
    let pixels = pixels_painted(&[(OUTSIDE_THE_BOX, TERMINAL_BACKGROUND), (STATUS_ROW, TEXT)]);

    let harness = harness_showing(terminal_input(), AfterCaret::Text, Some(pixels));

    assert_eq!(shown(&harness), (text("est"), None, GhostPlacement::AfterCaret));
}

#[test]
fn ghost_text_in_a_terminal_wraps_inside_the_box_of_the_app() {
    let pixels = pixels_painted(&[(OUTSIDE_THE_BOX, TERMINAL_BACKGROUND)]);

    let harness = harness_showing(terminal_input(), AfterCaret::Text, Some(pixels));

    assert_eq!(
        shown(&harness),
        (
            text("est "),
            Some(WrappedLines {
                left: 54,
                lines: vec!["regards to ".to_owned(), "you".to_owned()],
            }),
            GhostPlacement::AfterCaret
        )
    );
}

#[test]
fn ghost_text_in_a_terminal_stays_below_the_caret_without_pixels() {
    let harness = harness_showing(terminal_input(), AfterCaret::Nothing, None);

    assert_eq!(
        shown(&harness),
        (text(SUGGESTION), None, GhostPlacement::BelowCaret)
    );
}

#[test]
fn ghost_text_in_the_editor_of_vs_code_shows_after_the_caret_where_the_pixels_show_no_text() {
    let harness = harness_showing(code_editor(), AfterCaret::Text, Some(pixels_with(&[])));

    assert_eq!(shown(&harness).2, GhostPlacement::AfterCaret);
}

#[test]
fn ghost_text_in_the_editor_of_vs_code_shows_below_the_caret_where_the_pixels_show_text_after_it() {
    let text_after = ScreenRect {
        left: 103,
        top: 104,
        right: 108,
        bottom: 116,
    };

    let harness = harness_showing(
        code_editor(),
        AfterCaret::Nothing,
        Some(pixels_with(&[text_after])),
    );

    assert_eq!(shown(&harness).2, GhostPlacement::BelowCaret);
}

#[test]
fn ghost_text_does_not_wrap_past_the_lines_that_the_pixels_show() {
    let one_line = ScreenRect {
        bottom: 120,
        ..pixels_with(&[]).frame()
    };
    let colors = vec![BACKGROUND; usize::try_from((one_line.right - one_line.left) * 20).unwrap()];

    let harness = harness_showing(
        terminal_input(),
        AfterCaret::Text,
        ScreenPixels::new(one_line, colors),
    );

    assert_eq!(
        shown(&harness),
        (text("est regards"), None, GhostPlacement::AfterCaret)
    );
}

/// The badges that show in `field`, for a shell that gives badges of 20 pixels, when the look
/// sees text after the caret (in the editor of VS Code, the message that the editor gives in
/// place of its text) and `pixels`.
fn badge_with(field: Surroundings, pixels: ScreenPixels) -> Option<ScreenRect> {
    let mut harness = Harness::new(Settings::default());
    harness.shell.badge_side = Some(20);
    harness.shell.surroundings = field;
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        caret: Some(CARET),
        field: Some(FIELD),
        after: AfterCaret::Text,
        pixels: Some(pixels),
    });
    harness.show(SUGGESTION);
    harness.shell.shown_badge.and_then(|badge| badge.frame)
}

#[test]
fn badge_shows_at_the_end_of_the_line_of_the_caret_where_the_pixels_show_no_text() {
    let frame = badge_with(code_editor(), pixels_with(&[]));

    assert_eq!(
        frame,
        Some(ScreenRect {
            left: 196,
            top: 100,
            right: 216,
            bottom: 120,
        })
    );
}

#[test]
fn badge_hides_where_the_pixels_show_text_after_the_caret() {
    let word_after_caret = ScreenRect {
        left: 150,
        top: 104,
        right: 170,
        bottom: 116,
    };

    let frame = badge_with(code_editor(), pixels_with(&[word_after_caret]));

    assert_eq!(frame, None);
}

#[test]
fn badge_shows_before_the_edge_of_the_box_of_a_terminal_app() {
    let pixels = pixels_painted(&[(OUTSIDE_THE_BOX, TERMINAL_BACKGROUND)]);

    let frame = badge_with(terminal_input(), pixels);

    assert_eq!(
        frame,
        Some(ScreenRect {
            left: 156,
            top: 100,
            right: 176,
            bottom: 120,
        })
    );
}
