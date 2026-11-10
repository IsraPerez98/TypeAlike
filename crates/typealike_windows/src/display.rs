//! The monitors and the physical pixels of the screen.
//!
//! It gives the DPI, the work area and the whole area of a monitor, the scale from DIPs to pixels,
//! and the screen rectangles of core.

use typealike_core::desktop::ScreenRect;
use windows::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, MonitorFromWindow,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::core::Result;

use crate::struct_size;

/// The pixels in one inch at a scale of 100 %.
pub(crate) const DEFAULT_DPI: i32 = 96;

/// Reads the DPI of the monitor that shows `point`, or the default DPI when the monitor does not tell.
pub(crate) fn monitor_dpi(point: POINT) -> i32 {
    // SAFETY: the call takes no pointer, and it gives the nearest monitor for any point.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut horizontal = 0;
    let mut vertical = 0;
    // SAFETY: the outputs are locals.
    let read =
        unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut horizontal, &raw mut vertical) };
    read.ok()
        .and_then(|()| i32::try_from(horizontal).ok())
        .unwrap_or(DEFAULT_DPI)
}

/// Scales `dips` to physical pixels on a monitor of `dpi`.
pub(crate) fn to_pixels(dips: i32, dpi: i32) -> i32 {
    (dips * dpi + DEFAULT_DPI / 2) / DEFAULT_DPI
}

/// Reads the work area of the monitor that shows `point`: the monitor without the taskbar.
pub(crate) fn work_area(point: POINT) -> Result<RECT> {
    // SAFETY: the call takes no pointer, and it gives the nearest monitor for any point.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    Ok(monitor_areas(monitor)?.rcWork)
}

/// Reads the whole area of the monitor that shows the most of `window`, with the taskbar.
pub(crate) fn monitor_area(window: HWND) -> Result<RECT> {
    // SAFETY: the call takes no pointer, and it gives the nearest monitor for any window.
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    Ok(monitor_areas(monitor)?.rcMonitor)
}

fn monitor_areas(monitor: HMONITOR) -> Result<MONITORINFO> {
    let mut areas = MONITORINFO {
        cbSize: struct_size::<MONITORINFO>(),
        ..MONITORINFO::default()
    };
    // SAFETY: `areas` is a local with its size set, and an invalid monitor only makes the call
    // fail.
    unsafe { GetMonitorInfoW(monitor, &raw mut areas) }.ok()?;
    Ok(areas)
}

/// The rectangle of the screen that `rect`, in physical screen pixels, covers.
pub(crate) fn screen_rect(rect: RECT) -> ScreenRect {
    ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

/// Whether `rect` has a width and a height.
pub(crate) fn has_area(rect: RECT) -> bool {
    rect.right > rect.left && rect.bottom > rect.top
}

/// The rectangle between the corners `corner` and `opposite`, in any order. A mirrored window
/// (right to left) puts the left edge of its client area at the right on the screen.
pub(crate) fn rect_between(corner: POINT, opposite: POINT) -> RECT {
    RECT {
        left: corner.x.min(opposite.x),
        top: corner.y.min(opposite.y),
        right: corner.x.max(opposite.x),
        bottom: corner.y.max(opposite.y),
    }
}

/// The part of `frame`, the frame of the focused field, that `client_area`, the client area of the
/// focused window, shows. A UIA frame covers the whole field, also the part that the edge of the
/// window or a scrolled page hides. When the client area shows no part of the frame, as for a
/// hidden focus window without a size, the whole frame. Without a frame, the client area.
pub(crate) fn visible_field_frame(frame: Option<RECT>, client_area: Option<RECT>) -> Option<RECT> {
    let Some(frame) = frame else {
        return client_area;
    };
    let visible = client_area
        .map(|client_area| intersection(frame, client_area))
        .filter(|visible| has_area(*visible));
    Some(visible.unwrap_or(frame))
}

/// An edge of a rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge {
    Left,
    Top,
    Right,
    Bottom,
}

/// The rectangle of `size` with its top left corner at `origin`.
pub(crate) fn rect_at(origin: POINT, size: SIZE) -> RECT {
    RECT {
        left: origin.x,
        top: origin.y,
        right: origin.x.saturating_add(size.cx),
        bottom: origin.y.saturating_add(size.cy),
    }
}

/// The edges of `outer` that `inner` crosses: a part of `inner` is past each of them.
pub(crate) fn crossed_edges(inner: RECT, outer: RECT) -> Vec<Edge> {
    [
        (inner.left < outer.left, Edge::Left),
        (inner.top < outer.top, Edge::Top),
        (inner.right > outer.right, Edge::Right),
        (inner.bottom > outer.bottom, Edge::Bottom),
    ]
    .into_iter()
    .filter_map(|(is_crossed, edge)| is_crossed.then_some(edge))
    .collect()
}

