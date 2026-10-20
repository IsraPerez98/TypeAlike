//! The ghost text: a click-through, never-focused panel drawn right after the caret.

use ghosttype_core::engine::WordFix;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSAttributedStringNSStringDrawing, NSBackingStoreType, NSColor, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSPanel, NSScreen, NSStatusWindowLevel,
    NSStrikethroughColorAttributeName, NSStrikethroughStyleAttributeName, NSTextField, NSUnderlineStyle,
    NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString, NSNumber, NSPoint,
    NSRect, NSString,
};

use crate::focus::Caret;

#[derive(Debug)]
pub(crate) struct Overlay {
    panel: Retained<NSPanel>,
    label: Retained<NSTextField>,
    main_thread: MainThreadMarker,
}

impl Overlay {
    pub(crate) fn new(main_thread: MainThreadMarker) -> Self {
        let label = NSTextField::labelWithString(&NSString::new(), main_thread);
        label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let panel = floating_panel(main_thread);
        panel.setContentView(Some(&label));
        Self {
            panel,
            label,
            main_thread,
        }
    }

    /// Shows `text` starting at `caret`, in the system font sized to the caret.
    pub(crate) fn show(&self, text: &str, caret: Caret) {
        let font = font_at(caret);
        self.label.setFont(Some(&font));
        self.label.setTextColor(Some(&NSColor::secondaryLabelColor()));
        self.label.setStringValue(&NSString::from_str(text));
        self.present(caret);
    }

    /// Shows `fix` at `caret`: a red line through the typed letters before the caret, then the
    /// fixed word in green and the rest of the suggestion as ghost text. The line spans the width of
    /// the letters in the system font, so it can differ a little from the letters of the app.
    pub(crate) fn show_fix(&self, fix: &WordFix, caret: Caret) {
        let font = font_at(caret);
        let typo = attributed(
            &fix.typo,
            &font,
            &NSColor::clearColor(),
            Some(&NSColor::systemRedColor()),
        );
        let text = NSMutableAttributedString::new();
        text.appendAttributedString(&typo);
        text.appendAttributedString(&attributed(
            &format!(" {}", fix.word),
            &font,
            &NSColor::systemGreenColor(),
            None,
        ));
        text.appendAttributedString(&attributed(
            &fix.rest,
            &font,
            &NSColor::secondaryLabelColor(),
            None,
        ));
        self.label.setAttributedStringValue(&text);
        self.present(Caret {
            left: caret.left - typo.size().width,
            ..caret
        });
    }

    /// Sizes the panel to the label and shows it with its left edge at `caret`.
    fn present(&self, caret: Caret) {
        let Some(primary_screen_height) = primary_screen_height(self.main_thread) else {
            self.hide();
            return;
        };
        self.label.sizeToFit();
        let size = self.label.frame().size;
        let origin = panel_origin(caret, primary_screen_height, size.height);
        self.panel.setFrame_display(NSRect::new(origin, size), true);
        self.panel.orderFrontRegardless();
    }

    pub(crate) fn hide(&self) {
        self.panel.orderOut(None);
    }
}

/// Builds a borderless panel that floats over every app on every Space. It never takes focus,
/// and clicks go through it.
pub(crate) fn floating_panel(main_thread: MainThreadMarker) -> Retained<NSPanel> {
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(main_thread),
        NSRect::ZERO,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: the caller owns the panel, so AppKit must not release it on close.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setIgnoresMouseEvents(true);
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    panel.setHidesOnDeactivate(false);
    panel.setLevel(NSStatusWindowLevel);
    panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::Stationary,
    );
    panel
}

/// The height of the primary screen, which turns the top-left global coordinates of the caret
/// into the bottom-left ones of AppKit.
pub(crate) fn primary_screen_height(main_thread: MainThreadMarker) -> Option<CGFloat> {
    let primary_screen = NSScreen::screens(main_thread).firstObject()?;
    Some(primary_screen.frame().size.height)
}

/// Returns the system font whose line fits the caret.
fn font_at(caret: Caret) -> Retained<NSFont> {
    let font_at_caret_height = NSFont::systemFontOfSize(caret.height);
    NSFont::systemFontOfSize(fitted_font_size(caret.height, line_height(&font_at_caret_height)))
}

/// Returns `text` in `font` and `color`, struck through in `strike_color` when it has one.
fn attributed(
    text: &str,
    font: &NSFont,
    color: &NSColor,
    strike_color: Option<&NSColor>,
) -> Retained<NSAttributedString> {
    let single = NSNumber::new_isize(NSUnderlineStyle::Single.0);
    // SAFETY: the keys are constants of AppKit.
    let (font_key, color_key, strike_key, strike_color_key) = unsafe {
        (
            NSFontAttributeName,
            NSForegroundColorAttributeName,
            NSStrikethroughStyleAttributeName,
            NSStrikethroughColorAttributeName,
        )
    };
    let mut keys: Vec<&NSAttributedStringKey> = vec![font_key, color_key];
    let mut values: Vec<&AnyObject> = vec![font, color];
    if let Some(strike_color) = strike_color {
        keys.extend([strike_key, strike_color_key]);
        values.extend([&*single as &AnyObject, strike_color]);
    }
    let attributes = NSDictionary::from_slices(&keys, &values);
    // SAFETY: each value has the type that AppKit expects for its key.
    unsafe {
        NSAttributedString::initWithString_attributes(
            NSAttributedString::alloc(),
            &NSString::from_str(text),
            Some(&attributes),
        )
    }
}

fn line_height(font: &NSFont) -> CGFloat {
    font.ascender() - font.descender() + font.leading()
}

/// Returns the point size whose line height equals the caret's, given a font sized to the caret.
fn fitted_font_size(caret_height: CGFloat, line_height_at_caret_size: CGFloat) -> CGFloat {
    caret_height * caret_height / line_height_at_caret_size
}

/// Centers a panel `panel_height` tall on `caret`, in AppKit's bottom-left screen coordinates.
fn panel_origin(caret: Caret, primary_screen_height: CGFloat, panel_height: CGFloat) -> NSPoint {
    let caret_middle_from_top = caret.top + caret.height / 2.0;
    let caret_middle_from_bottom = primary_screen_height - caret_middle_from_top;
    NSPoint::new(caret.left, caret_middle_from_bottom - panel_height / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_is_centered_on_caret_in_flipped_coordinates() {
        let caret = Caret {
            left: 300.0,
            top: 100.0,
            height: 20.0,
        };

        let origin = panel_origin(caret, 1000.0, 24.0);

        assert_eq!(origin, NSPoint::new(300.0, 878.0));
    }

    #[test]
    fn font_shrinks_so_its_line_fits_the_caret() {
        let font_size = fitted_font_size(20.0, 25.0);

        assert!((font_size - 16.0).abs() < f64::EPSILON);
    }
}
