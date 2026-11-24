//! The wait for the caret after a key: the ghost text that shows when the caret settles, and only
//! at a caret that moved after the key.

use std::{fmt, mem};

use super::{
    AFTER_KEY_DELAY, CARET_MOVE_CHECK_DELAY, CARET_SETTLE_DELAY, CaretRead, Ghost, GhostPlacement, GhostView,
    MAX_CARET_MOVE_CHECKS, Shell, Timer,
};
use crate::desktop::ScreenRect;
use crate::key::Key;

/// The room around the focused field, in caret heights, where a caret still counts as a caret of
/// that field.
const FIELD_HALO_IN_CARET_HEIGHTS: i32 = 4;
/// The room before the right side of the focused field, in caret heights, where a caret that stays
/// over a whole key counts as a caret that the key moved.
const RIGHT_SIDE_IN_CARET_HEIGHTS: i32 = 2;

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
    /// Whether `ghost` is the rest of a suggestion that Tab or a typed key took a part of, and the
    /// user saw its next part. The rest counts as shown, so that a quick Tab types its next part
    /// and does not reach the app.
    is_rest: bool,
    /// The length in bytes of the end of the ghost text of the last view that the view did not
    /// show, because it did not fit on the line of the caret. Only the view of the next look
    /// changes it, because the keys keep that end.
    hidden_len: usize,
    /// The view that the last look showed, and the text that the keys typed after it.
    looked: Option<LookedView>,
    /// The view that stays after keys that typed the start of the ghost text of the last look,
    /// until a look finds the caret that the keys moved.
    kept: Option<PlacedView>,
    looks: CaretLooks,
}

/// A view of the ghost text, by where it shows.
enum PlacedView {
    /// Right after the caret, where the app writes the typed keys over it.
    AfterCaret(GhostView),
    /// Below the caret or in the mirror, where the typed keys do not reach it.
    Apart,
}

/// The view that a look showed, and the text that the keys typed after the look.
struct LookedView {
    view: PlacedView,
    /// The whole ghost text that the view fits.
    whole: Ghost,
    /// The text that the keys and Tab typed into the app after the look, a start of the whole
    /// ghost text. `None` after a key that changed the text in another way, or typed another text.
    typed: Option<String>,
}

impl LookedView {
    /// Notes `key`, which the app gets, after a dead key with `accent` or without one: a letter
    /// adds to the typed text. Tab adds the part that it takes in [`Self::on_tab_part`]. Each
    /// other key, and a letter in place of a marked accent, change the text in another way.
    fn on_key(&mut self, key: Key, accent: DeadKeyAccent) {
        match (key, &mut self.typed) {
            (Key::Char(_), _) if accent == DeadKeyAccent::Marked => self.typed = None,
            (Key::Char(character), Some(typed)) => typed.push(character),
            (Key::Char(_) | Key::Tab, _) => {}
            _ => self.typed = None,
        }
        self.forget_typed_text_off_the_ghost();
    }

    /// Notes `part`, the part of the suggestion that Tab types into the app.
    fn on_tab_part(&mut self, part: &str) {
        if let Some(typed) = &mut self.typed {
            typed.push_str(part);
        }
        self.forget_typed_text_off_the_ghost();
    }

    /// Forgets the typed text when it is not a start of the whole ghost text, because no later key
    /// makes it one again.
    fn forget_typed_text_off_the_ghost(&mut self) {
        let is_start_of_whole = match (&self.whole, &self.typed) {
            (Ghost::Text(whole), Some(typed)) => whole.starts_with(typed.as_str()),
            _ => false,
        };
        if !is_start_of_whole {
            self.typed = None;
        }
    }

    /// The view that stays after the keys, when `ghost` is the rest of the whole ghost text after
    /// the typed text, and the user still sees a next part: a view after the caret with the typed
    /// text as its taken part, else the view as it shows. `None` when the view must hide.
    fn after_keys(&self, ghost: &Ghost) -> Option<PlacedView> {
        let (Ghost::Text(whole), Ghost::Text(rest), Some(typed)) = (&self.whole, ghost, &self.typed) else {
            return None;
        };
        let is_rest_of_whole = whole.strip_prefix(typed.as_str()) == Some(rest.as_str());
        if !is_rest_of_whole || rest.trim().is_empty() {
            return None;
        }
        match &self.view {
            PlacedView::AfterCaret(view) => view.after_typed(typed).map(PlacedView::AfterCaret),
            PlacedView::Apart => Some(PlacedView::Apart),
        }
    }
}

/// Whether the user saw the next part of the rest of a suggestion that Tab took a part of.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RestSight {
    /// The last view showed the next word of the rest.
    Seen,
    /// The last view showed only the parts that Tab took, because the rest did not fit on the
    /// line of the caret.
    Hidden,
}

