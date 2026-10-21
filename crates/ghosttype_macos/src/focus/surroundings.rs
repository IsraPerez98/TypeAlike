//! The text around the focused control.
//!
//! It reads the title of the window, the labels of the control, the host of its web page, and
//! the text around the caret.

use ghosttype_core::desktop::{FieldLabels, FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET};
use objc2_application_services::AXUIElement;
use objc2_core_foundation::{CFNumber, CFRange, CFString, CFURL, Type};

use crate::accessibility::{
    PARENT_ATTRIBUTE, ROLE_ATTRIBUTE, WINDOW_ATTRIBUTE, WINDOW_ROLE, copy_attribute,
    copy_attribute_for_range, copy_string, selected_range,
};

const STRING_FOR_RANGE_ATTRIBUTE: &str = "AXStringForRange";
const NUMBER_OF_CHARACTERS_ATTRIBUTE: &str = "AXNumberOfCharacters";
const VALUE_ATTRIBUTE: &str = "AXValue";
const TITLE_ATTRIBUTE: &str = "AXTitle";
const PLACEHOLDER_ATTRIBUTE: &str = "AXPlaceholderValue";
const DESCRIPTION_ATTRIBUTE: &str = "AXDescription";
const HELP_ATTRIBUTE: &str = "AXHelp";
const TITLE_UI_ELEMENT_ATTRIBUTE: &str = "AXTitleUIElement";
const URL_ATTRIBUTE: &str = "AXURL";
const WEB_AREA_ROLE: &str = "AXWebArea";
/// The most elements that GhostType reads on the way up from the focused field to its web area.
const MAX_WEB_AREA_DISTANCE: usize = 64;

pub(super) fn window_title(element: &AXUIElement) -> Option<String> {
    let window = copy_attribute(element, WINDOW_ATTRIBUTE)?
        .downcast::<AXUIElement>()
        .ok()?;
    copy_string(&window, TITLE_ATTRIBUTE)
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
    let title_element = copy_attribute(element, TITLE_UI_ELEMENT_ATTRIBUTE)?
        .downcast::<AXUIElement>()
        .ok()?;
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
            Some(_) => current = copy_attribute(&current, PARENT_ATTRIBUTE)?.downcast().ok()?,
        }
    }
    None
}

fn url_host(element: &AXUIElement) -> Option<String> {
    let url = copy_attribute(element, URL_ATTRIBUTE)?.downcast::<CFURL>().ok()?;
    url.host_name().map(|host| host.to_string())
}

/// Reads the text around the selection of `element`, by range, or from its whole value when it
/// cannot read a range.
pub(super) fn field_text(element: &AXUIElement) -> Option<FieldText> {
    let ranges = SurroundingRanges::new(selected_range(element)?, character_count(element))?;
    field_text_by_range(element, &ranges).or_else(|| {
        let value = copy_string(element, VALUE_ATTRIBUTE)?;
        ranges.cut(&value)
    })
}

fn field_text_by_range(element: &AXUIElement, ranges: &SurroundingRanges) -> Option<FieldText> {
    let string_for_range = |range| {
        copy_attribute_for_range(element, STRING_FOR_RANGE_ATTRIBUTE, range)?
            .downcast::<CFString>()
            .ok()
            .map(|text| text.to_string())
    };
    Some(FieldText {
        before_caret: string_for_range(ranges.before)?,
        after_caret: string_for_range(ranges.after)?,
    })
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
            units.get(start..end).map(String::from_utf16_lossy)
        };
        Some(FieldText {
            before_caret: slice(self.before.location, self.before.length)?,
            after_caret: slice(self.after.location, self.after.length).unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(location: isize, length: isize) -> CFRange {
        CFRange { location, length }
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
        let ranges = SurroundingRanges::new(range(5, 2), None);

        assert_eq!(ranges.map(|ranges| ranges.after), Some(range(7, 300)));
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
