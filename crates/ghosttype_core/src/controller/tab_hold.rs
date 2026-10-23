//! The hold of a quick Tab after the last part of a suggestion that showed, until the next ghost
//! text shows.

use std::mem;
use std::time::Duration;

use super::{Shell, Timer};
use crate::key::Key;

/// The longest time that a Tab after the last part of a suggestion waits for the next suggestion.
pub(super) const TAB_HOLD_DELAY: Duration = Duration::from_millis(800);

/// Keeps a quick Tab after the last part of a suggestion that showed from the app while the next
/// ghost text can still come: the next suggestion, or the rest that did not fit in the field. Thus
/// the Tab does not type a tab character or move the focus. The session stays. A held Tab only waits and does not take the next suggestion when it shows, because Tab
/// types only text that the user saw on the screen.
#[derive(Debug, Default)]
pub(super) struct TabHold {
    is_on: bool,
}

impl TabHold {
    /// Starts the hold after the last part of a suggestion that showed, for [`TAB_HOLD_DELAY`].
    pub(super) fn start(&mut self, shell: &mut dyn Shell) {
        shell.start_timer(Timer::TabHold, TAB_HOLD_DELAY);
        self.is_on = true;
    }

    /// Returns whether the hold keeps `key` from the app and from the suggestions: a Tab while the
    /// next ghost text does not show. Any other key, and a Tab while the next ghost text shows, end
    /// the hold.
    pub(super) fn holds(&mut self, key: Key, shell: &mut dyn Shell) -> bool {
        let is_held = self.is_on && key == Key::Tab && !shell.is_ghost_shown();
        if !is_held {
            self.end(shell);
        }
        is_held
    }

    /// Ends the hold and stops its timer, when the hold is on.
    pub(super) fn end(&mut self, shell: &mut dyn Shell) {
        if mem::take(&mut self.is_on) {
            shell.stop_timer(Timer::TabHold);
        }
    }

    /// Ends the hold when its timer ran out.
    pub(super) fn on_timer(&mut self) {
        self.is_on = false;
    }
}