impl fmt::Debug for CaretWait {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CaretWait")
            .field("has_ghost", &self.ghost.is_some())
            .field("is_timer_pending", &self.is_timer_pending)
            .field("has_caret_settled", &self.has_caret_settled)
            .field("is_rest", &self.is_rest)
            .field("hidden_len", &self.hidden_len)
            .field("has_looked_view", &self.looked.is_some())
            .field("has_kept_view", &self.kept.is_some())
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
    /// The keys after the last look that found a caret that moved. At a look after more than one
    /// key, an app can show only some of the keys yet.
    keys_after_look: u8,
    /// Whether the caret can still move without a key: the last look came after more than one key,
    /// or it followed such a look and found the caret at another place.
    is_caret_catching_up: bool,
    /// The accent of a dead key, when the last key was a dead key.
    accent: DeadKeyAccent,
    /// Whether the caret stays at the right side of its field while the user types.
    right_side: RightSideCaret,
}

/// Whether the caret stays at the right side of its field while the user types, as in a field that
/// scrolls its line sideways, by what the looks at the caret found.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum RightSideCaret {
    /// No look after a key found the caret at the right side of its field where the key did not
    /// move it, since the last look that found a caret that moved.
    #[default]
    Unseen,
    /// A look after the last key found the caret at the right side of its field, where the key did
    /// not move it. A terminal also gives such a look before it writes the key back.
    Unmoved,
    /// The caret stayed at the right side of its field over a whole key. Thus a caret there that
    /// a key did not move is not stale.
    Stays,
}

/// The accent that a dead key types in the app, by what the looks at the caret found.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum DeadKeyAccent {
    /// The last key was not a dead key.
    #[default]
    None,
    /// The last key was a dead key, and no look found a caret that moved after it.
    Unseen,
    /// A look found a caret that the dead key moved: the app shows the accent as marked text, and
    /// the letter after it takes its place without a move of the caret.
    Marked,
}

impl CaretLooks {
    /// Gives `key` all the checks, and ends the look that waits, because the key can move the
    /// caret. A letter in place of a marked accent does not move the caret, so its ghost text does
    /// not wait for a caret that moved.
    fn on_key(&mut self, key: Key) {
        let replaces_marked_accent = self.accent == DeadKeyAccent::Marked && matches!(key, Key::Char(_));
        self.accent = if key == Key::Dead {
            DeadKeyAccent::Unseen
        } else {
            DeadKeyAccent::None
        };
        if self.right_side == RightSideCaret::Unmoved {
            self.right_side = RightSideCaret::Stays;
        }
        self.checks_left = MAX_CARET_MOVE_CHECKS;
        self.is_after_edit = !replaces_marked_accent;
        self.is_pending = false;
        self.keys_after_look = self.keys_after_look.saturating_add(1);
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
    fn on_read(&mut self, read: &CaretRead) -> Option<LookOutcome> {
        if !mem::take(&mut self.is_pending) {
            return None;
        }
        let Some(caret) = read.caret else {
            self.is_caret_catching_up = false;
            return Some(LookOutcome::GhostAndBadge);
        };
        if read.field.is_some_and(|field| !is_plausible_caret(caret, field)) {
            return Some(LookOutcome::Nothing);
        }
        if self.is_stale(caret, read.field) {
            return Some(LookOutcome::BadgeOnly);
        }
        let has_moved = self.last_caret != Some(caret);
        if has_moved {
            self.right_side = RightSideCaret::Unseen;
        }
        self.is_caret_catching_up = self.keys_after_look > 1 || (self.is_caret_catching_up && has_moved);
        self.keys_after_look = 0;
        let has_moved_after_edit =
            self.is_after_edit() && self.last_caret.is_some_and(|last_caret| last_caret != caret);
        if has_moved_after_edit && self.accent == DeadKeyAccent::Unseen {
            self.accent = DeadKeyAccent::Marked;
        }
        self.last_caret = Some(caret);
        self.is_after_edit = false;
        Some(LookOutcome::GhostAndBadge)
    }

    /// Whether `caret`, the caret of a look in `field` after a key, is still the caret before the
    /// key: it is at the place of the last look, and it did not stay at the right side of the
    /// field over a whole key. Notes a caret at the right side that the key did not move.
    fn is_stale(&mut self, caret: ScreenRect, field: Option<ScreenRect>) -> bool {
        if !self.is_after_edit() || self.last_caret != Some(caret) {
            return false;
        }
        if !field.is_some_and(|field| is_at_right_side(caret, field)) {
            return true;
        }
        if self.right_side == RightSideCaret::Stays {
            return false;
        }
        self.right_side = RightSideCaret::Unmoved;
        true
    }
}

impl CaretWait {
    /// Gives `key` all the looks at the caret, and waits for the caret to settle again, because
    /// the key can move the caret. The look that waits ends, because its caret can be stale.
    pub(super) fn on_key(&mut self, key: Key) {
        self.has_caret_settled = false;
        let accent = self.looks.accent;
        self.looks.on_key(key);
        if let Some(looked) = &mut self.looked {
            looked.on_key(key, accent);
        }
    }

