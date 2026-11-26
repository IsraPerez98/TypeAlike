//! The geometry of the band that the OCR reads.
//!
//! It has the band around the focused field, in the pixels of the window, and the check of the
//! field before the capture of the window.

use std::ops::Range;

use typealike_core::desktop::ScreenRect;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::POINT;

use crate::display::DEFAULT_DPI;
use crate::window_capture::PixelRect;

/// The height of the window above and below the field that the OCR reads, in pixels at a scale
/// of 100 %. Core keeps only the lines nearest to the caret, so an OCR of the lines farther away
/// is wasted work.
const BAND_DIPS: i32 = 600;

/// The reach of the band above and below the field, in pixels, for a window at `dpi`. Windows
/// gives a `dpi` of zero when it cannot tell.
pub(super) fn reach_for(dpi: u32) -> i32 {
    let dpi = i32::try_from(dpi)
        .ok()
        .filter(|dpi| *dpi > 0)
        .unwrap_or(DEFAULT_DPI);
    BAND_DIPS.saturating_mul(dpi) / DEFAULT_DPI
}

/// The band of a frame of `frame` pixels that the OCR reads: the rows from `reach` above `field`
/// to `reach` below it, as wide as the frame. A band larger than `max_side` in a direction
/// shrinks around the middle of the field. `None` when the field is outside of the frame.
pub(super) fn band_in_frame(
    field: PixelRect,
    frame: SizeInt32,
    reach: i32,
    max_side: i32,
) -> Option<PixelRect> {
    let rows = field.top.saturating_sub(reach).max(0)..field.bottom.saturating_add(reach).min(frame.Height);
    if !holds_field(field, frame) || rows.is_empty() {
        return None;
    }
    let middle = POINT {
        x: field.left.midpoint(field.right),
        y: field.top.midpoint(field.bottom),
    };
    let rows = shrink_around(rows, middle.y, max_side);
    let columns = shrink_around(0..frame.Width, middle.x, max_side);
    Some(PixelRect {
        left: columns.start,
        top: rows.start,
        right: columns.end,
        bottom: rows.end,
    })
}

/// Why the band of a window cannot hold the focused field, so that a capture of the window gives
/// the OCR nothing to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldMiss {
    /// The frame of the field has no width or no height, so it tells no place of the field.
    NoArea,
    /// No row of the field is in the window, or the middle of the field is not between the left
    /// and the right edge of the window.
    OutsideWindow,
}

/// Why the band of `window`, the frame of a window on the screen, cannot hold `field`, the frame of
/// its focused field on the screen. `None` when the band can hold it. The rule is the rule of the
/// band of a captured frame ([`band_in_frame`]).
pub(super) fn field_miss(field: ScreenRect, window: ScreenRect) -> Option<FieldMiss> {
    if !field.has_area() {
        return Some(FieldMiss::NoArea);
    }
    let origin = POINT {
        x: window.left,
        y: window.top,
    };
    let size = SizeInt32 {
        Width: window.right.saturating_sub(window.left),
        Height: window.bottom.saturating_sub(window.top),
    };
    (!holds_field(PixelRect::from_screen(field, origin), size)).then_some(FieldMiss::OutsideWindow)
}

/// Whether a frame of `frame` pixels holds `field` for the OCR: a row of the field is in the frame,
/// and the middle of the field is between the left and the right edge of the frame.
fn holds_field(field: PixelRect, frame: SizeInt32) -> bool {
    let overlaps_rows = field.top < frame.Height && field.bottom > 0;
    overlaps_rows && (0..frame.Width).contains(&field.left.midpoint(field.right))
}

/// Shrinks `range` to at most `max_length`, around `middle` as far as the range allows.
fn shrink_around(range: Range<i32>, middle: i32, max_length: i32) -> Range<i32> {
    if range.end.saturating_sub(range.start) <= max_length {
        return range;
    }
    let start = middle
        .saturating_sub(max_length / 2)
        .clamp(range.start, range.end.saturating_sub(max_length));
    start..start.saturating_add(max_length)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_capture::tests::rect;

    const FRAME: SizeInt32 = SizeInt32 {
        Width: 1000,
        Height: 2000,
    };

    #[test]
    fn band_reaches_above_and_below_the_field_within_the_frame() {
        let bands = [
            band_in_frame(rect(100, 1000, 500, 1030), FRAME, 600, 5000),
            band_in_frame(rect(100, 50, 500, 80), FRAME, 600, 5000),
        ];

        assert_eq!(
            bands,
            [Some(rect(0, 400, 1000, 1630)), Some(rect(0, 0, 1000, 680))]
        );
    }

    #[test]
    fn band_larger_than_the_engine_reads_shrinks_around_the_field() {
        let band = band_in_frame(rect(800, 1000, 900, 1030), FRAME, 600, 400);

        assert_eq!(band, Some(rect(600, 815, 1000, 1215)));
    }

    #[test]
    fn field_outside_of_the_frame_gives_no_band() {
        let bands = [
            band_in_frame(rect(1200, 1000, 1300, 1030), FRAME, 600, 5000),
            band_in_frame(rect(100, 2700, 500, 2730), FRAME, 600, 5000),
        ];

        assert_eq!(bands, [None, None]);
    }

    #[test]
    fn field_just_above_or_below_the_frame_gives_no_band() {
        let bands = [
            band_in_frame(rect(100, -100, 500, -70), FRAME, 600, 5000),
            band_in_frame(rect(100, 2070, 500, 2100), FRAME, 600, 5000),
        ];

        assert_eq!(bands, [None, None]);
    }

    const WINDOW: ScreenRect = ScreenRect {
        left: -1000,
        top: -500,
        right: 1000,
        bottom: 1500,
    };

    fn screen_rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn field_with_its_middle_in_the_window_gives_no_miss() {
        let fields = [
            screen_rect(-900, 500, -500, 530),
            screen_rect(-1100, -600, -700, -480),
            screen_rect(900, 1480, 1050, 1600),
        ];

        let misses = fields.map(|field| field_miss(field, WINDOW));

        assert_eq!(misses, [None, None, None]);
    }

    #[test]
    fn field_outside_of_the_window_is_a_miss() {
        let fields = [
            screen_rect(1100, 500, 1400, 530),
            screen_rect(800, 500, 1400, 530),
            screen_rect(-900, 1500, -500, 1530),
            screen_rect(-900, -600, -500, -500),
        ];

        let misses = fields.map(|field| field_miss(field, WINDOW));

        assert_eq!(misses, [Some(FieldMiss::OutsideWindow); 4]);
    }

    #[test]
    fn field_without_an_area_is_a_miss_also_in_the_window() {
        let fields = [
            ScreenRect::default(),
            screen_rect(-500, 500, -500, 530),
            screen_rect(-900, 500, -500, 500),
        ];

        let misses = fields.map(|field| field_miss(field, WINDOW));

        assert_eq!(misses, [Some(FieldMiss::NoArea); 3]);
    }

    #[test]
    fn reach_grows_with_the_scale_of_the_window() {
        let reaches = [reach_for(96), reach_for(144), reach_for(0)];

        assert_eq!(reaches, [600, 900, 600]);
    }
}
