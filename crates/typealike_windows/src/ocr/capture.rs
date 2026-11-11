//! The capture of a band of a window.
//!
//! The band holds rows of BGRA pixels without padding.

use std::hash::{DefaultHasher, Hasher};
use std::ops::Range;

use typealike_core::desktop::ScreenRect;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::HiDpi::GetDpiForWindow;

use super::OcrError;
use super::geometry::{band_in_frame, reach_for};
use crate::window_capture::{BYTES_PER_PIXEL, Capturer, PixelRect};

/// Captures the band of `window` from `BAND_DIPS` above `field` to `BAND_DIPS` below it, as wide
/// as the window and at most `max_side` pixels in each direction, at full resolution.
pub(super) fn capture_band(
    capturer: &Capturer,
    window: HWND,
    field: ScreenRect,
    max_side: i32,
) -> Result<Band, OcrError> {
    let frame = capturer.capture_window(window)?;
    // SAFETY: plain call with the handle of a window.
    let reach = reach_for(unsafe { GetDpiForWindow(window) });
    let field_in_window = PixelRect::from_screen(field, frame.origin);
    let band =
        band_in_frame(field_in_window, frame.size, reach, max_side).ok_or(OcrError::FieldOutsideWindow)?;
    Ok(Band {
        pixels: capturer.copy_pixels(&frame.texture, band)?,
        width: usize::try_from(band.width())?,
        height: usize::try_from(band.height())?,
        origin: POINT {
            x: frame.origin.x.saturating_add(band.left),
            y: frame.origin.y.saturating_add(band.top),
        },
    })
}

/// The pixels of a band of a window: rows of BGRA pixels without padding.
pub(super) struct Band {
    pub(super) pixels: Vec<u8>,
    pub(super) width: usize,
    pub(super) height: usize,
    /// Where the top left pixel of the band is on the screen.
    pub(super) origin: POINT,
}

impl Band {
    /// Sets the pixels of `field`, a rectangle on the screen, to zero. Thus the typed text and the
    /// caret in the field do not change the hash, and the OCR does not read them.
    pub(super) fn hide_field(&mut self, field: ScreenRect) {
        clear(
            &mut self.pixels,
            self.width,
            PixelRect::from_screen(field, self.origin),
        );
    }

    /// The hash of the pixels, which is the same for two bands with the same pixels.
    pub(super) fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(&self.pixels);
        hasher.finish()
    }
}

/// Sets the pixels of `rect` in `pixels`, rows of BGRA pixels `width` pixels wide, to zero.
fn clear(pixels: &mut [u8], width: usize, rect: PixelRect) {
    let row_bytes = width * BYTES_PER_PIXEL;
    if row_bytes == 0 {
        return;
    }
    let rows = clamped(rect.top..rect.bottom, pixels.len() / row_bytes);
    let columns = clamped(rect.left..rect.right, width);
    for row in pixels.chunks_exact_mut(row_bytes).take(rows.end).skip(rows.start) {
        row[columns.start * BYTES_PER_PIXEL..columns.end * BYTES_PER_PIXEL].fill(0);
    }
}

/// The indexes of `range` that lie in `0..limit`.
fn clamped(range: Range<i32>, limit: usize) -> Range<usize> {
    let to_index = |value: i32| usize::try_from(value).unwrap_or(0).min(limit);
    let start = to_index(range.start);
    start..to_index(range.end).max(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_capture::tests::rect;

    #[test]
    fn clear_sets_only_the_pixels_of_the_rectangle_to_zero() {
        let mut pixels = vec![1; 3 * 3 * BYTES_PER_PIXEL];

        clear(&mut pixels, 3, rect(1, 1, 5, 2));

        let mut expected = vec![1; 3 * 3 * BYTES_PER_PIXEL];
        expected[16..24].fill(0);
        assert_eq!(pixels, expected);
    }

    #[test]
    fn rectangle_outside_of_the_image_clears_nothing() {
        let mut pixels = vec![1; 2 * 2 * BYTES_PER_PIXEL];

        clear(&mut pixels, 2, rect(-10, -10, -1, -1));

        assert_eq!(pixels, vec![1; 2 * 2 * BYTES_PER_PIXEL]);
    }
}
