//! Colors, and shapes and grayscale text coverage turned into premultiplied color pixels for
//! alpha-blended windows.

use std::iter;
use std::ops::Range;

/// Samples along each axis of a pixel, to find how much of the pixel a shape covers.
const SAMPLES_PER_AXIS: i32 = 4;
const SAMPLES_PER_PIXEL: u32 = 16;
/// Sample coordinates count in steps of half the distance between samples, so that each sample is
/// in the middle of its part of the pixel.
const STEPS_PER_PIXEL: i32 = 2 * SAMPLES_PER_AXIS;
/// A line through text is about this many times thinner than the text is high.
const STRIKE_THICKNESS_DIVISOR: usize = 14;
/// The middle of the small letters of a line of text, in percent of its height from the top.
const STRIKE_MIDDLE_PERCENT: usize = 55;

/// An opaque color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
}

impl Rgb {
    pub(crate) const BLACK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
    };
    pub(crate) const WHITE: Self = Self {
        red: 255,
        green: 255,
        blue: 255,
    };

    /// A gray of `level` in each channel.
    pub(crate) const fn gray(level: u8) -> Self {
        Self {
            red: level,
            green: level,
            blue: level,
        }
    }

    /// Reads a Win32 `COLORREF` value laid out as `0x00BBGGRR`.
    pub(crate) fn from_colorref(value: u32) -> Self {
        let [red, green, blue, _] = value.to_le_bytes();
        Self { red, green, blue }
    }

    /// Returns the `COLORREF` value laid out as `0x00BBGGRR`.
    pub(crate) fn to_colorref(self) -> u32 {
        u32::from_le_bytes([self.red, self.green, self.blue, 0])
    }
}

/// Recolors `pixels` (BGRA, white text drawn on black) to `color`, with text coverage as alpha.
pub(crate) fn tint_by_coverage(pixels: &mut [u32], color: Rgb) {
    for pixel in pixels {
        *pixel = premultiplied(color, coverage(*pixel));
    }
}

/// Recolors `pixels` (BGRA, white text drawn on black, `width` pixels in each row) as
/// [`tint_by_coverage`] does, with the color of the span of `spans` that holds the column of each
/// pixel. A pixel in no span keeps its coverage in white.
pub(crate) fn tint_spans(pixels: &mut [u32], width: usize, spans: &[(Range<usize>, Rgb)]) {
    for (index, pixel) in pixels.iter_mut().enumerate() {
        let column = index % width.max(1);
        let color = spans
            .iter()
            .find(|(columns, _)| columns.contains(&column))
            .map_or(Rgb::WHITE, |&(_, color)| color);
        *pixel = premultiplied(color, coverage(*pixel));
    }
}

/// Fills the pixels at `columns` and `rows` of `pixels` (`width` pixels in each row) with the
/// opaque `color`.
pub(crate) fn fill_band(
    pixels: &mut [u32],
    width: usize,
    columns: Range<usize>,
    rows: Range<usize>,
    color: Rgb,
) {
    for row in rows {
        for column in columns.clone().filter(|&column| column < width) {
            if let Some(pixel) = pixels.get_mut(row * width + column) {
                *pixel = premultiplied(color, u8::MAX);
            }
        }
    }
}

/// Returns the rows of a line through text `height` pixels high: about one fourteenth of the height
/// thick, and at the middle of the small letters.
pub(crate) fn strike_rows(height: usize) -> Range<usize> {
    let thickness = (height / STRIKE_THICKNESS_DIVISOR).max(1);
    let middle = height * STRIKE_MIDDLE_PERCENT / 100;
    let top = middle.saturating_sub(thickness / 2);
    top..(top + thickness).min(height)
}

fn coverage(pixel: u32) -> u8 {
    let [blue, green, red, _] = pixel.to_le_bytes();
    blue.max(green).max(red)
}

fn premultiplied(color: Rgb, alpha: u8) -> u32 {
    let scale = |channel: u8| {
        u8::try_from(u16::from(channel) * u16::from(alpha) / u16::from(u8::MAX))
            .expect("a channel scaled by at most one fits in a byte")
    };
    u32::from_le_bytes([scale(color.blue), scale(color.green), scale(color.red), alpha])
}

/// A rectangle with rounded corners: a fill inside a border, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RoundedRectangle {
    pub(crate) width: i32,
    pub(crate) height: i32,
    pub(crate) corner_radius: i32,
    pub(crate) border_width: i32,
    pub(crate) fill: Rgb,
    pub(crate) border: Rgb,
}

impl RoundedRectangle {
    /// Draws the rectangle as premultiplied BGRA pixels, top row first, with smooth edges.
    pub(crate) fn pixels(&self) -> Vec<u32> {
        (0..self.height)
            .flat_map(|row| (0..self.width).map(move |column| self.pixel(column, row)))
            .collect()
    }

