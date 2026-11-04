//! The ghost text: a click-through, never-focused panel drawn right after the caret, and a second
//! one with the lines that a long ghost text wraps onto below the line of the caret.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSAppearance, NSAppearanceCustomization, NSAppearanceName, NSAppearanceNameAqua,
    NSAppearanceNameDarkAqua, NSAttributedStringNSStringDrawing, NSColor, NSFont, NSFontAttributeName,
    NSFontDescriptor, NSForegroundColorAttributeName, NSPanel, NSStrikethroughColorAttributeName,
    NSStrikethroughStyleAttributeName, NSTextField, NSUnderlineStyle, NSWorkspace,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{
    NSAttributedString, NSAttributedStringKey, NSDictionary, NSMutableAttributedString, NSNumber, NSPoint,
    NSRect, NSString,
};
use typealike_core::controller::{Ghost, WrappedLines};
use typealike_core::desktop::{GHOST_TEXT_ALPHA, TextColor};
use typealike_core::engine::WordFix;

use crate::focus::{FontFace, TextStyle};
use crate::geometry::Caret;
use crate::panel;

#[derive(Debug)]
pub(crate) struct Overlay {
    panel: Retained<NSPanel>,
    label: Retained<NSTextField>,
    wrapped_panel: Retained<NSPanel>,
    wrapped_label: Retained<NSTextField>,
    main_thread: MainThreadMarker,
}

impl Overlay {
    pub(crate) fn new(main_thread: MainThreadMarker) -> Self {
        let (panel, label) = label_panel(main_thread);
        let (wrapped_panel, wrapped_label) = label_panel(main_thread);
        Self {
            panel,
            label,
            wrapped_panel,
            wrapped_label,
            main_thread,
        }
    }

    /// Shows `text` starting at `caret`, then the `wrapped` lines below, in the face of `style` at
    /// the size that fits the caret, and in the colors for a field whose text has `style`.
    pub(crate) fn show(&self, text: &str, wrapped: Option<&WrappedLines>, caret: Caret, style: &TextStyle) {
        self.match_field(style.color);
        let font = font_at(caret, style.face.as_ref());
        let color = ghost_color(style.color, Contrast::read());
        self.label.setFont(Some(&font));
        self.label.setTextColor(Some(&color));
        self.label.setStringValue(&NSString::from_str(text));
        self.present(&self.panel, &self.label, caret, 0);
        self.show_wrapped(wrapped, caret, &font, &color);
    }

    /// Shows `fix` at `caret` in the face of `style`, for a field whose text has `style`: a red line
    /// through the typed letters before the caret, then the fixed word in green and the rest of the
    /// suggestion as ghost text, then the `wrapped` lines below. The line spans the width of the
    /// letters in the font of the overlay. When the face of the app is not installed, the line can
    /// differ a little from the letters of the app.
    pub(crate) fn show_fix(
        &self,
        fix: &WordFix,
        wrapped: Option<&WrappedLines>,
        caret: Caret,
        style: &TextStyle,
    ) {
        self.match_field(style.color);
        let font = font_at(caret, style.face.as_ref());
        let color = ghost_color(style.color, Contrast::read());
        let typo = attributed(
            &fix.typo,
            &font,
            &NSColor::clearColor(),
            Some(&NSColor::systemRedColor()),
        );
        let text = NSMutableAttributedString::new();
        text.appendAttributedString(&typo);
        append_fix_tail(&text, fix, &font, &color);
        self.label.setAttributedStringValue(&text);
        let typo_start = Caret {
            left: caret.left - typo.size().width,
            ..caret
        };
        self.present(&self.panel, &self.label, typo_start, 0);
        self.show_wrapped(wrapped, caret, &font, &color);
    }

    /// Shows the `wrapped` lines in `font` and `color` at their left edge, from the line below the
    /// line of `caret`, one caret height apart. Hides them without wrapped lines.
    fn show_wrapped(&self, wrapped: Option<&WrappedLines>, caret: Caret, font: &NSFont, color: &NSColor) {
        let Some(wrapped) = wrapped else {
            self.wrapped_panel.orderOut(None);
            return;
        };
        let lines = wrapped.lines.join("\n");
        self.wrapped_label
            .setAttributedStringValue(&attributed(&lines, font, color, None));
        let first_line = Caret {
            left: CGFloat::from(wrapped.left),
            top: caret.top + caret.height,
            height: caret.height,
        };
        let lines_below = wrapped.lines.len().saturating_sub(1);
        self.present(&self.wrapped_panel, &self.wrapped_label, first_line, lines_below);
    }

