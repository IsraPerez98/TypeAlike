//! What GhostType reads from the desktop while the user types: the seam the platform implements.

use std::fmt;
use std::time::Duration;

use crate::app::Field;
use crate::settings::OcrLevel;

/// The most characters before the caret that a platform reads. A suggestion uses fewer, because
/// the text drops its oldest characters when it reaches this many.
pub const MAX_CHARS_BEFORE_CARET: usize = 4000;

/// The most characters after the caret that a suggestion uses. A platform reads at most this many.
pub const MAX_CHARS_AFTER_CARET: usize = 300;

/// The text of the focused field around the caret. With a selection, `before_caret` stops where
/// the selection starts and `after_caret` starts where it stops, because a typed key replaces the
/// selection. `Debug` shows only lengths, so logs never hold what the user wrote.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct FieldText {
    /// The text before the caret.
    pub before_caret: String,
    /// The text after the caret.
    pub after_caret: String,
}

impl fmt::Debug for FieldText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FieldText")
            .field("before_caret_chars", &self.before_caret.chars().count())
            .field("after_caret_chars", &self.after_caret.chars().count())
            .finish()
    }
}

/// The text on the clipboard. `Debug` shows only its length, so logs never hold what the user
/// copied.
#[derive(Clone, PartialEq, Eq)]
pub struct CopiedText {
    /// The text that the user copied.
    pub text: String,
    /// The time since the copy, or `None` when the copy came before GhostType started and its
    /// time is unknown.
    pub age: Option<Duration>,
}

impl fmt::Debug for CopiedText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CopiedText")
            .field("chars", &self.text.chars().count())
            .field("age", &self.age)
            .finish()
    }
}

/// A rectangle on the screen, with the origin at the top left of the main display and y down: in
/// physical pixels on Windows and in points on macOS. Core compares only the rectangles of one
/// read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScreenRect {
    /// The left edge.
    pub left: i32,
    /// The top edge.
    pub top: i32,
    /// The right edge, after the last column of the rectangle.
    pub right: i32,
    /// The bottom edge, after the last row of the rectangle.
    pub bottom: i32,
}

/// The opacity, out of 255, of ghost text in the color of the field text. As a placeholder, the
/// ghost text then keeps the contrast of the field text at less emphasis on any background.
pub const GHOST_TEXT_ALPHA: u8 = 128;

/// The weights of red, green and blue in the relative luminance of WCAG 2.
const LUMINANCE_WEIGHTS: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// The opaque sRGB color in which a field draws its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextColor {
    /// The red channel.
    pub red: u8,
    /// The green channel.
    pub green: u8,
    /// The blue channel.
    pub blue: u8,
}

impl TextColor {
    /// Whether the text is light, as on a dark background: it has more contrast with black than
    /// with white.
    #[must_use]
    pub fn is_light(self) -> bool {
        let luminance = self.relative_luminance();
        contrast_ratio(luminance, 0.0) > contrast_ratio(1.0, luminance)
    }

    /// The contrast ratio of WCAG 2 between this color and `other`, from 1 to 21. Text needs at
    /// least 4.5 on its background.
    #[must_use]
    pub fn contrast_with(self, other: Self) -> f64 {
        let [first, second] = [self, other].map(Self::relative_luminance);
        contrast_ratio(first.max(second), first.min(second))
    }

    fn relative_luminance(self) -> f64 {
        [self.red, self.green, self.blue]
            .map(linear_channel)
            .iter()
            .zip(LUMINANCE_WEIGHTS)
            .map(|(channel, weight)| channel * weight)
            .sum()
    }
}

/// Undoes the sRGB gamma of `channel`.
fn linear_channel(channel: u8) -> f64 {
    let value = f64::from(channel) / f64::from(u8::MAX);
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The contrast ratio of WCAG 2 between a lighter and a darker relative luminance.
fn contrast_ratio(lighter: f64, darker: f64) -> f64 {
    (lighter + 0.05) / (darker + 0.05)
}

/// One line of text on the screen, outside of the focused field. `Debug` shows only its length
/// and frame, so logs never hold what the screen shows.
#[derive(Clone, PartialEq, Eq)]
pub struct ScreenLine {
    /// The text of the line.
    pub text: String,
    /// Where the line shows.
    pub frame: ScreenRect,
    /// How sure the platform is that `text` is what the screen shows.
    pub confidence: Confidence,
}

/// How sure the platform is of the text of a line, in percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Confidence(pub(crate) u8);

impl Confidence {
    /// The confidence of text that an app gives as text, not as pixels.
    pub const CERTAIN: Self = Self(100);

    /// The confidence of a recognized line, from a `fraction` between 0 and 1.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the percent is rounded into 0 to 100 first, and `as` gives 0 for NaN"
    )]
    pub fn from_fraction(fraction: f32) -> Self {
        Self((fraction * 100.0).round().clamp(0.0, 100.0) as u8)
    }
}

