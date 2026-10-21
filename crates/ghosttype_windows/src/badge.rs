//! The badge above the caret: a small button that opens the menu of GhostType. The mouse hook
//! takes its clicks, so its window lets clicks through and never takes focus.

use ghosttype_core::app::BadgeView;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor};
use windows::core::{Result, w};

use crate::caret::Caret;
use crate::display::{monitor_dpi, to_pixels};
use crate::gdi::glyph_bitmap;
use crate::pixels::{Rgb, RoundedRectangle};
use crate::system_theme::{Contrast, SystemTheme, ThemeMode};
use crate::tray::status_glyph;
use crate::window::{Window, WindowSpec};

const BADGE_SIDE_DIPS: i32 = 24;
const GLYPH_SIDE_DIPS: i32 = 14;
const CORNER_RADIUS_DIPS: i32 = 4;
const BORDER_WIDTH_DIPS: i32 = 1;
const GAP_ABOVE_CARET_DIPS: i32 = 4;

/// The light and dark colors follow the Fluent tooltip: its fill, then the surface stroke and the
/// secondary text, each blended over the fill.
const LIGHT_COLORS: TooltipColors = TooltipColors {
    fill: Rgb::gray(0xF9),
    border: Rgb::gray(0xC4),
    text: Rgb::gray(0x5F),
};
const DARK_COLORS: TooltipColors = TooltipColors {
    fill: Rgb::gray(0x2C),
    border: Rgb::gray(0x49),
    text: Rgb::gray(0xCF),
};

#[derive(Debug)]
pub(crate) struct Badge {
    window: Window,
    shown_frame: Option<RECT>,
}

impl Badge {
    pub(crate) fn create() -> Result<Self> {
        let window = Window::create(WindowSpec::floating(w!("GhostType.Badge")))?;
        Ok(Self {
            window,
            shown_frame: None,
        })
    }

    /// Shows `view` above `caret`, on the left of it, where the ghost text never goes.
    pub(crate) fn show(&mut self, view: &BadgeView, caret: &Caret) -> Result<()> {
        let layout = BadgeLayout::new(caret.origin, monitor_dpi(caret.origin));
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
        let mut bitmap = glyph_bitmap(status_glyph(view.status), layout.glyph_side, side, colors.text)?;
        bitmap.lay_over(&plate.pixels());
        let top_left = POINT {
            x: layout.frame.left,
            y: layout.frame.top,
        };
        bitmap.present(self.window.handle(), top_left)?;
        self.window.show_on_top()?;
        self.shown_frame = Some(layout.frame);
        Ok(())
    }

    pub(crate) fn hide(&mut self) {
        self.window.hide();
        self.shown_frame = None;
    }

    /// The bottom-left corner of the shown badge in physical screen pixels, where its menu starts.
    pub(crate) fn menu_anchor(&self) -> Option<POINT> {
        self.shown_frame.map(|frame| POINT {
            x: frame.left,
            y: frame.bottom,
        })
    }

    /// Whether `point`, in physical screen pixels, is on the shown badge.
    pub(crate) fn contains(&self, point: POINT) -> bool {
        self.shown_frame.is_some_and(|frame| frame_contains(frame, point))
    }
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
    /// Puts the badge above `caret` so that it ends where the caret starts, on a monitor of `dpi`.
    fn new(caret: POINT, dpi: i32) -> Self {
        let side = to_pixels(BADGE_SIDE_DIPS, dpi);
        let bottom = caret.y - to_pixels(GAP_ABOVE_CARET_DIPS, dpi);
        Self {
            frame: RECT {
                left: caret.x - side,
                top: bottom - side,
                right: caret.x,
                bottom,
            },
            glyph_side: to_pixels(GLYPH_SIDE_DIPS, dpi),
            corner_radius: to_pixels(CORNER_RADIUS_DIPS, dpi),
            border_width: to_pixels(BORDER_WIDTH_DIPS, dpi),
        }
    }
}

/// The colors of the badge and of the mirror below the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TooltipColors {
    pub(crate) fill: Rgb,
    pub(crate) border: Rgb,
    pub(crate) text: Rgb,
}

impl TooltipColors {
    pub(crate) fn for_theme(theme: SystemTheme) -> Self {
        match (theme.contrast, theme.mode) {
            (Contrast::High, _) => Self::high_contrast(),
            (Contrast::Standard, ThemeMode::Light) => LIGHT_COLORS,
            (Contrast::Standard, ThemeMode::Dark) => DARK_COLORS,
        }
    }

    fn high_contrast() -> Self {
        // SAFETY: plain calls.
        let (window, text) = unsafe { (GetSysColor(COLOR_WINDOW), GetSysColor(COLOR_WINDOWTEXT)) };
        Self {
            fill: Rgb::from_colorref(window),
            border: Rgb::from_colorref(text),
            text: Rgb::from_colorref(text),
        }
    }
}

fn frame_contains(frame: RECT, point: POINT) -> bool {
    (frame.left..frame.right).contains(&point.x) && (frame.top..frame.bottom).contains(&point.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARET: POINT = POINT { x: 300, y: 100 };

    #[test]
    fn badge_ends_above_the_caret_on_its_left() {
        let layout = BadgeLayout::new(CARET, 96);

        assert_eq!(
            layout,
            BadgeLayout {
                frame: RECT {
                    left: 276,
                    top: 72,
                    right: 300,
                    bottom: 96,
                },
                glyph_side: 14,
                corner_radius: 4,
                border_width: 1,
            }
        );
    }

    #[test]
    fn badge_scales_with_the_monitor() {
        let layout = BadgeLayout::new(CARET, 144);

        assert_eq!(
            layout,
            BadgeLayout {
                frame: RECT {
                    left: 264,
                    top: 58,
                    right: 300,
                    bottom: 94,
                },
                glyph_side: 21,
                corner_radius: 6,
                border_width: 2,
            }
        );
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
