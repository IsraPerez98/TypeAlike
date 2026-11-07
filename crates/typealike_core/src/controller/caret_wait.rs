//! The wait for the caret after a key: the ghost text that shows when the caret settles, and only
//! at a caret that moved after the key.

use std::{fmt, mem};

use super::{
    CARET_MOVE_CHECK_DELAY, CARET_SETTLE_DELAY, CaretRead, Ghost, GhostPlacement, GhostView,
    MAX_CARET_MOVE_CHECKS, Shell, Timer,
};
use crate::desktop::ScreenRect;

/// The room around the focused field, in caret heights, where a caret still counts as a caret of
/// that field.
const FIELD_HALO_IN_CARET_HEIGHTS: i32 = 4;

/// When the controller shows a ghost text that waits for the caret.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GhostTiming {
    /// At once, because the caret settled after the last key.
    Now,
    /// When the caret timer runs out.
    AfterSettle,
}

/// What the controller shows after a look at the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LookOutcome {
    /// The waiting ghost text and the badge, at the caret of the look.
    GhostAndBadge,
    /// Only the badge, because the caret did not move yet after the last key.
    BadgeOnly,
    /// Neither, because the caret is far from its field and thus is not a real caret.
    Nothing,
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
    looks: CaretLooks,
}

impl fmt::Debug for CaretWait {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CaretWait")
            .field("has_ghost", &self.ghost.is_some())
            .field("is_timer_pending", &self.is_timer_pending)
            .field("has_caret_settled", &self.has_caret_settled)
            .field("is_rest", &self.is_rest)
            .field("looks", &self.looks)
            .finish()
    }
}

/// The looks at the caret after a key: the checks for a caret that moved, and the look that
/// waits.
#[derive(Debug, Default)]
struct CaretLooks {
    /// The looks at the caret that are left before the ghost text shows at a caret that did not
    /// move after the last key.
    checks_left: u8,
    /// Whether a key changed the text after the last look that found a caret that moved, and the
    /// ghost text did not show since.
    is_after_edit: bool,
    /// Whether a look at the caret started and did not end, and no key came after it started.
    is_pending: bool,
    /// The caret of the last look that found a caret that moved.
    last_caret: Option<ScreenRect>,
}

impl CaretLooks {
    /// Gives a key all the checks, and ends the look that waits, because the key can move the
    /// caret.
    fn on_key(&mut self) {
        self.checks_left = MAX_CARET_MOVE_CHECKS;
        self.is_after_edit = true;
        self.is_pending = false;
    }

    /// Whether a key changed the text after the ghost text showed last, while checks are left.
    fn is_after_edit(&self) -> bool {
        self.is_after_edit && self.checks_left > 0
    }

    /// Ends the checks, because the ghost text shows.
    fn on_ghost_shown(&mut self) {
        self.checks_left = 0;
        self.is_after_edit = false;
    }

    /// Uses one check, and returns whether one was left.
    fn take_check(&mut self) -> bool {
        let Some(checks_left) = self.checks_left.checked_sub(1) else {
            return false;
        };
        self.checks_left = checks_left;
        true
    }

    /// Ends the look that waits, and returns what the controller shows for `read`. `None` when no
    /// look waits.
    fn on_read(&mut self, read: CaretRead) -> Option<LookOutcome> {
        if !mem::take(&mut self.is_pending) {
            return None;
        }
        let Some(caret) = read.caret else {
            return Some(LookOutcome::GhostAndBadge);
        };
        if read.field.is_some_and(|field| !is_plausible_caret(caret, field)) {
            return Some(LookOutcome::Nothing);
        }
        if self.is_after_edit() && self.last_caret == Some(caret) {
            return Some(LookOutcome::BadgeOnly);
        }
        self.last_caret = Some(caret);
        self.is_after_edit = false;
        Some(LookOutcome::GhostAndBadge)
    }
}

impl CaretWait {
    /// Gives a key all the looks at the caret, and waits for the caret to settle again, because
    /// the key can move the caret. The look that waits ends, because its caret can be stale.
    pub(super) fn on_key(&mut self) {
        self.has_caret_settled = false;
        self.looks.on_key();
    }

    /// Forgets the caret of the last look, because the caret is now in another field or at an
    /// unknown place.
    pub(super) fn on_context_lost(&mut self) {
        self.looks.last_caret = None;
    }

    /// Waits for the caret to settle again after a scroll. The look that waits ends, because its
    /// caret can be stale. A scroll does not change the text, so the ghost text can show again at
    /// a caret that did not move.
    pub(super) fn on_scroll(&mut self, shell: &mut dyn Shell) {
        self.looks.is_pending = false;
        self.restart_timer(shell);
    }

