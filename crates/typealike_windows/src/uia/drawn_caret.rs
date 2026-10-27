//! The caret that the focused control draws itself.
//!
//! Apps without a system caret draw a caret of their own. The caret has the color and the font face
//! of the text before it. In a terminal of xterm.js, the caret moves up to the row that holds it.

use typealike_core::desktop::{TextColor, is_line_break};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationTextRange, TextPatternRangeEndpoint, TextPatternRangeEndpoint_End,
    TextPatternRangeEndpoint_Start, TextUnit_Character, TextUnit_Line, UIA_BackgroundColorAttributeId,
    UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId, UIA_FontNameAttributeId,
    UIA_ForegroundColorAttributeId,
};

use super::com::SafeArray;
use super::{Uia, WHOLE_RANGE, caret_range, frame_with_area, is_password, is_terminal_input, text_of};
use crate::caret::{self, Caret};
use crate::display::round_to_pixel;
use crate::pixels::Rgb;
use crate::utf16::whole_chars;

/// The numbers that describe one bounding rectangle: left, top, width and height.
const VALUES_PER_RECTANGLE: usize = 4;
const CHARACTERS_BEFORE_CARET: i32 = -1;
const CHARACTERS_AFTER_CARET: i32 = 1;
/// The class of the screen of a terminal of xterm.js, which holds its rows.
const XTERM_SCREEN_CLASS: &str = "xterm-screen";

/// The caret of the focused control, and the frame of that control.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FocusedCaret {
    /// The caret, or `None` when the control shows no caret that TypeAlike can find.
    pub(crate) caret: Option<Caret>,
    /// The frame of the control in physical screen pixels, or `None` when UIA gives no frame with
    /// an area.
    pub(crate) frame: Option<RECT>,
}

impl Uia {
    /// Locates the caret of the focused control: the system caret, else the caret that the control
    /// draws itself. The caret has the color and the font face of the text before it, when the
    /// control tells them. The frame of the control comes with it.
    pub(crate) fn caret(&self) -> FocusedCaret {
        let element = self.focused_element();
        FocusedCaret {
            caret: self.styled_caret(element.as_ref()),
            frame: element.as_ref().and_then(frame_with_area),
        }
    }

    fn styled_caret(&self, element: Option<&IUIAutomationElement>) -> Option<Caret> {
        let caret = caret::system_caret().or_else(|| self.drawn_caret(element?))?;
        Some(element.map(text_style).unwrap_or_default().apply_to(caret))
    }

    /// Locates the caret that `element`, the focused control, draws itself: the caret of the caret
    /// object of its window, else the right edge of the character before the caret, or the start
    /// of the line after a line break. In a terminal of xterm.js, the caret moves up to the row
    /// that holds it.
    pub(super) fn drawn_caret(&self, element: &IUIAutomationElement) -> Option<Caret> {
        let caret = caret::accessible_caret().or_else(|| caret_of(element))?;
        Some(match self.terminal_input(element) {
            Some(input) => input.align(caret),
            None => caret,
        })
    }

    /// Reads the hidden input box of a terminal of xterm.js and the top of its screen, when
    /// `element` is that input box.
    fn terminal_input(&self, element: &IUIAutomationElement) -> Option<TerminalInput> {
        if !is_terminal_input(element) {
            return None;
        }
        let screen = self.terminal_screen(element)?;
        // SAFETY: plain calls on interfaces of this thread.
        let (frame, screen_frame) = unsafe {
            (
                element.CurrentBoundingRectangle().ok()?,
                screen.CachedBoundingRectangle().ok()?,
            )
        };
        Some(TerminalInput {
            frame,
            screen_top: screen_frame.top,
        })
    }

    /// Finds the screen of the terminal of xterm.js that holds `element`, with its bounds in the
    /// cache.
    fn terminal_screen(&self, element: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        // SAFETY: plain calls on interfaces of this thread.
        unsafe {
            let is_screen = self
                .automation
                .CreatePropertyCondition(UIA_ClassNamePropertyId, &VARIANT::from(XTERM_SCREEN_CLASS))
                .ok()?;
            let walker = self.automation.CreateTreeWalker(&is_screen).ok()?;
            let cache = self.cache_request(&[UIA_ClassNamePropertyId, UIA_BoundingRectanglePropertyId])?;
            let screen = walker.NormalizeElementBuildCache(element, &cache).ok()?;
            // UIA gives the root of the tree when no element above matches.
            (screen.CachedClassName().ok()? == XTERM_SCREEN_CLASS).then_some(screen)
        }
    }
}

