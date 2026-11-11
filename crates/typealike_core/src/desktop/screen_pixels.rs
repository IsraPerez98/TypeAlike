//! The pixels of the screen after the caret, which show what follows the caret where the field does
//! not tell it.

use std::fmt;

use super::{AfterCaret, FIELD_EDGE_GAP, ScreenRect};

/// The lines below the line of the caret that the pixels after the caret show. A ghost text wraps
/// only onto these lines, and a long suggestion needs at most about 3 lines in a narrow terminal.
const LINES_BELOW_IN_PIXELS: i32 = 4;
/// The most that a channel can change across pixels that count as one color, out of 255: about 5
/// percent. The highlight of the line of the caret in a dark theme can differ from the background
/// by less, and the faintest text, such as a rule line in a terminal, differs by more.
const SAME_COLOR_RANGE: u8 = 12;
/// The part of the caret height at the top and the bottom of the line of the caret where a column
/// of the cursor can have another color, as one in this many. The highlight of the line, or its
/// border, can end there.
const CURSOR_END_DIVISOR: i32 = 8;
/// The part of the caret height that the text of the line of the caret can reach below the caret,
/// as one in this many. The caret of a terminal of xterm.js is shorter than its line, so the
/// descenders and the cursor of the line reach below it.
const CARET_LINE_TEXT_BELOW_DIVISOR: i32 = 3;

/// The opaque color of one pixel of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PixelColor {
    /// The red channel.
    pub red: u8,
    /// The green channel.
    pub green: u8,
    /// The blue channel.
    pub blue: u8,
}

impl PixelColor {
    fn channels(self) -> [u8; 3] {
        [self.red, self.green, self.blue]
    }
}

/// The colors of the pixels of a rectangle of the screen, row by row, top row first. A platform
/// gives one pixel for each unit of [`ScreenRect`]. `Debug` shows only the rectangle, so logs never
/// hold what the screen shows.
#[derive(Clone, PartialEq, Eq)]
pub struct ScreenPixels {
    frame: ScreenRect,
    colors: Vec<PixelColor>,
}

impl ScreenPixels {
    /// The pixels of `frame`, from `colors`, row by row. `None` when `colors` does not have one
    /// color for each pixel of `frame`.
    #[must_use]
    pub fn new(frame: ScreenRect, colors: Vec<PixelColor>) -> Option<Self> {
        let width = usize::try_from(frame.right.checked_sub(frame.left)?).ok()?;
        let height = usize::try_from(frame.bottom.checked_sub(frame.top)?).ok()?;
        (width.checked_mul(height)? == colors.len()).then_some(Self { frame, colors })
    }

    /// The rectangle of the screen that the pixels show.
    #[must_use]
    pub fn frame(&self) -> ScreenRect {
        self.frame
    }

    /// The rectangle of the screen that a platform captures to tell what follows `caret` in
    /// `field`: the field from the top of the line of the caret to at most 4 lines below it.
    /// `None` when that rectangle has no area.
    #[must_use]
    pub fn area_after_caret(caret: ScreenRect, field: ScreenRect) -> Option<ScreenRect> {
        let line_height = caret.bottom.checked_sub(caret.top).filter(|height| *height > 0)?;
        let lines_bottom = caret
            .bottom
            .saturating_add(line_height.saturating_mul(LINES_BELOW_IN_PIXELS));
        let area = ScreenRect {
            left: field.left,
            top: caret.top,
            right: field.right,
            bottom: lines_bottom.min(field.bottom).max(caret.bottom),
        };
        area.has_area().then_some(area)
    }

