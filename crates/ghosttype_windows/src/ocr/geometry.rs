//! The geometry of the band that the OCR reads.
//!
//! It has the rectangles of pixels in a window or an image, and the band around the focused field.

use std::ops::Range;

use ghosttype_core::desktop::ScreenRect;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::POINT;

use crate::display::DEFAULT_DPI;

/// The height of the window above and below the field that the OCR reads, in pixels at a scale
/// of 100 %. Core keeps only the lines nearest to the caret, so an OCR of the lines farther away
/// is wasted work.
const BAND_DIPS: i32 = 600;

/// A rectangle of pixels from the top left corner of an image or a window: the columns
/// `left..right` of the rows `top..bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PixelRect {
    pub(super) left: i32,
    pub(super) top: i32,
    pub(super) right: i32,
    pub(super) bottom: i32,
}

impl PixelRect {
    /// The rectangle `rect` of the screen, counted from `origin`.
    pub(super) fn from_screen(rect: ScreenRect, origin: POINT) -> Self {
        Self {
            left: rect.left.saturating_sub(origin.x),
            top: rect.top.saturating_sub(origin.y),
            right: rect.right.saturating_sub(origin.x),
            bottom: rect.bottom.saturating_sub(origin.y),
        }
    }

    /// The rectangle of the screen where this rectangle, counted from `origin`, is.
    pub(super) fn on_screen(self, origin: POINT) -> ScreenRect {
        ScreenRect {
            left: self.left.saturating_add(origin.x),
            top: self.top.saturating_add(origin.y),
            right: self.right.saturating_add(origin.x),
            bottom: self.bottom.saturating_add(origin.y),
        }
    }

    pub(super) fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    pub(super) fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }
}

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
    let columns = 0..frame.Width;
    let middle = POINT {
        x: field.left.midpoint(field.right),
        y: field.top.midpoint(field.bottom),
    };
    let has_field = rows.start < rows.end && (columns.start..columns.end).contains(&middle.x);
    if !has_field {
        return None;
    }
    let rows = shrink_around(rows, middle.y, max_side);
    let columns = shrink_around(columns, middle.x, max_side);
    Some(PixelRect {
        left: columns.start,
        top: rows.start,
        right: columns.end,
        bottom: rows.end,
    })
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
    use crate::ocr::tests::rect;

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
    fn reach_grows_with_the_scale_of_the_window() {
        let reaches = [reach_for(96), reach_for(144), reach_for(0)];

        assert_eq!(reaches, [600, 900, 600]);
    }

    #[test]
    fn frame_in_the_band_moves_to_the_screen_and_back() {
        let origin = POINT { x: -1920, y: 300 };
        let field = ScreenRect {
            left: -1800,
            top: 900,
            right: -1200,
            bottom: 930,
        };

        let in_band = PixelRect::from_screen(field, origin);

        assert_eq!(
            (in_band, in_band.on_screen(origin)),
            (rect(120, 600, 720, 630), field)
        );
    }
}
