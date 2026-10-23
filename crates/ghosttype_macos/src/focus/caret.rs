//! What is at the caret of the focused control.
//!
//! It reads the bounds of the caret, the input box of a terminal of xterm.js, and the color of
//! the text before the caret.

use std::iter;
use std::ptr;

use ghosttype_core::desktop::TextColor;
use objc2_app_kit::{NSColor, NSColorSpace};
use objc2_application_services::{AXUIElement, AXValue};
use objc2_core_foundation::{CFAttributedString, CFRange, CFString, CGFloat, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGColor;

use crate::accessibility::{
    copy_attribute, copy_attribute_for_parameter, copy_attribute_for_range, frame, has_dom_class, parent,
    read_rect, selected_range,
};
use crate::geometry::is_caret_box;

const BOUNDS_FOR_RANGE_ATTRIBUTE: &str = "AXBoundsForRange";
const SELECTED_TEXT_MARKER_RANGE_ATTRIBUTE: &str = "AXSelectedTextMarkerRange";
const BOUNDS_FOR_TEXT_MARKER_RANGE_ATTRIBUTE: &str = "AXBoundsForTextMarkerRange";
const ATTRIBUTED_STRING_FOR_RANGE_ATTRIBUTE: &str = "AXAttributedStringForRange";
const FOREGROUND_COLOR_TEXT_ATTRIBUTE: &str = "AXForegroundColor";
/// The least opacity of text that shows. A web editor takes keys in a hidden input box whose text
/// is transparent.
const MIN_SHOWN_TEXT_ALPHA: CGFloat = 0.5;
/// The class of the hidden input box of a terminal of xterm.js, such as the terminal of VS Code.
const XTERM_INPUT_CLASS: &str = "xterm-helper-textarea";
/// The class of the screen of a terminal of xterm.js, which holds its rows.
const XTERM_SCREEN_CLASS: &str = "xterm-screen";
/// The most elements that GhostType reads on the way up from the input box of a terminal to its
/// screen, which holds the box two levels up.
const MAX_TERMINAL_SCREEN_DISTANCE: usize = 4;

/// The bounds of the text that ends at the caret of `element`, else the bounds of its selection.
pub(super) fn caret_bounds(element: &AXUIElement) -> Option<CGRect> {
    if is_terminal_input(element) {
        return terminal_input(element)?.caret_bounds();
    }
    [unit_before_caret_bounds, selection_marker_bounds]
        .into_iter()
        .find_map(|read| read(element).filter(is_caret_box))
}

fn unit_before_caret_bounds(element: &AXUIElement) -> Option<CGRect> {
    // TextEdit reports the empty range at the caret one line too high, so measure the text before it.
    bounds_for_range(element, unit_before_caret(element)?)
}

/// The bounds of the selection of `element` as a text marker range. Chromium gives no bounds for
/// a character range in a contenteditable field, but it gives them for its own selection.
fn selection_marker_bounds(element: &AXUIElement) -> Option<CGRect> {
    let selection = copy_attribute(element, SELECTED_TEXT_MARKER_RANGE_ATTRIBUTE)?;
    let bounds = copy_attribute_for_parameter(element, BOUNDS_FOR_TEXT_MARKER_RANGE_ATTRIBUTE, &selection)?
        .downcast::<AXValue>()
        .ok()?;
    read_rect(&bounds).filter(|bounds| marks_caret(*bounds, frame(element)))
}

/// Whether `selection`, the box of the selection of a field with `field_frame`, marks a caret.
/// For a field without laid-out text, such as the editor of VS Code, Chromium gives the frame of
/// the field.
fn marks_caret(selection: CGRect, field_frame: Option<CGRect>) -> bool {
    field_frame != Some(selection)
}

fn unit_before_caret(element: &AXUIElement) -> Option<CFRange> {
    let selection = selected_range(element)?;
    previous_unit_range(selection.location.checked_add(selection.length)?)
}

/// Reads the color of the character before the caret of `element`. `None` for the hidden input box
/// of a terminal of xterm.js, which does not show its text.
pub(super) fn text_color(element: &AXUIElement) -> Option<TextColor> {
    if is_terminal_input(element) {
        return None;
    }
    let text = copy_attribute_for_range(
        element,
        ATTRIBUTED_STRING_FOR_RANGE_ATTRIBUTE,
        unit_before_caret(element)?,
    )?
    .downcast::<CFAttributedString>()
    .ok()
    .filter(|text| text.length() > 0)?;
    let attribute = CFString::from_static_str(FOREGROUND_COLOR_TEXT_ATTRIBUTE);
    // SAFETY: the location is inside the string, and a null pointer asks for no effective range.
    let color = unsafe { text.attribute(0, Some(&attribute), ptr::null_mut()) }?
        .downcast::<CGColor>()
        .ok()?;
    let color = NSColor::colorWithCGColor(&color)?.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    text_color_of_components(
        [
            color.redComponent(),
            color.greenComponent(),
            color.blueComponent(),
        ],
        color.alphaComponent(),
    )
}

/// The color of text with the sRGB `components` and `alpha`, each from 0 to 1. `None` for text
/// that is mostly transparent, which does not show.
fn text_color_of_components(components: [CGFloat; 3], alpha: CGFloat) -> Option<TextColor> {
    if alpha < MIN_SHOWN_TEXT_ALPHA {
        return None;
    }
    let [red, green, blue] = components.map(channel);
    Some(TextColor { red, green, blue })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "no conversion from f64 to u8 exists without `as`, and `as` saturates"
)]
fn channel(component: CGFloat) -> u8 {
    (component * CGFloat::from(u8::MAX)).round() as u8
}