/// Where the platform reads the text near the focused field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenSource {
    /// The accessibility tree of the window: the text that the app gives as text.
    Accessibility,
    /// Optical character recognition (OCR) of a capture of the window, at this level.
    Ocr(OcrLevel),
}

/// Whether the platform can read the screen with OCR now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrAvailability {
    /// The platform has no OCR.
    Unsupported,
    /// The user did not give GhostType the permission to capture the screen.
    NotPermitted,
    /// The device saves power, so GhostType does no OCR.
    SavingPower,
    /// The platform can do an OCR.
    Ready,
}

/// The recognition levels that the OCR of the platform has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrLevels {
    /// The OCR has one level, so the user does not pick a level.
    One,
    /// The user picks the level of the OCR: [`OcrLevel::Fast`] or [`OcrLevel::Accurate`].
    Selectable,
}

impl fmt::Debug for ScreenLine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScreenLine")
            .field("chars", &self.text.chars().count())
            .field("frame", &self.frame)
            .field("confidence", &self.confidence)
            .finish()
    }
}

/// The text near the focused field on the screen, from one read of the focused window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScreenText {
    /// The lines of text in the window, in any order.
    pub lines: Vec<ScreenLine>,
    /// Where the caret is, or `None` when the platform cannot find it.
    pub caret: Option<ScreenRect>,
    /// Where the focused field is.
    pub field: ScreenRect,
}

/// Names one read of the screen. Core makes it, and the platform gives it back with the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScreenReadId(u64);

impl ScreenReadId {
    /// The id of the read after this one.
    #[must_use]
    pub(crate) fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// The answer to one read of the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenRead {
    /// The read that this answer is for.
    pub id: ScreenReadId,
    /// The text near the focused field, or `None` when the platform cannot read the window.
    pub text: Option<ScreenText>,
}

/// The labels that the focused field gives to assistive apps. Each one is `None` when the field
/// does not give it. `Debug` shows only which labels are known, so logs never hold them.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct FieldLabels {
    /// The visible label that the app links to the field, for example "Comments:".
    pub caption: Option<String>,
    /// The name of the field, for example "Message body" or "Subject".
    pub name: Option<String>,
    /// The text that shows in the field while it is empty, for example "Type a message".
    pub placeholder: Option<String>,
    /// The help text of the field, for example "Tell us what happened".
    pub help: Option<String>,
}

impl fmt::Debug for FieldLabels {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FieldLabels")
            .field("has_caption", &self.caption.is_some())
            .field("has_name", &self.name.is_some())
            .field("has_placeholder", &self.placeholder.is_some())
            .field("has_help", &self.help.is_some())
            .finish()
    }
}

/// What surrounds the caret in the focused app. `Debug` shows only what is known, so logs never
/// hold what the user wrote or which document is open.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Surroundings {
    /// The name of the app, as the user knows it, for example "Mail".
    pub app_name: Option<String>,
    /// The id of the app: its bundle identifier on macOS, for example "com.apple.mail", or the
    /// file name of its program on Windows, for example "OUTLOOK.EXE".
    pub app_id: Option<String>,
    /// The full path of the program of the app, which only the platform reads, for example to
    /// show the icon of the app. `None` when the platform finds the app from its id.
    pub app_path: Option<String>,
    /// The title of the focused window, for example "Re: Q3 budget review".
    pub window_title: Option<String>,
    /// What the focused field tells about itself.
    pub field_labels: FieldLabels,
    /// The host of the web page that holds the focused field, for example "mail.google.com".
    pub domain: Option<String>,
    /// The text of the focused field, or `None` when the platform cannot read the field.
    pub field_text: Option<FieldText>,
    /// Whether the focused field hides the text on the line of its caret, as the input box of a
    /// terminal of xterm.js does. Text can then follow the caret, also when `field_text` has none.
    pub is_caret_line_hidden: bool,
}

impl fmt::Debug for Surroundings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Surroundings")
            .field("has_app_name", &self.app_name.is_some())
            .field("has_app_id", &self.app_id.is_some())
            .field("has_app_path", &self.app_path.is_some())
            .field("has_window_title", &self.window_title.is_some())
            .field("field_labels", &self.field_labels)
            .field("has_domain", &self.domain.is_some())
            .field("field_text", &self.field_text)
            .field("is_caret_line_hidden", &self.is_caret_line_hidden)
            .finish()
    }
}

