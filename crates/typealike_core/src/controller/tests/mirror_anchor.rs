//! Tests of the anchor of the mirror in a field that gives no caret, which the mirror shows below,
//! else above: the block cursor that a look at the pixels of a terminal finds, else the bottom
//! edge of the terminal.

use super::harness::{Call, Harness, completed};
use crate::controller::{CaretLook, CaretRead, Delivery, FocusChange};
use crate::desktop::{
    CursorLook, OcrAvailability, ScreenRead, ScreenReadId, ScreenRect, ScreenSource, ScreenText, Surroundings,
};
use crate::engine::{MirrorAnchor, MirrorAnchorKind};
use crate::key::Key;
use crate::settings::Settings;

const WINDOW: ScreenRect = ScreenRect {
    left: 0,
    top: 33,
    right: 1512,
    bottom: 908,
};
const CURSOR: ScreenRect = cell(276, 713);
const CURSOR_ON_THE_NEXT_ROW: ScreenRect = cell(256, 733);
const BOTTOM_EDGE: MirrorAnchor = MirrorAnchor {
    frame: ScreenRect { top: 908, ..WINDOW },
    kind: MirrorAnchorKind::FieldBottom,
};

/// A cell of a terminal font of 9 x 20 points with its top left corner at `left`, `top`.
const fn cell(left: i32, top: i32) -> ScreenRect {
    ScreenRect {
        left,
        top,
        right: left + 9,
        bottom: top + 20,
    }
}

fn terminal() -> Surroundings {
    Surroundings {
        app_id: Some("dev.herdr.app".to_owned()),
        app_name: Some("Herdr".to_owned()),
        has_unreadable_text: true,
        shows_text_only_on_screen: true,
        ..Surroundings::default()
    }
}

/// A harness in a field of `surroundings` whose frame is [`WINDOW`] and which gives no caret, on
/// a platform that can capture the screen.
fn harness_in(surroundings: Surroundings) -> Harness {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = surroundings;
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        field: Some(WINDOW),
        ..CaretRead::default()
    });
    harness.shell.ocr = OcrAvailability::Ready;
    harness
}

/// The reads of the screen that the controller asked for since the last call.
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

/// The looks at the cursor that the controller asked for since the last call.
fn cursor_looks(harness: &mut Harness) -> Vec<ScreenReadId> {
    screen_reads(harness)
        .into_iter()
        .filter(|(_, source)| *source == ScreenSource::Cursor)
        .map(|(read, _)| read)
        .collect()
}

fn answer_read(
    harness: &mut Harness,
    read: ScreenReadId,
    text: Option<ScreenText>,
    blocks: Option<Vec<ScreenRect>>,
) {
    let cursor = blocks.map(|blocks| CursorLook {
        field: WINDOW,
        blocks,
    });
    harness.controller.on_delivery(
        Delivery::Screen(ScreenRead {
            id: read,
            text,
            cursor,
        }),
        &mut harness.shell,
    );
}

/// Types "D" in `harness`, answers the read of the accessibility tree of the key without text,
/// then the OCR read after it with a look that found `blocks`, shows the suggestion, and returns
/// the anchor of the mirror of the shown view.
fn anchor_after_a_session_read(
    harness: &mut Harness,
    blocks: Option<Vec<ScreenRect>>,
) -> Option<MirrorAnchor> {
    harness.press(Key::Char('D'));
    let (tree_read, _) = screen_reads(harness)
        .into_iter()
        .next()
        .expect("the first key reads the screen");
    answer_read(harness, tree_read, None, None);
    let (ocr_read, _) = screen_reads(harness)
        .into_iter()
        .find(|(_, source)| matches!(source, ScreenSource::Ocr(_)))
        .expect("a tree without text gets an OCR read");
    answer_read(harness, ocr_read, Some(ScreenText::default()), blocks);
    shown_anchor(harness)
}

/// Shows a suggestion after the typed text, and returns the anchor of the mirror of the view.
fn shown_anchor(harness: &mut Harness) -> Option<MirrorAnchor> {
    harness.answer(completed("eploy the staging server"));
    harness.settle_caret();
    harness
        .shell
        .ghost_view
        .clone()
        .expect("a shown ghost text")
        .mirror_anchor
}

/// Types `text` after a session read that found [`CURSOR`], and returns the looks at the cursor
/// that the shown suggestions asked for.
fn looks_after_typing(text: &str) -> Vec<ScreenReadId> {
    let mut harness = harness_in(terminal());
    anchor_after_a_session_read(&mut harness, Some(vec![CURSOR]));
    harness.take_calls();
    harness.type_text(text);
    shown_anchor(&mut harness);
    cursor_looks(&mut harness)
}

#[test]
fn mirror_of_a_terminal_without_a_found_cursor_anchors_at_its_bottom_edge() {
    let mut harness = harness_in(terminal());

    let anchor = anchor_after_a_session_read(&mut harness, None);

    assert_eq!(anchor, Some(BOTTOM_EDGE));
}

#[test]
fn mirror_of_an_editor_that_gives_no_caret_stays_next_to_the_field() {
    let editor = Surroundings {
        has_unreadable_text: true,
        ..Surroundings::default()
    };
    let mut harness = harness_in(editor);
    harness.press(Key::Char('D'));

    let anchor = shown_anchor(&mut harness);

    assert_eq!(anchor, None);
}