    fn pixel(&self, column: i32, row: i32) -> u32 {
        let outer = self.covered_samples(column, row, 0);
        let inner = self.covered_samples(column, row, self.border_width);
        let border = outer.saturating_sub(inner);
        let mean = |fill: u8, stroke: u8| {
            let sum = u32::from(fill) * inner + u32::from(stroke) * border;
            u8::try_from((sum + SAMPLES_PER_PIXEL / 2) / SAMPLES_PER_PIXEL)
                .expect("a mean of bytes fits in a byte")
        };
        let alpha = mean(u8::MAX, u8::MAX);
        u32::from_le_bytes([
            mean(self.fill.blue, self.border.blue),
            mean(self.fill.green, self.border.green),
            mean(self.fill.red, self.border.red),
            alpha,
        ])
    }

    /// Counts the samples of the pixel at `column` and `row` inside the rectangle made smaller by
    /// `inset` on each side.
    fn covered_samples(&self, column: i32, row: i32, inset: i32) -> u32 {
        let samples = 0..SAMPLES_PER_AXIS;
        let covered = samples
            .clone()
            .flat_map(|sample_row| {
                samples
                    .clone()
                    .map(move |sample_column| (sample_column, sample_row))
            })
            .filter(|&(sample_column, sample_row)| {
                self.contains(
                    sample_step(column, sample_column),
                    sample_step(row, sample_row),
                    inset,
                )
            })
            .count();
        u32::try_from(covered).expect("a pixel has few samples")
    }

    /// Whether the point at `x` and `y`, in sample steps, is inside the rectangle made smaller by
    /// `inset` pixels on each side. The smaller rectangle has corners smaller by the same inset.
    fn contains(&self, x: i32, y: i32, inset: i32) -> bool {
        let low = inset * STEPS_PER_PIXEL;
        let right = (self.width - inset) * STEPS_PER_PIXEL;
        let bottom = (self.height - inset) * STEPS_PER_PIXEL;
        let is_in_box = (low..=right).contains(&x) && (low..=bottom).contains(&y);
        if !is_in_box {
            return false;
        }
        let shorter_side = (right - low).min(bottom - low);
        let radius = ((self.corner_radius - inset) * STEPS_PER_PIXEL).clamp(0, shorter_side / 2);
        let horizontal = x - x.clamp(low + radius, right - radius);
        let vertical = y - y.clamp(low + radius, bottom - radius);
        horizontal * horizontal + vertical * vertical <= radius * radius
    }
}

/// The position of sample `sample` of pixel `pixel` along one axis, in sample steps.
fn sample_step(pixel: i32, sample: i32) -> i32 {
    pixel * STEPS_PER_PIXEL + 2 * sample + 1
}

/// Puts `pixels`, in rows of `width`, inside a transparent frame of `horizontal` pixels at the left
/// and the right and `vertical` pixels at the top and the bottom.
pub(crate) fn pad(pixels: &[u32], width: usize, horizontal: usize, vertical: usize) -> Vec<u32> {
    let blank_rows = iter::repeat_n(0, (width + 2 * horizontal) * vertical);
    let side = || iter::repeat_n(0, horizontal);
    blank_rows
        .clone()
        .chain(
            pixels
                .chunks(width.max(1))
                .flat_map(|row| side().chain(row.iter().copied()).chain(side())),
        )
        .chain(blank_rows)
        .collect()
}

/// Lays `front` over `backdrop`, both premultiplied BGRA pixels of the same size: the backdrop
/// shows where the front is transparent.
pub(crate) fn lay_over(front: &mut [u32], backdrop: &[u32]) {
    for (pixel, behind) in front.iter_mut().zip(backdrop) {
        *pixel = over(*pixel, *behind);
    }
}

