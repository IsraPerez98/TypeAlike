//! The text around the focused control.
//!
//! It reads the title of the window, the labels of the control, the host of its web page, and
//! the text around the caret.

use objc2_application_services::AXUIElement;
use objc2_core_foundation::{CFNumber, CFRange, CFURL, Type};
use typealike_core::desktop::{
    FieldLabels, FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET, is_line_break,
};

mod code_mirror;
mod text_markers;

use crate::accessibility::{
    ROLE_ATTRIBUTE, TITLE_ATTRIBUTE, VALUE_ATTRIBUTE, WINDOW_ATTRIBUTE, WINDOW_ROLE, copy_attribute,
    copy_element, copy_string, has_dom_class, parent, selected_range, string_for_range,
};
use crate::focus::surroundings::code_mirror::code_mirror_text;
use crate::focus::surroundings::text_markers::field_text_by_markers;

const NUMBER_OF_CHARACTERS_ATTRIBUTE: &str = "AXNumberOfCharacters";
const PLACEHOLDER_ATTRIBUTE: &str = "AXPlaceholderValue";
const DESCRIPTION_ATTRIBUTE: &str = "AXDescription";
const HELP_ATTRIBUTE: &str = "AXHelp";
const TITLE_UI_ELEMENT_ATTRIBUTE: &str = "AXTitleUIElement";
const URL_ATTRIBUTE: &str = "AXURL";
const FOCUSED_WINDOW_ATTRIBUTE: &str = "AXFocusedWindow";
const WEB_AREA_ROLE: &str = "AXWebArea";
/// The most elements that TypeAlike reads on the way up from the focused field to its web area.
const MAX_WEB_AREA_DISTANCE: usize = 64;
/// The DOM class of the editor of VS Code, an `EditContext` element. Its tree holds the text only
/// while the screen reader mode of VS Code is on.
const EDIT_CONTEXT_CLASS: &str = "native-edit-context";

/// Whether `element` is an editor whose tree can leave out its text, as the editor of VS Code.
pub(super) fn has_unreadable_text(element: &AXUIElement) -> bool {
    has_dom_class(element, EDIT_CONTEXT_CLASS)
}

/// Reads the title of the window of `element`, the focused element of `app`, as
/// [`picked_window_title`] picks it. Zed and Mac Catalyst apps focus an element without a window.
/// The in-cell editor of Excel has a window without a title.
pub(super) fn window_title(element: Option<&AXUIElement>, app: Option<&AXUIElement>) -> Option<String> {
    picked_window_title(
        || copy_element(element?, WINDOW_ATTRIBUTE).and_then(|window| copy_string(&window, TITLE_ATTRIBUTE)),
        || copy_string(element?, ROLE_ATTRIBUTE),
        || copy_string(element?, TITLE_ATTRIBUTE),
        || {
            copy_element(app?, FOCUSED_WINDOW_ATTRIBUTE)
                .and_then(|window| copy_string(&window, TITLE_ATTRIBUTE))
        },
    )
}

/// Picks the window title of an element: the title of its window, else its own title when its
/// `role` is a window, else the title of the focused window of its app. A read runs only when the
/// reads before it give no title. An empty title counts as no title.
fn picked_window_title(
    window_title: impl FnOnce() -> Option<String>,
    role: impl FnOnce() -> Option<String>,
    own_title: impl FnOnce() -> Option<String>,
    focused_window_title: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let is_title = |title: &String| !title.trim().is_empty();
    window_title()
        .filter(is_title)
        .or_else(|| {
            (role().as_deref() == Some(WINDOW_ROLE))
                .then(own_title)
                .flatten()
                .filter(is_title)
        })
        .or_else(|| focused_window_title().filter(is_title))
}

/// Reads the labels of `element`. Its caption is its title, else the text of the element that
/// gives its title, as a text label next to a field does.
pub(super) fn field_labels(element: &AXUIElement) -> FieldLabels {
    FieldLabels {
        caption: first_text(element, &[TITLE_ATTRIBUTE]).or_else(|| title_element_text(element)),
        name: first_text(element, &[DESCRIPTION_ATTRIBUTE]),
        placeholder: first_text(element, &[PLACEHOLDER_ATTRIBUTE]),
        help: first_text(element, &[HELP_ATTRIBUTE]),
    }
}

fn title_element_text(element: &AXUIElement) -> Option<String> {
    let title_element = copy_element(element, TITLE_UI_ELEMENT_ATTRIBUTE)?;
    first_text(&title_element, &[VALUE_ATTRIBUTE, TITLE_ATTRIBUTE])
}

