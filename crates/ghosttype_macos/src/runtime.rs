//! The AppKit run loop, and the one place macOS callbacks reach GhostType's state.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dispatch2::DispatchQueue;
use ghosttype_core::app::{BadgeView, Field, TrayView};
use ghosttype_core::controller::{
    BackendChoice, Controller, Delivery, DeliverySink, FocusChange, Ghost, GhostPlacement, GhostView,
    Services, Shell, Timer,
};
use ghosttype_core::desktop::{
    CopiedText, Desktop, OcrAvailability, OcrLevels, ScreenReadId, ScreenSource, Surroundings,
};
use ghosttype_core::inference::Backend;
use ghosttype_core::key::{Disposition, ReleaseFilter};
use ghosttype_core::settings::{ClipboardAccess, Settings};
use ghosttype_core::settings_view::{SettingsInput, SettingsView};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSButton, NSControl, NSMenuItem, NSPopUpButton, NSSwitch,
    NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification, NSWorkspaceDidActivateApplicationNotification,
};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventTapProxy, CGEventType, CGKeyCode};
use objc2_foundation::{NSNotification, NSTimeInterval, NSTimer};

use crate::RunError;
use crate::badge::Badge;
use crate::clipboard::ClipboardWatch;
use crate::event_tap::EventTap;
use crate::focus::{self, Caret, FocusTracker};
use crate::injection::{self, INJECTION_TAG, Replacement};
use crate::keys::{self, Chord};
use crate::main_menu;
use crate::menu_bar::{self, MenuBarItem};
use crate::mirror::Mirror;
use crate::ocr;
use crate::overlay::Overlay;
use crate::screen_text::ScreenTextReader;
use crate::settings::{ActionTarget, SettingsWindow};

const NEXT_RUN_LOOP_PASS: NSTimeInterval = 0.0;

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Shows the menu bar extra and offers suggestions from the runner of `services` until the user
/// quits. GhostType starts with the saved `settings`.
pub(crate) fn run(settings: Settings, services: Services) -> Result<(), RunError> {
    let main_thread = MainThreadMarker::new().ok_or(RunError::NotMainThread)?;
    let application = NSApplication::sharedApplication(main_thread);
    application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    if !focus::request_accessibility_access() {
        tracing::warn!("accessibility access is not granted yet");
    }
    let tap = EventTap::install(Some(tap_callback)).ok_or(RunError::KeyboardAccess)?;
    let target = RuntimeTarget::new(main_thread);
    let deliver: DeliverySink = Arc::new(|delivery| {
        let sent = Instant::now();
        DispatchQueue::main().exec_async(move || {
            if let Delivery::Screen(read) = &delivery {
                tracing::debug!(read = ?read.id, trip = ?sent.elapsed(), "screen read reached the main thread");
            }
            with_runtime(|runtime| runtime.on_delivery(delivery));
        });
    });
    let controller = Controller::new(
        settings,
        services,
        BackendChoice::Platform(Backend::Metal),
        &deliver,
    );
    let menu_bar = MenuBarItem::add(
        &controller.tray_view(),
        Retained::into_super(target.clone()),
        sel!(pickMenuItem:),
        main_thread,
    );
    observe_workspace(&target);
    main_menu::install(main_thread);
    RUNTIME.set(Some(Runtime {
        controller,
        shell: AppKitShell {
            focus: FocusTracker::new(),
            screen_text: ScreenTextReader::new(Arc::clone(&deliver)),
            clipboard: ClipboardWatch::start(),
            menu_bar,
            overlay: Overlay::new(main_thread),
            mirror: Mirror::new(main_thread),
            badge: Badge::new(main_thread),
            target,
            caret_timer: None,
            settle_timer: None,
            pending_injection: None,
            settings: None,
        },
        tap,
        release_filter: ReleaseFilter::default(),
        is_badge_pressed: false,
    }));
    tracing::info!("running in the menu bar");
    application.run();
    RUNTIME.take();
    Ok(())
}

fn observe_workspace(target: &RuntimeTarget) {
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: the target implements each selector, and the center drops deallocated observers itself.
    unsafe {
        center.addObserver_selector_name_object(
            target,
            sel!(activeSpaceDidChange:),
            Some(NSWorkspaceActiveSpaceDidChangeNotification),
            None,
        );
        center.addObserver_selector_name_object(
            target,
            sel!(applicationDidActivate:),
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
        );
    }
}

