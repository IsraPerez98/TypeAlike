//! The badge above the caret: a small button that pauses or resumes suggestions. The event tap
//! takes its clicks, so its panel lets clicks through and never takes focus.

use ghosttype_core::app::BadgeView;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSBox, NSBoxType, NSColor, NSImageView, NSPanel, NSTitlePosition};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::focus::Caret;
use crate::{menu_bar, overlay};

const BADGE_SIZE: CGFloat = 20.0;
const BADGE_PADDING: CGFloat = 3.0;
const BADGE_CORNER_RADIUS: CGFloat = 6.0;
const BADGE_BORDER_WIDTH: CGFloat = 1.0;
const GAP_ABOVE_CARET: CGFloat = 2.0;

#[derive(Debug)]
pub(crate) struct Badge {
    panel: Retained<NSPanel>,
    glyph: Retained<NSImageView>,
    shown_frame: Option<CGRect>,
    main_thread: MainThreadMarker,
}

impl Badge {
    pub(crate) fn new(main_thread: MainThreadMarker) -> Self {
        let glyph = NSImageView::new(main_thread);
        glyph.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
        let background = NSBox::new(main_thread);
        background.setBoxType(NSBoxType::Custom);
        background.setTitlePosition(NSTitlePosition::NoTitle);
        background.setCornerRadius(BADGE_CORNER_RADIUS);
        background.setBorderWidth(BADGE_BORDER_WIDTH);
        background.setBorderColor(&NSColor::separatorColor());
        background.setFillColor(&NSColor::windowBackgroundColor());
        background.setContentViewMargins(NSSize::new(BADGE_PADDING, BADGE_PADDING));
        background.setContentView(Some(&glyph));
        let panel = overlay::floating_panel(main_thread);
        panel.setContentView(Some(&background));
        Self {
            panel,
            glyph,
            shown_frame: None,
            main_thread,
        }
    }

    /// Shows `view` above `caret`, on the left of it, where the ghost text never goes.
    pub(crate) fn show(&mut self, view: &BadgeView, caret: Caret) {
        let Some(primary_screen_height) = overlay::primary_screen_height(self.main_thread) else {
            self.hide();
            return;
        };
        let image = menu_bar::symbol_image(view.status, view.label);
        if image.is_none() {
            tracing::warn!(status = ?view.status, "badge symbol is unavailable");
        }
        self.glyph.setImage(image.as_deref());
        let frame = badge_frame(caret);
        let origin = appkit_origin(frame, primary_screen_height);
        self.panel.setFrame_display(NSRect::new(origin, frame.size), true);
        self.panel.orderFrontRegardless();
        self.shown_frame = Some(frame);
    }

    pub(crate) fn hide(&mut self) {
        self.panel.orderOut(None);
        self.shown_frame = None;
    }

    /// Whether `point`, in global points with a top-left origin, is on the shown badge.
    pub(crate) fn contains(&self, point: CGPoint) -> bool {
        self.shown_frame.is_some_and(|frame| frame_contains(frame, point))
    }
}

/// The frame of the badge that ends above `caret` where the caret starts, in global points with a
/// top-left origin.
fn badge_frame(caret: Caret) -> CGRect {
    CGRect::new(
        CGPoint::new(caret.left - BADGE_SIZE, caret.top - GAP_ABOVE_CARET - BADGE_SIZE),
        CGSize::new(BADGE_SIZE, BADGE_SIZE),
    )
}

/// The origin of `frame` in AppKit's bottom-left screen coordinates.
pub(crate) fn appkit_origin(frame: CGRect, primary_screen_height: CGFloat) -> NSPoint {
    NSPoint::new(
        frame.origin.x,
        primary_screen_height - frame.origin.y - frame.size.height,
    )
}

fn frame_contains(frame: CGRect, point: CGPoint) -> bool {
    let horizontal = frame.origin.x..frame.origin.x + frame.size.width;
    let vertical = frame.origin.y..frame.origin.y + frame.size.height;
    horizontal.contains(&point.x) && vertical.contains(&point.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARET: Caret = Caret {
        left: 300.0,
        top: 100.0,
        height: 20.0,
    };

    #[test]
    fn badge_ends_above_the_caret_on_its_left() {
        let frame = badge_frame(CARET);

        assert_eq!(
            frame,
            CGRect::new(CGPoint::new(280.0, 78.0), CGSize::new(20.0, 20.0))
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

        let hits = points.map(|point| frame_contains(frame, point));

        assert_eq!(hits, [true, true, false, false, false]);
    }
}
