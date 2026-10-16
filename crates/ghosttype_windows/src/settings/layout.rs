//! Settings window geometry: effective pixels scaled to the window's DPI, the column and row
//! arithmetic that places controls, and the scroll bar of a page.

const DEFAULT_DPI: i64 = 96;

/// Dots per inch of the monitor that shows a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dpi(pub(crate) u32);

impl Dpi {
    /// Converts effective pixels, as in the Fluent type ramp and spacing, to physical pixels.
    pub(crate) fn scale(self, effective: i32) -> i32 {
        let scaled = i64::from(effective) * i64::from(self.0);
        let rounded = (scaled + DEFAULT_DPI / 2).div_euclid(DEFAULT_DPI);
        i32::try_from(rounded).unwrap_or(i32::MAX)
    }
}

/// A point in physical pixels, relative to its window's client area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Point {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

/// A rectangle in physical pixels, relative to its window's client area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Bounds {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

impl Bounds {
    pub(crate) fn bottom(self) -> i32 {
        self.y.saturating_add(self.height)
    }

    pub(crate) fn right(self) -> i32 {
        self.x.saturating_add(self.width)
    }

    /// Whether `point` is inside the rectangle; the right and bottom edges are outside.
    pub(crate) fn contains(self, point: Point) -> bool {
        (self.x..self.right()).contains(&point.x) && (self.y..self.bottom()).contains(&point.y)
    }

    /// Shrinks the rectangle by `horizontal` on the left and right and `vertical` on the top and bottom.
    #[must_use]
    pub(crate) fn inset(self, horizontal: i32, vertical: i32) -> Self {
        Self {
            x: self.x + horizontal,
            y: self.y + vertical,
            width: (self.width - 2 * horizontal).max(0),
            height: (self.height - 2 * vertical).max(0),
        }
    }

    /// Returns a rectangle of `height` centered vertically in this one, as wide as this one.
    #[must_use]
    pub(crate) fn centered_band(self, height: i32) -> Self {
        Self {
            y: self.y + (self.height - height) / 2,
            height,
            ..self
        }
    }

    /// Divides off a part of `width` at the right edge, separated by `gap`: returns (rest, part).
    #[must_use]
    pub(crate) fn split_trailing(self, width: i32, gap: i32) -> (Self, Self) {
        let part_width = width.clamp(0, self.width);
        let part = Self {
            x: self.right() - part_width,
            width: part_width,
            ..self
        };
        let rest = Self {
            width: (self.width - part_width - gap).max(0),
            ..self
        };
        (rest, part)
    }

    /// Moves the rectangle up by `offset`.
    #[must_use]
    pub(crate) fn scrolled(self, offset: i32) -> Self {
        Self {
            y: self.y - offset,
            ..self
        }
    }
}

/// Places blocks top to bottom at one width, like a vertical stack.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Column {
    x: i32,
    width: i32,
    top: i32,
}

impl Column {
    pub(crate) fn new(x: i32, top: i32, width: i32) -> Self {
        Self { x, width, top }
    }

    /// Returns the next block of `height` and moves below it.
    pub(crate) fn take(&mut self, height: i32) -> Bounds {
        let block = Bounds {
            x: self.x,
            y: self.top,
            width: self.width,
            height,
        };
        self.top = block.bottom();
        block
    }

    /// Leaves `height` of empty space.
    pub(crate) fn skip(&mut self, height: i32) {
        self.top = self.top.saturating_add(height);
    }

    /// The top of the next block.
    pub(crate) fn top(self) -> i32 {
        self.top
    }

    /// The width inside a block with `padding` at the left and right, never negative.
    pub(crate) fn inner_width(self, padding: i32) -> i32 {
        (self.width - 2 * padding).max(0)
    }
}

/// Returns the width of the content column: the space between the side `padding`, at most `max_width`.
pub(crate) fn content_width(available: i32, padding: i32, max_width: i32) -> i32 {
    (available - 2 * padding).clamp(0, max_width)
}

/// Keeps a scroll offset between the top and the point where the content's end meets the viewport's end.
pub(crate) fn clamp_scroll(offset: i32, content_height: i32, viewport_height: i32) -> i32 {
    offset.clamp(0, (content_height - viewport_height).max(0))
}

/// Returns the top-left corner that centers a window of `size` in `area`.
pub(crate) fn centered_origin(area: Bounds, width: i32, height: i32) -> (i32, i32) {
    (
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
    )
}

/// A request to scroll a page, from its scroll bar, the mouse wheel or keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScrollMove {
    PageUp,
    PageDown,
    /// To this offset, from a dragged thumb or a control that must show.
    To(i32),
    /// By this many pixels, down when positive.
    By(i32),
}

impl ScrollMove {
    /// The offset this move asks for from `offset` in pages of `page` pixels, before it is
    /// clamped to the content.
    pub(crate) fn target(self, offset: i32, page: i32) -> i32 {
        match self {
            Self::PageUp => offset.saturating_sub(page),
            Self::PageDown => offset.saturating_add(page),
            Self::To(target) => target,
            Self::By(distance) => offset.saturating_add(distance),
        }
    }
}