fn with_runtime<Output>(action: impl FnOnce(&mut Runtime) -> Output) -> Option<Output> {
    RUNTIME.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(action))
}

struct Runtime {
    controller: Controller,
    shell: AppKitShell,
    tap: EventTap,
    release_filter: ReleaseFilter<CGKeyCode>,
    is_badge_pressed: bool,
}

impl Runtime {
    fn on_event(&mut self, kind: CGEventType, event: &CGEvent) -> Disposition {
        if CGEvent::integer_value_field(Some(event), CGEventField::EventSourceUserData) == INJECTION_TAG {
            return Disposition::Pass;
        }
        self.shell.clipboard.on_input();
        self.controller.on_input(&mut self.shell);
        match kind {
            CGEventType::KeyDown => self.on_key_down(event),
            CGEventType::KeyUp => {
                keys::key_code(event).map_or(Disposition::Pass, |key| self.release_filter.on_release(key))
            }
            CGEventType::LeftMouseDown if self.shell.badge.contains(CGEvent::location(Some(event))) => {
                self.is_badge_pressed = true;
                self.controller.on_badge_click(&mut self.shell);
                Disposition::Swallow
            }
            CGEventType::LeftMouseUp if mem::take(&mut self.is_badge_pressed) => Disposition::Swallow,
            CGEventType::LeftMouseDown | CGEventType::RightMouseDown | CGEventType::OtherMouseDown => {
                self.controller.on_context_lost(&mut self.shell);
                Disposition::Pass
            }
            _ => Disposition::Pass,
        }
    }

    fn on_key_down(&mut self, event: &CGEvent) -> Disposition {
        let code = keys::key_code(event);
        let chord = Chord::from_flags(CGEvent::flags(Some(event)));
        let key = code.map(|code| keys::translate(code, chord, keys::typed(event)));
        let disposition = self.controller.on_key_down(key, &mut self.shell);
        if let Some(code) = code {
            self.release_filter.on_press(code, disposition);
        }
        disposition
    }

    fn on_caret_settled(&mut self) {
        self.shell.caret_timer = None;
        self.controller.on_timer(Timer::CaretSettle, &mut self.shell);
    }

    fn on_input_settled(&mut self) {
        self.shell.settle_timer = None;
        self.controller.on_timer(Timer::InputSettle, &mut self.shell);
    }

    fn on_space_change(&mut self) {
        self.controller.on_context_lost(&mut self.shell);
        self.controller.on_input(&mut self.shell);
    }

    fn on_app_activation(&mut self) {
        self.controller.on_input(&mut self.shell);
    }

    fn on_menu_item(&mut self, tag: isize) {
        if let Some(command) = menu_bar::command_for_tag(&self.controller.tray_view().items, tag) {
            self.controller.on_tray_command(command, &mut self.shell);
        }
    }

    fn on_delivery(&mut self, delivery: Delivery) {
        self.controller.on_delivery(delivery, &mut self.shell);
    }

    /// Handles the input that `read` reads from the settings window, if any.
    fn on_settings_control(&mut self, read: impl FnOnce(&SettingsWindow) -> Option<SettingsInput>) {
        if let Some(input) = self.shell.settings.as_ref().and_then(read) {
            self.controller.on_settings_input(&input, &mut self.shell);
        }
    }
}

/// What the controller reads and does through Accessibility and AppKit.
struct AppKitShell {
    focus: FocusTracker,
    screen_text: ScreenTextReader,
    clipboard: ClipboardWatch,
    menu_bar: MenuBarItem,
    overlay: Overlay,
    mirror: Mirror,
    badge: Badge,
    target: Retained<RuntimeTarget>,
    caret_timer: Option<Retained<NSTimer>>,
    settle_timer: Option<Retained<NSTimer>>,
    pending_injection: Option<Replacement>,
    settings: Option<SettingsWindow>,
}

impl AppKitShell {
    fn show_at_caret(&self, view: &GhostView, caret: Caret) {
        if view.placement == GhostPlacement::BelowCaret {
            self.overlay.hide();
            self.mirror.show_below_caret(&view.ghost, caret);
            return;
        }
        self.mirror.hide();
        match &view.ghost {
            Ghost::Text(text) => self.overlay.show(text, caret),
            Ghost::Fix(fix) => self.overlay.show_fix(fix, caret),
        }
    }

