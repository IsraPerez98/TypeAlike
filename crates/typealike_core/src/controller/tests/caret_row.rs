//! Tests of the row of the caret on the screen, which the mirror shows below in a field that hides
//! its text and gives no caret, as the terminal of Ghostty or the window of Herdr.

use super::harness::{Call, Harness, completed};
use crate::controller::{CaretLook, CaretRead, Delivery};
use crate::desktop::{Confidence, ScreenLine, ScreenRead, ScreenRect, ScreenText, Surroundings};
use crate::key::Key;
use crate::settings::Settings;

const WINDOW: ScreenRect = rect(0, 800);
const ANSWER_ROW: ScreenRect = rect(660, 676);
const INPUT_ROW: ScreenRect = rect(700, 716);
const STATUS_ROW: ScreenRect = rect(740, 756);

const fn rect(top: i32, bottom: i32) -> ScreenRect {
    ScreenRect {
        left: 10,
        top,
        right: 990,
        bottom,
    }
}

fn row(text: &str, frame: ScreenRect) -> ScreenLine {
    ScreenLine {
        text: text.to_owned(),
        frame,
        confidence: Confidence::CERTAIN,
    }
}

/// Types "D" in a window that shows its text only on the screen, and answers the suggestion and then
/// the screen read of the key, with the rows of a coding agent in the field `read_field`. The new
/// screen text asks for the suggestion again. Then shows that suggestion, and returns the row of the
/// caret of the shown view.
fn caret_row_after_a_read_in(read_field: ScreenRect) -> Option<ScreenRect> {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = Surroundings {
        app_id: Some("dev.herdr.app".to_owned()),
        app_name: Some("Herdr".to_owned()),
        has_unreadable_text: true,
        ..Surroundings::default()
    };
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        field: Some(WINDOW),
        ..CaretRead::default()
    });
    harness.press(Key::Char('D'));
    let read = harness
        .take_calls()
        .into_iter()
        .find_map(|call| match call {
            Call::ReadScreen(read, _) => Some(read),
            _ => None,
        })
        .expect("the first key reads the screen");
    harness.answer(completed("eploy the staging server"));
    let rows = vec![
        row("Done, the tests pass.", ANSWER_ROW),
        row("❯ D", INPUT_ROW),
        row("? for shortcuts", STATUS_ROW),
    ];
    let text = ScreenText {
        lines: rows,
        caret: None,
        field: read_field,
    };
    harness.controller.on_delivery(
        Delivery::Screen(ScreenRead {
            id: read,
            text: Some(text),
        }),
        &mut harness.shell,
    );

    harness.answer(completed("eploy the staging server"));
    harness.settle_caret();

    harness
        .shell
        .ghost_view
        .clone()
        .expect("a shown ghost text")
        .caret_row
}

#[test]
fn view_without_a_caret_gets_the_row_that_shows_the_typed_text() {
    let row = caret_row_after_a_read_in(WINDOW);

    assert_eq!(row, Some(INPUT_ROW));
}

#[test]
fn view_gets_no_row_from_a_read_of_a_field_frame_that_moved_since() {
    let moved = ScreenRect {
        left: 100,
        right: 1100,
        ..WINDOW
    };

    let row = caret_row_after_a_read_in(moved);

    assert_eq!(row, None);
}
