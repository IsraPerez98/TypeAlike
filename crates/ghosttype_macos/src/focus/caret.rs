//! What is at the caret of the focused control.
//!
//! It reads the bounds of the caret, the input box of a terminal of xterm.js, and the color and
//! the font face of the text before the caret.

use std::iter;
use std::ptr;

use ghosttype_core::desktop::{TextColor, is_line_break};
use objc2::rc::Retained;
use objc2_app_kit::{NSColor, NSColorSpace};
use objc2_application_services::{AXUIElement, AXValue};
use objc2_core_foundation::{
    CFAttributedString, CFDictionary, CFNumber, CFRange, CFRetained, CFString, CFType, CGFloat, CGPoint,
    CGRect, CGSize,
};
use objc2_core_graphics::CGColor;

use crate::accessibility::{
    copy_attribute, copy_attribute_for_index, copy_attribute_for_parameter, copy_attribute_for_range, frame,
    has_dom_class, parent, read_range, read_rect, selected_range, string_for_range,
};
use crate::geometry::is_caret_box;

const BOUNDS_FOR_RANGE_ATTRIBUTE: &str = "AXBoundsForRange";
const LINE_FOR_INDEX_ATTRIBUTE: &str = "AXLineForIndex";
const RANGE_FOR_LINE_ATTRIBUTE: &str = "AXRangeForLine";
const SELECTED_TEXT_MARKER_RANGE_ATTRIBUTE: &str = "AXSelectedTextMarkerRange";
const BOUNDS_FOR_TEXT_MARKER_RANGE_ATTRIBUTE: &str = "AXBoundsForTextMarkerRange";
const ATTRIBUTED_STRING_FOR_RANGE_ATTRIBUTE: &str = "AXAttributedStringForRange";
const FOREGROUND_COLOR_TEXT_ATTRIBUTE: &str = "AXForegroundColor";
const FONT_TEXT_ATTRIBUTE: &str = "AXFont";
const FONT_NAME_KEY: &str = "AXFontName";
const FONT_FAMILY_KEY: &str = "AXFontFamily";
const FONT_VISIBLE_NAME_KEY: &str = "AXVisibleName";
/// The first character of the names of the hidden faces of the system font, such as
/// ".SFNS-Regular". macOS gives another font for such a name.
const HIDDEN_FACE_PREFIX: char = '.';
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

/// The look of the text before the caret, as far as the focused control tells it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TextStyle {
    /// The color of the text.
    pub(crate) color: Option<TextColor>,
    /// The font face of the text.
    pub(crate) face: Option<FontFace>,
}

/// A font face, as a field names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FontFace {
    /// The name of one face of a font, such as "Menlo-Regular".
    Named(String),
    /// Only the family of a font, such as "Aptos".
    Family(String),
}

impl FontFace {
    fn name(&self) -> &str {
        match self {
            Self::Named(name) | Self::Family(name) => name,
        }
    }

    fn is_hidden_system_face(&self) -> bool {
        self.name().starts_with(HIDDEN_FACE_PREFIX)
    }
}

/// The bounds of the caret of `element`, from its text, else from the bounds of its selection.
pub(super) fn caret_bounds(element: &AXUIElement) -> Option<CGRect> {
    if is_terminal_input(element) {
        return terminal_input(element)?.caret_bounds();
    }
    [text_caret_bounds, selection_marker_bounds]
        .into_iter()
        .find_map(|read| read(element).filter(is_caret_box))
}

/// The bounds of the character before the caret of `element`, else the bounds of the start of the
/// line of the caret when the caret is at the start of the text or after a line break.
fn text_caret_bounds(element: &AXUIElement) -> Option<CGRect> {
    let caret_index = caret_index(element)?;
    // TextEdit reports the empty range at the caret one line too high, so measure a character or a line.
    let unit_before = previous_unit_range(caret_index)
        .filter(|&unit| !string_for_range(element, unit).is_some_and(|text| text.ends_with(is_line_break)));
    match unit_before {
        Some(unit) => bounds_for_range(element, unit),
        None => line_bounds(element, caret_index),
    }
}

