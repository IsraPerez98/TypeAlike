//! The mirror: a small panel next to a focused field whose caret Accessibility does not give, as
//! in the editor of VS Code. It shows the end of the line before the caret and the ghost text.

use ghosttype_core::controller::{Ghost, GhostView};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSBox, NSBoxType, NSColor, NSFont, NSPanel, NSScreen, NSTextField, NSTitlePosition};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::{NSMutableAttributedString, NSPoint, NSRect, NSSize, NSString};

use crate::{badge, overlay};

const MIRROR_PADDING: CGFloat = 6.0;
const MIRROR_CORNER_RADIUS: CGFloat = 6.0;
const MIRROR_BORDER_WIDTH: CGFloat = 1.0;
const MAX_TEXT_WIDTH: CGFloat = 480.0;
const GAP_TO_FIELD: CGFloat = 4.0;

#[derive(Debug)]
pub(crate) struct Mirror {
    panel: Retained<NSPanel>,
    label: Retained<NSTextField>,
    main_thread: MainThreadMarker,
}

impl Mirror {
    pub(crate) fn new(main_thread: MainThreadMarker) -> Self {
        let label = NSTextField::wrappingLabelWithString(&NSString::new(), main_thread);
        label.setPreferredMaxLayoutWidth(MAX_TEXT_WIDTH);
        let background = NSBox::new(main_thread);
        background.setBoxType(NSBoxType::Custom);
        background.setTitlePosition(NSTitlePosition::NoTitle);
        background.setCornerRadius(MIRROR_CORNER_RADIUS);
        background.setBorderWidth(MIRROR_BORDER_WIDTH);
        background.setBorderColor(&NSColor::separatorColor());
        background.setFillColor(&NSColor::windowBackgroundColor());
        background.setContentViewMargins(NSSize::new(MIRROR_PADDING, MIRROR_PADDING));
        background.setContentView(Some(&label));
        let panel = overlay::floating_panel(main_thread);
        panel.setContentView(Some(&background));
        Self {
            panel,
            label,
            main_thread,
        }
    }

    /// Shows `view` below `field`, the frame of the focused field in global points with a top-left
    /// origin, or above it when it does not fit below.
    pub(crate) fn show(&self, view: &GhostView, field: CGRect) {
        let Some(primary_screen_height) = overlay::primary_screen_height(self.main_thread) else {
            self.hide();
            return;
        };
        let Some(screen) = visible_frame_at(field.origin, primary_screen_height, self.main_thread) else {
            self.hide();
            return;
        };
        let font = NSFont::systemFontOfSize(NSFont::systemFontSize());
        self.label.setAttributedStringValue(&mirror_text(view, &font));
        let text_size = self.text_size();
        self.label.setFrameSize(text_size);
        let inset = 2.0 * (MIRROR_PADDING + MIRROR_BORDER_WIDTH);
        let size = CGSize::new(text_size.width + inset, text_size.height + inset);
        let frame = mirror_frame(field, size, screen);
        let origin = badge::appkit_origin(frame, primary_screen_height);
        self.panel.setFrame_display(NSRect::new(origin, size), true);
        self.panel.orderFrontRegardless();
    }

    pub(crate) fn hide(&self) {
        self.panel.orderOut(None);
    }

    pub(crate) fn is_shown(&self) -> bool {
        self.panel.isVisible()
    }

    /// The size of the text of the label, wrapped at the widest text of the mirror.
    fn text_size(&self) -> NSSize {
        let bounds = NSRect::new(NSPoint::ZERO, NSSize::new(MAX_TEXT_WIDTH, CGFloat::MAX));
        self.label
            .cell()
            .map_or(NSSize::ZERO, |cell| cell.cellSizeForBounds(bounds))
    }
}