    /// Draws in the appearance for the background of a field with text in `text_color`, so that
    /// the colors of a fix keep their contrast there. Without it, the panels follow the app.
    fn match_field(&self, text_color: Option<TextColor>) {
        let appearance =
            text_color.and_then(|color| NSAppearance::appearanceNamed(field_appearance_name(color)));
        self.panel.setAppearance(appearance.as_deref());
        self.wrapped_panel.setAppearance(appearance.as_deref());
    }

    /// Sizes `panel` to its `label` and shows it with its left edge at `caret`, its first line
    /// centered on `caret`, and `lines_below` lines of the caret height below it.
    fn present(&self, panel: &NSPanel, label: &NSTextField, caret: Caret, lines_below: usize) {
        let Some(primary_screen_height) = panel::primary_screen_height(self.main_thread) else {
            self.hide();
            return;
        };
        label.sizeToFit();
        let size = label.frame().size;
        let origin = panel_origin(caret, lines_below, primary_screen_height, size.height);
        panel.setFrame_display(NSRect::new(origin, size), true);
        panel.orderFrontRegardless();
    }

    pub(crate) fn hide(&self) {
        self.panel.orderOut(None);
        self.wrapped_panel.orderOut(None);
    }

    pub(crate) fn is_shown(&self) -> bool {
        self.panel.isVisible()
    }
}

/// Returns a floating panel with an empty label as its content.
fn label_panel(main_thread: MainThreadMarker) -> (Retained<NSPanel>, Retained<NSTextField>) {
    let label = NSTextField::labelWithString(&NSString::new(), main_thread);
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    let panel = panel::floating_panel(main_thread);
    panel.setContentView(Some(&label));
    (panel, label)
}

/// Returns the width of `ghost` after `caret` in `face`, as the overlay draws it right after the
/// caret. The color does not change the width.
pub(crate) fn inline_width(ghost: &Ghost, caret: Caret, face: Option<&FontFace>) -> CGFloat {
    let font = font_at(caret, face);
    let color = NSColor::labelColor();
    match ghost {
        Ghost::Text(text) => attributed(text, &font, &color, None).size().width,
        Ghost::Fix(fix) => {
            let tail = NSMutableAttributedString::new();
            append_fix_tail(&tail, fix, &font, &color);
            tail.size().width
        }
    }
}

/// Whether the user asks macOS for more contrast with Increase Contrast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Contrast {
    Standard,
    Increased,
}

impl Contrast {
    pub(crate) fn read() -> Self {
        if NSWorkspace::sharedWorkspace().accessibilityDisplayShouldIncreaseContrast() {
            Self::Increased
        } else {
            Self::Standard
        }
    }
}

/// The color of ghost text in a field with text in `text_color`: that color at less opacity, or
/// with Increase Contrast the secondary label color, which is stronger then. Without `text_color`,
/// the background is unknown: the system gray is the same in the light and the dark appearance, so
/// it shows on both.
fn ghost_color(text_color: Option<TextColor>, contrast: Contrast) -> Retained<NSColor> {
    match (text_color, contrast) {
        (None, _) => NSColor::systemGrayColor(),
        (Some(_), Contrast::Increased) => NSColor::secondaryLabelColor(),
        (Some(color), Contrast::Standard) => {
            let [red, green, blue, alpha] = [color.red, color.green, color.blue, GHOST_TEXT_ALPHA]
                .map(|channel| CGFloat::from(channel) / CGFloat::from(u8::MAX));
            NSColor::colorWithSRGBRed_green_blue_alpha(red, green, blue, alpha)
        }
    }
}

/// The appearance for the background of a field with text in `text_color`: dark behind light
/// text, else light. AppKit gives each its Increase Contrast variant when that setting is on.
fn field_appearance_name(text_color: TextColor) -> &'static NSAppearanceName {
    // SAFETY: the names are constants of AppKit.
    unsafe {
        if text_color.is_light() {
            NSAppearanceNameDarkAqua
        } else {
            NSAppearanceNameAqua
        }
    }
}

/// Returns the font of `face` when it is installed, else the system font, at the size whose line
/// fits the caret. The size never comes from the field, because a field gives the size of its text
/// in document units, without its zoom.
fn font_at(caret: Caret, face: Option<&FontFace>) -> Retained<NSFont> {
    let font_of_size = |size| {
        face.and_then(|face| installed_font(face, size))
            .unwrap_or_else(|| NSFont::systemFontOfSize(size))
    };
    let font_at_caret_height = font_of_size(caret.height);
    font_of_size(fitted_font_size(caret.height, line_height(&font_at_caret_height)))
}