fn over(front: u32, backdrop: u32) -> u32 {
    let front = front.to_le_bytes();
    let backdrop = backdrop.to_le_bytes();
    let transparency = u16::from(u8::MAX - front[3]);
    let channels: [u8; 4] = std::array::from_fn(|index| {
        let behind = u8::try_from(u16::from(backdrop[index]) * transparency / u16::from(u8::MAX))
            .expect("a byte scaled by at most one fits in a byte");
        front[index].saturating_add(behind)
    });
    u32::from_le_bytes(channels)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb {
        red: 255,
        green: 0,
        blue: 0,
    };

    #[test]
    fn each_span_tints_its_columns_by_coverage() {
        let mut pixels = [u32::MAX; 4];

        tint_spans(&mut pixels, 2, &[(0..1, RED), (1..2, Rgb::BLACK)]);

        assert_eq!(
            pixels,
            [
                premultiplied(RED, 255),
                premultiplied(Rgb::BLACK, 255),
                premultiplied(RED, 255),
                premultiplied(Rgb::BLACK, 255)
            ]
        );
    }

    #[test]
    fn band_fills_only_its_columns_and_rows() {
        let mut pixels = [0_u32; 6];

        fill_band(&mut pixels, 3, 0..2, 1..2, RED);

        let red = premultiplied(RED, 255);
        assert_eq!(pixels, [0, 0, 0, red, red, 0]);
    }

    #[test]
    fn strike_is_one_row_for_small_text_and_thicker_for_large_text() {
        let rows = [strike_rows(14), strike_rows(42)];

        assert_eq!(rows, [7..8, 22..25]);
    }

    const GRAY: Rgb = Rgb {
        red: 0x80,
        green: 0x60,
        blue: 0x40,
    };

    #[test]
    fn full_coverage_is_opaque_color() {
        let mut pixels = [0x00FF_FFFF];

        tint_by_coverage(&mut pixels, GRAY);

        assert_eq!(pixels, [0xFF80_6040]);
    }

    #[test]
    fn no_coverage_is_transparent() {
        let mut pixels = [0x0000_0000];

        tint_by_coverage(&mut pixels, GRAY);

        assert_eq!(pixels, [0x0000_0000]);
    }

    #[test]
    fn partial_coverage_is_premultiplied() {
        let mut pixels = [0x0080_8080];

        tint_by_coverage(&mut pixels, Rgb::WHITE);

        assert_eq!(pixels, [0x8080_8080]);
    }

    const SQUARE: RoundedRectangle = RoundedRectangle {
        width: 12,
        height: 12,
        corner_radius: 4,
        border_width: 1,
        fill: Rgb::WHITE,
        border: Rgb::BLACK,
    };

    fn pixel_at(pixels: &[u32], column: usize, row: usize) -> u32 {
        pixels[row * 12 + column]
    }

    #[test]
    fn middle_of_square_is_opaque_fill() {
        let pixels = SQUARE.pixels();

        assert_eq!(pixel_at(&pixels, 6, 6), 0xFFFF_FFFF);
    }

    #[test]
    fn straight_edge_of_square_is_opaque_border() {
        let pixels = SQUARE.pixels();

        assert_eq!(pixel_at(&pixels, 6, 0), 0xFF00_0000);
    }

    #[test]
    fn corner_outside_the_arc_is_transparent() {
        let pixels = SQUARE.pixels();

        assert_eq!(pixel_at(&pixels, 0, 0), 0);
    }

    #[test]
    fn pixel_on_the_arc_is_partly_covered() {
        let pixels = SQUARE.pixels();

        let alpha = pixel_at(&pixels, 1, 1) >> 24;

        assert!((1..255).contains(&alpha), "alpha {alpha}");
    }

    #[test]
    fn square_has_a_pixel_for_each_point() {
        let pixels = SQUARE.pixels();

        assert_eq!(pixels.len(), 144);
    }

    #[test]
    fn short_sides_of_a_wide_rectangle_are_opaque_border() {
        let wide = RoundedRectangle {
            width: 20,
            height: 8,
            corner_radius: 2,
            ..SQUARE
        };

        let pixels = wide.pixels();

        assert_eq!(
            [pixels[4 * 20], pixels[4 * 20 + 10], pixels[4 * 20 + 19]],
            [0xFF00_0000, 0xFFFF_FFFF, 0xFF00_0000]
        );
    }

    #[test]
    fn padding_puts_transparent_pixels_around_the_text() {
        let padded = pad(&[0xFFFF_FFFF, 0x8080_8080], 2, 1, 1);

        assert_eq!(padded, [0, 0, 0, 0, 0, 0xFFFF_FFFF, 0x8080_8080, 0, 0, 0, 0, 0,]);
    }

    #[test]
    fn transparent_front_shows_backdrop() {
        let mut front = [0x0000_0000];

        lay_over(&mut front, &[0xFF10_2030]);

        assert_eq!(front, [0xFF10_2030]);
    }

    #[test]
    fn opaque_front_hides_backdrop() {
        let mut front = [0xFF80_6040];

        lay_over(&mut front, &[0xFFFF_FFFF]);

        assert_eq!(front, [0xFF80_6040]);
    }

    #[test]
    fn half_transparent_front_blends_with_backdrop() {
        let mut front = [0x8080_8080];

        lay_over(&mut front, &[0xFF00_0000]);

        assert_eq!(front, [0xFF80_8080]);
    }

    #[test]
    fn colorref_round_trips() {
        let color = Rgb::from_colorref(0x0040_6080);

        assert_eq!((color, color.to_colorref()), (GRAY, 0x0040_6080));
    }
}