/// The bounds of the start of the line of the caret at `caret_index` of `element`, from the
/// first line of [`line_start_indices`] that holds text.
fn line_bounds(element: &AXUIElement, caret_index: isize) -> Option<CGRect> {
    line_start_indices(caret_index).find_map(|index| {
        let line = line_range(element, index)?;
        Some(line_start_bounds(
            caret_index,
            line,
            bounds_for_range(element, line)?,
        ))
    })
}

/// The indices whose lines give the start of the line of a caret at `caret_index`: the caret,
/// else the character before it. A text view of AppKit gives an empty line for a caret after a
/// line break at the end of the text.
fn line_start_indices(caret_index: isize) -> impl Iterator<Item = isize> {
    iter::once(caret_index).chain(previous_unit_range(caret_index).map(|unit| unit.location))
}

/// Reads the range of the line of `element` that holds `index`. `None` for a line without text.
fn line_range(element: &AXUIElement, index: isize) -> Option<CFRange> {
    let line = copy_attribute_for_index(element, LINE_FOR_INDEX_ATTRIBUTE, index)?
        .downcast::<CFNumber>()
        .ok()?
        .as_isize()?;
    let range = copy_attribute_for_index(element, RANGE_FOR_LINE_ATTRIBUTE, line)?
        .downcast::<AXValue>()
        .ok()?;
    read_range(&range).filter(|range| range.length > 0)
}

/// The bounds of a caret at the start of its line, without width, from `line_box`, the bounds of
/// `line`. A line that starts before `caret_index` ends with the line break before the caret, so
/// the caret is one line lower.
fn line_start_bounds(caret_index: isize, line: CFRange, line_box: CGRect) -> CGRect {
    let top = if line.location < caret_index {
        line_box.origin.y + line_box.size.height
    } else {
        line_box.origin.y
    };
    CGRect::new(
        CGPoint::new(line_box.origin.x, top),
        CGSize::new(0.0, line_box.size.height),
    )
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

fn caret_index(element: &AXUIElement) -> Option<isize> {
    let selection = selected_range(element)?;
    selection.location.checked_add(selection.length)
}

/// Reads the color and the font face of the character before the caret of `element`, from one
/// read of its attributed text. Gives neither for the hidden input box of a terminal of xterm.js,
/// which does not show its text.
pub(super) fn text_style(element: &AXUIElement) -> TextStyle {
    let Some(text) = attributed_character_before_caret(element) else {
        return TextStyle::default();
    };
    text_style_of(srgb_text_color(&text).as_deref(), font_face_of(&text))
}

/// Reads the character before the caret of `element` with its attributes. `None` for the hidden
/// input box of a terminal of xterm.js.
fn attributed_character_before_caret(element: &AXUIElement) -> Option<CFRetained<CFAttributedString>> {
    if is_terminal_input(element) {
        return None;
    }
    copy_attribute_for_range(
        element,
        ATTRIBUTED_STRING_FOR_RANGE_ATTRIBUTE,
        previous_unit_range(caret_index(element)?)?,
    )?
    .downcast::<CFAttributedString>()
    .ok()
    .filter(|text| text.length() > 0)
}

/// Reads the value of the attribute `name` of the first character of `text`.
fn first_character_attribute(text: &CFAttributedString, name: &'static str) -> Option<CFRetained<CFType>> {
    let name = CFString::from_static_str(name);
    // SAFETY: the location is inside the string, and a null pointer asks for no effective range.
    unsafe { text.attribute(0, Some(&name), ptr::null_mut()) }
}

/// Reads the color of the first character of `text` in the sRGB color space.
fn srgb_text_color(text: &CFAttributedString) -> Option<Retained<NSColor>> {
    let color = first_character_attribute(text, FOREGROUND_COLOR_TEXT_ATTRIBUTE)?
        .downcast::<CGColor>()
        .ok()?;
    NSColor::colorWithCGColor(&color)?.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
}

/// Reads the font face of the first character of `text` from its `AXFont` dictionary.
fn font_face_of(text: &CFAttributedString) -> Option<FontFace> {
    let font = first_character_attribute(text, FONT_TEXT_ATTRIBUTE)?
        .downcast::<CFDictionary>()
        .ok()?;
    // SAFETY: the `AXFont` dictionary of Accessibility maps strings to CF objects.
    let font: CFRetained<CFDictionary<CFString, CFType>> = unsafe { CFRetained::cast_unchecked(font) };
    let part = |key: &'static str| {
        font.get(&CFString::from_static_str(key))?
            .downcast::<CFString>()
            .ok()
            .map(|part| part.to_string())
    };
    font_face(
        part(FONT_NAME_KEY).as_deref(),
        part(FONT_FAMILY_KEY).as_deref(),
        part(FONT_VISIBLE_NAME_KEY).as_deref(),
    )
}

