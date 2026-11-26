//! The block cursor of a terminal in the pixels of a field that gives no caret.

use std::collections::HashMap;
use std::ops::RangeInclusive;

use super::{ChannelRange, PixelColor, ScreenPixels, is_one_color};
use crate::desktop::ScreenRect;

/// The widths of a block cursor, in the units of [`ScreenRect`]: one cell of a terminal font from
/// about 8 to 30 points.
const CURSOR_WIDTHS: RangeInclusive<i32> = 4..=20;
/// The heights of a block cursor: one line of a terminal font from about 8 to 30 points.
const CURSOR_HEIGHTS: RangeInclusive<i32> = 10..=40;
/// The width of a block cursor in percent of its height. A cell of a terminal font is about half
/// as wide as it is high. A bar cursor is narrower, and a run of highlighted cells is wider.
const CURSOR_WIDTH_PERCENTS: RangeInclusive<i32> = 30..=75;
/// The lowest contrast of a block cursor to the background, as a contrast ratio of the Web Content
/// Accessibility Guidelines (WCAG): 3:1, the least that they ask for a part of a user interface. A
/// highlight behind text, as in a diff, has less, so that its text stays readable.
const MIN_CURSOR_CONTRAST: f64 = 3.0;
/// The distance between pixels of the background that the search for its color counts.
const BACKGROUND_SAMPLE_STEP: usize = 4;

/// A run of pixels of one color on one row, from `left` to `right`.
#[derive(Debug, Clone, Copy)]
struct Run {
    left: i32,
    right: i32,
    color: PixelColor,
}

/// Runs of the same columns and color on consecutive rows.
#[derive(Debug, Clone, Copy)]
struct Block {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
    color: PixelColor,
}

impl Block {
    fn continues_with(&self, run: &Run) -> bool {
        run.left == self.left && run.right == self.right && is_one_color([self.color, run.color])
    }

    fn rect(&self) -> ScreenRect {
        ScreenRect {
            left: self.left,
            top: self.top,
            right: self.right,
            bottom: self.bottom,
        }
    }

    fn has_cursor_shape(&self) -> bool {
        let width = self.right - self.left;
        let height = self.bottom - self.top;
        CURSOR_WIDTHS.contains(&width)
            && CURSOR_HEIGHTS.contains(&height)
            && CURSOR_WIDTH_PERCENTS.contains(&(width * 100 / height))
    }
}

impl ScreenPixels {
    /// The blocks of these pixels that can be the block cursor of a terminal: rectangles of one
    /// color, about as wide as a cell and as high as a line, with another color at the left and
    /// the right of each row, and with the contrast of a cursor to the background, the color of
    /// the most pixels.
    #[must_use]
    pub fn cursor_blocks(&self) -> Vec<ScreenRect> {
        let background = Luminance::of(self.most_common_color());
        let mut open: Vec<Block> = Vec::new();
        let mut closed = Vec::new();
        for row in self.frame.top..self.frame.bottom {
            let runs = self.cell_runs(row, background);
            let mut is_taken = vec![false; runs.len()];
            let mut next = Vec::with_capacity(runs.len());
            for block in open {
                let continuing = runs
                    .iter()
                    .enumerate()
                    .find(|(index, run)| !is_taken[*index] && block.continues_with(run));
                match continuing {
                    Some((index, _)) => {
                        is_taken[index] = true;
                        next.push(Block {
                            bottom: row + 1,
                            ..block
                        });
                    }
                    None => closed.push(block),
                }
            }
            next.extend(
                runs.iter()
                    .zip(&is_taken)
                    .filter(|(_, is_taken)| !**is_taken)
                    .map(|(run, _)| Block {
                        left: run.left,
                        right: run.right,
                        top: row,
                        bottom: row + 1,
                        color: run.color,
                    }),
            );
            open = next;
        }
        closed.extend(open);
        closed
            .iter()
            .filter(|block| block.has_cursor_shape())
            .map(Block::rect)
            .collect()
    }

    /// The runs of `row` of one color as wide as a cell, with another color on each side, in a
    /// color with the contrast of a cursor to `background`.
    fn cell_runs(&self, row: i32, background: Luminance) -> Vec<Run> {
        let mut runs = Vec::new();
        let mut left = self.frame.left;
        while left < self.frame.right {
            let color = self.color_at(left, row);
            let mut channels = ChannelRange::EMPTY.with(color);
            let mut right = left + 1;
            while right < self.frame.right {
                channels = channels.with(self.color_at(right, row));
                if !channels.is_one_color() {
                    break;
                }
                right += 1;
            }
            let is_bounded = left > self.frame.left && right < self.frame.right;
            if is_bounded
                && CURSOR_WIDTHS.contains(&(right - left))
                && Luminance::of(color).contrast_to(background) >= MIN_CURSOR_CONTRAST
                && !is_one_color([color, self.color_at(left - 1, row)])
                && !is_one_color([color, self.color_at(right, row)])
            {
                runs.push(Run { left, right, color });
            }
            left = right;
        }
        runs
    }

    /// The color that the most pixels have, the background of a terminal.
    fn most_common_color(&self) -> PixelColor {
        let mut counts: HashMap<PixelColor, usize> = HashMap::new();
        for color in self.colors.iter().step_by(BACKGROUND_SAMPLE_STEP) {
            *counts.entry(*color).or_default() += 1;
        }
        counts
            .into_iter()
            .max_by_key(|(color, count)| (*count, color.channels()))
            .map_or(
                PixelColor {
                    red: 0,
                    green: 0,
                    blue: 0,
                },
                |(color, _)| color,
            )
    }
}

/// The relative luminance of a color, from 0 for black to 1 for white, as WCAG defines it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Luminance(f64);

impl Luminance {
    fn of(color: PixelColor) -> Self {
        let [red, green, blue] = color.channels().map(linear_channel);
        Self(0.2126 * red + 0.7152 * green + 0.0722 * blue)
    }

    /// The contrast ratio of WCAG between this luminance and `other`, from 1 to 21.
    fn contrast_to(self, other: Self) -> f64 {
        let (lighter, darker) = if self.0 >= other.0 {
            (self.0, other.0)
        } else {
            (other.0, self.0)
        };
        (lighter + 0.05) / (darker + 0.05)
    }
}

/// The linear light of an sRGB channel, from 0 to 1.
fn linear_channel(channel: u8) -> f64 {
    let value = f64::from(channel) / 255.0;
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
