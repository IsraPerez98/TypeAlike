//! The pure geometry of the caret and of the screen coordinates.
//!
//! It flips frames between the top-left global coordinates and the bottom-left ones of AppKit,
//! and turns frames into core screen rectangles.

use ghosttype_core::desktop::ScreenRect;
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::NSPoint;

/// Where ghost text starts: just right of the caret, in global points with a top-left origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Caret {
    pub(crate) left: CGFloat,
    pub(crate) top: CGFloat,
    pub(crate) height: CGFloat,
}

impl Caret {
    /// Builds a caret from the bounds of the text that ends at it; `None` when it has no height.
    pub(crate) fn from_bounds(bounds: CGRect) -> Option<Self> {
        (bounds.size.height > 0.0).then_some(Self {
            left: bounds.origin.x + bounds.size.width,
            top: bounds.origin.y,
            height: bounds.size.height,
        })
    }

    /// A caret at the top-left corner of `field`, as high as the field, for a field that gives no
    /// caret; `None` when the field has no height.
    pub(crate) fn at_start_of(field: CGRect) -> Option<Self> {
        Self::from_bounds(CGRect::new(field.origin, CGSize::new(0.0, field.size.height)))
    }
}

/// The origin of `frame` in AppKit's bottom-left screen coordinates.
pub(crate) fn appkit_origin(frame: CGRect, primary_screen_height: CGFloat) -> NSPoint {
    NSPoint::new(
        frame.origin.x,
        primary_screen_height - frame.origin.y - frame.size.height,
    )
}

/// Turns `frame`, in AppKit's bottom-left screen coordinates, into a frame with a top-left origin.
pub(crate) fn top_left_frame(frame: CGRect, primary_screen_height: CGFloat) -> CGRect {
    let top = primary_screen_height - frame.origin.y - frame.size.height;
    CGRect::new(CGPoint::new(frame.origin.x, top), frame.size)
}

/// Whether `frame` holds `point`. The left and top edges are in the frame, and the right and bottom
/// edges are not.
pub(crate) fn contains(frame: CGRect, point: CGPoint) -> bool {
    let horizontal = frame.origin.x..frame.origin.x + frame.size.width;
    let vertical = frame.origin.y..frame.origin.y + frame.size.height;
    horizontal.contains(&point.x) && vertical.contains(&point.y)
}

pub(crate) fn moved_by(frame: CGRect, origin: CGPoint) -> CGRect {
    CGRect::new(
        CGPoint::new(frame.origin.x + origin.x, frame.origin.y + origin.y),
        frame.size,
    )
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "no conversion from f64 to i32 exists without `as`, and `as` saturates"
)]
pub(crate) fn screen_rect(frame: CGRect) -> ScreenRect {
    ScreenRect {
        left: frame.origin.x.round() as i32,
        top: frame.origin.y.round() as i32,
        right: (frame.origin.x + frame.size.width).round() as i32,
        bottom: (frame.origin.y + frame.size.height).round() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: f64, y: f64, width: f64, height: f64) -> CGRect {
        CGRect::new(CGPoint::new(x, y), CGSize::new(width, height))
    }

    #[test]
    fn caret_starts_at_right_edge_of_bounds() {
        let bounds = CGRect::new(CGPoint::new(100.0, 40.0), CGSize::new(7.0, 18.0));

        assert_eq!(
            Caret::from_bounds(bounds),
            Some(Caret {
                left: 107.0,
                top: 40.0,
                height: 18.0
            })
        );
    }

    #[test]
    fn bounds_without_height_have_no_caret() {
        let bounds = CGRect::new(CGPoint::new(100.0, 40.0), CGSize::new(0.0, 0.0));

        assert_eq!(Caret::from_bounds(bounds), None);
    }

    #[test]
    fn caret_at_the_start_of_a_field_is_at_its_left_edge_and_as_high_as_the_field() {
        let field = CGRect::new(CGPoint::new(511.0, 101.0), CGSize::new(860.0, 18.0));

        assert_eq!(
            Caret::at_start_of(field),
            Some(Caret {
                left: 511.0,
                top: 101.0,
                height: 18.0
            })
        );
    }

    #[test]
    fn badge_origin_is_flipped_to_bottom_left_coordinates() {
        let frame = CGRect::new(CGPoint::new(280.0, 78.0), CGSize::new(20.0, 20.0));

        let origin = appkit_origin(frame, 1000.0);

        assert_eq!(origin, NSPoint::new(280.0, 902.0));
    }

    #[test]
    fn only_points_inside_the_frame_are_on_it() {
        let frame = CGRect::new(CGPoint::new(280.0, 78.0), CGSize::new(20.0, 20.0));
        let points = [
            CGPoint::new(290.0, 88.0),
            CGPoint::new(280.0, 78.0),
            CGPoint::new(279.5, 88.0),
            CGPoint::new(300.0, 88.0),
            CGPoint::new(290.0, 98.0),
        ];

        let hits = points.map(|point| contains(frame, point));

        assert_eq!(hits, [true, true, false, false, false]);
    }

    #[test]
    fn screen_frame_is_flipped_to_a_top_left_origin() {
        let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1440.0, 875.0));

        let flipped = top_left_frame(frame, 900.0);

        assert_eq!(
            flipped,
            CGRect::new(CGPoint::new(0.0, 25.0), CGSize::new(1440.0, 875.0))
        );
    }

    #[test]
    fn frame_rounds_to_whole_points() {
        let rect = screen_rect(frame(10.4, 20.6, 100.2, 18.0));

        assert_eq!(
            rect,
            ScreenRect {
                left: 10,
                top: 21,
                right: 111,
                bottom: 39,
            }
        );
    }
}
