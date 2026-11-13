//! The box that a terminal app draws around the caret in Windows Terminal.
//!
//! A terminal app such as opencode draws its input box as cells with a background of their own,
//! with no border character on its right side. UIA gives the background of each cell, so the box
//! on the row of the caret is the run of cells from the caret that have the background of the cell
//! before the caret.

use typealike_core::desktop::is_line_break;
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationTextRange, TextPatternRangeEndpoint_End,
    TextPatternRangeEndpoint_Start, TextUnit_Character, TextUnit_Line, UIA_BackgroundColorAttributeId,
};

use super::drawn_caret::{VALUES_PER_RECTANGLE, bounding_rectangles, colorref, pixel_edges};
use super::{WHOLE_RANGE, caret_range};
use crate::utf16::whole_chars;

/// The class of the terminal control of Windows Terminal in UIA.
const WINDOWS_TERMINAL_CLASS: &str = "TermControl";

/// The box of a terminal app around the caret, on the row of the caret.
#[derive(Debug)]
pub(super) struct CaretBox {
    /// The cells of the box from the caret to the end of the box.
    after_caret: IUIAutomationTextRange,
    /// The right edge of the box in physical screen pixels, or `None` when the box goes to the end
    /// of the row.
    right_edge: Option<i32>,
}

impl CaretBox {
    /// Reads the box around the caret of `element` when `element` is the terminal of Windows
    /// Terminal. The box starts at the caret, because an app can draw its own cursor in the cell
    /// at the caret with other colors. `None` in another control, at the end of the row, or when
    /// UIA does not give the background of the cell before the caret.
    pub(super) fn of(element: &IUIAutomationElement) -> Option<Self> {
        if !is_windows_terminal(element) {
            return None;
        }
        // SAFETY: plain calls on interfaces of this thread.
        let (caret, row) = unsafe {
            let caret = caret_range(element)?;
            caret
                .MoveEndpointByRange(
                    TextPatternRangeEndpoint_End,
                    &caret,
                    TextPatternRangeEndpoint_Start,
                )
                .ok()?;
            let row = caret.Clone().ok()?;
            row.ExpandToEnclosingUnit(TextUnit_Line).ok()?;
            (caret, row)
        };
        let cells_after_caret = cells_after(&caret, &row)?.checked_sub(1)?;
        let background = background_before(&caret, &row)?;
        let after_cursor = cells_from(&caret, 1, 1)?;
        let has_background = |cells: Option<IUIAutomationTextRange>| {
            cells.as_ref().and_then(background_of) == Some(background)
        };
        let cells_after_cursor = box_cells(
            cells_after_caret,
            |cells| has_background(cells_from(&after_cursor, 0, cells)),
            |cell| has_background(cells_from(&after_cursor, cell, cell.saturating_add(1))),
        );
        let after_caret = cells_from(&caret, 0, cells_after_cursor.saturating_add(1))?;
        let right_edge = (cells_after_cursor < cells_after_caret)
            .then(|| right_edge_of(&after_caret))
            .flatten();
        Some(Self {
            after_caret,
            right_edge,
        })
    }

    /// The text range from the caret to the end of the box.
    pub(super) fn after_caret(&self) -> &IUIAutomationTextRange {
        &self.after_caret
    }

    /// `frame` with its right edge at the right edge of the box, when the box ends before the end
    /// of its row and inside `frame`.
    pub(super) fn narrow(&self, frame: RECT) -> RECT {
        let Some(right_edge) = self
            .right_edge
            .filter(|edge| (frame.left..frame.right).contains(edge))
        else {
            return frame;
        };
        RECT {
            right: right_edge,
            ..frame
        }
    }
}

fn is_windows_terminal(element: &IUIAutomationElement) -> bool {
    // SAFETY: plain call on an interface of this thread.
    unsafe { element.CurrentClassName() }.is_ok_and(|class| class == WINDOWS_TERMINAL_CLASS)
}

/// The cells of `row` from `caret` to the end of the row, without the line break at its end.
fn cells_after(caret: &IUIAutomationTextRange, row: &IUIAutomationTextRange) -> Option<usize> {
    // SAFETY: plain calls on interfaces of this thread.
    let text = unsafe {
        let rest = caret.Clone().ok()?;
        rest.MoveEndpointByRange(TextPatternRangeEndpoint_End, row, TextPatternRangeEndpoint_End)
            .ok()?;
        rest.GetText(WHOLE_RANGE).ok()?
    };
    Some(whole_chars(&text).trim_end_matches(is_line_break).chars().count())
}

