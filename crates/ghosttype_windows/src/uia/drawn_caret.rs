//! The caret that the focused control draws itself.
//!
//! Apps without a system caret draw a caret of their own. The caret has the color of the text
//! before it. In a terminal of xterm.js, the caret moves up to the row that holds it.

use ghosttype_core::desktop::TextColor;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationTextRange, TextPatternRangeEndpoint_Start, TextUnit_Character,
    UIA_BackgroundColorAttributeId, UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId,
    UIA_ForegroundColorAttributeId,
};

use super::com::SafeArray;
use super::{Uia, caret_range, is_password, is_terminal_input};
use crate::caret::{self, Caret};
use crate::pixels::Rgb;

/// The numbers that describe one bounding rectangle: left, top, width and height.
const VALUES_PER_RECTANGLE: usize = 4;
const CHARACTERS_BEFORE_CARET: i32 = -1;
/// The class of the screen of a terminal of xterm.js, which holds its rows.
const XTERM_SCREEN_CLASS: &str = "xterm-screen";

impl Uia {
    /// Locates the caret of the focused control: the system caret, else the caret that the control
    /// draws itself. The caret has the color of the text before it, when the control tells it.
    pub(crate) fn caret(&self) -> Option<Caret> {
        let element = self.focused_element();
        let caret = caret::system_caret().or_else(|| self.drawn_caret(element.as_ref()?))?;
        Some(Caret {
            text_color: element.as_ref().and_then(text_color),
            ..caret
        })
    }

    /// Locates the caret that `element`, the focused control, draws itself: the caret of the caret
    /// object of its window, else the right edge of the character before the caret. In a terminal
    /// of xterm.js, the caret moves up to the row that holds it.
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

/// Locates the caret in `element` from the bounds of the character before it.
fn caret_of(element: &IUIAutomationElement) -> Option<Caret> {
    if is_password(element) {
        return None;
    }
    // Many apps give no bounds for the empty range at the caret, so measure the character before it.
    caret_from_rectangles(&bounding_rectangles(&character_before_caret(element)?)?)
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

/// Reads the color of the character before the caret of `element`. `None` for the hidden input box
/// of a terminal of xterm.js, which does not show its text.
fn text_color(element: &IUIAutomationElement) -> Option<TextColor> {
    if is_terminal_input(element) {
        return None;
    }
    let range = character_before_caret(element)?;
    // SAFETY: plain calls on an interface of this thread.
    let (text, background) = unsafe {
        (
            range.GetAttributeValue(UIA_ForegroundColorAttributeId).ok()?,
            range.GetAttributeValue(UIA_BackgroundColorAttributeId).ok(),
        )
    };
    shown_text_color(colorref(&text)?, background.as_ref().and_then(colorref))
}

/// Reads a `COLORREF` attribute value. `None` when UIA gives a value of another type: the value
/// for an attribute that the control does not support, or for colors that change in the range.
fn colorref(value: &VARIANT) -> Option<u32> {
    i32::try_from(value).ok().map(i32::cast_unsigned)
}

/// The color of text in the `COLORREF` `text` on the `COLORREF` `background`. `None` when both
/// are the same: Chromium drops the alpha of the transparent text of a hidden input box.
fn shown_text_color(text: u32, background: Option<u32>) -> Option<TextColor> {
    (background != Some(text)).then(|| Rgb::from_colorref(text).into())
}

fn bounding_rectangles(range: &IUIAutomationTextRange) -> Option<Vec<f64>> {
    // SAFETY: plain call on an interface of this thread; the array is owned right after.
    let array = SafeArray(unsafe { range.GetBoundingRectangles() }.ok()?);
    array.doubles()
}

/// Builds the caret at the right edge of the last of `rectangles`, each given as left, top, width
/// and height in physical screen pixels.
fn caret_from_rectangles(rectangles: &[f64]) -> Option<Caret> {
    let [left, top, width, height] = *rectangles.as_chunks::<VALUES_PER_RECTANGLE>().0.last()?;
    let top_left = POINT {
        x: to_pixel(left),
        y: to_pixel(top),
    };
    let bottom_right = POINT {
        x: to_pixel(left + width),
        y: to_pixel(top + height),
    };
    Caret::from_corners(top_left, bottom_right, None)
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

#[expect(
    clippy::cast_possible_truncation,
    reason = "no conversion from f64 to i32 exists without `as`, and `as` saturates"
)]
fn to_pixel(coordinate: f64) -> i32 {
    coordinate.round() as i32
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
    fn text_color_comes_from_its_colorref() {
        let colors = [
            shown_text_color(0x0033_2211, Some(0x00FF_FFFF)),
            shown_text_color(0x0033_2211, None),
        ];

        let expected = Some(TextColor {
            red: 0x11,
            green: 0x22,
            blue: 0x33,
        });
        assert_eq!(colors, [expected; 2]);
    }

    #[test]
    fn text_in_the_color_of_its_background_has_no_color() {
        let color = shown_text_color(0, Some(0));

        assert_eq!(color, None);
    }
}
