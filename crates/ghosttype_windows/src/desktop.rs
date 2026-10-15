//! The desktop as core reads it while the user types: the foreground window and the copied text.

use ghosttype_core::app::Field;
use ghosttype_core::desktop::{Desktop, Surroundings};

use crate::clipboard::ClipboardWatch;
use crate::focus::FocusTracker;

/// The focused field of the foreground window, and the text that the clipboard watch keeps.
#[derive(Debug)]
pub(crate) struct ForegroundDesktop<'a> {
    pub(crate) focus: &'a FocusTracker,
    pub(crate) clipboard: &'a ClipboardWatch,
}

impl Desktop for ForegroundDesktop<'_> {
    fn field(&self) -> Field {
        self.focus.field()
    }

    fn surroundings(&self) -> Surroundings {
        self.focus.surroundings()
    }

    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.copied_text().map(str::to_owned)
    }
}