    /// What these pixels show after `caret` in `field`, where a ghost text after the caret would
    /// show: on the line of the caret up to the right edge of the field, and on the lines below
    /// that the pixels show. `None` when the pixels do not show the line of the caret.
    ///
    /// The line of the caret is empty when each row of its pixels has one color, as an empty line
    /// and a highlight of the line do. Near the caret, a column of one color is the cursor: a bar,
    /// or the block of a terminal. The lines below are empty when all their pixels have one color.
    #[must_use]
    pub fn after_caret(&self, caret: ScreenRect, field: ScreenRect) -> Option<AfterCaret> {
        let line_height = caret.bottom.saturating_sub(caret.top);
        let caret_line = ScreenRect {
            left: caret.right,
            top: caret.top,
            right: field.right.saturating_sub(FIELD_EDGE_GAP).max(caret.right),
            bottom: caret.bottom,
        };
        if !self.shows(caret_line) {
            return None;
        }
        if self.has_text_after_cursor(caret_line) {
            return Some(AfterCaret::Text);
        }
        let lines_below = ScreenRect {
            left: field.left.saturating_add(FIELD_EDGE_GAP),
            top: caret
                .bottom
                .saturating_add(line_height / CARET_LINE_TEXT_BELOW_DIVISOR),
            right: field.right.saturating_sub(FIELD_EDGE_GAP),
            bottom: field.bottom.min(self.frame.bottom),
        };
        let is_below_empty =
            !lines_below.has_area() || self.shows(lines_below) && is_one_color(self.pixels_in(lines_below));
        Some(if is_below_empty {
            AfterCaret::Nothing
        } else {
            AfterCaret::TextBelow
        })
    }

    /// Whether `line`, the line of the caret from the caret to the right edge of the field, has a
    /// row of more than one color after the cursor. The cursor is the run of columns from the
    /// caret, at most as wide as the line is high, that each have one color between the rows at
    /// the top and the bottom of the line where the highlight of the line can end.
    fn has_text_after_cursor(&self, line: ScreenRect) -> bool {
        let line_height = line.bottom.saturating_sub(line.top);
        let cursor_end = line.left.saturating_add(line_height).min(line.right);
        let end_rows = line_height / CURSOR_END_DIVISOR;
        let inside_rows = line.top.saturating_add(end_rows)..line.bottom.saturating_sub(end_rows);
        let after_cursor = (line.left..cursor_end)
            .find(|&column| !is_one_color(inside_rows.clone().map(|row| self.color_at(column, row))))
            .unwrap_or(cursor_end);
        (line.top..line.bottom)
            .any(|row| !is_one_color((after_cursor..line.right).map(|column| self.color_at(column, row))))
    }

    /// Whether the pixels show all of `area`.
    fn shows(&self, area: ScreenRect) -> bool {
        area.left >= self.frame.left
            && area.top >= self.frame.top
            && area.right <= self.frame.right
            && area.bottom <= self.frame.bottom
    }

    /// The colors of the pixels of `area`, which the pixels show.
    fn pixels_in(&self, area: ScreenRect) -> impl Iterator<Item = PixelColor> + '_ {
        (area.top..area.bottom)
            .flat_map(move |row| (area.left..area.right).map(move |column| self.color_at(column, row)))
    }

    /// The color of the pixel at `column` and `row` on the screen, inside the frame.
    fn color_at(&self, column: i32, row: i32) -> PixelColor {
        let width = self.frame.right.saturating_sub(self.frame.left);
        let index = row
            .saturating_sub(self.frame.top)
            .saturating_mul(width)
            .saturating_add(column.saturating_sub(self.frame.left));
        usize::try_from(index)
            .ok()
            .and_then(|index| self.colors.get(index).copied())
            .expect("the caller asks only for a pixel inside the frame")
    }
}

/// Whether `colors` count as one color: no channel changes by more than [`SAME_COLOR_RANGE`].
/// No colors count as one color.
fn is_one_color(colors: impl IntoIterator<Item = PixelColor>) -> bool {
    let mut colors = colors.into_iter().map(PixelColor::channels);
    let Some(first) = colors.next() else {
        return true;
    };
    let (lowest, highest) = colors.fold((first, first), |(lowest, highest), channels| {
        (
            [0, 1, 2].map(|channel| lowest[channel].min(channels[channel])),
            [0, 1, 2].map(|channel| highest[channel].max(channels[channel])),
        )
    });
    lowest
        .iter()
        .zip(highest)
        .all(|(low, high)| high.abs_diff(*low) <= SAME_COLOR_RANGE)
}

