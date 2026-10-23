//! The wait for the caret after a key: the ghost text that shows when the caret settles.

use std::fmt;

use super::{
    CARET_MOVE_CHECK_DELAY, CARET_SETTLE_DELAY, Ghost, GhostPlacement, GhostView, MAX_CARET_MOVE_CHECKS,
    Shell, Timer,
};

/// When the controller shows a ghost text that waits for the caret.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GhostTiming {
    /// At once, because the caret settled after the last key.
    Now,
    /// When the caret timer runs out.
    AfterSettle,
}

/// The ghost text that waits for the caret to settle, and the timer that the wait runs.
/// `Debug` shows whether a ghost text waits, but not its text, so logs never hold a suggestion.
#[derive(Default)]
pub(super) struct CaretWait {
    ghost: Option<Ghost>,
    is_timer_pending: bool,
    /// Whether the caret timer ran out after the last key. A stop of the wait keeps it, because
    /// the request when the input settles stops the wait but does not move the caret.
    has_caret_settled: bool,
    /// Whether `ghost` is the rest of a suggestion that Tab took a part of. The rest counts as
    /// shown, so that a quick Tab types its next part and does not reach the app.
    is_rest: bool,
    /// The looks at the caret that are left before the ghost text shows at a caret that did not
    /// move after the last key.
    move_checks_left: u8,
}

impl fmt::Debug for CaretWait {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CaretWait")
            .field("has_ghost", &self.ghost.is_some())
            .field("is_timer_pending", &self.is_timer_pending)
            .field("has_caret_settled", &self.has_caret_settled)
            .field("is_rest", &self.is_rest)
            .field("move_checks_left", &self.move_checks_left)
            .finish()
    }
}

impl CaretWait {
    /// Gives a key all the looks at the caret, and waits for the caret to settle again, because
    /// the key can move the caret.
    pub(super) fn on_key(&mut self) {
        self.move_checks_left = MAX_CARET_MOVE_CHECKS;
        self.has_caret_settled = false;
    }

    /// Whether the waiting ghost text is the rest of a suggestion that Tab took a part of.
    pub(super) fn is_rest(&self) -> bool {
        self.is_rest
    }

    /// Whether the caret timer runs.
    pub(super) fn is_timer_pending(&self) -> bool {
        self.is_timer_pending
    }

    /// Shows `ghost` at the settled caret. When the caret settled after the last key, the
    /// controller shows `ghost` at once. Else the caret timer starts again.
    pub(super) fn show_at_settled_caret(&mut self, ghost: Ghost, shell: &mut dyn Shell) -> GhostTiming {
        self.ghost = Some(ghost);
        if self.has_caret_settled {
            return GhostTiming::Now;
        }
        self.restart_timer(shell);
        GhostTiming::AfterSettle
    }

    /// Shows `rest`, the rest of a suggestion that Tab took a part of, when the caret settles
    /// after the part that Tab typed.
    pub(super) fn show_rest_after_settle(&mut self, rest: String, shell: &mut dyn Shell) {
        self.ghost = Some(Ghost::Text(rest));
        self.restart_timer(shell);
        self.is_rest = true;
    }

    /// Shows `ghost` in place of the waiting ghost text, and does not make the wait longer.
    pub(super) fn replace_ghost(&mut self, ghost: Ghost) {
        self.ghost = Some(ghost);
    }

    /// Starts the caret timer again, so that the ghost text and the badge show at the settled caret.
    pub(super) fn restart_timer(&mut self, shell: &mut dyn Shell) {
        shell.start_timer(Timer::CaretSettle, CARET_SETTLE_DELAY);
        self.is_timer_pending = true;
    }

    /// Handles the caret timer that ran out, before the controller shows the ghost text. The caret
    /// settled, so a ghost text that comes later shows at once.
    pub(super) fn on_timer(&mut self) {
        self.is_timer_pending = false;
        self.has_caret_settled = true;
    }

    /// After the controller showed the waiting ghost text, looks at the caret again later when the
    /// shell did not show it at the caret. The wait ends after the last look, or when it shows.
    pub(super) fn after_render(&mut self, shell: &mut dyn Shell) {
        if self.ghost.is_none() {
            return;
        }
        if shell.is_ghost_shown() {
            self.move_checks_left = 0;
        } else if let Some(checks_left) = self.move_checks_left.checked_sub(1) {
            self.move_checks_left = checks_left;
            shell.start_timer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY);
            self.is_timer_pending = true;
        }
        self.is_rest &= self.is_timer_pending;
    }

    /// Returns the view of the waiting ghost text after `text_before_caret`, at `placement`.
    pub(super) fn ghost_view(&self, text_before_caret: &str, placement: GhostPlacement) -> Option<GhostView> {
        self.ghost.clone().map(|ghost| GhostView {
            is_after_edit: self.move_checks_left > 0,
            ..GhostView::new(ghost, text_before_caret, placement)
        })
    }

    /// Stops the wait and forgets the waiting ghost text.
    pub(super) fn stop(&mut self, shell: &mut dyn Shell) {
        self.ghost = None;
        shell.stop_timer(Timer::CaretSettle);
        self.is_timer_pending = false;
        self.is_rest = false;
    }
}
