//! The Win32 shell of the controller on the main thread.
//!
//! It reads the desktop and does the effects that the controller asks for.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use typealike_core::app::{BadgeView, TrayView};
use typealike_core::controller::{CaretLook, FocusChange, Ghost, GhostView, Shell, Timer};
use typealike_core::desktop::{
    CopiedText, Desktop, Field, OcrAvailability, Permission, ScreenReadId, ScreenSource, Surroundings,
    Timestamp,
};
use typealike_core::settings::ClipboardAccess;
use typealike_core::settings_window::view::SettingsView;
use typealike_core::setup::SetupView;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, PostQuitMessage, SetTimer};
use windows::core::Error;

use super::{
    CARET_TIMER_ID, FIELD_PAUSE_TIMER_ID, FIELD_TIMER_ID, IDLE_TIMER_ID, INJECT_MESSAGE, SCROLL_TIMER_ID,
    SETTLE_TIMER_ID, TAB_HOLD_TIMER_ID,
};
use crate::badge::Badge;
use crate::caret::Caret;
use crate::caret_reader::{CaretReader, KeptFieldText, NewestCaret};
use crate::clipboard::ClipboardWatch;
use crate::focus::FocusTracker;
use crate::injection::{Replacement, ReplacementQueue};
use crate::ocr::OcrSupport;
use crate::overlay::{self, Overlay};
use crate::screen_text::ScreenTextReader;
use crate::settings::SettingsThread;
use crate::tray::Tray;
use crate::uia::ShownField;
use crate::window::{Window, post_message};

/// The time that the focused app takes to handle a key, a click or a move of the focus. The caret
/// reader reads the field text after it.
const FIELD_READ_DELAY: Duration = Duration::from_millis(40);

/// What the controller reads and does through Win32.
pub(super) struct Win32Shell {
    pub(super) focus: FocusTracker,
    pub(super) clipboard: ClipboardWatch,
    pub(super) overlay: Overlay,
    pub(super) badge: Badge,
    pub(super) caret_reader: CaretReader,
    pub(super) screen_text: ScreenTextReader,
    pub(super) ocr: OcrSupport,
    /// The newest read of the caret, and the caret of its answer, where the ghost text and the
    /// badge show.
    pub(super) newest_caret: NewestCaret,
    /// The text around the caret that the hooks give to the controller.
    pub(super) kept_field_text: KeptFieldText,
    pub(super) tray: Tray,
    pub(super) window: Window,
    pub(super) pending_injections: ReplacementQueue,
    pub(super) settings: SettingsThread,
}

impl Win32Shell {
    /// Posts `message` to the main window, which handles it after the current event.
    pub(super) fn post(&self, message: u32) -> windows::core::Result<()> {
        post_message(self.window.handle(), message)
    }

    /// Forgets the kept field text, because an input can change the field now, and reads it again
    /// after [`FIELD_READ_DELAY`].
    pub(super) fn forget_field_text(&mut self) {
        self.kept_field_text.forget_text();
        self.start_window_timer(FIELD_TIMER_ID, FIELD_READ_DELAY);
    }

    /// Forgets the kept field text and whether the focused field hides its text, because a click
    /// or a move of the focus can put the focus in another field now, and reads the field again
    /// after [`FIELD_READ_DELAY`].
    pub(super) fn forget_focused_field(&mut self) {
        self.kept_field_text.forget_focused_field();
        self.start_window_timer(FIELD_TIMER_ID, FIELD_READ_DELAY);
    }

    /// Asks the caret reader for the field text as it is now.
    pub(super) fn read_field_text(&mut self) {
        self.stop_window_timer(FIELD_TIMER_ID);
        self.caret_reader.read_field_text(self.kept_field_text.newest());
    }

