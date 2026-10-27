//! The seam to the platform: what the controller reads from it and asks it to do.

use std::time::Duration;

use super::{Ghost, GhostView};
use crate::app::{BadgeView, TrayView};
use crate::desktop::{Desktop, Permission, ScreenReadId, ScreenRect, ScreenSource};
use crate::settings::ClipboardAccess;
use crate::settings_window::view::SettingsView;

/// A timer that the shell runs for the controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timer {
    /// Waits until the host app moved its caret, so that the ghost text and the badge show at it.
    CaretSettle,
    /// Waits for [`INPUT_SETTLE_DELAY`](crate::app::INPUT_SETTLE_DELAY) without keys and clicks.
    InputSettle,
    /// Ends the hold of a quick Tab after the last part of a suggestion that showed. After it, a
    /// Tab reaches the app again while no ghost text shows.
    TabHold,
    /// Waits for the idle timeout of the Energy saving pane without keys and clicks.
    Idle,
}

/// Whether another window or app has the focus since the shell looked last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusChange {
    /// The same window or app has the focus.
    Unchanged,
    /// Another window or app has the focus.
    Changed,
}

/// One look of the platform at the caret of the focused field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaretRead {
    /// The caret, or `None` when the platform finds no caret.
    pub caret: Option<ScreenRect>,
    /// The frame of the focused field, or `None` when the platform cannot read it.
    pub field: Option<ScreenRect>,
}

/// How a shell looks at the caret before the controller shows something at the caret. With each
/// look, the controller keeps the ghost text off a caret that did not move after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaretLook {
    /// The shell starts the read of the caret now and gives the read later to
    /// [`Controller::on_caret_read`](super::Controller::on_caret_read). The controller then shows
    /// the ghost text and the badge.
    Later,
    /// The shell read the caret at once. The controller shows the ghost text and the badge now.
    Now(CaretRead),
}

/// The platform as the controller drives it: the desktop that it reads, and what it shows and
/// does.
pub trait Shell: Desktop {
    /// Looks at the focused window again, and tells whether another window or app has the focus
    /// since the last look.
    fn refresh_focus(&mut self) -> FocusChange;

    /// Starts a look at the caret before the controller shows the ghost text and the badge, and
    /// tells how the shell looks.
    fn read_caret(&mut self) -> CaretLook;

    /// Returns the width of `ghost` after the caret, as the shell draws it right after the caret
    /// of the read that it gave last to the controller, in the units of [`ScreenRect`]. Returns 0
    /// when the shell cannot measure it, so that the ghost text stays whole.
    fn ghost_width(&self, ghost: &Ghost) -> i32;

    /// Shows the `ghost` text at its placement at the caret of the focused field and the `badge`
    /// at the caret. The caret is the caret of the read that the shell gave last to the
    /// controller. Hides the ghost text for no `ghost` and the badge for no `badge`. When the caret
    /// is not known, it can show the badge at the start of the focused field and the mirror of
    /// `ghost` next to the field, else it hides them.
    fn render_at_caret(&mut self, ghost: Option<&GhostView>, badge: Option<BadgeView>);

    /// Hides the ghost text at once.
    fn hide_ghost(&mut self);

    /// Hides the badge at once.
    fn hide_badge(&mut self);

    /// Deletes the `deleted` characters before the caret of the focused app and types `text`
    /// there, after the event that the controller handles now.
    fn replace_before_caret(&mut self, deleted: usize, text: String);

    /// Starts `timer`, or starts it again when it runs. After `delay`, the shell gives the timer
    /// one time to [`Controller::on_timer`](super::Controller::on_timer).
    fn start_timer(&mut self, timer: Timer, delay: Duration);

    /// Stops `timer` when it runs.
    fn stop_timer(&mut self, timer: Timer);

    /// Shows `view` in the tray icon and its menu.
    fn render_tray(&mut self, view: &TrayView);

    /// Shows `view` in the settings window, when the window exists.
    fn render_settings(&mut self, view: &SettingsView);

    /// Shows the settings window with `view` and brings it to the front. Makes the window the
    /// first time. With a `page`, the window selects the page of that index in `view`, else it
    /// keeps the page that it shows.
    fn show_settings(&mut self, view: &SettingsView, page: Option<usize>);

    /// Keeps the text that the user copies while `access` allows it, and forgets it when not.
    fn use_clipboard_access(&mut self, access: ClipboardAccess);

    /// Reads the text near the focused field on the screen, off the thread of the keys, without
    /// password fields. Gives the answer with the id `read` to
    /// [`Controller::on_delivery`](super::Controller::on_delivery) as a
    /// [`Delivery::Screen`](super::Delivery::Screen).
    fn read_screen(&mut self, read: ScreenReadId, source: ScreenSource);

    /// Asks the user for `permission`: shows the prompt of the system, or the place in the system
    /// settings where the user gives it. When the OS keeps an old grant that it refuses, the shell
    /// removes that grant first, so that the user can give the permission again.
    fn ask_for_permission(&mut self, permission: Permission);

    /// Ends TypeAlike after the event that the controller handles now.
    fn quit(&mut self);
}