/// Reads the first of `attributes` of `element` that holds text that is not empty.
fn first_text(element: &AXUIElement, attributes: &[&'static str]) -> Option<String> {
    attributes
        .iter()
        .filter_map(|attribute| copy_string(element, attribute))
        .find(|text| !text.trim().is_empty())
}

/// Reads the host of the web page that holds `element`, from the URL of the nearest web area
/// above it in its window.
pub(super) fn web_domain(element: &AXUIElement) -> Option<String> {
    let mut current = element.retain();
    for _ in 0..MAX_WEB_AREA_DISTANCE {
        match copy_string(&current, ROLE_ATTRIBUTE).as_deref() {
            Some(WEB_AREA_ROLE) => return url_host(&current),
            Some(WINDOW_ROLE) | None => return None,
            Some(_) => current = parent(&current)?,
        }
    }
    None
}

fn url_host(element: &AXUIElement) -> Option<String> {
    let url = copy_attribute(element, URL_ATTRIBUTE)?.downcast::<CFURL>().ok()?;
    url.host_name().map(|host| host.to_string())
}

/// Reads the text around the selection of `element`: from its lines in a CodeMirror editor, else
/// by range, or from its whole value when it cannot read a range. An element without a text range,
/// as the body of a message in Mail, gives its text through text markers.
pub(super) fn field_text(element: &AXUIElement) -> Option<FieldText> {
    field_text_by_selected_range(element).or_else(|| field_text_by_markers(element))
}

fn field_text_by_selected_range(element: &AXUIElement) -> Option<FieldText> {
    let selection = selected_range(element)?;
    let text_length = character_count(element);
    let ranges = SurroundingRanges::new(selection, text_length)?;
    code_mirror_text(element, selection, ranges.before)
        .or_else(|| field_text_by_range(element, &ranges))
        .or_else(|| field_text_by_value(element, selection, text_length))
}

fn field_text_by_range(element: &AXUIElement, ranges: &SurroundingRanges) -> Option<FieldText> {
    Some(FieldText {
        before_caret: string_for_range(element, ranges.before)?,
        after_caret: string_for_range(element, ranges.after)?,
    })
}

/// Cuts the text around the selection from the whole value of `element`. The selection first
/// moves past the line breaks of the value that the range text leaves out. When the two texts
/// differ in more than these line breaks, the selection keeps the offsets of the range text.
fn field_text_by_value(
    element: &AXUIElement,
    selection: CFRange,
    text_length: Option<isize>,
) -> Option<FieldText> {
    let value = copy_string(element, VALUE_ATTRIBUTE)?;
    let up_to_selection_end = CFRange {
        location: 0,
        length: selection.location.checked_add(selection.length)?,
    };
    let value_selection = string_for_range(element, up_to_selection_end)
        .and_then(|range_text| value_selection(&range_text, selection, &value))
        .unwrap_or(selection);
    SurroundingRanges::new(value_selection, text_length)?.cut(&value)
}

fn character_count(element: &AXUIElement) -> Option<isize> {
    copy_attribute(element, NUMBER_OF_CHARACTERS_ATTRIBUTE)?
        .downcast::<CFNumber>()
        .ok()?
        .as_isize()
}

/// The UTF-16 ranges of the text that a suggestion reads before and after a selection. When the
/// length of the text is unknown, the range after the selection has the most length it can have.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SurroundingRanges {
    before: CFRange,
    after: CFRange,
}

impl SurroundingRanges {
    fn new(selection: CFRange, text_length: Option<isize>) -> Option<Self> {
        let max_before = isize::try_from(MAX_CHARS_BEFORE_CARET).ok()?;
        let max_after = isize::try_from(MAX_CHARS_AFTER_CARET).ok()?;
        let before_start = selection.location.saturating_sub(max_before).max(0);
        let after_start = selection.location.checked_add(selection.length)?;
        Some(Self {
            before: CFRange {
                location: before_start,
                length: selection.location - before_start,
            },
            after: CFRange {
                location: after_start,
                length: text_length.map_or(max_after, |length| {
                    length.saturating_sub(after_start).clamp(0, max_after)
                }),
            },
        })
    }

    /// Cuts the text of the ranges from `value`, the whole text.
    fn cut(&self, value: &str) -> Option<FieldText> {
        let units: Vec<u16> = value.encode_utf16().collect();
        let slice = |start: isize, length: isize| {
            let start = usize::try_from(start).ok()?;
            let end = start.checked_add(usize::try_from(length).ok()?)?.min(units.len());
            units.get(start..end).map(whole_characters)
        };
        Some(FieldText {
            before_caret: slice(self.before.location, self.before.length)?,
            after_caret: slice(self.after.location, self.after.length).unwrap_or_default(),
        })
    }
}