    /// Starts the window timer `id`, or starts it again when it runs.
    fn start_window_timer(&self, id: usize, delay: Duration) {
        let delay_ms = u32::try_from(delay.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: plain call on our own window.
        let is_started = unsafe { SetTimer(Some(self.window.handle()), id, delay_ms, None) } != 0;
        if !is_started {
            tracing::warn!(error = %Error::from_thread(), id, "failed to start a timer");
        }
    }

    fn stop_window_timer(&self, id: usize) {
        // Fails only when no timer is pending, which needs no handling.
        // SAFETY: plain call on our own window.
        let _ = unsafe { KillTimer(Some(self.window.handle()), id) };
    }

    fn show_ghost(&mut self, ghost: Option<&GhostView>, caret: Option<&Caret>) {
        let (Some(ghost), Some(caret)) = (ghost, caret) else {
            self.overlay.hide();
            return;
        };
        if let Err(error) = self.overlay.show(ghost, caret) {
            tracing::warn!(%error, "failed to show ghost text");
        }
    }

    fn show_badge(&mut self, badge: Option<BadgeView>, caret: Option<&Caret>) {
        let (Some(view), Some(caret)) = (badge, caret) else {
            self.badge.hide();
            return;
        };
        if let Err(error) = self.badge.show(&view, caret) {
            tracing::warn!(%error, "failed to show the badge");
            self.badge.hide();
        }
    }
}

impl Desktop for Win32Shell {
    /// Excludes a field that hides its text. The hooks cannot wait for UIA, so after a click or a
    /// move of the focus, a field also counts as one that hides its text until a read of it tells
    /// that it shows its text. A read that fails or does not finish in time does not tell it.
    fn field(&self) -> Field {
        if self.kept_field_text.may_hide_text() {
            Field::Excluded
        } else {
            self.focus.field()
        }
    }

    fn surroundings(&self) -> Surroundings {
        let field = self
            .kept_field_text
            .read_in(self.focus.foreground())
            .and_then(|read| read.field.shown());
        merged_surroundings(self.focus.surroundings(), field)
    }

    fn copied_text(&self) -> Option<CopiedText> {
        self.clipboard.copied_text()
    }

    fn ocr_availability(&self) -> OcrAvailability {
        self.ocr.availability()
    }

    fn now(&self) -> Option<Timestamp> {
        let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
        Some(Timestamp(since_epoch.as_secs()))
    }

    /// The overlay hides the ghost text when the caret reader finds no caret.
    fn is_ghost_shown(&self) -> bool {
        self.overlay.is_shown()
    }
}

impl Shell for Win32Shell {
    fn refresh_focus(&mut self) -> FocusChange {
        self.focus.refresh()
    }

    /// Asks the caret reader for the caret. The runtime gives the answer to the controller.
    fn read_caret(&mut self) -> CaretLook {
        let read = self.newest_caret.next_read();
        self.caret_reader.read(read);
        CaretLook::Later
    }

    /// Measures `ghost` in the font of the overlay at the caret of the newest answer of the caret
    /// reader.
    fn ghost_width(&self, ghost: &Ghost) -> i32 {
        let Some(caret) = self.newest_caret.caret() else {
            return 0;
        };
        overlay::inline_width(ghost, caret).unwrap_or_else(|error| {
            tracing::warn!(%error, "failed to measure ghost text");
            0
        })
    }

    /// Shows `ghost` and `badge` at the caret of the newest answer of the caret reader, and hides
    /// them when that answer found no caret.
    fn render_at_caret(&mut self, ghost: Option<&GhostView>, badge: Option<BadgeView>) {
        let caret = self.newest_caret.caret().cloned();
        self.show_ghost(ghost, caret.as_ref());
        self.show_badge(badge, caret.as_ref());
    }

    fn hide_ghost(&mut self) {
        self.overlay.hide();
    }

    fn hide_badge(&mut self) {
        self.badge.hide();
    }

    fn replace_before_caret(&mut self, deleted: usize, text: String) {
        let window = self.window.handle();
        let posting = self.pending_injections.queue(Replacement { deleted, text }, || {
            post_message(window, INJECT_MESSAGE)
        });
        if let Err(error) = posting {
            tracing::warn!(%error, "failed to queue accepted text");
        }
    }

    fn start_timer(&mut self, timer: Timer, delay: Duration) {
        self.start_window_timer(timer_id(timer), delay);
    }

    fn stop_timer(&mut self, timer: Timer) {
        self.stop_window_timer(timer_id(timer));
    }

    fn render_tray(&mut self, view: &TrayView) {
        if let Err(error) = self.tray.update(view) {
            tracing::warn!(%error, "failed to update tray icon");
        }
    }

    fn render_settings(&mut self, view: &SettingsView) {
        self.settings.render(view.clone());
    }

    fn show_settings(&mut self, view: &SettingsView, page: Option<usize>) {
        self.settings.show(view.clone(), page);
    }