/// The style of text in `color`, an sRGB color, and in `face`. Text that is mostly transparent
/// does not show, as in the hidden input box of a web editor, so it gives neither a color nor a
/// face.
fn text_style_of(color: Option<&NSColor>, face: Option<FontFace>) -> TextStyle {
    match color {
        Some(color) if color.alphaComponent() < MIN_SHOWN_TEXT_ALPHA => TextStyle::default(),
        color => TextStyle {
            color: color.map(text_color_of),
            face,
        },
    }
}

/// The text color of `color`, an sRGB color, without its opacity.
fn text_color_of(color: &NSColor) -> TextColor {
    let [red, green, blue] = [
        color.redComponent(),
        color.greenComponent(),
        color.blueComponent(),
    ]
    .map(channel);
    TextColor { red, green, blue }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "no conversion from f64 to u8 exists without `as`, and `as` saturates"
)]
fn channel(component: CGFloat) -> u8 {
    (component * CGFloat::from(u8::MAX)).round() as u8
}

/// The face in which to draw text whose `AXFont` gives the face `name`, the `family` and the
/// `visible_name`, each when the field gives it. A face name that does not belong to the family
/// gives the family, because Word gives the fixed name "Helvetica" for each face. A hidden face of
/// the system font gives no face, because macOS gives another font for its name.
fn font_face(name: Option<&str>, family: Option<&str>, visible_name: Option<&str>) -> Option<FontFace> {
    let family = non_empty(family).or_else(|| non_empty(visible_name));
    let face = match (non_empty(name), family) {
        (Some(name), Some(family)) if !is_face_of(name, family) => FontFace::Family(family.to_owned()),
        (Some(name), _) => FontFace::Named(name.to_owned()),
        (None, family) => FontFace::Family(family?.to_owned()),
    };
    Some(face).filter(|face| !face.is_hidden_system_face())
}

fn non_empty(part: Option<&str>) -> Option<&str> {
    part.filter(|part| !part.is_empty())
}

/// Whether the face `name` belongs to `family`. The name of a face starts with the letters of its
/// family without spaces, as `TimesNewRomanPSMT` does for "Times New Roman".
fn is_face_of(name: &str, family: &str) -> bool {
    letters_of(name).starts_with(&letters_of(family))
}