/// Returns the installed font of `face` at `size`, or `None` when `face` is not installed. A
/// family gives its regular face.
fn installed_font(face: &FontFace, size: CGFloat) -> Option<Retained<NSFont>> {
    match face {
        FontFace::Named(name) => NSFont::fontWithName_size(&NSString::from_str(name), size),
        FontFace::Family(family) => {
            // SAFETY: a descriptor without attributes holds no value of a wrong type.
            let any_font = unsafe { NSFontDescriptor::fontDescriptorWithFontAttributes(None) };
            let descriptor = any_font.fontDescriptorWithFamily(&NSString::from_str(family));
            NSFont::fontWithDescriptor_size(&descriptor, size)
                .filter(|font| font.familyName().is_some_and(|name| name.to_string() == *family))
        }
    }
}

/// Returns `text` in `font` and `color`, struck through in `strike_color` when it has one.
pub(crate) fn attributed(
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

/// Appends the end of `fix` to `text`: the fixed word in green after a space, then the rest of the
/// suggestion in `ghost_color`.
pub(crate) fn append_fix_tail(
    text: &NSMutableAttributedString,
    fix: &WordFix,
    font: &NSFont,
    ghost_color: &NSColor,
) {
    text.appendAttributedString(&attributed(
        &format!(" {}", fix.word),
        font,
        &NSColor::systemGreenColor(),
        None,
    ));
    text.appendAttributedString(&attributed(&fix.rest, font, ghost_color, None));
}

fn line_height(font: &NSFont) -> CGFloat {
    font.ascender() - font.descender() + font.leading()
}

/// Returns the point size whose line height equals the caret's, given a font sized to the caret.
fn fitted_font_size(caret_height: CGFloat, line_height_at_caret_size: CGFloat) -> CGFloat {
    caret_height * caret_height / line_height_at_caret_size
}

/// Puts a panel `panel_height` tall at `caret`, with its first line centered on `caret` and
/// `lines_below` lines of the caret height below it, in AppKit's bottom-left screen coordinates.
fn panel_origin(
    caret: Caret,
    lines_below: usize,
    primary_screen_height: CGFloat,
    panel_height: CGFloat,
) -> NSPoint {
    let caret_middle_from_top = caret.top + caret.height / 2.0;
    let caret_middle_from_bottom = primary_screen_height - caret_middle_from_top;
    let lines_below_height = CGFloat::from(u32::try_from(lines_below).unwrap_or(u32::MAX)) * caret.height;
    NSPoint::new(
        caret.left,
        caret_middle_from_bottom - panel_height.midpoint(lines_below_height),
    )
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
    fn panel_is_centered_on_caret_in_flipped_coordinates() {
        let origin = panel_origin(CARET, 0, 1000.0, 24.0);

        assert_eq!(origin, NSPoint::new(300.0, 878.0));
    }

    #[test]
    fn panel_of_several_lines_keeps_its_first_line_centered_on_the_caret() {
        let origin = panel_origin(CARET, 2, 1000.0, 64.0);

        assert_eq!(origin, NSPoint::new(300.0, 838.0));
    }

    const LIGHT_TEXT: TextColor = TextColor {
        red: 212,
        green: 212,
        blue: 212,
    };
    const DARK_TEXT: TextColor = TextColor {
        red: 31,
        green: 35,
        blue: 40,
    };

    #[test]
    fn field_appearance_is_dark_behind_light_text_and_light_behind_dark_text() {
        let names = [LIGHT_TEXT, DARK_TEXT].map(field_appearance_name);

        // SAFETY: the names are constants of AppKit.
        let expected = unsafe { [NSAppearanceNameDarkAqua, NSAppearanceNameAqua] };
        assert_eq!(names, expected);
    }

    #[test]
    fn ghost_text_has_the_field_text_color_at_the_ghost_text_alpha() {
        let color = ghost_color(Some(LIGHT_TEXT), Contrast::Standard);

        let expected = NSColor::colorWithSRGBRed_green_blue_alpha(
            212.0 / 255.0,
            212.0 / 255.0,
            212.0 / 255.0,
            f64::from(GHOST_TEXT_ALPHA) / 255.0,
        );
        assert_eq!(color, expected);
    }

    #[test]
    fn increase_contrast_keeps_the_secondary_label_color() {
        let color = ghost_color(Some(LIGHT_TEXT), Contrast::Increased);

        assert_eq!(color, NSColor::secondaryLabelColor());
    }

    #[test]
    fn field_without_a_text_color_gets_the_gray_that_shows_on_light_and_dark() {
        let colors = [Contrast::Standard, Contrast::Increased].map(|contrast| ghost_color(None, contrast));

        assert_eq!(colors, [NSColor::systemGrayColor(), NSColor::systemGrayColor()]);
    }

    #[test]
    fn font_shrinks_so_its_line_fits_the_caret() {
        let font_size = fitted_font_size(20.0, 25.0);

        assert!((font_size - 16.0).abs() < f64::EPSILON);
    }
}