#[test]
fn mirror_shows_below_the_cursor_that_the_session_read_finds() {
    let mut harness = harness_in(terminal());

    let anchor = anchor_after_a_session_read(&mut harness, Some(vec![CURSOR]));

    assert_eq!(
        anchor,
        Some(MirrorAnchor {
            frame: CURSOR,
            kind: MirrorAnchorKind::Cursor,
        })
    );
}

#[test]
fn look_with_two_blocks_leaves_the_mirror_at_the_bottom_edge() {
    let mut harness = harness_in(terminal());

    let anchor = anchor_after_a_session_read(&mut harness, Some(vec![CURSOR, CURSOR_ON_THE_NEXT_ROW]));

    assert_eq!(anchor, Some(BOTTOM_EDGE));
}

#[test]
fn moved_field_drops_the_cursor_of_an_earlier_frame() {
    let mut harness = harness_in(terminal());
    anchor_after_a_session_read(&mut harness, Some(vec![CURSOR]));
    let moved = ScreenRect {
        left: 100,
        right: 1100,
        ..WINDOW
    };
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        field: Some(moved),
        ..CaretRead::default()
    });

    let anchor = shown_anchor(&mut harness);

    assert_eq!(
        anchor,
        Some(MirrorAnchor {
            frame: ScreenRect { top: 908, ..moved },
            kind: MirrorAnchorKind::FieldBottom,
        })
    );
}

#[test]
fn look_starts_after_twenty_typed_characters() {
    let looks = [
        looks_after_typing("eploy the staging"),
        looks_after_typing("eploy the staging server"),
    ];

    assert_eq!(looks.map(|looks| looks.len()), [0, 1]);
}

#[test]
fn look_of_the_next_prompt_after_enter_moves_the_mirror() {
    let mut harness = harness_in(terminal());
    anchor_after_a_session_read(&mut harness, Some(vec![CURSOR]));
    harness.press(Key::Enter);
    harness.take_calls();

    let anchor = anchor_after_a_session_read(&mut harness, Some(vec![CURSOR_ON_THE_NEXT_ROW]));

    assert_eq!(
        anchor,
        Some(MirrorAnchor {
            frame: CURSOR_ON_THE_NEXT_ROW,
            kind: MirrorAnchorKind::Cursor,
        })
    );
}

#[test]
fn look_that_finds_no_block_keeps_the_cursor_of_the_session() {
    let mut harness = harness_in(terminal());
    anchor_after_a_session_read(&mut harness, Some(vec![CURSOR]));
    harness.type_text("eploy the staging server");
    shown_anchor(&mut harness);
    let look = cursor_looks(&mut harness)[0];

    answer_read(&mut harness, look, None, Some(vec![]));

    assert_eq!(
        shown_anchor(&mut harness),
        Some(MirrorAnchor {
            frame: CURSOR,
            kind: MirrorAnchorKind::Kept,
        })
    );
}

#[test]
fn look_moves_the_mirror_below_the_new_row_of_the_cursor() {
    let mut harness = harness_in(terminal());
    anchor_after_a_session_read(&mut harness, Some(vec![CURSOR]));
    harness.type_text("eploy the staging server");
    shown_anchor(&mut harness);
    let look = cursor_looks(&mut harness)[0];

    answer_read(&mut harness, look, None, Some(vec![CURSOR_ON_THE_NEXT_ROW]));

    assert_eq!(
        harness
            .shell
            .ghost_view
            .clone()
            .expect("a shown ghost text")
            .mirror_anchor,
        Some(MirrorAnchor {
            frame: CURSOR_ON_THE_NEXT_ROW,
            kind: MirrorAnchorKind::Cursor,
        })
    );
}

#[test]
fn look_on_the_same_row_keeps_the_mirror_in_place() {
    let mut harness = harness_in(terminal());
    anchor_after_a_session_read(&mut harness, Some(vec![CURSOR]));
    harness.type_text("eploy the staging server");
    shown_anchor(&mut harness);
    let look = cursor_looks(&mut harness)[0];
    let further_on_the_row = ScreenRect {
        left: CURSOR.left + 216,
        right: CURSOR.right + 216,
        ..CURSOR
    };

    answer_read(&mut harness, look, None, Some(vec![further_on_the_row]));

    assert_eq!(
        shown_anchor(&mut harness),
        Some(MirrorAnchor {
            frame: CURSOR,
            kind: MirrorAnchorKind::Cursor,
        })
    );
}

#[test]
fn new_session_drops_the_cursor_of_the_session_before() {
    let mut harness = harness_in(terminal());
    anchor_after_a_session_read(&mut harness, Some(vec![CURSOR]));
    harness.shell.focus_change = FocusChange::Changed;

    let anchor = anchor_after_a_session_read(&mut harness, None);

    assert_eq!(anchor, Some(BOTTOM_EDGE));
}

#[test]
fn found_cursor_does_not_change_the_prompt() {
    let prompts = [None, Some(vec![CURSOR])].map(|blocks| {
        let mut harness = harness_in(terminal());
        anchor_after_a_session_read(&mut harness, blocks);
        harness.last_request().expect("a request")
    });

    assert_eq!(prompts[0], prompts[1]);
}