    /// Handles a replacement of `deleted` characters before the caret with `text`. A replacement
    /// that types as many characters as it deletes, as the fix of a typo of the same length, does
    /// not move the caret. Thus the ghost text after it does not wait for a caret that moved.
    pub(super) fn on_replace(&mut self, deleted: usize, text: &str) {
        if text.chars().count() == deleted {
            self.looks.is_after_edit = false;
        }
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
    /// after the part that Tab typed. The user saw `rest`, so it counts as shown while it waits.
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

    /// Notes a look at the caret that starts now.
    pub(super) fn wait_for_look(&mut self) {
        self.looks.is_pending = true;
    }

    /// Ends the look that waits, and returns what the controller shows for `read`. `None` when no
    /// look waits, because a key came after the look started. A caret far from its field counts as
    /// no caret. After a key, a caret at the place of the last look gets only the badge while looks
    /// are left, because the host app did not move it yet.
    pub(super) fn on_caret_read(&mut self, read: CaretRead) -> Option<LookOutcome> {
        self.looks.on_read(read)
    }

    /// After the controller showed the waiting ghost text, looks at the caret again later when the
    /// shell did not show it at the caret. The wait ends after the last look, or when it shows.
    pub(super) fn after_render(&mut self, shell: &mut dyn Shell) {
        if self.ghost.is_none() {
            return;
        }
        if shell.is_ghost_shown() {
            self.looks.on_ghost_shown();
        } else if self.looks.take_check() {
            shell.start_timer(Timer::CaretSettle, CARET_MOVE_CHECK_DELAY);
            self.is_timer_pending = true;
        }
        self.is_rest &= self.is_timer_pending;
    }

    /// Returns the view of the waiting ghost text after `text_before_caret`, at `placement`.
    pub(super) fn ghost_view(&self, text_before_caret: &str, placement: GhostPlacement) -> Option<GhostView> {
        let ghost = self.ghost.clone()?;
        Some(GhostView::new(ghost, text_before_caret, placement))
    }

    /// Stops the wait and forgets the waiting ghost text. A look that waits goes on, so that the
    /// badge shows when it comes.
    pub(super) fn stop(&mut self, shell: &mut dyn Shell) {
        self.ghost = None;
        shell.stop_timer(Timer::CaretSettle);
        self.is_timer_pending = false;
        self.is_rest = false;
    }
}

/// Whether `caret` can be the caret of `field`: the middle of the caret is in the field grown by
/// [`FIELD_HALO_IN_CARET_HEIGHTS`] caret heights on each side. A field without area is not known,
/// so each caret can be its caret.
fn is_plausible_caret(caret: ScreenRect, field: ScreenRect) -> bool {
    if !field.has_area() {
        return true;
    }
    let halo = caret
        .bottom
        .saturating_sub(caret.top)
        .saturating_mul(FIELD_HALO_IN_CARET_HEIGHTS);
    let middle_x = caret.left.midpoint(caret.right);
    let middle_y = caret.top.midpoint(caret.bottom);
    (field.left.saturating_sub(halo)..=field.right.saturating_add(halo)).contains(&middle_x)
        && (field.top.saturating_sub(halo)..=field.bottom.saturating_add(halo)).contains(&middle_y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELD: ScreenRect = ScreenRect {
        left: 100,
        top: 100,
        right: 500,
        bottom: 140,
    };

    /// A caret of 20 pixels at `left` and `top`, which gives a halo of 80 pixels.
    fn caret_at(left: i32, top: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right: left,
            bottom: top + 20,
        }
    }

    #[test]
    fn caret_is_plausible_in_its_field_and_in_the_halo_around_it() {
        let cases = [
            (caret_at(300, 110), true),
            (caret_at(40, 110), true),
            (caret_at(10, 110), false),
            (caret_at(560, 110), true),
            (caret_at(590, 110), false),
            (caret_at(300, 30), true),
            (caret_at(300, -10), false),
            (caret_at(300, 190), true),
            (caret_at(300, 230), false),
        ];

        let plausible = cases.map(|(caret, _)| is_plausible_caret(caret, FIELD));

        assert_eq!(plausible, cases.map(|(_, expected)| expected));
    }

    #[test]
    fn each_caret_is_plausible_in_a_field_without_area() {
        let fields = [
            ScreenRect { right: 100, ..FIELD },
            ScreenRect { bottom: 100, ..FIELD },
            ScreenRect::default(),
        ];

        let plausible = fields.map(|field| is_plausible_caret(caret_at(-900, 2000), field));

        assert_eq!(plausible, [true; 3]);
    }

    #[test]
    fn caret_at_the_end_of_the_coordinates_does_not_overflow() {
        let caret = ScreenRect {
            left: i32::MAX,
            top: i32::MAX - 20,
            right: i32::MAX,
            bottom: i32::MAX,
        };
        let field = ScreenRect {
            left: i32::MAX - 400,
            top: i32::MAX - 40,
            right: i32::MAX,
            bottom: i32::MAX,
        };

        assert!(is_plausible_caret(caret, field));
    }

    #[test]
    fn huge_caret_does_not_overflow_its_halo() {
        let caret = ScreenRect {
            left: 300,
            top: i32::MIN,
            right: 300,
            bottom: i32::MAX,
        };

        assert!(is_plausible_caret(caret, FIELD));
    }
}
