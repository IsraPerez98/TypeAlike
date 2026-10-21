//! The mirror: a small panel next to a focused field whose caret Accessibility does not give, as
//! in the editor of VS Code. It shows the end of the typed word, then the ghost text. When text can
//! follow the caret on its line, as in a terminal of xterm.js, it shows them below the caret.

use ghosttype_core::controller::{Ghost, GhostView};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAttributedStringNSStringDrawing, NSBox, NSBoxType, NSColor, NSFont, NSFontWeightSemibold, NSPanel,
    NSScreen, NSTextField, NSTitlePosition,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::{NSAttributedString, NSMutableAttributedString, NSPoint, NSRect, NSSize, NSString};

use crate::focus::Caret;
use crate::overlay::Contrast;
use crate::{badge, overlay};

const MIRROR_PADDING: CGFloat = 6.0;
const MIRROR_CORNER_RADIUS: CGFloat = 6.0;
const MIRROR_BORDER_WIDTH: CGFloat = 1.0;
const MAX_TEXT_WIDTH: CGFloat = 480.0;
const GAP_TO_FIELD: CGFloat = 4.0;
/// The distance from an edge of the mirror to its text.
const TEXT_INSET: CGFloat = MIRROR_PADDING + MIRROR_BORDER_WIDTH;
/// The opacity of the label color for the ghost text. It keeps a contrast of 5.5:1 on the mirror
/// in the light appearance, where the secondary label color has only 3.8:1.
const GHOST_TEXT_OPACITY: CGFloat = 0.6;

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

    /// Shows `view` below the line of `caret` with the first letter of the ghost text under the
    /// caret, or above the line when it does not fit below.
    pub(crate) fn show_below_caret(&self, view: &GhostView, caret: Caret) {
        let text = MirrorText::of(view, Contrast::read());
        self.present(&text.text, caret_line(caret, text.typed_width));
    }

    /// Shows `view` below `field`, the frame of the focused field in global points with a top-left
    /// origin, or above it when it does not fit below.
    pub(crate) fn show(&self, view: &GhostView, field: CGRect) {
        self.present(&MirrorText::of(view, Contrast::read()).text, field);
    }

    /// Shows `text` below `anchor`, a frame in global points with a top-left origin, or above it
    /// when it does not fit below.
    fn present(&self, text: &NSAttributedString, anchor: CGRect) {
        let Some(primary_screen_height) = overlay::primary_screen_height(self.main_thread) else {
            self.hide();
            return;
        };
        let Some(screen) = visible_frame_at(anchor.origin, primary_screen_height, self.main_thread) else {
            self.hide();
            return;
        };
        self.label.setAttributedStringValue(text);
        let text_size = self.text_size();
        self.label.setFrameSize(text_size);
        let inset = 2.0 * TEXT_INSET;
        let size = CGSize::new(text_size.width + inset, text_size.height + inset);
        let frame = mirror_frame(anchor, size, screen);
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

/// The text of the mirror: the typed letters before the caret, then the ghost text.
struct MirrorText {
    text: Retained<NSMutableAttributedString>,
    /// The width of the typed letters, which end at the caret.
    typed_width: CGFloat,
}

impl MirrorText {
    /// The text of `view` with `contrast`: the end of the typed word and the typo of a fix in the
    /// label color and the semibold system font, with a red line through the typo. Then the ghost
    /// text as the overlay shows it, in a ghost color that keeps the contrast of text.
    fn of(view: &GhostView, contrast: Contrast) -> Self {
        let font_size = NSFont::systemFontSize();
        let font = NSFont::systemFontOfSize(font_size);
        // SAFETY: reads an AppKit constant that is initialized before any code runs.
        let typed_font = NSFont::systemFontOfSize_weight(font_size, unsafe { NSFontWeightSemibold });
        let label_color = NSColor::labelColor();
        let ghost_color = ghost_color(contrast);
        let text = NSMutableAttributedString::new();
        let append = |part: &str, font: &NSFont, color: &NSColor, strike_color: Option<&NSColor>| {
            text.appendAttributedString(&overlay::attributed(part, font, color, strike_color));
        };
        append(&view.typed_word, &typed_font, &label_color, None);
        if let Ghost::Fix(fix) = &view.ghost {
            append(
                &fix.typo,
                &typed_font,
                &label_color,
                Some(&NSColor::systemRedColor()),
            );
        }
        let typed_width = text.size().width;
        match &view.ghost {
            Ghost::Text(ghost) => append(ghost, &font, &ghost_color, None),
            Ghost::Fix(fix) => {
                append(
                    &format!(" {}", fix.word),
                    &font,
                    &NSColor::systemGreenColor(),
                    None,
                );
                append(&fix.rest, &font, &ghost_color, None);
            }
        }
        Self { text, typed_width }
    }
}

/// The color of the ghost text in the mirror: the label color at less opacity, or with Increase
/// Contrast the secondary label color, which is stronger then.
fn ghost_color(contrast: Contrast) -> Retained<NSColor> {
    match contrast {
        Contrast::Standard => NSColor::labelColor().colorWithAlphaComponent(GHOST_TEXT_OPACITY),
        Contrast::Increased => NSColor::secondaryLabelColor(),
    }
}

/// The line of `caret` as a frame without width, so that the text of a mirror below it starts
/// `typed_width` before the caret, and the ghost text after it starts under the caret.
fn caret_line(caret: Caret, typed_width: CGFloat) -> CGRect {
    CGRect::new(
        CGPoint::new(caret.left - TEXT_INSET - typed_width, caret.top),
        CGSize::new(0.0, caret.height),
    )
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
    fn mirror_below_the_caret_starts_its_ghost_text_under_the_caret() {
        let caret = Caret {
            left: 600.0,
            top: 200.0,
            height: 18.0,
        };

        let frame = mirror_frame(caret_line(caret, 20.0), SIZE, SCREEN);

        assert_eq!(frame.origin, CGPoint::new(573.0, 222.0));
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