/// Locates the caret in `element` at the right edge of the character before it. At the start of
/// the text or after a line break, the caret is at the start of its line.
fn caret_of(element: &IUIAutomationElement) -> Option<Caret> {
    if is_password(element) {
        return None;
    }
    let caret = caret_range(element)?;
    // Many apps give no bounds for the empty range at the caret, so measure a character or a line.
    match character_beside(&caret, TextPatternRangeEndpoint_Start, CHARACTERS_BEFORE_CARET) {
        Some(before) if !ends_line(&before) => caret_from_rectangles(&bounding_rectangles(&before)?),
        _ => line_start_caret(&caret),
    }
}

/// Locates the caret at the start of its line: at the left edge of the character after `caret`,
/// else at the left edge of the line of `caret`.
fn line_start_caret(caret: &IUIAutomationTextRange) -> Option<Caret> {
    character_beside(caret, TextPatternRangeEndpoint_End, CHARACTERS_AFTER_CARET)
        .and_then(|after| caret_at_line_start(&bounding_rectangles(&after)?, MeasuredLine::OfCaret))
        .or_else(|| line_box_caret(caret))
}

/// Locates the caret at the left edge of the line of `caret`. Some apps give the line above for a
/// caret after a line break at the end of the text.
fn line_box_caret(caret: &IUIAutomationTextRange) -> Option<Caret> {
    // SAFETY: plain calls on interfaces of this thread.
    let (line, start_order) = unsafe {
        let line = caret.Clone().ok()?;
        line.ExpandToEnclosingUnit(TextUnit_Line).ok()?;
        let start_order = line
            .CompareEndpoints(
                TextPatternRangeEndpoint_Start,
                caret,
                TextPatternRangeEndpoint_Start,
            )
            .ok()?;
        (line, start_order)
    };
    caret_at_line_start(
        &bounding_rectangles(&line)?,
        MeasuredLine::from_start_order(start_order),
    )
}

/// The range of `caret` with its `endpoint` moved by `characters`, so that it also holds the
/// character next to the caret on that side. `None` when no character is there, at the start or
/// at the end of the text.
fn character_beside(
    caret: &IUIAutomationTextRange,
    endpoint: TextPatternRangeEndpoint,
    characters: i32,
) -> Option<IUIAutomationTextRange> {
    // SAFETY: plain calls on interfaces of this thread.
    unsafe {
        let range = caret.Clone().ok()?;
        let moved = range
            .MoveEndpointByUnit(endpoint, TextUnit_Character, characters)
            .ok()?;
        (moved != 0).then_some(range)
    }
}

/// Whether the text of `range` ends with a line break.
fn ends_line(range: &IUIAutomationTextRange) -> bool {
    // SAFETY: plain call on an interface of this thread.
    unsafe { range.GetText(WHOLE_RANGE) }.is_ok_and(|text| whole_chars(&text).ends_with(is_line_break))
}

fn character_before_caret(element: &IUIAutomationElement) -> Option<IUIAutomationTextRange> {
    let range = caret_range(element)?;
    // SAFETY: plain call on an interface of this thread.
    unsafe {
        range.MoveEndpointByUnit(
            TextPatternRangeEndpoint_Start,
            TextUnit_Character,
            CHARACTERS_BEFORE_CARET,
        )
    }
    .ok()?;
    Some(range)
}

/// How the text before the caret looks, as far as the control tells it.
#[derive(Debug, Clone, Default, PartialEq)]
struct TextStyle {
    color: Option<TextColor>,
    font_face: Option<String>,
}

impl TextStyle {
    /// Gives `caret` this style. The face that the window of a system caret reports stays.
    fn apply_to(self, caret: Caret) -> Caret {
        Caret {
            font_face: caret.font_face.or(self.font_face),
            text_color: self.color,
            ..caret
        }
    }
}

