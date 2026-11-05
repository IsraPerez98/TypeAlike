//! The text of a CodeMirror editor, as in Obsidian. Chromium gives its range text with each line
//! after the other and no line break, and with one line break for each empty line, so the line
//! breaks come from the lines of the editor.

use objc2_application_services::AXUIElement;
use objc2_core_foundation::{CFRange, CFRetained, CFString};
use typealike_core::desktop::{FieldText, MAX_CHARS_AFTER_CARET, is_line_break};

use crate::accessibility::{children, copy_attribute_for_parameter, frame, has_dom_class, string_for_range};
use crate::focus::caret::caret_bounds;

/// The DOM class of the content of a CodeMirror editor. Each of its children is one line.
const CONTENT_CLASS: &str = "cm-content";
const TEXT_MARKER_RANGE_FOR_ELEMENT_ATTRIBUTE: &str = "AXTextMarkerRangeForUIElement";
const STRING_FOR_TEXT_MARKER_RANGE_ATTRIBUTE: &str = "AXStringForTextMarkerRange";
/// The character that the text of a line gives for a widget, such as an image, which the range
/// text leaves out.
const OBJECT_REPLACEMENT: char = '\u{fffc}';

/// Reads the text around `selection` of `element`, the content of a CodeMirror editor, with a line
/// break between its lines. `before` is the range of the text before the selection. `None` when
/// `element` is no CodeMirror content, or when its lines do not give its range text.
pub(super) fn code_mirror_text(
    element: &AXUIElement,
    selection: CFRange,
    before: CFRange,
) -> Option<FieldText> {
    if !has_dom_class(element, CONTENT_CLASS) {
        return None;
    }
    let range_before = string_for_range(element, before)?;
    let lines = children(element);
    let texts = lines
        .iter()
        .map(|line| line_text(element, line))
        .collect::<Option<Vec<_>>>()?;
    let selection = Selection {
        start: usize::try_from(selection.location).ok()?,
        end: usize::try_from(selection.location.checked_add(selection.length)?).ok()?,
    };
    text_around(&texts, &range_before, selection, |line| {
        is_caret_on(element, &lines[line])
    })
}

/// Reads the text of `line`, a line of the CodeMirror content `element`, without widgets and
/// without the line break that an empty line gives for its `<br>`.
fn line_text(element: &AXUIElement, line: &CFRetained<AXUIElement>) -> Option<String> {
    let range =
        copy_attribute_for_parameter(element, TEXT_MARKER_RANGE_FOR_ELEMENT_ATTRIBUTE, line.as_ref())?;
    let text = copy_attribute_for_parameter(element, STRING_FOR_TEXT_MARKER_RANGE_ATTRIBUTE, &range)?
        .downcast::<CFString>()
        .ok()?;
    let is_kept = |character: &char| *character != OBJECT_REPLACEMENT && !is_line_break(*character);
    Some(text.to_string().chars().filter(is_kept).collect())
}

/// Whether the caret of `element` is on `line`: the middle of the caret box is not above the line.
fn is_caret_on(element: &AXUIElement, line: &AXUIElement) -> bool {
    caret_bounds(element)
        .zip(frame(line))
        .is_some_and(|(caret, line)| caret.mid().y >= line.min().y)
}

/// The UTF-16 offsets of a selection in the range text.
#[derive(Debug, Clone, Copy)]
struct Selection {
    start: usize,
    end: usize,
}

/// Returns the text around `selection` with a line break between the lines `texts`. `range_before`
/// is the range text just before the selection, which must match the lines. At the start of a line
/// after a line with text, the range text has the same offset for the end of the line before, so
/// `is_caret_on_line` tells whether the caret is on the line with that index.
fn text_around(
    texts: &[String],
    range_before: &str,
    selection: Selection,
    is_caret_on_line: impl Fn(usize) -> bool,
) -> Option<FieldText> {
    let lines = JoinedLines::of(texts);
    let before: Vec<u16> = range_before.encode_utf16().collect();
    let before_start = selection.start.checked_sub(before.len())?;
    if lines.range_text.get(before_start..selection.start)? != before.as_slice() {
        return None;
    }
    let after_end = selection
        .end
        .saturating_add(MAX_CHARS_AFTER_CARET)
        .min(lines.range_text.len());
    let mut caret = *lines.offsets.get(selection.start)?;
    let shared_start = lines
        .starts
        .iter()
        .position(|&start| start == selection.start)
        .filter(|&line| line > 0 && !texts[line - 1].is_empty());
    if shared_start.is_some_and(|line| !is_caret_on_line(line)) {
        caret -= 1;
    }
    let after_start = if selection.end == selection.start {
        caret
    } else {
        *lines.offsets.get(selection.end)?
    };
    let text_slice = |start: usize, end: usize| {
        lines
            .text
            .get(start..end.max(start))
            .map(String::from_utf16_lossy)
    };
    Some(FieldText {
        before_caret: text_slice(lines.offsets[before_start], caret)?,
        after_caret: text_slice(after_start, *lines.offsets.get(after_end)?)?,
    })
}

