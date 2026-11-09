//! What TypeAlike reads from the desktop while the user types: the seam the platform implements.

mod text_color;

pub use text_color::{GHOST_TEXT_ALPHA, TextColor};

use std::fmt;
use std::time::Duration;

use crate::settings::OcrLevel;

/// The most characters before the caret that a platform reads: the most that a session keeps on
/// any backend. A suggestion uses fewer, because the text drops its oldest characters when it
/// reaches the limit of the backend.
pub const MAX_CHARS_BEFORE_CARET: usize = 9000;

/// The most characters after the caret that a platform reads: enough text for the budget of the
/// text after the caret in the prompt of an email.
pub const MAX_CHARS_AFTER_CARET: usize = 900;

/// Whether `character` ends a line in the text of a field. Word ends a line inside a paragraph
/// with a vertical tab.
///
/// ```
/// use typealike_core::desktop::is_line_break;
///
/// assert!(is_line_break('\r'));
/// assert!(!is_line_break(' '));
/// ```
#[must_use]
pub fn is_line_break(character: char) -> bool {
    matches!(character, '\n' | '\r' | '\u{b}' | '\u{2028}' | '\u{2029}')
}

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

/// The text after the caret on its line: the first line of `text_after`, or empty when that line
/// has only white space.
pub(crate) fn line_after_caret(text_after: &str) -> &str {
    let line = text_after.lines().next().unwrap_or_default();
    if line.trim().is_empty() { "" } else { line }
}

/// What the platform sees after the caret at a look. The text that a session reads at its start
/// can be stale at the look: an empty field can show a placeholder that goes away at the first
/// key. An app can also select text after the caret that the next key replaces, as the address bar
/// of a browser does for its inline completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AfterCaret {
    /// The platform cannot tell, so the text that the session read at its start counts.
    #[default]
    Unknown,
    /// No text is selected, and the line of the caret has only white space after the caret.
    Nothing,
    /// No text is selected, and text follows the caret on its line.
    Text,
    /// Text is selected, so it follows the caret. The app shows it as a suggestion of its own.
    Selection,
}

impl AfterCaret {
    /// What a look sees when `selected` is the start of the selected text, empty when nothing is
    /// selected, and `text_after` is the text after the selection.
    #[must_use]
    pub fn seen(selected: &str, text_after: &str) -> Self {
        if !selected.is_empty() {
            Self::Selection
        } else if line_after_caret(text_after).is_empty() {
            Self::Nothing
        } else {
            Self::Text
        }
    }
}

/// A moment of the clock of the OS: the seconds since the Unix epoch, in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub u64);

impl Timestamp {
    const SECONDS_PER_DAY: u64 = 86_400;

    /// The UTC day of the moment, counted from the Unix epoch.
    #[must_use]
    pub fn day(self) -> u64 {
        self.0 / Self::SECONDS_PER_DAY
    }
}

/// The text on the clipboard. `Debug` shows only its length, so logs never hold what the user
/// copied.
#[derive(Clone, PartialEq, Eq)]
pub struct CopiedText {
    /// The text that the user copied.
    pub text: String,
    /// The time since the copy, or `None` when the copy came before TypeAlike started and its
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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

impl ScreenRect {
    /// Whether the rectangle has a width and a height above 0.
    #[must_use]
    pub fn has_area(&self) -> bool {
        self.right > self.left && self.bottom > self.top
    }
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
    /// Optical character recognition (OCR) of a capture of the window.
    Ocr(OcrRead),
}

/// How the platform reads the screen with OCR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcrRead {
    /// The level of the OCR.
    pub level: OcrLevel,
    /// The part of the window that the OCR reads.
    pub area: OcrArea,
}

/// The part of the window around the focused field that an OCR reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrArea {
    /// The whole window.
    Window,
    /// The content that holds the field when the platform can find it, such as the web page in a
    /// browser or the pane of the window: not the tabs, the address bar, the bookmarks bar or a
    /// sidebar. Else the whole window.
    Content,
}

/// Whether the platform can read the screen with OCR now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrAvailability {
    /// The platform has no OCR.
    Unsupported,
    /// The user did not give TypeAlike the permission to capture the screen.
    NotPermitted,
    /// The device saves power, so TypeAlike does no OCR.
    SavingPower,
    /// The platform can do an OCR.
    Ready,
}

/// A permission that the OS asks the user to give, before TypeAlike can use a part of the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Read the field where the user types, watch the keys and type suggestions. Without it,
    /// TypeAlike shows no suggestion.
    Accessibility,
    /// Capture the windows on the screen, for the OCR of the screen text.
    ScreenRecording,
}

/// Whether the user gave TypeAlike a permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    /// TypeAlike has the permission.
    Granted,
    /// TypeAlike does not have the permission, or the OS refuses an old grant.
    Missing,
}

