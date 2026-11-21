//! Tests of the row of the caret on the screen, which the mirror shows below in a field that hides
//! its text and gives no caret, as the terminal of Ghostty or the window of Herdr.

use super::harness::{Call, Harness, completed};
use crate::controller::{CaretLook, CaretRead, Delivery};
use crate::desktop::{
    Confidence, OcrAvailability, ScreenLine, ScreenRead, ScreenReadId, ScreenRect, ScreenSource, ScreenText,
    Surroundings,
};
use crate::inference::InferenceRequest;
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

/// A harness in the window of Herdr, which shows its text only on the screen and gives no caret,
/// on a platform that can do an OCR.
fn herdr_with_ocr() -> Harness {
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
    harness.shell.ocr = OcrAvailability::Ready;
    harness
}

/// The reads of the screen that the controller asked for since the last call, with their sources.
fn screen_reads(harness: &mut Harness) -> Vec<(ScreenReadId, ScreenSource)> {
    harness
        .take_calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::ReadScreen(read, source) => Some((read, source)),
            _ => None,
        })
        .collect()
}

/// The id of the only read of the screen that the controller asked for since the last call.
fn only_screen_read(harness: &mut Harness) -> ScreenReadId {
    let reads = screen_reads(harness);
    assert_eq!(reads.len(), 1, "{reads:?}");
    reads[0].0
}

fn answer_screen_read(harness: &mut Harness, read: ScreenReadId, rows: Vec<ScreenLine>) {
    let text = ScreenText {
        lines: rows,
        caret: None,
        field: WINDOW,
    };
    harness.controller.on_delivery(
        Delivery::Screen(ScreenRead {
            id: read,
            text: Some(text),
        }),
        &mut harness.shell,
    );
}

/// Answers the read of the screen at the first key of a session, as the platform does when it
/// captures the window before the app writes the key back: the tree has no lines, and the OCR
/// after it shows the input row of a coding agent without the typed text.
fn answer_session_read_before_the_echo(harness: &mut Harness) {
    let tree_read = only_screen_read(harness);
    answer_screen_read(harness, tree_read, Vec::new());
    let ocr_read = only_screen_read(harness);
    answer_screen_read(
        harness,
        ocr_read,
        vec![
            row("Done, the tests pass.", ANSWER_ROW),
            row("❯", INPUT_ROW),
            row("? for shortcuts", STATUS_ROW),
        ],
    );
}

fn input_rows(typed: &str) -> Vec<ScreenLine> {
    vec![
        row("Done, the tests pass.", ANSWER_ROW),
        row(&format!("❯ {typed}"), INPUT_ROW),
        row("? for shortcuts", STATUS_ROW),
    ]
}

/// Types "D" in the window of Herdr, answers its suggestion, then the session read before the echo,
/// which asks for the suggestion again, and shows `completion`. Returns the reads of the screen
/// that the shown suggestion asked for.
fn show_after_a_session_read_before_the_echo(
    harness: &mut Harness,
    completion: &str,
) -> Vec<(ScreenReadId, ScreenSource)> {
    harness.press(Key::Char('D'));
    harness.answer(completed(completion));
    answer_session_read_before_the_echo(harness);
    harness.answer(completed(completion));
    harness.settle_caret();
    screen_reads(harness)
}

fn shown_row(harness: &Harness) -> Option<ScreenRect> {
    harness
        .shell
        .ghost_view
        .clone()
        .expect("a shown ghost text")
        .caret_row
}

#[test]
fn mirror_without_a_row_asks_for_an_ocr_read_of_the_row() {
    let mut harness = herdr_with_ocr();

    let reads = show_after_a_session_read_before_the_echo(&mut harness, "eploy the staging server");

    assert!(
        matches!(reads.as_slice(), [(_, ScreenSource::Ocr(_))]),
        "{reads:?}"
    );
}

#[test]
fn answer_of_the_row_read_moves_the_mirror_below_the_input_row() {
    let mut harness = herdr_with_ocr();
    let reads = show_after_a_session_read_before_the_echo(&mut harness, "eploy the staging server");

    answer_screen_read(&mut harness, reads[0].0, input_rows("D"));

    assert_eq!(shown_row(&harness), Some(INPUT_ROW));
}

#[test]
fn answer_of_the_row_read_asks_for_no_other_suggestion() {
    let mut harness = herdr_with_ocr();
    let reads = show_after_a_session_read_before_the_echo(&mut harness, "eploy the staging server");
    let requests = harness.requests().len();

    answer_screen_read(&mut harness, reads[0].0, input_rows("D"));

    assert_eq!(harness.requests().len(), requests);
}