/// The lines of a CodeMirror editor, as its range text gives them and with line breaks.
struct JoinedLines {
    /// The lines in UTF-16, each after the other, with a line break for each empty line.
    range_text: Vec<u16>,
    /// The lines in UTF-16 with a line break between them.
    text: Vec<u16>,
    /// The offset in `text` for each offset in `range_text`, and for its end. An offset at the
    /// start of a line is the start of that line.
    offsets: Vec<usize>,
    /// The offset in `range_text` where each line starts.
    starts: Vec<usize>,
}

impl JoinedLines {
    fn of(texts: &[String]) -> Self {
        let mut lines = Self {
            range_text: Vec::new(),
            text: Vec::new(),
            offsets: Vec::new(),
            starts: Vec::new(),
        };
        for (index, line) in texts.iter().enumerate() {
            if index > 0 {
                lines.text.push(u16::from(b'\n'));
            }
            lines.starts.push(lines.range_text.len());
            if line.is_empty() {
                lines.offsets.push(lines.text.len());
                lines.range_text.push(u16::from(b'\n'));
                continue;
            }
            for unit in line.encode_utf16() {
                lines.offsets.push(lines.text.len());
                lines.range_text.push(unit);
                lines.text.push(unit);
            }
        }
        lines.offsets.push(lines.text.len());
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: [&str; 4] = ["# Trip", "", "Flight at 07:45.", "- Day one"];

    fn texts(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|&line| line.to_owned()).collect()
    }

    /// The range text of `lines`: each line after the other, a line break for an empty line.
    fn range_text(lines: &[&str]) -> String {
        lines
            .iter()
            .map(|&line| if line.is_empty() { "\n" } else { line })
            .collect()
    }

    fn caret_at(offset: usize) -> Selection {
        Selection {
            start: offset,
            end: offset,
        }
    }

    fn field_text(before_caret: &str, after_caret: &str) -> FieldText {
        FieldText {
            before_caret: before_caret.to_owned(),
            after_caret: after_caret.to_owned(),
        }
    }

    #[test]
    fn lines_of_the_editor_get_a_line_break_between_them() {
        let range = range_text(&NOTE);

        let text = text_around(&texts(&NOTE), &range, caret_at(range.len()), |_| true);

        assert_eq!(
            text,
            Some(field_text("# Trip\n\nFlight at 07:45.\n- Day one", ""))
        );
    }

    #[test]
    fn caret_inside_a_line_splits_the_text_there() {
        let range = range_text(&NOTE);
        let caret = range.find("07:45").unwrap();

        let text = text_around(&texts(&NOTE), &range[..caret], caret_at(caret), |_| true);

        assert_eq!(
            text,
            Some(field_text("# Trip\n\nFlight at ", "07:45.\n- Day one"))
        );
    }

    #[test]
    fn caret_at_the_start_of_a_line_after_text_goes_where_the_caret_box_is() {
        let range = range_text(&NOTE);
        let caret = range.find("- Day").unwrap();

        let [on_the_line, before_it] = [true, false]
            .map(|is_on_line| text_around(&texts(&NOTE), &range[..caret], caret_at(caret), |_| is_on_line));

        assert_eq!(
            [on_the_line, before_it],
            [
                Some(field_text("# Trip\n\nFlight at 07:45.\n", "- Day one")),
                Some(field_text("# Trip\n\nFlight at 07:45.", "\n- Day one")),
            ]
        );
    }

    #[test]
    fn caret_on_a_new_empty_line_is_on_that_line_when_the_caret_box_is_there() {
        let lines = ["Hello", ""];
        let range = range_text(&lines);

        let text = text_around(&texts(&lines), "Hello", caret_at(5), |line| line == 1);

        assert_eq!(text, Some(field_text("Hello\n", "")));
        assert_eq!(range, "Hello\n");
    }

    #[test]
    fn range_text_that_the_lines_do_not_give_gives_no_text() {
        let text = text_around(&texts(&NOTE), "Flight at 7", caret_at(18), |_| true);

        assert_eq!(text, None);
    }

    #[test]
    fn range_text_before_a_part_of_the_lines_keeps_only_that_part() {
        let range = range_text(&NOTE);
        let caret = range.find("07:45").unwrap();
        let start = range.find("Flight").unwrap();

        let text = text_around(&texts(&NOTE), &range[start..caret], caret_at(caret), |_| true);

        assert_eq!(text, Some(field_text("Flight at ", "07:45.\n- Day one")));
    }
}