/// The text of `units`, a UTF-16 cut of a text, without the half of a character that the cut
/// splits at either end.
fn whole_characters(units: &[u16]) -> String {
    char::decode_utf16(units.iter().copied())
        .filter_map(Result::ok)
        .collect()
}

/// Moves `selection` from the UTF-16 offsets of the range text to the UTF-16 offsets of `value`.
/// `range_text` is the range text from the start of the field to the end of the selection. Gives
/// `None` when that text is not the start of `value` without some of its line breaks.
fn value_selection(range_text: &str, selection: CFRange, value: &str) -> Option<CFRange> {
    let range_units: Vec<u16> = range_text.encode_utf16().collect();
    let value_units: Vec<u16> = value.encode_utf16().collect();
    let value_offset = |range_offset: isize| {
        let range_prefix = range_units.get(..usize::try_from(range_offset).ok()?)?;
        isize::try_from(value_offset_after(range_prefix, &value_units)?).ok()
    };
    let start = value_offset(selection.location)?;
    let end = value_offset(selection.location.checked_add(selection.length)?)?;
    Some(CFRange {
        location: start,
        length: end - start,
    })
}

/// The UTF-16 offset in `value` just after `range_prefix`, when `range_prefix` is the start of
/// `value` without some of its line breaks. A Chromium contenteditable puts a line break between
/// blocks in its value, but its range text joins the blocks. The walk skips a line break of
/// `value` only when the next unit of `range_prefix` is a different unit. Thus the offset is before
/// the line breaks after the prefix. Any other difference gives `None`.
fn value_offset_after(range_prefix: &[u16], value: &[u16]) -> Option<usize> {
    let is_left_out_break = |value_unit: &u16, range_unit: &u16| {
        value_unit != range_unit && char::from_u32(u32::from(*value_unit)).is_some_and(is_line_break)
    };
    let mut offset = 0;
    for range_unit in range_prefix {
        while value
            .get(offset)
            .is_some_and(|value_unit| is_left_out_break(value_unit, range_unit))
        {
            offset += 1;
        }
        if value.get(offset) != Some(range_unit) {
            return None;
        }
        offset += 1;
    }
    Some(offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(location: isize, length: isize) -> CFRange {
        CFRange { location, length }
    }

    fn read(text: &'static str) -> impl FnOnce() -> Option<String> {
        move || Some(text.to_owned())
    }

    fn read_nothing() -> Option<String> {
        None
    }

    fn unread() -> Option<String> {
        unreachable!("a read after a title ran")
    }

    #[test]
    fn title_of_the_window_of_the_element_wins_without_other_reads() {
        let title = picked_window_title(read("Book1"), unread, unread, unread);

        assert_eq!(title.as_deref(), Some("Book1"));
    }

    #[test]
    fn element_that_is_a_window_gives_its_own_title() {
        let title = picked_window_title(read_nothing, read(WINDOW_ROLE), read("main.rs - project"), unread);

        assert_eq!(title.as_deref(), Some("main.rs - project"));
    }

    #[test]
    fn element_that_is_no_window_gives_the_title_of_the_focused_window() {
        let title = picked_window_title(read_nothing, read("AXTextArea"), read("Message"), read("Book1"));

        assert_eq!(title.as_deref(), Some("Book1"));
    }

    #[test]
    fn empty_titles_count_as_no_title() {
        let title = picked_window_title(read(""), read(WINDOW_ROLE), read("  "), read("Book1"));

        assert_eq!(title.as_deref(), Some("Book1"));
    }

    #[test]
    fn no_title_anywhere_gives_none() {
        let title = picked_window_title(read_nothing, read_nothing, unread, read_nothing);

        assert_eq!(title, None);
    }

    #[test]
    fn ranges_reach_the_limits_around_the_caret() {
        let max_before = isize::try_from(MAX_CHARS_BEFORE_CARET).unwrap();
        let max_after = isize::try_from(MAX_CHARS_AFTER_CARET).unwrap();
        let caret = max_before + 500;

        let ranges = SurroundingRanges::new(range(caret, 0), Some(caret + max_after + 500));

        assert_eq!(
            ranges,
            Some(SurroundingRanges {
                before: range(500, max_before),
                after: range(caret, max_after),
            })
        );
    }

    #[test]
    fn ranges_stop_at_the_ends_of_the_text_and_skip_the_selection() {
        let ranges = SurroundingRanges::new(range(5, 3), Some(10));

        assert_eq!(
            ranges,
            Some(SurroundingRanges {
                before: range(0, 5),
                after: range(8, 2),
            })
        );
    }

    #[test]
    fn text_of_unknown_length_reads_the_most_after_the_selection() {
        let max_after = isize::try_from(MAX_CHARS_AFTER_CARET).unwrap();

        let ranges = SurroundingRanges::new(range(5, 2), None);

        assert_eq!(ranges.map(|ranges| ranges.after), Some(range(7, max_after)));
    }

    fn value_offset(range_prefix: &str, value: &str) -> Option<usize> {
        let range_prefix: Vec<u16> = range_prefix.encode_utf16().collect();
        let value: Vec<u16> = value.encode_utf16().collect();
        value_offset_after(&range_prefix, &value)
    }

    #[test]
    fn value_offset_skips_the_line_breaks_that_the_range_text_leaves_out() {
        let cases = [
            ("abc\ndefghi", "abc\ndef\nghi", Some(11)),
            (
                "First paragraph ends here.Se",
                "First paragraph ends here.\nSecond",
                Some(29),
            ),
            (
                "Things:• SPACE HANDLING• Fo",
                "Things:\n• SPACE HANDLING\n• Font size",
                Some(29),
            ),
            ("abc\nde", "abc\ndef\nghi", Some(6)),
            ("abc\n", "abc\ndef", Some(4)),
            ("Hi 👋there", "Hi 👋\nthere", Some(11)),
            ("ab", "a\u{2029}b", Some(3)),
            ("abc", "abc", Some(3)),
        ];

        let offsets = cases.map(|(range_prefix, value, _)| value_offset(range_prefix, value));

        assert_eq!(offsets, cases.map(|(_, _, offset)| offset));
    }

    #[test]
    fn value_offset_stays_before_the_line_breaks_after_the_range_text() {
        let cases = [
            (
                "First paragraph ends here.",
                "First paragraph ends here.\nSecond line",
                Some(26),
            ),
            (
                "First paragraph ends here.",
                "First paragraph ends here.\n",
                Some(26),
            ),
            ("abc", "abc\ndef", Some(3)),
            ("", "\nabc", Some(0)),
            ("", "", Some(0)),
        ];

        let offsets = cases.map(|(range_prefix, value, _)| value_offset(range_prefix, value));

        assert_eq!(offsets, cases.map(|(_, _, offset)| offset));
    }

    #[test]
    fn texts_that_differ_in_more_than_line_breaks_give_no_value_offset() {
        let cases = [("abd", "abc"), ("abcd", "abc"), ("abc", "ab\u{fffc}c")];

        let offsets = cases.map(|(range_prefix, value)| value_offset(range_prefix, value));

        assert_eq!(offsets, [None, None, None]);
    }

    #[test]
    fn selection_moves_past_the_line_breaks_that_the_range_text_leaves_out() {
        let selection = value_selection("abc\ndefg", range(6, 2), "abc\ndef\nghi");

        assert_eq!(selection, Some(range(6, 3)));
    }

    #[test]
    fn texts_that_differ_in_more_than_line_breaks_give_no_value_selection() {
        let selection = value_selection("abc\ndefg", range(6, 2), "abc\ndef\nxyz");

        assert_eq!(selection, None);
    }

    #[test]
    fn whole_value_is_cut_at_the_caret_of_the_range_text() {
        let selection = value_selection("abc\ndefg", range(8, 0), "abc\ndef\nghi").expect("the texts align");
        let ranges = SurroundingRanges::new(selection, Some(11)).expect("the selection is valid");

        let field_text = ranges.cut("abc\ndef\nghi");

        assert_eq!(
            field_text,
            Some(FieldText {
                before_caret: "abc\ndef\ng".to_owned(),
                after_caret: "hi".to_owned(),
            })
        );
    }

    #[test]
    fn cut_never_splits_an_emoji_at_the_limits() {
        let before = format!("a👋{}", "b".repeat(MAX_CHARS_BEFORE_CARET - 1));
        let after = format!("{}👋", "c".repeat(MAX_CHARS_AFTER_CARET - 1));
        let caret = isize::try_from(before.encode_utf16().count()).unwrap();
        let ranges = SurroundingRanges::new(range(caret, 0), None).expect("the selection is valid");

        let field_text = ranges
            .cut(&format!("{before}{after}"))
            .expect("the ranges are in the value");

        assert_eq!(
            (
                field_text.before_caret.contains('\u{fffd}'),
                field_text.after_caret.contains('\u{fffd}'),
            ),
            (false, false),
            "(before, after) holds U+FFFD"
        );
    }

    #[test]
    fn whole_value_is_cut_around_the_selection() {
        let ranges = SurroundingRanges::new(range(5, 1), None).expect("the selection is valid");

        let field_text = ranges.cut("Hello world");

        assert_eq!(
            field_text,
            Some(FieldText {
                before_caret: "Hello".to_owned(),
                after_caret: "world".to_owned(),
            })
        );
    }
}