/// The rectangle that both `first` and `second` cover. It has no area when they do not overlap.
fn intersection(first: RECT, second: RECT) -> RECT {
    RECT {
        left: first.left.max(second.left),
        top: first.top.max(second.top),
        right: first.right.min(second.right),
        bottom: first.bottom.min(second.bottom),
    }
}

/// Rounds `coordinate`, in physical pixels, to the nearest whole pixel.
#[expect(
    clippy::cast_possible_truncation,
    reason = "no conversion from f64 to i32 exists without `as`, and `as` saturates"
)]
pub(crate) fn round_to_pixel(coordinate: f64) -> i32 {
    coordinate.round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_with_a_width_and_a_height_has_an_area() {
        let cases = [
            (
                RECT {
                    left: 10,
                    top: 20,
                    right: 30,
                    bottom: 40,
                },
                true,
            ),
            (
                RECT {
                    left: 10,
                    top: 20,
                    right: 10,
                    bottom: 40,
                },
                false,
            ),
            (
                RECT {
                    left: 10,
                    top: 20,
                    right: 30,
                    bottom: 20,
                },
                false,
            ),
            (
                RECT {
                    left: 30,
                    top: 40,
                    right: 10,
                    bottom: 20,
                },
                false,
            ),
        ];

        let areas = cases.map(|(rect, _)| has_area(rect));

        assert_eq!(areas, cases.map(|(_, expected)| expected));
    }

    const fn rect(left: i32, top: i32, right: i32, bottom: i32) -> RECT {
        RECT {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn rect_between_corners_has_its_left_edge_at_the_left_also_in_a_mirrored_window() {
        let corners = [
            (POINT { x: 10, y: 20 }, POINT { x: 300, y: 200 }),
            (POINT { x: 300, y: 20 }, POINT { x: 10, y: 200 }),
        ];

        let rects = corners.map(|(corner, opposite)| rect_between(corner, opposite));

        assert_eq!(rects, [rect(10, 20, 300, 200), rect(10, 20, 300, 200)]);
    }

    /// The client area of a window from 100 to 900 across and from 50 to 650 down.
    const CLIENT_AREA: RECT = rect(100, 50, 900, 650);

    #[test]
    fn visible_field_frame_is_the_part_of_the_frame_that_the_client_area_shows() {
        let frames = [
            rect(200, 100, 600, 300),
            rect(200, 500, 1000, 1400),
            rect(0, -300, 600, 300),
        ];

        let visible = frames.map(|frame| visible_field_frame(Some(frame), Some(CLIENT_AREA)));

        assert_eq!(
            visible,
            [
                Some(rect(200, 100, 600, 300)),
                Some(rect(200, 500, 900, 650)),
                Some(rect(100, 50, 600, 300)),
            ]
        );
    }

    #[test]
    fn visible_field_frame_is_the_whole_frame_when_the_client_area_shows_no_part_of_it() {
        let frame = rect(200, 100, 600, 300);
        let client_areas = [
            Some(rect(0, 0, 0, 0)),
            Some(rect(700, 400, 900, 650)),
            Some(rect(200, 300, 600, 400)),
            None,
        ];

        let visible = client_areas.map(|client_area| visible_field_frame(Some(frame), client_area));

        assert_eq!(visible, [Some(frame); 4]);
    }

    #[test]
    fn rect_at_an_origin_has_the_size_of_the_bitmap() {
        let origin = POINT { x: 100, y: 40 };

        let covered = rect_at(origin, SIZE { cx: 300, cy: 60 });

        assert_eq!(covered, rect(100, 40, 400, 100));
    }

    /// A window from 100 to 900 across and from 50 to 650 down.
    const WINDOW: RECT = rect(100, 50, 900, 650);

    #[test]
    fn crossed_edges_are_the_edges_of_the_window_that_the_ghost_text_goes_past() {
        let ghost_texts = [
            rect(200, 100, 600, 300),
            rect(100, 50, 900, 650),
            rect(700, 600, 1000, 700),
            rect(0, 0, 1000, 700),
        ];

        let edges = ghost_texts.map(|ghost_text| crossed_edges(ghost_text, WINDOW));

        assert_eq!(
            edges,
            [
                vec![],
                vec![],
                vec![Edge::Right, Edge::Bottom],
                vec![Edge::Left, Edge::Top, Edge::Right, Edge::Bottom],
            ]
        );
    }

    #[test]
    fn visible_field_frame_without_a_frame_is_the_client_area() {
        let client_areas = [Some(CLIENT_AREA), None];

        let visible = client_areas.map(|client_area| visible_field_frame(None, client_area));

        assert_eq!(visible, [Some(CLIENT_AREA), None]);
    }
}
