//! Tests of the anchor of the mirror in a field that gives no caret, which the mirror shows below,
//! else above.

use super::harness::{Harness, completed};
use crate::controller::{CaretLook, CaretRead};
use crate::desktop::{ScreenRect, Surroundings};
use crate::key::Key;
use crate::settings::Settings;

const WINDOW: ScreenRect = ScreenRect {
    left: 0,
    top: 33,
    right: 1512,
    bottom: 908,
};

/// Types "D" in a field of `surroundings` whose frame is [`WINDOW`] and which gives no caret, shows
/// the suggestion, and returns the anchor of the mirror of the shown view.
fn mirror_anchor_in(surroundings: Surroundings) -> Option<ScreenRect> {
    let mut harness = Harness::new(Settings::default());
    harness.shell.surroundings = surroundings;
    harness.shell.caret_look = CaretLook::Now(CaretRead {
        field: Some(WINDOW),
        ..CaretRead::default()
    });
    harness.press(Key::Char('D'));
    harness.answer(completed("eploy the staging server"));
    harness.settle_caret();

    harness
        .shell
        .ghost_view
        .clone()
        .expect("a shown ghost text")
        .mirror_anchor
}

#[test]
fn mirror_of_a_field_that_shows_its_text_only_on_the_screen_anchors_at_its_bottom_edge() {
    let terminal = Surroundings {
        has_unreadable_text: true,
        shows_text_only_on_screen: true,
        ..Surroundings::default()
    };

    let anchor = mirror_anchor_in(terminal);

    assert_eq!(anchor, Some(ScreenRect { top: 908, ..WINDOW }));
}

#[test]
fn mirror_of_an_editor_that_gives_no_caret_stays_next_to_the_field() {
    let editor = Surroundings {
        has_unreadable_text: true,
        ..Surroundings::default()
    };

    let anchor = mirror_anchor_in(editor);

    assert_eq!(anchor, None);
}