    /// Notes `part`, the part of the suggestion that Tab types into the app.
    pub(super) fn on_tab_part(&mut self, part: &str) {
        if let Some(looked) = &mut self.looked {
            looked.on_tab_part(part);
        }
    }

    /// Forgets the caret and the view of the last look, because the caret is now in another field
    /// or at an unknown place.
    pub(super) fn on_context_lost(&mut self) {
        self.looks.last_caret = None;
        self.looked = None;
        self.kept = None;
    }

    /// Waits for the caret to settle again after a scroll. The look that waits ends, because its
    /// caret can be stale. A scroll does not change the text, so the ghost text can show again at
    /// a caret that did not move.
    pub(super) fn on_scroll(&mut self, shell: &mut dyn Shell) {
        self.looks.is_pending = false;
        self.kept = None;
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

    /// Whether the waiting ghost text is the rest of a suggestion that Tab or a typed key took a
    /// part of, and the user saw its next part.
    pub(super) fn is_rest(&self) -> bool {
        self.is_rest
    }

    /// Whether the caret timer runs.
    pub(super) fn is_timer_pending(&self) -> bool {
        self.is_timer_pending
    }

    /// Shows `ghost` at the settled caret. When the caret settled after the last key, the
    /// controller shows `ghost` at once. Else the caret timer starts again, and the shown ghost
    /// text stays while `ghost` is its rest after the typed keys, or hides. When the user saw the
    /// next part of `ghost` before the key, it counts as shown while it waits.
    pub(super) fn show_at_settled_caret(&mut self, ghost: Ghost, shell: &mut dyn Shell) -> GhostTiming {
        if self.has_caret_settled {
            self.ghost = Some(ghost);
            return GhostTiming::Now;
        }
        self.is_rest = (self.is_rest || shell.is_ghost_shown()) && self.shows_next_part_of(&ghost);
        self.keep_view_after_key(&ghost, shell);
        self.ghost = Some(ghost);
        self.restart_timer(shell);
        GhostTiming::AfterSettle
    }

    /// Keeps the shown view of the last look while the caret moves after the keys, when `ghost`
    /// is the rest of its ghost text after the typed text. A view after the caret then shows the
    /// typed text clear after the key ([`Timer::AfterKey`]), where the app writes it, so the rest
    /// does not move. Else the shown ghost text hides, because the app writes the key over it.
    fn keep_view_after_key(&mut self, ghost: &Ghost, shell: &mut dyn Shell) {
        let is_shown = shell.is_ghost_shown();
        self.kept = self
            .looked
            .as_ref()
            .filter(|_| is_shown)
            .and_then(|looked| looked.after_keys(ghost));
        match &self.kept {
            Some(PlacedView::AfterCaret(_)) => shell.start_timer(Timer::AfterKey, AFTER_KEY_DELAY),
            None if is_shown => shell.hide_ghost(),
            Some(PlacedView::Apart) | None => {}
        }
    }

    /// Whether a view stays after keys that typed the start of its ghost text, while the caret
    /// moves.
    pub(super) fn has_kept_view(&self) -> bool {
        self.kept.is_some()
    }

    /// Shows the view after the caret that stays after the keys, with the typed text clear.
    pub(super) fn redraw_kept_view(&self, shell: &mut dyn Shell) {
        if let Some(PlacedView::AfterCaret(view)) = &self.kept {
            shell.redraw_ghost(view);
        }
    }

    /// Whether the last view of the waiting ghost text showed the next part of `ghost`, which
    /// stays of it after a typed key: the end of its text, or its fix with the typed key after the
    /// typo.
    fn shows_next_part_of(&self, ghost: &Ghost) -> bool {
        match (&self.ghost, ghost) {
            (Some(Ghost::Text(earlier)), Ghost::Text(rest)) => {
                earlier.ends_with(rest.as_str()) && sight_of(rest, self.hidden_len) == RestSight::Seen
            }
            (Some(Ghost::Fix(earlier)), Ghost::Fix(fix)) => {
                fix.typo.starts_with(&earlier.typo) && earlier.rest.ends_with(&fix.rest)
            }
            _ => false,
        }
    }

    /// Shows `rest`, the rest of a suggestion that Tab took a part of, when the caret settles
    /// after the part that Tab typed. Until then, the shown ghost text stays or hides as after a
    /// typed key. When the user saw its next word, the rest counts as shown while it waits.
    /// Returns whether the user saw it.
    pub(super) fn show_rest_after_settle(&mut self, rest: String, shell: &mut dyn Shell) -> RestSight {
        let sight = sight_of(&rest, self.hidden_len);
        let ghost = Ghost::Text(rest);
        self.keep_view_after_key(&ghost, shell);
        self.ghost = Some(ghost);
        self.restart_timer(shell);
        self.is_rest = sight == RestSight::Seen;
        sight
    }

    /// Notes `view`, the view of the waiting ghost text that the controller shows now at `caret`,
    /// or `None` when no ghost text shows. The view of the keys before ends.
    pub(super) fn note_view(&mut self, view: Option<&GhostView>, caret: Option<ScreenRect>) {
        self.hidden_len = view
            .zip(self.ghost.as_ref())
            .map_or(0, |(view, ghost)| view.hidden_len(ghost));
        self.looked = view.zip(self.ghost.clone()).map(|(view, whole)| LookedView {
            view: match (caret, view.placement) {
                (Some(_), GhostPlacement::AfterCaret) => PlacedView::AfterCaret(view.clone()),
                _ => PlacedView::Apart,
            },
            whole,
            typed: Some(String::new()),
        });
        self.kept = None;
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
    /// are left, because the host app did not move it yet. A caret that stayed at the right side of
    /// its field over a whole key gets the ghost text, because a field that scrolls its line
    /// sideways keeps the caret there.
    pub(super) fn on_caret_read(&mut self, read: &CaretRead) -> Option<LookOutcome> {
        let outcome = self.looks.on_read(read);
        if outcome == Some(LookOutcome::Nothing) {
            self.kept = None;
        }
        outcome
    }

    /// After the controller showed the waiting ghost text, looks at the caret again later when the
    /// shell did not show it at the caret, when a view stays after the keys, or when the caret can
    /// still move after fast keys. The wait ends after the last look, or when it shows at a caret
    /// that does not catch up.
    pub(super) fn after_render(&mut self, shell: &mut dyn Shell) {
        if self.ghost.is_none() {
            return;
        }
        if shell.is_ghost_shown() && !self.looks.is_caret_catching_up && self.kept.is_none() {
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

    /// Forgets the waiting ghost text, and keeps the caret timer, so that the badge still shows
    /// when the caret settles.
    pub(super) fn forget_ghost(&mut self) {
        self.ghost = None;
        self.is_rest = false;
        self.kept = None;
    }

    /// Stops the wait and forgets the waiting ghost text. A look that waits goes on, so that the
    /// badge shows when it comes.
    pub(super) fn stop(&mut self, shell: &mut dyn Shell) {
        self.forget_ghost();
        shell.stop_timer(Timer::CaretSettle);
        self.is_timer_pending = false;
    }
}

/// Whether the user saw the next word of `rest`, whose last `hidden_len` bytes did not show.
fn sight_of(rest: &str, hidden_len: usize) -> RestSight {
    let shown = rest.len().checked_sub(hidden_len).and_then(|end| rest.get(..end));
    if shown.is_some_and(|shown| !shown.trim().is_empty()) {
        RestSight::Seen
    } else {
        RestSight::Hidden
    }
}

/// Whether `caret` is at the right side of `field`: at most [`RIGHT_SIDE_IN_CARET_HEIGHTS`] caret
/// heights before it, or after it. A field that scrolls its line sideways, as classic Notepad does,
/// or that aligns its text to the right keeps the caret there while the user types. A field without
/// area is not known.
fn is_at_right_side(caret: ScreenRect, field: ScreenRect) -> bool {
    if !field.has_area() {
        return false;
    }
    let room = caret
        .bottom
        .saturating_sub(caret.top)
        .saturating_mul(RIGHT_SIDE_IN_CARET_HEIGHTS);
    caret.left >= field.right.saturating_sub(room)
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
    fn caret_is_at_the_right_side_up_to_two_caret_heights_before_it() {
        let cases = [
            (caret_at(300, 110), false),
            (caret_at(459, 110), false),
            (caret_at(460, 110), true),
            (caret_at(490, 110), true),
            (caret_at(520, 110), true),
        ];

        let at_right_side = cases.map(|(caret, _)| is_at_right_side(caret, FIELD));

        assert_eq!(at_right_side, cases.map(|(_, expected)| expected));
    }

    #[test]
    fn no_caret_is_at_the_right_side_of_a_field_without_area() {
        let field = ScreenRect { right: 100, ..FIELD };

        assert!(!is_at_right_side(caret_at(100, 110), field));
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