#[test]
fn found_row_needs_no_other_read_while_the_typed_line_stays_short() {
    let mut harness = herdr_with_ocr();
    let reads =
        show_after_a_session_read_before_the_echo(&mut harness, "eploy the staging server and the database");
    answer_screen_read(&mut harness, reads[0].0, input_rows("D"));
    harness.take_calls();

    harness.type_text("eploy");
    harness.settle_caret();

    assert_eq!(screen_reads(&mut harness), []);
    assert_eq!(shown_row(&harness), Some(INPUT_ROW));
}

#[test]
fn found_row_gets_another_read_after_the_typed_line_grows_past_the_read() {
    let mut harness = herdr_with_ocr();
    let reads =
        show_after_a_session_read_before_the_echo(&mut harness, "eploy the staging server and the database");
    answer_screen_read(&mut harness, reads[0].0, input_rows("D"));
    harness.take_calls();

    harness.type_text("eploy the staging server and ");
    harness.settle_caret();

    let reads = screen_reads(&mut harness);
    assert!(
        matches!(reads.as_slice(), [(_, ScreenSource::Ocr(_))]),
        "{reads:?}"
    );
    assert_eq!(shown_row(&harness), Some(INPUT_ROW));
}

#[test]
fn no_row_read_starts_while_the_session_read_runs() {
    let mut harness = herdr_with_ocr();
    harness.press(Key::Char('D'));
    harness.take_calls();

    harness.answer(completed("eploy the staging server"));
    harness.settle_caret();

    assert_eq!(screen_reads(&mut harness), []);
}

#[test]
fn mirror_without_an_ocr_asks_for_no_row_read() {
    let mut harness = herdr_with_ocr();
    harness.shell.ocr = OcrAvailability::NotPermitted;
    harness.press(Key::Char('D'));
    let tree_read = only_screen_read(&mut harness);
    answer_screen_read(&mut harness, tree_read, input_rows(""));

    harness.answer(completed("eploy the staging server"));
    harness.settle_caret();

    assert_eq!(screen_reads(&mut harness), []);
}

#[test]
fn row_read_that_got_no_answer_does_not_stop_the_row_reads_of_the_next_session() {
    let mut harness = herdr_with_ocr();
    show_after_a_session_read_before_the_echo(&mut harness, "eploy the staging server");
    harness.press(Key::Enter);
    harness.take_calls();

    let reads = show_after_a_session_read_before_the_echo(&mut harness, "eploy the staging server");

    assert!(
        matches!(reads.as_slice(), [(_, ScreenSource::Ocr(_))]),
        "{reads:?}"
    );
}

/// The prompt of the last request for a suggestion.
fn last_prompt(harness: &Harness) -> String {
    match harness.last_request() {
        Some(InferenceRequest::Complete { prompt, .. }) => prompt.0,
        request => panic!("no request for a suggestion: {request:?}"),
    }
}

#[test]
fn text_of_a_row_read_stays_out_of_the_prompt_of_the_next_session() {
    const ROW_READ_LINE: &str = "Only the row read shows this line.";
    let mut harness = herdr_with_ocr();
    let reads = show_after_a_session_read_before_the_echo(&mut harness, "eploy the staging server");
    let mut rows = input_rows("D");
    rows.push(row(ROW_READ_LINE, rect(620, 636)));
    answer_screen_read(&mut harness, reads[0].0, rows);
    harness.press(Key::Enter);

    harness.press(Key::Char('N'));

    let prompt = last_prompt(&harness);
    assert!(prompt.contains("Done, the tests pass."), "{prompt}");
    assert!(!prompt.contains(ROW_READ_LINE), "{prompt}");
}

/// Answers the row read `read` with rows that show none of the typed text, types `text`, and
/// returns the reads of the screen that the next view asked for.
fn answer_without_a_row_then_type(
    harness: &mut Harness,
    read: ScreenReadId,
    text: &str,
) -> Vec<(ScreenReadId, ScreenSource)> {
    answer_screen_read(harness, read, input_rows(""));
    harness.type_text(text);
    harness.settle_caret();
    screen_reads(harness)
}

#[test]
fn session_stops_its_row_reads_after_three_reads_without_a_row() {
    let mut harness = herdr_with_ocr();
    let first =
        show_after_a_session_read_before_the_echo(&mut harness, "eploy the staging server and the database");
    let second = answer_without_a_row_then_type(&mut harness, first[0].0, "eploy");
    let third = answer_without_a_row_then_type(&mut harness, second[0].0, " the s");

    let fourth = answer_without_a_row_then_type(&mut harness, third[0].0, "taging");

    assert_eq!([second.len(), third.len(), fourth.len()], [1, 1, 0]);
}
