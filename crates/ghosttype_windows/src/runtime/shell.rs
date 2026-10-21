//! The shell of the controller on the main thread: it reads the desktop and does the effects that
//! the controller asks for, through Win32.

use std::time::Duration;

use ghosttype_core::app::{BadgeView, TrayView};
use ghosttype_core::controller::{FocusChange, GhostView, Shell, Timer};
use ghosttype_core::desktop::{
    CopiedText, Desktop, Field, OcrAvailability, ScreenReadId, ScreenSource, Surroundings,
};
use ghosttype_core::settings::ClipboardAccess;
use ghosttype_core::settings_window::view::SettingsView;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, PostMessageW, PostQuitMessage, SetTimer};
use windows::core::Error;

use super::{CARET_TIMER_ID, FIELD_TIMER_ID, INJECT_MESSAGE, SETTLE_TIMER_ID};
use crate::badge::Badge;
use crate::caret_reader::{CaretRead, CaretReader, CaretView, KeptFieldText, PendingView};
use crate::clipboard::ClipboardWatch;
use crate::focus::{Caret, FocusTracker};
use crate::injection::Replacement;
use crate::ocr::OcrSupport;
use crate::overlay::Overlay;
use crate::screen_text::ScreenTextReader;
use crate::settings::SettingsThread;
use crate::tray::Tray;
use crate::uia::ShownField;
use crate::window::Window;

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
    /// What shows at the caret when the caret reader answers.
    pub(super) pending_view: PendingView,
    /// The text around the caret that the hooks give to the controller.
    pub(super) kept_field_text: KeptFieldText,
    pub(super) tray: Tray,
    pub(super) window: Window,
    pub(super) pending_injection: Option<Replacement>,
    pub(super) settings: SettingsThread,
}

impl Win32Shell {
    /// Posts `message` to the main window, which handles it after the current event.
    pub(super) fn post(&self, message: u32) -> windows::core::Result<()> {
        // SAFETY: plain call on our own window.
        unsafe { PostMessageW(Some(self.window.handle()), message, WPARAM(0), LPARAM(0)) }
    }

    /// Forgets the kept field text, because an input can change the field now, and reads it again
    /// after [`FIELD_READ_DELAY`].
    pub(super) fn forget_field_text(&mut self) {
        self.kept_field_text.forget();
        self.set_window_timer(FIELD_TIMER_ID, FIELD_READ_DELAY);
    }

    /// Forgets the kept field text and whether the focused field hides its text, because a click
    /// or a move of the focus can put the focus in another field now, and reads the field again
    /// after [`FIELD_READ_DELAY`].
    pub(super) fn forget_focused_field(&mut self) {
        self.kept_field_text.forget_focused_field();
        self.set_window_timer(FIELD_TIMER_ID, FIELD_READ_DELAY);
    }

    /// Asks the caret reader for the field text as it is now.
    pub(super) fn read_field_text(&mut self) {
        self.kill_window_timer(FIELD_TIMER_ID);
        self.caret_reader.read_field_text(self.kept_field_text.newest());
    }

    /// Starts the window timer `id`, or starts it again when it runs.
    fn set_window_timer(&self, id: usize, delay: Duration) {
        let delay_ms = u32::try_from(delay.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: plain call on our own window.
        let is_started = unsafe { SetTimer(Some(self.window.handle()), id, delay_ms, None) } != 0;
        if !is_started {
            tracing::warn!(error = %Error::from_thread(), id, "failed to start a timer");
        }
    }

    fn kill_window_timer(&self, id: usize) {
        // Fails only when no timer is pending, which needs no handling.
        // SAFETY: plain call on our own window.
        let _ = unsafe { KillTimer(Some(self.window.handle()), id) };
    }

    /// Shows the view that waits for `read` at the caret of the answer. Does nothing for a stale
    /// read.
    pub(super) fn show_at_caret(&mut self, read: &CaretRead) {
        let Some(view) = self.pending_view.take(read.id) else {
            return;
        };
        self.show_ghost(view.ghost.as_ref(), read.caret.as_ref());
        self.show_badge(view.badge, read.caret.as_ref());
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
    /// move of the focus, a field also counts as one that hides its text until the next read of it
    /// answers.
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
            .and_then(|read| read.shown.as_ref());
        merged_surroundings(self.focus.surroundings(), field)
    }

    fn copied_text(&self) -> Option<CopiedText> {
        self.clipboard.copied_text()
    }

    fn ocr_availability(&self) -> OcrAvailability {
        self.ocr.availability()
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

    /// Asks the caret reader for the caret, then shows `ghost` and `badge` at it when the answer
    /// comes.
    fn render_at_caret(&mut self, ghost: Option<&GhostView>, badge: Option<BadgeView>) {
        let read = self.pending_view.wait(CaretView {
            ghost: ghost.cloned(),
            badge,
        });
        self.caret_reader.read(read);
    }

    fn hide_ghost(&mut self) {
        self.overlay.hide();
        self.pending_view.forget_ghost();
    }

    fn hide_badge(&mut self) {
        self.badge.hide();
        self.pending_view.forget_badge();
    }

    fn type_text(&mut self, text: String) {
        self.replace_before_caret(0, text);
    }

    fn replace_before_caret(&mut self, deleted: usize, text: String) {
        self.pending_injection = Some(Replacement { deleted, text });
        if let Err(error) = self.post(INJECT_MESSAGE) {
            tracing::warn!(%error, "failed to queue accepted text");
        }
    }

    fn start_timer(&mut self, timer: Timer, delay: Duration) {
        self.set_window_timer(timer_id(timer), delay);
    }

    fn stop_timer(&mut self, timer: Timer) {
        self.kill_window_timer(timer_id(timer));
    }

    fn render_tray(&mut self, view: &TrayView) {
        if let Err(error) = self.tray.update(view) {
            tracing::warn!(%error, "failed to update tray icon");
        }
    }

    fn render_settings(&mut self, view: &SettingsView) {
        self.settings.render(view.clone());
    }

    fn show_settings(&mut self, view: &SettingsView) {
        self.settings.show(view.clone());
    }

    fn use_clipboard_access(&mut self, access: ClipboardAccess) {
        self.clipboard.use_access(access);
    }

    fn read_screen(&mut self, read: ScreenReadId, source: ScreenSource) {
        self.screen_text.read(read, source);
    }

    /// Windows asks for no permission to capture a window, so core never asks for it.
    fn ask_for_ocr_permission(&mut self) {}

    fn quit(&mut self) {
        // SAFETY: plain call.
        unsafe { PostQuitMessage(0) };
    }
}

fn timer_id(timer: Timer) -> usize {
    match timer {
        Timer::CaretSettle => CARET_TIMER_ID,
        Timer::InputSettle => SETTLE_TIMER_ID,
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
        ..app
    }
}

#[cfg(test)]
mod tests {
    use ghosttype_core::desktop::{FieldLabels, FieldText};

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
}
