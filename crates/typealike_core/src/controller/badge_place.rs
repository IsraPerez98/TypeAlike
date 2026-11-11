//! Where the badge at the caret shows: at the end of the line of the caret, inside its field. The
//! ghost text after the caret ends before it, and a line that ends at the caret has no text there.

use crate::desktop::{FIELD_EDGE_GAP, ScreenRect};

/// Returns the frame of a square badge of `side` at the end of the line of `caret` in `field`:
/// [`FIELD_EDGE_GAP`] inside the right edge of the field, in the middle of the line of the caret
/// and inside the field. `None` when the line of the caret has no room for the badge and a gap
/// after the caret, when that line is outside the field, or when the field is lower than the
/// badge.
pub(super) fn line_end_badge(caret: ScreenRect, field: ScreenRect, side: i32) -> Option<ScreenRect> {
    let right = field.right.saturating_sub(FIELD_EDGE_GAP);
    let left = right.saturating_sub(side);
    let lowest_top = field.bottom.saturating_sub(side);
    let has_room_after_caret = left.saturating_sub(caret.right) >= FIELD_EDGE_GAP;
    let is_line_in_field = caret.top < field.bottom && caret.bottom > field.top;
    let is_field_high_enough = lowest_top >= field.top;
    (has_room_after_caret && is_line_in_field && is_field_high_enough).then(|| {
        let line_middle = caret
            .top
            .saturating_add(caret.bottom.saturating_sub(caret.top) / 2);
        let top = line_middle.saturating_sub(side / 2).clamp(field.top, lowest_top);
        ScreenRect {
            left,
            top,
            right,
            bottom: top.saturating_add(side),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text field of Notepad at a scale of 100 %, in physical pixels, from a log of a real app
    /// test. Its first line starts at its top edge, just below the menu bar.
    const NOTEPAD: ScreenRect = ScreenRect {
        left: 208,
        top: 201,
        right: 1075,
        bottom: 639,
    };
    /// The side of the badge on a line of 18 pixels at a scale of 100 %.
    const SIDE: i32 = 18;

    fn caret(left: i32, top: i32, height: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right: left,
            bottom: top + height,
        }
    }

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn badge_shows_on_the_line_of_the_caret_at_the_right_edge_of_its_field() {
        let single_line_field = rect(224, 315, 658, 353);
        let cases = [
            (caret(280, 201, 18), NOTEPAD),
            (caret(480, 237, 18), NOTEPAD),
            (caret(480, 621, 18), NOTEPAD),
            (caret(400, 324, 20), single_line_field),
        ];

        let frames = cases.map(|(caret, field)| line_end_badge(caret, field, caret.bottom - caret.top));

        assert_eq!(
            frames,
            [
                Some(rect(1053, 201, 1071, 219)),
                Some(rect(1053, 237, 1071, 255)),
                Some(rect(1053, 621, 1071, 639)),
                Some(rect(634, 324, 654, 344)),
            ]
        );
    }

    #[test]
    fn badge_shows_in_the_middle_of_a_line_that_is_higher_than_the_badge() {
        let tall_caret = caret(480, 237, 40);

        let frame = line_end_badge(tall_caret, NOTEPAD, 36);

        assert_eq!(frame, Some(rect(1035, 239, 1071, 275)));
    }

    #[test]
    fn badge_higher_than_a_line_at_the_edge_of_the_field_stays_inside_the_field() {
        let cases = [caret(480, 201, 12), caret(480, 627, 12)];

        let frames = cases.map(|caret| line_end_badge(caret, NOTEPAD, 16));

        assert_eq!(
            frames,
            [Some(rect(1055, 201, 1071, 217)), Some(rect(1055, 623, 1071, 639)),]
        );
    }

    #[test]
    fn badge_needs_a_gap_between_the_caret_and_itself() {
        let carets = [caret(1049, 201, 18), caret(1050, 201, 18), caret(1075, 201, 18)];

        let frames = carets.map(|caret| line_end_badge(caret, NOTEPAD, SIDE));

        assert_eq!(frames, [Some(rect(1053, 201, 1071, 219)), None, None]);
    }

    #[test]
    fn badge_does_not_show_for_a_caret_line_outside_its_field() {
        let carets = [caret(480, 160, 18), caret(480, 639, 18)];

        let frames = carets.map(|caret| line_end_badge(caret, NOTEPAD, SIDE));

        assert_eq!(frames, [None, None]);
    }

    #[test]
    fn badge_does_not_show_in_a_field_lower_than_the_badge() {
        let low_field = rect(208, 201, 1075, 215);

        let frame = line_end_badge(caret(480, 201, 14), low_field, 16);

        assert_eq!(frame, None);
    }
}