/// The sizes of a page's scroll bar, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScrollBarSizes {
    /// The width of the band at the right edge that takes the pointer.
    pub(crate) gutter: i32,
    pub(crate) thumb_width: i32,
    /// The space between the thumb and the viewport's right, top and bottom edges.
    pub(crate) edge_gap: i32,
    pub(crate) min_thumb: i32,
}

/// Where a page's scroll bar sits: a thin thumb in a gutter at the right edge, like the WinUI
/// scroll bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScrollBar {
    pub(crate) gutter: Bounds,
    pub(crate) thumb: Bounds,
    travel: i32,
    scrollable: i32,
}

impl ScrollBar {
    /// The scroll bar of `viewport` showing `content_height` from `offset` down; `None` when
    /// the content fits.
    pub(crate) fn place(
        viewport: Bounds,
        content_height: i32,
        offset: i32,
        sizes: ScrollBarSizes,
    ) -> Option<Self> {
        let scrollable = content_height - viewport.height;
        if scrollable <= 0 || viewport.height <= 0 {
            return None;
        }
        let track_top = viewport.y + sizes.edge_gap;
        let track_height = (viewport.height - 2 * sizes.edge_gap).max(1);
        let share = i64::from(track_height) * i64::from(viewport.height) / i64::from(content_height);
        let thumb_height = i32::try_from(share)
            .unwrap_or(track_height)
            .max(sizes.min_thumb)
            .min(track_height);
        let travel = track_height - thumb_height;
        let moved = i64::from(travel) * i64::from(offset.clamp(0, scrollable)) / i64::from(scrollable);
        Some(Self {
            gutter: Bounds {
                x: viewport.right() - sizes.gutter,
                width: sizes.gutter,
                ..viewport
            },
            thumb: Bounds {
                x: viewport.right() - sizes.edge_gap - sizes.thumb_width,
                y: track_top + i32::try_from(moved).unwrap_or(travel),
                width: sizes.thumb_width,
                height: thumb_height,
            },
            travel,
            scrollable,
        })
    }

    /// The part of the gutter beside the thumb, where a press starts a drag.
    pub(crate) fn thumb_band(self) -> Bounds {
        Bounds {
            x: self.gutter.x,
            width: self.gutter.width,
            ..self.thumb
        }
    }

    /// The offset after the thumb moves `distance` pixels down from where it was at `offset`.
    pub(crate) fn offset_after_drag(self, offset: i32, distance: i32) -> i32 {
        if self.travel <= 0 {
            return offset;
        }
        let moved = i64::from(distance) * i64::from(self.scrollable) / i64::from(self.travel);
        offset.saturating_add(i32::try_from(moved).unwrap_or(if moved < 0 { i32::MIN } else { i32::MAX }))
    }
}

