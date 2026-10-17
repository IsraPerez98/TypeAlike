//! What GhostType reads from the desktop while the user types: the seam the platform implements.

use std::fmt;

use crate::app::Field;

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

/// What surrounds the caret in the focused app. `Debug` shows only what is known, so logs never
/// hold what the user wrote or which document is open.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Surroundings {
    /// The name of the app, as the user knows it, for example "Mail".
    pub app_name: Option<String>,
    /// The title of the focused window, for example "Re: Q3 budget review".
    pub window_title: Option<String>,
    /// The text of the focused field, or `None` when the platform cannot read the field.
    pub field_text: Option<FieldText>,
}

impl fmt::Debug for Surroundings {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Surroundings")
            .field("has_app_name", &self.app_name.is_some())
            .field("has_window_title", &self.window_title.is_some())
            .field("field_text", &self.field_text)
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
    fn clipboard_text(&self) -> Option<String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_reveal_text_or_window() {
        let surroundings = Surroundings {
            app_name: Some("Mail".to_owned()),
            window_title: Some("secret title".to_owned()),
            field_text: Some(FieldText {
                before_caret: "secret before".to_owned(),
                after_caret: "secret after".to_owned(),
            }),
        };

        let debug = format!("{surroundings:?}");

        assert!(!debug.contains("secret"), "{debug}");
    }
}