    /// Shows `view` in the mirror next to the focused field, which gives no caret. Hides it when
    /// the focused control holds no text.
    fn show_mirror(&self, view: &GhostView) {
        self.overlay.hide();
        match self.focus.field_frame() {
            Some(field) => self.mirror.show(view, field),
            None => self.mirror.hide(),
        }
    }

    fn schedule(&self, delay: NSTimeInterval, action: Sel) -> Retained<NSTimer> {
        // SAFETY: every action passed here is a method of the target that takes the timer.
        unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                delay,
                &self.target,
                action,
                None,
                false,
            )
        }
    }

    fn timer_slot(&mut self, timer: Timer) -> &mut Option<Retained<NSTimer>> {
        match timer {
            Timer::CaretSettle => &mut self.caret_timer,
            Timer::InputSettle => &mut self.settle_timer,
        }
    }
}

impl Desktop for AppKitShell {
    fn field(&self) -> Field {
        self.focus.field()
    }

    fn surroundings(&self) -> Surroundings {
        self.focus.surroundings()
    }

    fn copied_text(&self) -> Option<CopiedText> {
        self.clipboard.copied_text()
    }

    fn ocr_availability(&self) -> OcrAvailability {
        ocr::availability()
    }

    /// Vision reads at the level `.fast` or `.accurate`.
    fn ocr_levels(&self) -> OcrLevels {
        OcrLevels::Selectable
    }

    /// The ghost text shows at the caret, else in the mirror next to a field without a caret.
    fn is_ghost_shown(&self) -> bool {
        self.overlay.is_shown() || self.mirror.is_shown()
    }
}

impl Shell for AppKitShell {
    fn refresh_focus(&mut self) -> FocusChange {
        self.focus.refresh()
    }

    fn render_at_caret(&mut self, ghost: Option<&GhostView>, badge: Option<BadgeView>) {
        let caret = self.focus.caret();
        match (badge, caret) {
            (Some(badge), Some(caret)) => self.badge.show(&badge, caret),
            _ => self.badge.hide(),
        }
        match (ghost, caret) {
            (Some(view), Some(caret)) => self.show_at_caret(view, caret),
            (Some(view), None) => self.show_mirror(view),
            (None, _) => self.hide_ghost(),
        }
    }

    fn hide_ghost(&mut self) {
        self.overlay.hide();
        self.mirror.hide();
    }

    fn hide_badge(&mut self) {
        self.badge.hide();
    }

    fn type_text(&mut self, text: String) {
        self.replace_before_caret(0, text);
    }

    fn replace_before_caret(&mut self, deleted: usize, text: String) {
        self.pending_injection = Some(Replacement { deleted, text });
        self.schedule(NEXT_RUN_LOOP_PASS, sel!(injectPendingText:));
    }

    fn start_timer(&mut self, timer: Timer, delay: Duration) {
        self.stop_timer(timer);
        let action = match timer {
            Timer::CaretSettle => sel!(caretDidSettle:),
            Timer::InputSettle => sel!(inputDidSettle:),
        };
        let scheduled = self.schedule(delay.as_secs_f64(), action);
        *self.timer_slot(timer) = Some(scheduled);
    }

    fn stop_timer(&mut self, timer: Timer) {
        if let Some(scheduled) = self.timer_slot(timer).take() {
            scheduled.invalidate();
        }
    }

    fn render_tray(&mut self, view: &TrayView) {
        self.menu_bar.update(view);
    }

    fn render_settings(&mut self, view: &SettingsView) {
        if let Some(settings) = &mut self.settings {
            settings.render(view);
        }
    }

    fn show_settings(&mut self, view: &SettingsView) {
        let target = &self.target;
        let settings = self.settings.get_or_insert_with(|| {
            let action_target = ActionTarget {
                target: Retained::into_super(target.clone()),
                submit_action: sel!(submitSetting:),
                toggle_action: sel!(toggleSetting:),
                pick_action: sel!(pickSetting:),
                press_action: sel!(pressSetting:),
                edit_action: sel!(editSetting:),
            };
            SettingsWindow::new(action_target, view, target.mtm())
        });
        settings.render(view);
        settings.show();
    }