/// Reads the style of the character before the caret of `element`. No style for the hidden input
/// box of a terminal of xterm.js, which does not show its text.
fn text_style(element: &IUIAutomationElement) -> TextStyle {
    if is_terminal_input(element) {
        return TextStyle::default();
    }
    let Some(range) = character_before_caret(element) else {
        return TextStyle::default();
    };
    // SAFETY: plain calls on an interface of this thread.
    let (text, background, font_face) = unsafe {
        (
            range.GetAttributeValue(UIA_ForegroundColorAttributeId),
            range.GetAttributeValue(UIA_BackgroundColorAttributeId),
            range.GetAttributeValue(UIA_FontNameAttributeId),
        )
    };
    shown_style(
        text.ok().as_ref().and_then(colorref),
        background.ok().as_ref().and_then(colorref),
        font_face.ok().as_ref().and_then(text_of),
    )
}

/// Reads a `COLORREF` attribute value. `None` when UIA gives a value of another type: the value
/// for an attribute that the control does not support, or for colors that change in the range.
fn colorref(value: &VARIANT) -> Option<u32> {
    i32::try_from(value).ok().map(i32::cast_unsigned)
}

/// The style of text in the `COLORREF` `text` on the `COLORREF` `background`, in `font_face`. No
/// style when both colors are the same: Chromium drops the alpha of the transparent text of a
/// hidden input box. An empty face is no face.
fn shown_style(text: Option<u32>, background: Option<u32>, font_face: Option<String>) -> TextStyle {
    if text.is_some() && text == background {
        return TextStyle::default();
    }
    TextStyle {
        color: text.map(|text| Rgb::from_colorref(text).into()),
        font_face: font_face.filter(|face| !face.is_empty()),
    }
}

fn bounding_rectangles(range: &IUIAutomationTextRange) -> Option<Vec<f64>> {
    // SAFETY: plain call on an interface of this thread; the array is owned right after.
    let array = SafeArray(unsafe { range.GetBoundingRectangles() }.ok()?);
    array.doubles()
}

/// Builds the caret at the right edge of the last of `rectangles`, each given as left, top, width
/// and height in physical screen pixels. `None` when a value or an edge of that rectangle is not a
/// finite number.
fn caret_from_rectangles(rectangles: &[f64]) -> Option<Caret> {
    let [left, top, right, bottom] = pixel_edges(*rectangles.as_chunks::<VALUES_PER_RECTANGLE>().0.last()?)?;
    Caret::from_corners(POINT { x: left, y: top }, POINT { x: right, y: bottom }, None)
}

/// The line of a box that marks the start of a line, as seen from the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MeasuredLine {
    /// The line of the caret.
    OfCaret,
    /// The line above the caret, which ends with the line break before the caret.
    Above,
}

impl MeasuredLine {
    /// The line that starts at `start_order` from the caret: negative before it, 0 at it and
    /// positive after it. A line that starts before a caret at the start of its line is the line
    /// above.
    fn from_start_order(start_order: i32) -> Self {
        if start_order < 0 {
            Self::Above
        } else {
            Self::OfCaret
        }
    }
}

/// Builds the caret at the left edge of the first of `rectangles`, which marks the start of
/// `line`, each given as left, top, width and height in physical screen pixels. A caret below the
/// line above is one line lower. `None` when a value or an edge of that rectangle is not a finite
/// number.
fn caret_at_line_start(rectangles: &[f64], line: MeasuredLine) -> Option<Caret> {
    let [left, top, _, bottom] = pixel_edges(*rectangles.as_chunks::<VALUES_PER_RECTANGLE>().0.first()?)?;
    let (top, bottom) = match line {
        MeasuredLine::OfCaret => (top, bottom),
        MeasuredLine::Above => (bottom, bottom.checked_add(bottom.checked_sub(top)?)?),
    };
    Caret::from_corners(POINT { x: left, y: top }, POINT { x: left, y: bottom }, None)
}