/// The hidden input box of a terminal of xterm.js, which the terminal moves to the cell of its
/// cursor, and the screen of the terminal. The text in the box is not the text of the terminal:
/// xterm.js keeps the text of each input event there until Enter.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TerminalInput {
    /// The frame of the input box, which is one row high.
    frame: CGRect,
    /// The top of the screen, where the first row starts.
    screen_top: CGFloat,
}

impl TerminalInput {
    /// The left edge of the cell of the cursor, as bounds without width, at the top of the row
    /// that holds the input box. VS Code draws the input box below the top of that row.
    fn caret_bounds(self) -> Option<CGRect> {
        let row_height = self.frame.size.height;
        if row_height <= 0.0 {
            return None;
        }
        let below_row = (self.frame.origin.y - self.screen_top).rem_euclid(row_height);
        Some(CGRect::new(
            CGPoint::new(self.frame.origin.x, self.frame.origin.y - below_row),
            CGSize::new(0.0, row_height),
        ))
    }
}

/// Whether `element` is the hidden input box of a terminal of xterm.js.
pub(super) fn is_terminal_input(element: &AXUIElement) -> bool {
    has_dom_class(element, XTERM_INPUT_CLASS)
}

/// Reads the input box of a terminal of xterm.js, `element`, and the top of its screen.
fn terminal_input(element: &AXUIElement) -> Option<TerminalInput> {
    let screen = iter::successors(parent(element), |current| parent(current))
        .take(MAX_TERMINAL_SCREEN_DISTANCE)
        .find(|ancestor| has_dom_class(ancestor, XTERM_SCREEN_CLASS))?;
    Some(TerminalInput {
        frame: frame(element)?,
        screen_top: frame(&screen)?.origin.y,
    })
}

fn previous_unit_range(caret_index: isize) -> Option<CFRange> {
    (caret_index > 0).then(|| CFRange {
        location: caret_index - 1,
        length: 1,
    })
}

fn bounds_for_range(element: &AXUIElement, range: CFRange) -> Option<CGRect> {
    let bounds = copy_attribute_for_range(element, BOUNDS_FOR_RANGE_ATTRIBUTE, range)?
        .downcast::<AXValue>()
        .ok()?;
    read_rect(&bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_unit_ends_at_caret() {
        assert_eq!(
            previous_unit_range(5),
            Some(CFRange {
                location: 4,
                length: 1
            })
        );
    }

    #[test]
    fn caret_at_start_has_no_previous_unit() {
        assert_eq!(previous_unit_range(0), None);
    }

    const EDITOR_LINE: CGRect = CGRect::new(CGPoint::new(504.0, 102.0), CGSize::new(860.0, 18.0));

    #[test]
    fn selection_box_inside_the_field_marks_a_caret() {
        let selection = CGRect::new(CGPoint::new(612.0, 102.0), CGSize::new(0.0, 18.0));

        assert!(marks_caret(selection, Some(EDITOR_LINE)));
    }

    #[test]
    fn selection_box_that_is_the_frame_of_the_field_marks_no_caret() {
        assert!(!marks_caret(EDITOR_LINE, Some(EDITOR_LINE)));
    }

    #[test]
    fn opaque_color_components_round_to_channels() {
        let color = text_color_of_components([1.0, 0.5, 0.0], 1.0);

        assert_eq!(
            color,
            Some(TextColor {
                red: 255,
                green: 128,
                blue: 0
            })
        );
    }

    #[test]
    fn mostly_transparent_text_has_no_color() {
        let color = text_color_of_components([1.0, 1.0, 1.0], 0.0);

        assert_eq!(color, None);
    }

    fn terminal_input_at(top: CGFloat, height: CGFloat) -> TerminalInput {
        TerminalInput {
            frame: CGRect::new(CGPoint::new(645.0, top), CGSize::new(8.0, height)),
            screen_top: 262.0,
        }
    }

    #[test]
    fn terminal_caret_moves_up_to_the_row_of_the_input_box() {
        let input = terminal_input_at(810.0, 15.0);

        let bounds = input.caret_bounds();

        assert_eq!(
            bounds,
            Some(CGRect::new(CGPoint::new(645.0, 802.0), CGSize::new(0.0, 15.0)))
        );
    }

    #[test]
    fn terminal_caret_of_an_input_box_on_its_row_stays() {
        let input = terminal_input_at(802.0, 15.0);

        let bounds = input.caret_bounds();

        assert_eq!(
            bounds,
            Some(CGRect::new(CGPoint::new(645.0, 802.0), CGSize::new(0.0, 15.0)))
        );
    }

    #[test]
    fn terminal_input_box_without_height_has_no_caret() {
        let input = terminal_input_at(810.0, 0.0);

        assert_eq!(input.caret_bounds(), None);
    }
}