/// The offset that shows all of `target` in a viewport of `viewport_height` that starts at `offset`.
pub(crate) fn reveal(offset: i32, target: Bounds, viewport_height: i32) -> i32 {
    let content_top = target.y + offset;
    let content_bottom = target.bottom() + offset;
    if content_top < offset {
        content_top
    } else if content_bottom > offset + viewport_height {
        content_bottom - viewport_height
    } else {
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROW: Bounds = Bounds {
        x: 10,
        y: 20,
        width: 300,
        height: 60,
    };

    #[test]
    fn default_dpi_keeps_effective_pixels() {
        assert_eq!(Dpi(96).scale(14), 14);
    }

    #[test]
    fn higher_dpi_scales_and_rounds() {
        assert_eq!((Dpi(144).scale(14), Dpi(120).scale(13)), (21, 16));
    }

    #[test]
    fn huge_value_saturates() {
        assert_eq!(Dpi(960).scale(i32::MAX), i32::MAX);
    }

    #[test]
    fn column_stacks_blocks_downward() {
        let mut column = Column::new(8, 4, 100);

        let first = column.take(20);
        column.skip(6);
        let second = column.take(10);

        assert_eq!(
            (first, second, column.top()),
            (
                Bounds {
                    x: 8,
                    y: 4,
                    width: 100,
                    height: 20
                },
                Bounds {
                    x: 8,
                    y: 30,
                    width: 100,
                    height: 10
                },
                40
            )
        );
    }

    #[test]
    fn inner_width_removes_padding_on_both_sides() {
        assert_eq!(Column::new(8, 4, 100).inner_width(16), 68);
    }

    #[test]
    fn inner_width_of_narrow_column_is_zero() {
        assert_eq!(Column::new(8, 4, 0).inner_width(16), 0);
    }

    #[test]
    fn split_trailing_gives_right_part_and_rest() {
        let (rest, part) = ROW.split_trailing(100, 12);

        assert_eq!(
            (rest, part),
            (
                Bounds { width: 188, ..ROW },
                Bounds {
                    x: 210,
                    width: 100,
                    ..ROW
                }
            )
        );
    }

    #[test]
    fn split_trailing_wider_than_row_takes_all() {
        let (rest, part) = ROW.split_trailing(500, 12);

        assert_eq!((rest.width, part), (0, ROW));
    }

    #[test]
    fn inset_shrinks_both_sides() {
        assert_eq!(
            ROW.inset(16, 10),
            Bounds {
                x: 26,
                y: 30,
                width: 268,
                height: 40
            }
        );
    }

    #[test]
    fn centered_band_sits_in_the_middle() {
        assert_eq!(
            ROW.centered_band(20),
            Bounds {
                y: 40,
                height: 20,
                ..ROW
            }
        );
    }

    #[test]
    fn scrolled_moves_up() {
        assert_eq!(ROW.scrolled(15).y, 5);
    }

    #[test]
    fn content_width_stops_at_maximum() {
        assert_eq!(
            (content_width(2000, 32, 1000), content_width(500, 32, 1000)),
            (1000, 436)
        );
    }

    #[test]
    fn content_width_never_negative() {
        assert_eq!(content_width(40, 32, 1000), 0);
    }

    #[test]
    fn scroll_stops_at_content_end() {
        assert_eq!(clamp_scroll(900, 1000, 400), 600);
    }

    #[test]
    fn scroll_never_above_top() {
        assert_eq!(clamp_scroll(-50, 1000, 400), 0);
    }

    #[test]
    fn short_content_does_not_scroll() {
        assert_eq!(clamp_scroll(100, 300, 400), 0);
    }

    #[test]
    fn window_centers_in_area() {
        let area = Bounds {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1040,
        };

        assert_eq!(centered_origin(area, 960, 720), (-1440, 160));
    }

    #[test]
    fn scroll_moves_reach_their_targets() {
        let cases = [
            (ScrollMove::PageUp, -300),
            (ScrollMove::PageDown, 500),
            (ScrollMove::To(250), 250),
            (ScrollMove::By(-30), 70),
        ];

        let targets = cases.map(|(scroll, _)| scroll.target(100, 400));

        assert_eq!(targets, cases.map(|(_, target)| target));
    }

    #[test]
    fn contains_takes_left_and_top_edges_only() {
        let cases = [
            (Point { x: 10, y: 20 }, true),
            (Point { x: 309, y: 79 }, true),
            (Point { x: 310, y: 40 }, false),
            (Point { x: 40, y: 80 }, false),
        ];

        let inside = cases.map(|(point, _)| ROW.contains(point));

        assert_eq!(inside, cases.map(|(_, expected)| expected));
    }

    const VIEWPORT: Bounds = Bounds {
        x: 0,
        y: 0,
        width: 600,
        height: 400,
    };

    const SIZES: ScrollBarSizes = ScrollBarSizes {
        gutter: 12,
        thumb_width: 2,
        edge_gap: 3,
        min_thumb: 24,
    };

    #[test]
    fn scroll_bar_hides_when_content_fits() {
        assert_eq!(ScrollBar::place(VIEWPORT, 400, 0, SIZES), None);
    }

    #[test]
    fn thumb_shows_the_viewport_share_of_the_track() {
        let bar = ScrollBar::place(VIEWPORT, 800, 0, SIZES).unwrap();

        assert_eq!(
            (bar.gutter, bar.thumb),
            (
                Bounds {
                    x: 588,
                    width: 12,
                    ..VIEWPORT
                },
                Bounds {
                    x: 595,
                    y: 3,
                    width: 2,
                    height: 197
                }
            )
        );
    }

    #[test]
    fn thumb_reaches_the_track_end_at_the_last_offset() {
        let bar = ScrollBar::place(VIEWPORT, 800, 400, SIZES).unwrap();

        assert_eq!(bar.thumb.bottom(), 397);
    }

    #[test]
    fn thumb_never_gets_shorter_than_the_minimum() {
        let bar = ScrollBar::place(VIEWPORT, 100_000, 0, SIZES).unwrap();

        assert_eq!(bar.thumb.height, 24);
    }

    #[test]
    fn dragging_the_thumb_over_its_travel_scrolls_the_whole_content() {
        let bar = ScrollBar::place(VIEWPORT, 800, 0, SIZES).unwrap();

        assert_eq!(bar.offset_after_drag(0, 197), 400);
    }

    #[test]
    fn reveal_scrolls_down_to_control_below_viewport() {
        let control = Bounds {
            x: 0,
            y: 450,
            width: 10,
            height: 30,
        };

        assert_eq!(reveal(100, control, 400), 180);
    }

    #[test]
    fn reveal_scrolls_up_to_control_above_viewport() {
        let control = Bounds {
            x: 0,
            y: -20,
            width: 10,
            height: 30,
        };

        assert_eq!(reveal(100, control, 400), 80);
    }

    #[test]
    fn reveal_keeps_offset_for_visible_control() {
        let control = Bounds {
            x: 0,
            y: 50,
            width: 10,
            height: 30,
        };

        assert_eq!(reveal(100, control, 400), 100);
    }
}