/// The line before the caret in the color of text, then the ghost text as the overlay shows it,
/// with the typo of a fix in the color of text.
fn mirror_text(view: &GhostView, font: &NSFont) -> Retained<NSMutableAttributedString> {
    let text = NSMutableAttributedString::new();
    let append = |part: &str, color: &NSColor, strike_color: Option<&NSColor>| {
        text.appendAttributedString(&overlay::attributed(part, font, color, strike_color));
    };
    append(&view.line_before_caret, &NSColor::labelColor(), None);
    match &view.ghost {
        Ghost::Text(ghost) => append(ghost, &NSColor::secondaryLabelColor(), None),
        Ghost::Fix(fix) => {
            append(
                &fix.typo,
                &NSColor::labelColor(),
                Some(&NSColor::systemRedColor()),
            );
            append(&format!(" {}", fix.word), &NSColor::systemGreenColor(), None);
            append(&fix.rest, &NSColor::secondaryLabelColor(), None);
        }
    }
    text
}

/// The visible frame of the screen that holds `point`, in global points with a top-left origin.
fn visible_frame_at(
    point: CGPoint,
    primary_screen_height: CGFloat,
    main_thread: MainThreadMarker,
) -> Option<CGRect> {
    NSScreen::screens(main_thread)
        .iter()
        .map(|screen| {
            (
                top_left_frame(screen.frame(), primary_screen_height),
                top_left_frame(screen.visibleFrame(), primary_screen_height),
            )
        })
        .find(|(frame, _)| contains(*frame, point))
        .map(|(_, visible_frame)| visible_frame)
}

/// Turns `frame`, in AppKit's bottom-left screen coordinates, into a frame with a top-left origin.
fn top_left_frame(frame: CGRect, primary_screen_height: CGFloat) -> CGRect {
    let top = primary_screen_height - frame.origin.y - frame.size.height;
    CGRect::new(CGPoint::new(frame.origin.x, top), frame.size)
}

fn contains(frame: CGRect, point: CGPoint) -> bool {
    let horizontal = frame.origin.x..frame.origin.x + frame.size.width;
    let vertical = frame.origin.y..frame.origin.y + frame.size.height;
    horizontal.contains(&point.x) && vertical.contains(&point.y)
}

/// The frame of a mirror of `size` below `field`, or above it when it does not fit below on
/// `screen`, kept inside `screen`. All frames are in global points with a top-left origin.
fn mirror_frame(field: CGRect, size: CGSize, screen: CGRect) -> CGRect {
    let screen_bottom = screen.origin.y + screen.size.height;
    let below = field.origin.y + field.size.height + GAP_TO_FIELD;
    let above = field.origin.y - GAP_TO_FIELD - size.height;
    let top = if below + size.height <= screen_bottom {
        below
    } else {
        above.max(screen.origin.y)
    };
    let rightmost_left = screen.origin.x + screen.size.width - size.width;
    let left = field.origin.x.min(rightmost_left).max(screen.origin.x);
    CGRect::new(CGPoint::new(left, top), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: CGRect = CGRect::new(CGPoint::new(0.0, 25.0), CGSize::new(1440.0, 875.0));
    const SIZE: CGSize = CGSize::new(300.0, 30.0);

    #[test]
    fn mirror_shows_below_the_field_at_its_left_edge() {
        let field = CGRect::new(CGPoint::new(511.0, 101.0), CGSize::new(860.0, 18.0));

        let frame = mirror_frame(field, SIZE, SCREEN);

        assert_eq!(frame, CGRect::new(CGPoint::new(511.0, 123.0), SIZE));
    }

    #[test]
    fn mirror_shows_above_a_field_at_the_bottom_of_the_screen() {
        let field = CGRect::new(CGPoint::new(511.0, 860.0), CGSize::new(860.0, 18.0));

        let frame = mirror_frame(field, SIZE, SCREEN);

        assert_eq!(frame, CGRect::new(CGPoint::new(511.0, 826.0), SIZE));
    }

    #[test]
    fn mirror_stays_inside_the_right_edge_of_the_screen() {
        let field = CGRect::new(CGPoint::new(1300.0, 101.0), CGSize::new(100.0, 18.0));

        let frame = mirror_frame(field, SIZE, SCREEN);

        assert_eq!(frame.origin.x, 1140.0);
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
}