/// The letters and digits of `name` in lowercase.
fn letters_of(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
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

    #[test]
    fn line_of_the_caret_comes_before_the_line_of_the_character_before_it() {
        let indices: Vec<isize> = line_start_indices(31).collect();

        assert_eq!(indices, [31, 30]);
    }

    #[test]
    fn caret_at_start_reads_only_its_own_line() {
        let indices: Vec<isize> = line_start_indices(0).collect();

        assert_eq!(indices, [0]);
    }

    const SECOND_LINE: CFRange = CFRange {
        location: 6,
        length: 5,
    };

    #[test]
    fn caret_that_starts_its_line_is_at_the_left_edge_of_the_line_box() {
        let line_box = CGRect::new(CGPoint::new(40.0, 120.0), CGSize::new(60.0, 18.0));

        let bounds = line_start_bounds(6, SECOND_LINE, line_box);

        assert_eq!(
            bounds,
            CGRect::new(CGPoint::new(40.0, 120.0), CGSize::new(0.0, 18.0))
        );
    }

    #[test]
    fn caret_after_the_line_break_of_the_line_it_reads_is_one_line_lower() {
        let line_above = CFRange {
            location: 0,
            length: 6,
        };
        let line_box = CGRect::new(CGPoint::new(40.0, 102.0), CGSize::new(60.0, 18.0));

        let bounds = line_start_bounds(6, line_above, line_box);

        assert_eq!(
            bounds,
            CGRect::new(CGPoint::new(40.0, 120.0), CGSize::new(0.0, 18.0))
        );
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

    fn menlo() -> FontFace {
        FontFace::Named("Menlo-Regular".to_owned())
    }

    #[test]
    fn opaque_color_rounds_to_channels() {
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 0.5, 0.0, 1.0);

        let style = text_style_of(Some(&color), None);

        assert_eq!(
            style.color,
            Some(TextColor {
                red: 255,
                green: 128,
                blue: 0
            })
        );
    }

    #[test]
    fn mostly_transparent_text_gives_neither_a_color_nor_a_face() {
        let color = NSColor::colorWithSRGBRed_green_blue_alpha(1.0, 1.0, 1.0, 0.0);

        let style = text_style_of(Some(&color), Some(menlo()));

        assert_eq!(style, TextStyle::default());
    }

    #[test]
    fn text_without_a_color_keeps_its_face() {
        let style = text_style_of(None, Some(menlo()));

        assert_eq!(
            style,
            TextStyle {
                color: None,
                face: Some(menlo())
            }
        );
    }

    #[test]
    fn face_name_of_its_family_stays() {
        let face = font_face(Some("Menlo-Regular"), Some("Menlo"), Some("Menlo Regular"));

        assert_eq!(face, Some(menlo()));
    }

    #[test]
    fn face_name_without_the_spaces_of_its_family_stays() {
        let face = font_face(Some("TimesNewRomanPSMT"), Some("Times New Roman"), None);

        assert_eq!(face, Some(FontFace::Named("TimesNewRomanPSMT".to_owned())));
    }

    #[test]
    fn face_name_that_contradicts_its_family_gives_the_family() {
        let face = font_face(Some("Helvetica"), Some("Aptos"), Some("Aptos"));

        assert_eq!(face, Some(FontFace::Family("Aptos".to_owned())));
    }

    #[test]
    fn visible_name_stands_for_a_missing_family() {
        let face = font_face(Some("Helvetica"), None, Some("Aptos"));

        assert_eq!(face, Some(FontFace::Family("Aptos".to_owned())));
    }

    #[test]
    fn empty_family_counts_as_missing() {
        let face = font_face(Some("Helvetica"), Some(""), Some("Aptos"));

        assert_eq!(face, Some(FontFace::Family("Aptos".to_owned())));
    }

    #[test]
    fn face_name_without_a_family_stays() {
        let face = font_face(Some("Menlo-Regular"), None, None);

        assert_eq!(face, Some(menlo()));
    }

    #[test]
    fn family_without_a_face_name_gives_the_family() {
        let face = font_face(None, Some("Aptos"), None);

        assert_eq!(face, Some(FontFace::Family("Aptos".to_owned())));
    }

    #[test]
    fn font_without_names_gives_no_face() {
        let face = font_face(None, None, None);

        assert_eq!(face, None);
    }

    #[test]
    fn hidden_face_of_the_system_font_gives_no_face() {
        let face = font_face(Some(".SFNS-Regular"), Some(".SF NS"), Some("System Font Regular"));

        assert_eq!(face, None);
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