impl fmt::Debug for ScreenPixels {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScreenPixels")
            .field("frame", &self.frame)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKGROUND: PixelColor = gray(30);
    const TEXT: PixelColor = gray(220);
    const CURSOR: PixelColor = gray(190);
    /// The highlight of the line of the caret, as faint as in a dark theme.
    const HIGHLIGHT: PixelColor = gray(39);
    const RULE: PixelColor = gray(50);
    /// A field of 120 x 80 pixels at the top left of the screen.
    const FIELD: ScreenRect = rect(0, 0, 120, 80);
    /// A caret 16 pixels high at 40, on the line from 10 to 26.
    const CARET: ScreenRect = rect(40, 10, 40, 26);

    const fn gray(level: u8) -> PixelColor {
        PixelColor {
            red: level,
            green: level,
            blue: level,
        }
    }

    const fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right,
            bottom,
        }
    }

    /// A picture of the screen, painted one rectangle at a time on the background.
    struct Canvas {
        frame: ScreenRect,
        colors: Vec<PixelColor>,
    }

    impl Canvas {
        fn of(frame: ScreenRect) -> Self {
            let width = usize::try_from(frame.right - frame.left).unwrap();
            let height = usize::try_from(frame.bottom - frame.top).unwrap();
            Self {
                frame,
                colors: vec![BACKGROUND; width * height],
            }
        }

        fn paint(mut self, area: ScreenRect, color: PixelColor) -> Self {
            let width = self.frame.right - self.frame.left;
            for y in area.top.max(self.frame.top)..area.bottom.min(self.frame.bottom) {
                for x in area.left.max(self.frame.left)..area.right.min(self.frame.right) {
                    let index = (y - self.frame.top) * width + (x - self.frame.left);
                    self.colors[usize::try_from(index).unwrap()] = color;
                }
            }
            self
        }

        fn pixels(self) -> ScreenPixels {
            ScreenPixels::new(self.frame, self.colors).unwrap()
        }
    }

    fn after_caret_in(canvas: Canvas) -> Option<AfterCaret> {
        canvas.pixels().after_caret(CARET, FIELD)
    }

    #[test]
    fn colors_must_fill_the_frame() {
        let frames = [rect(0, 0, 3, 2), rect(0, 0, 2, 2), rect(0, 0, 3, -2)];

        let pixels = frames.map(|frame| ScreenPixels::new(frame, vec![BACKGROUND; 6]).is_some());

        assert_eq!(pixels, [true, false, false]);
    }

    #[test]
    fn area_after_caret_spans_the_field_from_the_caret_line_to_four_lines_below_it() {
        let fields = [FIELD, rect(0, 0, 120, 40), rect(0, 0, 120, 20)];

        let areas = fields.map(|field| ScreenPixels::area_after_caret(CARET, field));

        assert_eq!(
            areas,
            [
                Some(rect(0, 10, 120, 80)),
                Some(rect(0, 10, 120, 40)),
                Some(rect(0, 10, 120, 26)),
            ]
        );
    }

    #[test]
    fn caret_without_height_has_no_area_after_it() {
        let area = ScreenPixels::area_after_caret(rect(40, 10, 40, 10), FIELD);

        assert_eq!(area, None);
    }

    #[test]
    fn background_after_the_caret_shows_nothing() {
        let after = after_caret_in(Canvas::of(FIELD));

        assert_eq!(after, Some(AfterCaret::Nothing));
    }

    #[test]
    fn text_on_the_line_of_the_caret_follows_the_caret() {
        let canvas = Canvas::of(FIELD).paint(rect(90, 14, 96, 22), TEXT);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Text));
    }

    #[test]
    fn character_right_after_the_caret_follows_the_caret() {
        let canvas = Canvas::of(FIELD).paint(rect(41, 14, 43, 24), TEXT);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Text));
    }

    #[test]
    fn bar_cursor_at_the_caret_is_no_text() {
        let canvas = Canvas::of(FIELD).paint(rect(39, 10, 41, 26), CURSOR);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Nothing));
    }

    #[test]
    fn block_cursor_of_a_terminal_is_no_text() {
        let canvas = Canvas::of(FIELD).paint(rect(39, 9, 48, 27), CURSOR);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Nothing));
    }

    #[test]
    fn highlight_of_the_line_of_the_caret_is_no_text() {
        let canvas = Canvas::of(FIELD)
            .paint(rect(0, 11, 120, 26), HIGHLIGHT)
            .paint(rect(39, 11, 41, 26), CURSOR);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Nothing));
    }

    #[test]
    fn border_of_the_highlight_of_the_line_of_the_caret_is_no_text() {
        let canvas = Canvas::of(FIELD)
            .paint(rect(0, 10, 120, 12), RULE)
            .paint(rect(0, 24, 120, 26), RULE)
            .paint(rect(39, 10, 41, 26), CURSOR);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Nothing));
    }

    #[test]
    fn line_of_one_color_far_from_the_caret_follows_the_caret() {
        let canvas = Canvas::of(FIELD).paint(rect(100, 0, 101, 80), RULE);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Text));
    }

    #[test]
    fn text_before_the_caret_is_not_after_it() {
        let canvas = Canvas::of(FIELD).paint(rect(4, 14, 38, 24), TEXT);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Nothing));
    }

    #[test]
    fn text_at_the_right_edge_of_the_field_where_no_ghost_text_shows_is_no_text() {
        let canvas = Canvas::of(FIELD).paint(rect(118, 0, 120, 80), RULE);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Nothing));
    }

    #[test]
    fn text_before_a_caret_shorter_than_its_line_is_not_text_below() {
        let canvas = Canvas::of(FIELD).paint(rect(4, 14, 38, 31), TEXT);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::Nothing));
    }

    #[test]
    fn text_on_a_line_below_the_caret_is_text_below() {
        let canvas = Canvas::of(FIELD).paint(rect(10, 46, 30, 54), TEXT);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::TextBelow));
    }

    #[test]
    fn rule_line_below_the_caret_is_text_below() {
        let canvas = Canvas::of(FIELD).paint(rect(0, 33, 120, 34), RULE);

        assert_eq!(after_caret_in(canvas), Some(AfterCaret::TextBelow));
    }

    #[test]
    fn pixels_that_end_above_the_bottom_of_the_field_tell_about_the_lines_that_they_show() {
        let canvas = Canvas::of(rect(0, 10, 120, 42)).paint(rect(10, 36, 30, 42), TEXT);

        let after = canvas.pixels().after_caret(CARET, FIELD);

        assert_eq!(after, Some(AfterCaret::TextBelow));
    }

    #[test]
    fn pixels_narrower_than_the_lines_below_do_not_let_the_ghost_text_wrap() {
        let canvas = Canvas::of(rect(20, 10, 120, 80));

        let after = canvas.pixels().after_caret(CARET, FIELD);

        assert_eq!(after, Some(AfterCaret::TextBelow));
    }

    #[test]
    fn pixels_without_the_whole_line_of_the_caret_tell_nothing() {
        let frames = [rect(0, 12, 120, 80), rect(0, 0, 100, 80), rect(50, 0, 120, 80)];

        let seen = frames.map(|frame| Canvas::of(frame).pixels().after_caret(CARET, FIELD));

        assert_eq!(seen, [None, None, None]);
    }

    #[test]
    fn debug_does_not_reveal_the_pixels() {
        let pixels = Canvas::of(rect(0, 0, 2, 1)).pixels();

        let shown = format!("{pixels:?}");

        assert_eq!(
            shown,
            "ScreenPixels { frame: ScreenRect { left: 0, top: 0, right: 2, bottom: 1 }, .. }"
        );
    }
}