/// The left, top, right and bottom edges in whole pixels of the rectangle with `left`, `top`,
/// `width` and `height`. `None` when a value or an edge is not a finite number.
fn pixel_edges([left, top, width, height]: [f64; VALUES_PER_RECTANGLE]) -> Option<[i32; 4]> {
    let edges = [left, top, left + width, top + height];
    edges
        .into_iter()
        .all(f64::is_finite)
        .then(|| edges.map(round_to_pixel))
}

/// The hidden input box of a terminal of xterm.js, which the terminal moves to the cell of its
/// cursor, and the screen of the terminal.
#[derive(Debug, Clone, Copy)]
struct TerminalInput {
    /// The frame of the input box, which is one row high.
    frame: RECT,
    /// The top of the screen, where the first row starts.
    screen_top: i32,
}

impl TerminalInput {
    /// Moves `caret`, the caret in the input box, up to the row that holds the input box. VS Code
    /// draws the input box below the top of that row.
    fn align(self, caret: Caret) -> Caret {
        let row_height = self.frame.bottom.saturating_sub(self.frame.top);
        let below_row = self
            .frame
            .top
            .saturating_sub(self.screen_top)
            .checked_rem_euclid(row_height)
            .unwrap_or_default();
        Caret {
            origin: POINT {
                x: caret.origin.x,
                y: caret.origin.y.saturating_sub(below_row),
            },
            ..caret
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_starts_at_right_edge_of_last_rectangle() {
        let rectangles = [10.0, 20.0, 5.0, 16.0, 100.0, 40.0, 7.0, 18.0];

        let caret = caret_from_rectangles(&rectangles);

        assert_eq!(
            caret,
            Some(Caret {
                origin: POINT { x: 107, y: 40 },
                height: 18,
                font_face: None,
                text_color: None,
            })
        );
    }

    #[test]
    fn fractional_bounds_round_to_pixels() {
        let rectangles = [100.4, 40.6, 7.2, 18.0];

        let caret = caret_from_rectangles(&rectangles);

        assert_eq!(
            caret,
            Some(Caret {
                origin: POINT { x: 108, y: 41 },
                height: 18,
                font_face: None,
                text_color: None,
            })
        );
    }

    #[test]
    fn caret_at_the_start_of_its_line_is_at_the_left_edge_of_the_first_rectangle() {
        let rectangles = [40.0, 120.0, 8.0, 18.0, 200.0, 138.0, 8.0, 18.0];

        let caret = caret_at_line_start(&rectangles, MeasuredLine::OfCaret);

        assert_eq!(
            caret,
            Some(Caret {
                origin: POINT { x: 40, y: 120 },
                height: 18,
                font_face: None,
                text_color: None,
            })
        );
    }

    #[test]
    fn caret_below_the_line_above_is_one_line_lower() {
        let rectangles = [40.0, 102.0, 300.0, 18.0];

        let caret = caret_at_line_start(&rectangles, MeasuredLine::Above);

        assert_eq!(
            caret,
            Some(Caret {
                origin: POINT { x: 40, y: 120 },
                height: 18,
                font_face: None,
                text_color: None,
            })
        );
    }

    #[test]
    fn line_start_without_a_finite_rectangle_gives_no_caret() {
        let carets = [
            caret_at_line_start(&[], MeasuredLine::OfCaret),
            caret_at_line_start(&[f64::NAN, 120.0, 8.0, 18.0], MeasuredLine::OfCaret),
            caret_at_line_start(&[40.0, f64::MAX, 8.0, f64::MAX], MeasuredLine::Above),
        ];

        assert_eq!(carets, [None, None, None]);
    }

    #[test]
    fn line_that_starts_before_the_caret_is_the_line_above() {
        let orders = [-1, 0, 1];

        let lines = orders.map(MeasuredLine::from_start_order);

        assert_eq!(
            lines,
            [MeasuredLine::Above, MeasuredLine::OfCaret, MeasuredLine::OfCaret]
        );
    }

    fn caret_at(y: i32) -> Caret {
        Caret {
            origin: POINT { x: 443, y },
            height: 16,
            font_face: None,
            text_color: None,
        }
    }

    fn terminal_input(top: i32, bottom: i32) -> TerminalInput {
        TerminalInput {
            frame: RECT {
                left: 442,
                top,
                right: 452,
                bottom,
            },
            screen_top: 538,
        }
    }

    #[test]
    fn caret_moves_up_to_the_row_of_the_terminal_input() {
        let input = terminal_input(1284, 1302);

        let caret = input.align(caret_at(1285));

        assert_eq!(caret, caret_at(1277));
    }

    #[test]
    fn caret_of_a_terminal_input_on_its_row_stays() {
        let input = terminal_input(1276, 1294);

        let caret = input.align(caret_at(1277));

        assert_eq!(caret, caret_at(1277));
    }

    #[test]
    fn terminal_input_without_height_leaves_the_caret() {
        let input = terminal_input(1284, 1284);

        let caret = input.align(caret_at(1285));

        assert_eq!(caret, caret_at(1285));
    }

    #[test]
    fn no_rectangle_gives_no_caret() {
        let caret = caret_from_rectangles(&[]);

        assert_eq!(caret, None);
    }

    #[test]
    fn rectangle_with_a_value_that_is_not_finite_gives_no_caret() {
        let carets = [
            caret_from_rectangles(&[f64::NAN, 40.0, 7.0, 18.0]),
            caret_from_rectangles(&[100.0, 40.0, 7.0, f64::INFINITY]),
            caret_from_rectangles(&[100.0, f64::NEG_INFINITY, 7.0, 18.0]),
        ];

        assert_eq!(carets, [None, None, None]);
    }

    #[test]
    fn rectangle_with_an_edge_past_the_largest_number_gives_no_caret() {
        let carets = [
            caret_from_rectangles(&[f64::MAX, 40.0, f64::MAX, 18.0]),
            caret_from_rectangles(&[100.0, f64::MAX, 7.0, f64::MAX]),
        ];

        assert_eq!(carets, [None, None]);
    }

    const TEXT_COLOR: TextColor = TextColor {
        red: 0x11,
        green: 0x22,
        blue: 0x33,
    };

    #[test]
    fn text_color_comes_from_its_colorref() {
        let colors = [
            shown_style(Some(0x0033_2211), Some(0x00FF_FFFF), None).color,
            shown_style(Some(0x0033_2211), None, None).color,
        ];

        assert_eq!(colors, [Some(TEXT_COLOR); 2]);
    }

    #[test]
    fn font_face_comes_with_the_text_color() {
        let style = shown_style(Some(0x0033_2211), None, Some("Consolas".to_owned()));

        assert_eq!(
            style,
            TextStyle {
                color: Some(TEXT_COLOR),
                font_face: Some("Consolas".to_owned()),
            }
        );
    }

    #[test]
    fn font_face_comes_without_a_text_color() {
        let style = shown_style(None, None, Some("Consolas".to_owned()));

        assert_eq!(style.font_face.as_deref(), Some("Consolas"));
    }

    #[test]
    fn empty_font_face_is_no_face() {
        let style = shown_style(None, None, Some(String::new()));

        assert_eq!(style.font_face, None);
    }

    #[test]
    fn text_in_the_color_of_its_background_has_no_style() {
        let style = shown_style(Some(0), Some(0), Some("Arial".to_owned()));

        assert_eq!(style, TextStyle::default());
    }

    #[test]
    fn face_of_the_caret_window_comes_before_the_face_of_the_text() {
        let caret = Caret {
            font_face: Some("Tahoma".to_owned()),
            ..caret_at(40)
        };
        let style = TextStyle {
            color: Some(TEXT_COLOR),
            font_face: Some("Consolas".to_owned()),
        };

        let styled = style.apply_to(caret);

        assert_eq!(
            styled,
            Caret {
                font_face: Some("Tahoma".to_owned()),
                text_color: Some(TEXT_COLOR),
                ..caret_at(40)
            }
        );
    }

    #[test]
    fn face_of_the_text_fills_a_caret_without_a_face() {
        let style = TextStyle {
            color: None,
            font_face: Some("Consolas".to_owned()),
        };

        let styled = style.apply_to(caret_at(40));

        assert_eq!(styled.font_face.as_deref(), Some("Consolas"));
    }
}
