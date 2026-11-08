//! The text of an editable web area that gives no text range, only text markers, as the body of a
//! message in Mail.

use objc2_application_services::{AXTextMarker, AXTextMarkerRange, AXUIElement};
use objc2_core_foundation::{CFArray, CFNumber, CFRange, CFRetained, CFType};
use typealike_core::desktop::FieldText;

use crate::accessibility::{
    SELECTED_TEXT_MARKER_RANGE_ATTRIBUTE, TEXT_MARKER_RANGE_FOR_ELEMENT_ATTRIBUTE, VALUE_ATTRIBUTE,
    copy_attribute, copy_attribute_for_index, copy_attribute_for_parameter, is_settable,
    string_for_text_marker_range,
};
use crate::focus::surroundings::SurroundingRanges;

const INDEX_FOR_TEXT_MARKER_ATTRIBUTE: &str = "AXIndexForTextMarker";
const TEXT_MARKER_FOR_INDEX_ATTRIBUTE: &str = "AXTextMarkerForIndex";
const TEXT_MARKER_RANGE_FOR_UNORDERED_TEXT_MARKERS_ATTRIBUTE: &str =
    "AXTextMarkerRangeForUnorderedTextMarkers";

/// Reads the text around the selection of `element` through its text markers. `None` when the
/// user cannot edit `element`: Mail and Safari let an assistive app set the value of a web area
/// only when the user can edit its page.
pub(super) fn field_text_by_markers(element: &AXUIElement) -> Option<FieldText> {
    if !is_settable(element, VALUE_ATTRIBUTE) {
        return None;
    }
    let selection = marker_ends(copy_attribute(element, SELECTED_TEXT_MARKER_RANGE_ATTRIBUTE)?)?;
    let field: &CFType = element;
    let text = marker_ends(copy_attribute_for_parameter(
        element,
        TEXT_MARKER_RANGE_FOR_ELEMENT_ATTRIBUTE,
        field,
    )?)?;
    let index = |marker: &AXTextMarker| marker_index(element, marker);
    let ranges = MarkerIndices {
        text_start: index(&text.start)?,
        selection_start: index(&selection.start)?,
        selection_end: index(&selection.end)?,
        text_end: index(&text.end)?,
    }
    .surrounding_ranges()?;
    let before_start = marker_for_index(element, ranges.before.location)?;
    let after_end = marker_for_index(element, ranges.after.location + ranges.after.length)?;
    Some(FieldText {
        before_caret: text_between(element, &before_start, &selection.start)?,
        after_caret: text_between(element, &selection.end, &after_end)?,
    })
}

/// The text markers at the ends of a text marker range.
struct MarkerEnds {
    start: CFRetained<AXTextMarker>,
    end: CFRetained<AXTextMarker>,
}

fn marker_ends(value: CFRetained<CFType>) -> Option<MarkerEnds> {
    let range = value.downcast::<AXTextMarkerRange>().ok()?;
    // SAFETY: plain calls on a valid text marker range, which holds both markers.
    let (start, end) = unsafe { (range.start_marker(), range.end_marker()) };
    Some(MarkerEnds { start, end })
}

fn marker_index(element: &AXUIElement, marker: &AXTextMarker) -> Option<isize> {
    copy_attribute_for_parameter(element, INDEX_FOR_TEXT_MARKER_ATTRIBUTE, marker)?
        .downcast::<CFNumber>()
        .ok()?
        .as_isize()
}

fn marker_for_index(element: &AXUIElement, index: isize) -> Option<CFRetained<AXTextMarker>> {
    copy_attribute_for_index(element, TEXT_MARKER_FOR_INDEX_ATTRIBUTE, index)?
        .downcast()
        .ok()
}

/// Reads the text of `element` between the markers `first` and `second`, in either order.
fn text_between(element: &AXUIElement, first: &AXTextMarker, second: &AXTextMarker) -> Option<String> {
    let markers = CFArray::from_objects(&[first, second]);
    let range = copy_attribute_for_parameter(
        element,
        TEXT_MARKER_RANGE_FOR_UNORDERED_TEXT_MARKERS_ATTRIBUTE,
        &markers,
    )?;
    string_for_text_marker_range(element, &range)
}

/// The indices of the text markers at the ends of the text of a field and of its selection.
#[derive(Debug, Clone, Copy)]
struct MarkerIndices {
    text_start: isize,
    selection_start: isize,
    selection_end: isize,
    text_end: isize,
}

impl MarkerIndices {
    /// The ranges of the text that a suggestion reads around the selection, as marker indices.
    /// `None` when the selection is not in the text.
    fn surrounding_ranges(self) -> Option<SurroundingRanges> {
        let is_in_text = self.text_start <= self.selection_start
            && self.selection_start <= self.selection_end
            && self.selection_end <= self.text_end;
        if !is_in_text {
            return None;
        }
        let text_length = self.text_end.checked_sub(self.text_start)?;
        let selection = CFRange {
            location: self.selection_start - self.text_start,
            length: self.selection_end - self.selection_start,
        };
        let ranges = SurroundingRanges::new(selection, Some(text_length))?;
        let to_marker_indices = |range: CFRange| CFRange {
            location: range.location + self.text_start,
            ..range
        };
        Some(SurroundingRanges {
            before: to_marker_indices(ranges.before),
            after: to_marker_indices(ranges.after),
        })
    }
}

#[cfg(test)]
mod tests {
    use typealike_core::desktop::{MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET};

    use super::*;

    fn range(location: isize, length: isize) -> CFRange {
        CFRange { location, length }
    }

    fn indices(
        text_start: isize,
        selection_start: isize,
        selection_end: isize,
        text_end: isize,
    ) -> MarkerIndices {
        MarkerIndices {
            text_start,
            selection_start,
            selection_end,
            text_end,
        }
    }

    #[test]
    fn marker_ranges_reach_the_limits_around_the_caret() {
        let max_before = isize::try_from(MAX_CHARS_BEFORE_CARET).unwrap();
        let max_after = isize::try_from(MAX_CHARS_AFTER_CARET).unwrap();
        let caret = max_before + 500;

        let ranges = indices(0, caret, caret, caret + max_after + 500).surrounding_ranges();

        assert_eq!(
            ranges,
            Some(SurroundingRanges {
                before: range(500, max_before),
                after: range(caret, max_after),
            })
        );
    }

    #[test]
    fn marker_ranges_stop_at_the_ends_of_the_field_text_and_skip_the_selection() {
        let ranges = indices(100, 150, 152, 160).surrounding_ranges();

        assert_eq!(
            ranges,
            Some(SurroundingRanges {
                before: range(100, 50),
                after: range(152, 8),
            })
        );
    }

    #[test]
    fn selection_outside_the_field_text_gives_no_marker_ranges() {
        let cases = [
            indices(100, 90, 90, 160),
            indices(100, 150, 170, 160),
            indices(100, 152, 150, 160),
            indices(0, isize::MAX, isize::MAX, 45),
        ];

        let ranges = cases.map(MarkerIndices::surrounding_ranges);

        assert_eq!(ranges, [None; 4]);
    }

    #[test]
    fn field_text_longer_than_an_index_can_count_gives_no_marker_ranges() {
        let ranges = indices(isize::MIN, 0, 0, isize::MAX).surrounding_ranges();

        assert_eq!(ranges, None);
    }
}
