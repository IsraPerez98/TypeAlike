//! The ghost text: a click-through, never-focused panel drawn right after the caret.

use ghosttype_core::controller::Ghost;
use ghosttype_core::desktop::{GHOST_TEXT_ALPHA, TextColor};
use ghosttype_core::engine::WordFix;
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

use crate::focus::{FontFace, TextStyle};
use crate::geometry::Caret;
use crate::panel;

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
        let panel = panel::floating_panel(main_thread);
        panel.setContentView(Some(&label));
        Self {
            panel,
            label,
            main_thread,
        }
    }

    /// Shows `text` starting at `caret`, in the face of `style` at the size that fits the caret, and
    /// in the colors for a field whose text has `style`.
    pub(crate) fn show(&self, text: &str, caret: Caret, style: &TextStyle) {
        self.match_field(style.color);
        let contrast = Contrast::read();
        let font = font_at(caret, style.face.as_ref());
        self.label.setFont(Some(&font));
        self.label.setTextColor(Some(&ghost_color(style.color, contrast)));
        self.label.setStringValue(&NSString::from_str(text));
        self.present(caret);
    }

    /// Shows `fix` at `caret` in the face of `style`, for a field whose text has `style`: a red line
    /// through the typed letters before the caret, then the fixed word in green and the rest of the
    /// suggestion as ghost text. The line spans the width of the letters in the font of the
    /// overlay. When the face of the app is not installed, the line can differ a little from the
    /// letters of the app.
    pub(crate) fn show_fix(&self, fix: &WordFix, caret: Caret, style: &TextStyle) {
        self.match_field(style.color);
        let contrast = Contrast::read();
        let font = font_at(caret, style.face.as_ref());
        let typo = attributed(
            &fix.typo,
            &font,
            &NSColor::clearColor(),
            Some(&NSColor::systemRedColor()),
        );
        let text = NSMutableAttributedString::new();
        text.appendAttributedString(&typo);
        append_fix_tail(&text, fix, &font, &ghost_color(style.color, contrast));
        self.label.setAttributedStringValue(&text);
        self.present(Caret {
            left: caret.left - typo.size().width,
            ..caret
        });
    }

    /// Draws in the appearance for the background of a field with text in `text_color`, so that
    /// the colors of a fix keep their contrast there. Without it, the panel follows the app.
    fn match_field(&self, text_color: Option<TextColor>) {
        let appearance =
            text_color.and_then(|color| NSAppearance::appearanceNamed(field_appearance_name(color)));
        self.panel.setAppearance(appearance.as_deref());
    }

    /// Sizes the panel to the label and shows it with its left edge at `caret`.
    fn present(&self, caret: Caret) {
        let Some(primary_screen_height) = panel::primary_screen_height(self.main_thread) else {
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

    pub(crate) fn is_shown(&self) -> bool {
        self.panel.isVisible()
    }
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