/// The background of the cell before `caret` on `row`, else of the cell at `caret` at the start of
/// the row.
fn background_before(caret: &IUIAutomationTextRange, row: &IUIAutomationTextRange) -> Option<u32> {
    // SAFETY: plain calls on interfaces of this thread.
    let cell_before = unsafe {
        let cell = caret.Clone().ok()?;
        let moved = cell
            .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -1)
            .ok()?;
        let is_on_row = moved == -1
            && cell
                .CompareEndpoints(
                    TextPatternRangeEndpoint_Start,
                    row,
                    TextPatternRangeEndpoint_Start,
                )
                .ok()?
                >= 0;
        is_on_row.then_some(cell)
    };
    background_of(&cell_before.or_else(|| cells_from(caret, 0, 1))?)
}

/// The cells from `first` to `end`, counted from `start`, the empty range at a cell. Each move is a
/// call to the app, so a range from the start moves only its end.
fn cells_from(start: &IUIAutomationTextRange, first: usize, end: usize) -> Option<IUIAutomationTextRange> {
    let (first, end) = (i32::try_from(first).ok()?, i32::try_from(end).ok()?);
    // SAFETY: plain calls on interfaces of this thread.
    unsafe {
        let cells = start.Clone().ok()?;
        cells
            .MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, end)
            .ok()?;
        if first > 0 {
            cells
                .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, first)
                .ok()?;
        }
        Some(cells)
    }
}

/// The background of all cells of `cells`, or `None` when they have more than one background or
/// UIA does not give it.
fn background_of(cells: &IUIAutomationTextRange) -> Option<u32> {
    // SAFETY: plain call on an interface of this thread.
    let value = unsafe { cells.GetAttributeValue(UIA_BackgroundColorAttributeId) }.ok()?;
    colorref(&value)
}

/// The right edge of the cells of `cells` in physical screen pixels.
fn right_edge_of(cells: &IUIAutomationTextRange) -> Option<i32> {
    let rectangles = bounding_rectangles(cells)?;
    rectangles
        .as_chunks::<VALUES_PER_RECTANGLE>()
        .0
        .iter()
        .filter_map(|&rectangle| pixel_edges(rectangle))
        .map(|[_, _, right, _]| right)
        .max()
}

/// The cells of the box after the cursor: the longest run of cells from the start of a row of
/// `limit` cells that have the background of the box. `spans_background(cells)` tells whether the
/// first `cells` cells have it, and `has_background(cell)` tells it for one cell. Windows Terminal
/// leaves out the last cell of a range of several cells when it gives their background, so the
/// last cell of the run must also have the background alone.
fn box_cells(
    limit: usize,
    spans_background: impl Fn(usize) -> bool,
    has_background: impl Fn(usize) -> bool,
) -> usize {
    let longest = longest_run(limit, spans_background);
    match longest.checked_sub(1) {
        Some(last) if !has_background(last) => last,
        _ => longest,
    }
}

/// The most cells, at most `limit`, for which `spans_background` holds. It holds for no cells, and
/// when it holds for some cells, it also holds for fewer.
fn longest_run(limit: usize, spans_background: impl Fn(usize) -> bool) -> usize {
    if limit == 0 || spans_background(limit) {
        return limit;
    }
    let (mut longest, mut shortest_without) = (0, limit);
    while shortest_without - longest > 1 {
        let middle = longest + (shortest_without - longest) / 2;
        if spans_background(middle) {
            longest = middle;
        } else {
            shortest_without = middle;
        }
    }
    longest
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: u32 = 0x1E_1E1E;
    const TERMINAL: u32 = 0x0A_0A0A;

    /// A row of `cells` cells after the cursor, whose first `box_cells` cells have the background
    /// of the box.
    fn row(box_cells: usize, cells: usize) -> Vec<u32> {
        let mut row = vec![BOX; box_cells];
        row.resize(cells, TERMINAL);
        row
    }

    fn spans(row: &[u32], cells: usize) -> bool {
        row[..cells].iter().all(|&background| background == BOX)
    }

    /// As Windows Terminal gives the background of a range of several cells: without its last cell.
    fn spans_without_last_cell(row: &[u32], cells: usize) -> bool {
        spans(row, cells.saturating_sub(1))
    }

    #[test]
    fn box_ends_at_the_first_cell_of_another_background() {
        let rows = [
            row(8, 10),
            row(10, 10),
            row(0, 10),
            row(9, 10),
            row(1, 3),
            row(0, 0),
        ];

        let found = rows
            .clone()
            .map(|row| box_cells(row.len(), |cells| spans(&row, cells), |cell| row[cell] == BOX));

        assert_eq!(found, [8, 10, 0, 9, 1, 0]);
    }

    #[test]
    fn box_ends_at_the_first_cell_of_another_background_also_when_ranges_leave_out_their_last_cell() {
        let rows = [
            row(8, 10),
            row(10, 10),
            row(0, 10),
            row(9, 10),
            row(1, 3),
            row(0, 0),
        ];

        let found = rows.clone().map(|row| {
            box_cells(
                row.len(),
                |cells| spans_without_last_cell(&row, cells),
                |cell| row[cell] == BOX,
            )
        });

        assert_eq!(found, [8, 10, 0, 9, 1, 0]);
    }
}
