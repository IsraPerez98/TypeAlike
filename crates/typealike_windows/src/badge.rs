//! The badge at the end of the line of the caret: a small button that opens the menu of
//! TypeAlike. The mouse hook takes its clicks, so its window lets clicks through and never takes
//! focus.

use typealike_core::app::BadgeView;
use windows::Win32::Foundation::{POINT, RECT};
use windows::core::{Result, w};

use crate::display::{monitor_dpi, to_pixels};
use crate::gdi::TextBitmap;
use crate::pixels::RoundedRectangle;
use crate::system_theme::{SystemTheme, TooltipColors};
use crate::tray::{RobotColor, robot_icon};
use crate::window::{Window, WindowSpec};

/// The badge is as high as the line of the caret, from this lowest side ...
const MIN_SIDE_DIPS: i32 = 16;
/// ... to this highest side.
const MAX_SIDE_DIPS: i32 = 24;
const MAX_GLYPH_SIDE_DIPS: i32 = 16;
/// The room between each edge of the badge and its glyph, with the border.
const GLYPH_INSET_DIPS: i32 = 2;
const CORNER_RADIUS_DIPS: i32 = 4;
const BORDER_WIDTH_DIPS: i32 = 1;

#[derive(Debug)]
pub(crate) struct Badge {
    window: Window,
    shown_frame: Option<RECT>,
}

impl Badge {
    pub(crate) fn create() -> Result<Self> {
        let window = Window::create(WindowSpec::floating(w!("TypeAlike.Badge")))?;
        Ok(Self {
            window,
            shown_frame: None,
        })
    }

    /// Shows `view` in `frame`, in physical screen pixels: the frame that core gives at the end of
    /// the line of the caret.
    pub(crate) fn show(&mut self, view: &BadgeView, frame: RECT) -> Result<()> {
        let top_left = POINT {
            x: frame.left,
            y: frame.top,
        };
        let layout = BadgeLayout::new(frame, monitor_dpi(top_left));
        let colors = TooltipColors::for_theme(SystemTheme::read());
        let side = layout.frame.right - layout.frame.left;
        let plate = RoundedRectangle {
            width: side,
            height: side,
            corner_radius: layout.corner_radius,
            border_width: layout.border_width,
            fill: colors.fill,
            border: colors.border,
        };
        let robot = robot_icon(view.status, RobotColor::White, layout.glyph_side)?;
        let mut bitmap =
            TextBitmap::render_icon_centered(*robot, layout.glyph_side, colors.text.into(), side)?;
        bitmap.lay_over(&plate.pixels());
        bitmap.present(self.window.handle(), top_left)?;
        self.window.show_on_top()?;
        self.shown_frame = Some(layout.frame);
        Ok(())
    }

    pub(crate) fn hide(&mut self) {
        self.window.hide();
        self.shown_frame = None;
    }

    /// Where the menu of the shown badge starts, in physical screen pixels: below the badge, at its
    /// right edge, so that the menu opens toward the field.
    pub(crate) fn menu_anchor(&self) -> Option<POINT> {
        self.shown_frame.map(menu_anchor_of)
    }

    /// Whether `point`, in physical screen pixels, is on the shown badge.
    pub(crate) fn contains(&self, point: POINT) -> bool {
        self.shown_frame.is_some_and(|frame| frame_contains(frame, point))
    }
}

/// Returns the side of the square badge on a line of the caret of `line_height` pixels, on a
/// monitor of `dpi`: the height of the line, from [`MIN_SIDE_DIPS`] to [`MAX_SIDE_DIPS`].
pub(crate) fn side_on_line(line_height: i32, dpi: i32) -> i32 {
    line_height.clamp(to_pixels(MIN_SIDE_DIPS, dpi), to_pixels(MAX_SIDE_DIPS, dpi))
}

/// Where the badge shows and how large its parts are, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct BadgeLayout {
    frame: RECT,
    glyph_side: i32,
    corner_radius: i32,
    border_width: i32,
}

impl BadgeLayout {
    /// Fits the parts of the badge in `frame`, a square on a monitor of `dpi`. The glyph keeps
    /// [`GLYPH_INSET_DIPS`] inside each edge.
    fn new(frame: RECT, dpi: i32) -> Self {
        let side = frame.right.saturating_sub(frame.left);
        let room_for_glyph = side.saturating_sub(2 * to_pixels(GLYPH_INSET_DIPS, dpi));
        Self {
            frame,
            glyph_side: room_for_glyph.min(to_pixels(MAX_GLYPH_SIDE_DIPS, dpi)),
            corner_radius: to_pixels(CORNER_RADIUS_DIPS, dpi),
            border_width: to_pixels(BORDER_WIDTH_DIPS, dpi),
        }
    }
}

/// The corner of `frame`, the frame of the badge, where its menu starts.
fn menu_anchor_of(frame: RECT) -> POINT {
    POINT {
        x: frame.right,
        y: frame.bottom,
    }
}

fn frame_contains(frame: RECT, point: POINT) -> bool {
    (frame.left..frame.right).contains(&point.x) && (frame.top..frame.bottom).contains(&point.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_is_as_high_as_the_line_of_the_caret_from_16_to_24_dips() {
        let cases = [
            (18, 96),
            (12, 96),
            (30, 96),
            (27, 144),
            (20, 144),
            (40, 144),
            (36, 192),
            (20, 192),
            (60, 192),
        ];

        let sides = cases.map(|(line_height, dpi)| side_on_line(line_height, dpi));

        assert_eq!(sides, [18, 16, 24, 27, 24, 36, 36, 32, 48]);
    }

    fn square(side: i32) -> RECT {
        RECT {
            left: 1000,
            top: 200,
            right: 1000 + side,
            bottom: 200 + side,
        }
    }

    #[test]
    fn glyph_of_16_dips_gets_smaller_in_a_badge_lower_than_20_dips() {
        let cases = [
            (24, 96),
            (18, 96),
            (16, 96),
            (36, 144),
            (27, 144),
            (48, 192),
            (36, 192),
        ];

        let parts = cases.map(|(side, dpi)| {
            let layout = BadgeLayout::new(square(side), dpi);
            (layout.glyph_side, layout.corner_radius, layout.border_width)
        });

        assert_eq!(
            parts,
            [
                (16, 4, 1),
                (14, 4, 1),
                (12, 4, 1),
                (24, 6, 2),
                (21, 6, 2),
                (32, 8, 2),
                (28, 8, 2),
            ]
        );
    }

    #[test]
    fn menu_starts_below_the_badge_at_its_right_edge() {
        let anchor = menu_anchor_of(square(18));

        assert_eq!(anchor, POINT { x: 1018, y: 218 });
    }

    #[test]
    fn only_points_inside_the_frame_are_on_it() {
        let frame = RECT {
            left: 280,
            top: 76,
            right: 300,
            bottom: 96,
        };
        let points = [
            POINT { x: 290, y: 86 },
            POINT { x: 280, y: 76 },
            POINT { x: 279, y: 86 },
            POINT { x: 300, y: 86 },
            POINT { x: 290, y: 96 },
        ];

        let hits = points.map(|point| frame_contains(frame, point));

        assert_eq!(hits, [true, true, false, false, false]);
    }
}