    /// macOS reads the clipboard only when core asks for its text, so it keeps no copied text.
    fn use_clipboard_access(&mut self, _access: ClipboardAccess) {}

    fn read_screen(&mut self, read: ScreenReadId, source: ScreenSource) {
        self.screen_text
            .read(read, source, self.focus.frontmost_process());
    }

    fn ask_for_ocr_permission(&mut self) {
        ocr::ask_for_permission();
    }

    /// Ends the app from a timer, outside the borrow of the runtime that the end of the app drops.
    fn quit(&mut self) {
        self.schedule(NEXT_RUN_LOOP_PASS, sel!(quitApplication:));
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class does not implement Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "GhostTypeRuntimeTarget"]
    struct RuntimeTarget;

    impl RuntimeTarget {
        // SAFETY: matches the action signature, called with the picked menu item.
        #[unsafe(method(pickMenuItem:))]
        fn pick_menu_item(&self, sender: &NSMenuItem) {
            let tag = sender.tag();
            with_runtime(|runtime| runtime.on_menu_item(tag));
        }

        // SAFETY: matches the action signature, called with a switch of a settings page.
        #[unsafe(method(toggleSetting:))]
        fn toggle_setting(&self, sender: &NSSwitch) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.toggled(sender)));
        }

        // SAFETY: matches the action signature, called with a menu of a settings page.
        #[unsafe(method(pickSetting:))]
        fn pick_setting(&self, sender: &NSPopUpButton) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.picked(sender)));
        }

        // SAFETY: matches the action signature, called with a button of a settings page.
        #[unsafe(method(pressSetting:))]
        fn press_setting(&self, sender: &NSButton) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.pressed(sender)));
        }

        // SAFETY: matches the notification observer signature, called for each change of the text
        // of a text view.
        #[unsafe(method(editSetting:))]
        fn edit_setting(&self, notification: &NSNotification) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.edited(notification)));
        }

        // SAFETY: matches the action signature, called with a text field of a settings page or its
        // button.
        #[unsafe(method(submitSetting:))]
        fn submit_setting(&self, sender: &NSControl) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.submitted(sender)));
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(caretDidSettle:))]
        fn caret_did_settle(&self, _timer: &NSTimer) {
            with_runtime(Runtime::on_caret_settled);
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(inputDidSettle:))]
        fn input_did_settle(&self, _timer: &NSTimer) {
            with_runtime(Runtime::on_input_settled);
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(injectPendingText:))]
        fn inject_pending_text(&self, _timer: &NSTimer) {
            let replacement = with_runtime(|runtime| runtime.shell.pending_injection.take()).flatten();
            if let Some(replacement) = replacement
                && let Err(error) = injection::replace_text(&replacement)
            {
                tracing::warn!(%error, "failed to type accepted text");
            }
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(quitApplication:))]
        fn quit_application(&self, _timer: &NSTimer) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }

        // SAFETY: matches the notification observer signature.
        #[unsafe(method(activeSpaceDidChange:))]
        fn active_space_did_change(&self, _notification: &NSNotification) {
            with_runtime(Runtime::on_space_change);
        }

        // SAFETY: matches the notification observer signature.
        #[unsafe(method(applicationDidActivate:))]
        fn application_did_activate(&self, _notification: &NSNotification) {
            with_runtime(Runtime::on_app_activation);
        }
    }
);

impl RuntimeTarget {
    fn new(main_thread: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(());
        // SAFETY: NSObject's init takes no arguments and returns an initialized object.
        unsafe { msg_send![super(this), init] }
    }
}

extern "C-unwind" fn tap_callback(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    _user_info: *mut c_void,
) -> *mut CGEvent {
    if kind == CGEventType::TapDisabledByTimeout {
        with_runtime(|runtime| runtime.tap.enable());
        return event.as_ptr();
    }
    // SAFETY: the tap passes an event that stays valid for the whole callback.
    let event_ref = unsafe { event.as_ref() };
    match with_runtime(|runtime| runtime.on_event(kind, event_ref)) {
        Some(Disposition::Swallow) => ptr::null_mut(),
        Some(Disposition::Pass) | None => event.as_ptr(),
    }
}