/// The desktop as GhostType sees it while the user types. The platform implements it, and core
/// calls it before the key reaches the focused app.
pub trait Desktop {
    /// Whether the focused field may see suggestions.
    fn field(&self) -> Field;

    /// Reads the app, the window and the text around the caret of the focused field.
    fn surroundings(&self) -> Surroundings;

    /// Reads the text on the clipboard. Core calls it only when the user allows it.
    fn copied_text(&self) -> Option<CopiedText>;

    /// Tells whether the platform can read the screen with OCR now. A platform without OCR keeps
    /// this default.
    fn ocr_availability(&self) -> OcrAvailability {
        OcrAvailability::Unsupported
    }

    /// Tells which recognition levels the OCR of the platform has. A platform whose OCR has one
    /// level, or that has no OCR, keeps this default.
    fn ocr_levels(&self) -> OcrLevels {
        OcrLevels::One
    }

    /// Tells whether the ghost text shows on the screen now. The platform hides it when it finds
    /// no caret to put it at. A desktop that always shows its ghost text keeps this default.
    fn is_ghost_shown(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_takes_a_fraction_in_percent() {
        let fractions = [0.0, 0.304, 0.5, 1.0, 1.7, -0.2, f32::NAN];

        let confidences = fractions.map(Confidence::from_fraction);

        assert_eq!(confidences, [0, 30, 50, 100, 100, 0, 0].map(Confidence));
    }

    const fn gray(level: u8) -> TextColor {
        TextColor {
            red: level,
            green: level,
            blue: level,
        }
    }

    const fn rgb(value: u32) -> TextColor {
        let [_, red, green, blue] = value.to_be_bytes();
        TextColor { red, green, blue }
    }

    #[test]
    fn dark_text_is_not_light() {
        let colors = [gray(0), rgb(0x1F_2328), rgb(0x33_3333), rgb(0x09_69DA)];

        let shades = colors.map(TextColor::is_light);

        assert_eq!(shades, [false; 4]);
    }

    #[test]
    fn light_text_is_light() {
        let colors = [gray(255), rgb(0xCC_CCCC), rgb(0xD4_D4D4), rgb(0x58_A6FF)];

        let shades = colors.map(TextColor::is_light);

        assert_eq!(shades, [true; 4]);
    }

    #[test]
    fn contrast_of_black_and_white_is_21_to_1_in_both_orders() {
        let pairs = [(gray(0), gray(255)), (gray(255), gray(0))];

        let ratios = pairs.map(|(first, second)| (first.contrast_with(second) * 100.0).round());

        assert_eq!(ratios, [2100.0; 2]);
    }

    #[test]
    fn contrast_of_mid_gray_on_white_is_below_the_text_minimum() {
        let ratio = gray(0x80).contrast_with(gray(0xFF));

        assert_eq!((ratio * 100.0).round(), 395.0);
    }

    #[test]
    fn gray_text_is_light_from_where_it_contrasts_more_with_black_than_with_white() {
        let colors = [gray(117), gray(118)];

        let shades = colors.map(TextColor::is_light);

        assert_eq!(shades, [false, true]);
    }

    #[test]
    fn debug_does_not_reveal_text_or_window() {
        let surroundings = Surroundings {
            app_name: Some("Mail".to_owned()),
            app_id: Some("com.apple.mail".to_owned()),
            app_path: Some("C:\\secret\\MAIL.EXE".to_owned()),
            window_title: Some("secret title".to_owned()),
            field_labels: FieldLabels {
                caption: Some("secret caption".to_owned()),
                name: Some("secret name".to_owned()),
                placeholder: Some("secret placeholder".to_owned()),
                help: Some("secret help".to_owned()),
            },
            domain: Some("secret.example.com".to_owned()),
            field_text: Some(FieldText {
                before_caret: "secret before".to_owned(),
                after_caret: "secret after".to_owned(),
            }),
            is_caret_line_hidden: false,
        };

        let debug = format!("{surroundings:?}");

        assert!(!debug.contains("secret"), "{debug}");
    }

    #[test]
    fn debug_does_not_reveal_screen_text() {
        let line = ScreenLine {
            text: "secret".to_owned(),
            confidence: Confidence::CERTAIN,
            frame: ScreenRect::default(),
        };

        let debug = format!("{line:?}");

        assert!(!debug.contains("secret"), "{debug}");
    }

    #[test]
    fn debug_does_not_reveal_copied_text() {
        let copied = CopiedText {
            text: "secret".to_owned(),
            age: None,
        };

        let debug = format!("{copied:?}");

        assert!(!debug.contains("secret"), "{debug}");
    }
}