/// Whether the computer saves energy now: energy saver or battery saver on Windows, Low Power Mode
/// on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnergySaver {
    /// The computer runs as usual.
    #[default]
    Off,
    /// The computer saves energy.
    On,
}

/// Whether the app in front is a game: an app of the game list of Windows (Game Bar), or an app in
/// the Games category on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameActivity {
    /// The app in front is not a game.
    #[default]
    None,
    /// A game is in front.
    InFront,
}

/// Whether the app in front shows in full screen: its window fills the whole display, as a video,
/// a slideshow or a game in full screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FullScreenActivity {
    /// The app in front does not fill the display.
    #[default]
    None,
    /// An app in full screen is in front.
    InFront,
}

/// The recognition levels that the OCR of the platform has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcrLevels {
    /// The OCR has one level, so the user does not pick a level.
    One,
    /// The user picks the level of the OCR: [`OcrLevel::Fast`] or [`OcrLevel::Accurate`].
    Selectable,
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
    /// Whether the role of the focused field marks it as a search box, as the role of an HTML
    /// `<input type="search">` does. The platform tells it from the role, never from the labels.
    pub is_search_field: bool,
    /// Whether the focused field can show text that the platform cannot read, as the editor of VS
    /// Code (an `EditContext` element) does while its screen reader mode is off. Its `field_text`
    /// then has no text.
    pub has_unreadable_text: bool,
}

impl Surroundings {
    /// Whether the focused field holds text: the platform reads its text, or the field shows text
    /// that the platform cannot read.
    pub(crate) fn holds_text(&self) -> bool {
        self.field_text.is_some() || self.has_unreadable_text
    }

    /// Whether the focused field shows text that the platform cannot read: a field with unreadable
    /// text whose read gave no text.
    pub(crate) fn hides_field_text(&self) -> bool {
        self.has_unreadable_text
            && self
                .field_text
                .as_ref()
                .is_none_or(|text| text.before_caret.is_empty() && text.after_caret.is_empty())
    }
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
            .field("is_search_field", &self.is_search_field)
            .field("has_unreadable_text", &self.has_unreadable_text)
            .finish()
    }
}

/// Whether the focused field may see suggestions, as classified by the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// An ordinary text field.
    Open,
    /// A field TypeAlike stays out of: passwords, its own windows, apps it cannot type into.
    Excluded,
}

/// The desktop as TypeAlike sees it while the user types. The platform implements it, and core
/// calls it before the key reaches the focused app.
pub trait Desktop {
    /// Whether the focused field may see suggestions. When no read since the last move of the
    /// focus tells whether the focused field hides its text, for example because the reads failed
    /// or did not finish in time, the platform gives [`Field::Excluded`].
    fn field(&self) -> Field;

    /// Reads the app, the window and the text around the caret of the focused field.
    fn surroundings(&self) -> Surroundings;

    /// Reads the text on the clipboard. Core calls it only when the user allows it.
    fn copied_text(&self) -> Option<CopiedText>;

    /// Reads the clock of the OS. A desktop without a clock keeps this default, and the texts
    /// that the history keeps from it then have no time.
    fn now(&self) -> Option<Timestamp> {
        None
    }

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

    /// Tells the state of each permission that the OS asks the user for, in the order of the
    /// Permissions pane. A platform whose OS asks for no permission keeps this default.
    fn permissions(&self) -> Vec<(Permission, PermissionState)> {
        Vec::new()
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
    fn line_feed_return_vertical_tab_and_unicode_separators_break_a_line() {
        let characters = ['\n', '\r', '\u{b}', '\u{2028}', '\u{2029}'];

        assert_eq!(characters.map(is_line_break), [true; 5]);
    }

    #[test]
    fn other_whitespace_and_letters_do_not_break_a_line() {
        let characters = [' ', '\t', '\u{a0}', '\u{c}', 'a'];

        assert_eq!(characters.map(is_line_break), [false; 5]);
    }

    #[test]
    fn look_sees_what_follows_the_caret() {
        let looks = [
            ("", "", AfterCaret::Nothing),
            ("", "  \nSee you", AfterCaret::Nothing),
            ("", " on Friday.", AfterCaret::Text),
            ("", "Try \"create a util\"   \n", AfterCaret::Text),
            ("1", "", AfterCaret::Selection),
            ("o", " on Friday.", AfterCaret::Selection),
        ];

        let seen = looks.map(|(selected, text_after, _)| AfterCaret::seen(selected, text_after));

        assert_eq!(seen, looks.map(|(_, _, expected)| expected));
    }

    #[test]
    fn confidence_takes_a_fraction_in_percent() {
        let fractions = [0.0, 0.304, 0.5, 1.0, 1.7, -0.2, f32::NAN];

        let confidences = fractions.map(Confidence::from_fraction);

        assert_eq!(confidences, [0, 30, 50, 100, 100, 0, 0].map(Confidence));
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
            is_search_field: false,
            has_unreadable_text: false,
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