    fn show_setup(&mut self, view: &SetupView) {
        self.settings.show_setup(view.clone());
    }

    fn render_setup(&mut self, view: &SetupView) {
        self.settings.render_setup(view.clone());
    }

    fn close_setup(&mut self) {
        self.settings.close_setup();
    }

    fn use_clipboard_access(&mut self, access: ClipboardAccess) {
        self.clipboard.use_access(access);
    }

    fn read_screen(&mut self, read: ScreenReadId, source: ScreenSource) {
        self.screen_text.read(read, source);
    }

    /// Windows asks for no permission, so core never asks for one.
    fn ask_for_permission(&mut self, _permission: Permission) {}

    fn open_web_page(&mut self, url: &str) {
        crate::web_page::open(url);
    }

    fn quit(&mut self) {
        // SAFETY: the call takes no pointer and only posts a message to this thread.
        unsafe { PostQuitMessage(0) };
    }
}

fn timer_id(timer: Timer) -> usize {
    match timer {
        Timer::CaretSettle => CARET_TIMER_ID,
        Timer::InputSettle => SETTLE_TIMER_ID,
        Timer::TabHold => TAB_HOLD_TIMER_ID,
        Timer::Idle => IDLE_TIMER_ID,
        Timer::FieldPause => FIELD_PAUSE_TIMER_ID,
        Timer::ScrollSettle => SCROLL_TIMER_ID,
    }
}

/// Adds to `app`, the surroundings of the foreground app, what `field`, the focused field that
/// shows its text, tells about itself. Without that field, the field parts stay empty.
fn merged_surroundings(app: Surroundings, field: Option<&ShownField>) -> Surroundings {
    Surroundings {
        field_text: field.and_then(|field| field.text.clone()),
        field_labels: field.map(|field| field.labels.clone()).unwrap_or_default(),
        domain: field.and_then(|field| field.domain.clone()),
        is_caret_line_hidden: field.is_some_and(|field| field.is_caret_line_hidden),
        is_search_field: field.is_some_and(|field| field.is_search_field),
        has_unreadable_text: field.is_some_and(|field| field.has_unreadable_text),
        ..app
    }
}

#[cfg(test)]
mod tests {
    use typealike_core::desktop::{FieldLabels, FieldText};

    use super::*;

    fn notepad() -> Surroundings {
        Surroundings {
            app_name: Some("notepad".to_owned()),
            window_title: Some("Notes".to_owned()),
            ..Surroundings::default()
        }
    }

    #[test]
    fn shown_field_gives_its_text_labels_and_web_page() {
        let field = ShownField {
            text: Some(FieldText {
                before_caret: "Dear ".to_owned(),
                after_caret: String::new(),
            }),
            labels: FieldLabels {
                name: Some("Message".to_owned()),
                ..FieldLabels::default()
            },
            domain: Some("mail.example.com".to_owned()),
            is_caret_line_hidden: false,
            is_search_field: false,
            has_unreadable_text: false,
        };

        let surroundings = merged_surroundings(notepad(), Some(&field));

        assert_eq!(
            surroundings,
            Surroundings {
                field_text: field.text.clone(),
                field_labels: field.labels.clone(),
                domain: field.domain.clone(),
                ..notepad()
            }
        );
    }

    #[test]
    fn no_shown_field_keeps_only_the_app() {
        let surroundings = merged_surroundings(notepad(), None);

        assert_eq!(surroundings, notepad());
    }

    #[test]
    fn field_that_hides_its_caret_line_tells_it() {
        let field = ShownField {
            is_caret_line_hidden: true,
            ..ShownField::default()
        };

        let surroundings = merged_surroundings(notepad(), Some(&field));

        assert!(surroundings.is_caret_line_hidden);
    }

    #[test]
    fn field_with_unreadable_text_tells_it() {
        let field = ShownField {
            has_unreadable_text: true,
            ..ShownField::default()
        };

        let surroundings = merged_surroundings(notepad(), Some(&field));

        assert!(surroundings.has_unreadable_text);
    }

    #[test]
    fn search_field_tells_it() {
        let field = ShownField {
            is_search_field: true,
            ..ShownField::default()
        };

        let surroundings = merged_surroundings(notepad(), Some(&field));

        assert!(surroundings.is_search_field);
    }
}
