//! The AppKit run loop, and the one place macOS callbacks reach GhostType's state.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::{self, NonNull};

use ghosttype_core::app::{App, TrayOutcome};
use ghosttype_core::engine::Reaction;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSMenuItem, NSWorkspace,
    NSWorkspaceActiveSpaceDidChangeNotification,
};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventTapProxy, CGEventType};
use objc2_foundation::{NSNotification, NSTimeInterval, NSTimer};

use crate::RunError;
use crate::event_tap::EventTap;
use crate::focus::{self, FocusChange, FocusTracker};
use crate::injection::{self, INJECTION_TAG};
use crate::keys::{self, Chord, Disposition, ReleaseFilter};
use crate::menu_bar::{self, MenuBarItem};
use crate::overlay::Overlay;

const CARET_SETTLE_DELAY: NSTimeInterval = 0.04;
const NEXT_RUN_LOOP_PASS: NSTimeInterval = 0.0;

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Shows the menu bar extra and offers suggestions until the user quits.
pub(crate) fn run(app: App) -> Result<(), RunError> {
    let main_thread = MainThreadMarker::new().ok_or(RunError::NotMainThread)?;
    let application = NSApplication::sharedApplication(main_thread);
    application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    if !focus::request_accessibility_access() {
        tracing::warn!("accessibility access is not granted yet");
    }
    let tap = EventTap::install(Some(tap_callback)).ok_or(RunError::KeyboardAccess)?;
    let target = RuntimeTarget::new(main_thread);
    let menu_bar = MenuBarItem::add(
        &app.tray_view(),
        Retained::into_super(target.clone()),
        sel!(pickMenuItem:),
        main_thread,
    );
    observe_space_changes(&target);
    RUNTIME.set(Some(Runtime {
        app,
        focus: FocusTracker::new(),
        menu_bar,
        overlay: Overlay::new(main_thread),
        tap,
        target,
        ghost_timer: None,
        pending_ghost: None,
        pending_injection: None,
        release_filter: ReleaseFilter::default(),
    }));
    tracing::info!("running in the menu bar");
    application.run();
    RUNTIME.take();
    Ok(())
}

fn observe_space_changes(target: &RuntimeTarget) {
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: the target implements the selector, and the center drops deallocated observers itself.
    unsafe {
        center.addObserver_selector_name_object(
            target,
            sel!(activeSpaceDidChange:),
            Some(NSWorkspaceActiveSpaceDidChangeNotification),
            None,
        );
    }
}

fn with_runtime<Output>(action: impl FnOnce(&mut Runtime) -> Output) -> Option<Output> {
    RUNTIME.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(action))
}

struct Runtime {
    app: App,
    focus: FocusTracker,
    menu_bar: MenuBarItem,
    overlay: Overlay,
    tap: EventTap,
    target: Retained<RuntimeTarget>,
    ghost_timer: Option<Retained<NSTimer>>,
    pending_ghost: Option<String>,
    pending_injection: Option<String>,
    release_filter: ReleaseFilter,
}

impl Runtime {
    fn on_event(&mut self, kind: CGEventType, event: &CGEvent) -> Disposition {
        if CGEvent::integer_value_field(Some(event), CGEventField::EventSourceUserData) == INJECTION_TAG {
            return Disposition::Pass;
        }
        match kind {
            CGEventType::KeyDown => self.on_key_down(event),
            CGEventType::KeyUp => {
                keys::key_code(event).map_or(Disposition::Pass, |key| self.release_filter.on_release(key))
            }
            CGEventType::LeftMouseDown | CGEventType::RightMouseDown | CGEventType::OtherMouseDown => {
                self.on_context_lost();
                Disposition::Pass
            }
            _ => Disposition::Pass,
        }
    }

    fn on_key_down(&mut self, event: &CGEvent) -> Disposition {
        if self.focus.refresh() == FocusChange::Changed {
            self.on_context_lost();
        }
        let Some(key) = keys::key_code(event) else {
            return Disposition::Pass;
        };
        let chord = Chord::from_flags(CGEvent::flags(Some(event)));
        let core_key = keys::translate(key, chord, keys::typed_char(event));
        let reaction = self.app.on_key(core_key, self.focus.field());
        let disposition = self.apply(reaction);
        self.release_filter.on_press(key, disposition);
        disposition
    }

    fn on_context_lost(&mut self) {
        let reaction = self.app.on_context_lost();
        self.apply(reaction);
    }

    fn apply(&mut self, reaction: Reaction) -> Disposition {
        match reaction {
            Reaction::Show(text) => {
                self.pending_ghost = Some(text);
                self.restart_ghost_timer();
                Disposition::Pass
            }
            Reaction::Hide => {
                self.hide_ghost();
                Disposition::Pass
            }
            Reaction::Accept(text) => {
                self.hide_ghost();
                self.pending_injection = Some(text);
                self.schedule(NEXT_RUN_LOOP_PASS, sel!(injectPendingText:));
                Disposition::Swallow
            }
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

    fn restart_ghost_timer(&mut self) {
        self.stop_ghost_timer();
        self.ghost_timer = Some(self.schedule(CARET_SETTLE_DELAY, sel!(showPendingGhost:)));
    }

    fn stop_ghost_timer(&mut self) {
        if let Some(timer) = self.ghost_timer.take() {
            timer.invalidate();
        }
    }

    fn hide_ghost(&mut self) {
        self.pending_ghost = None;
        self.stop_ghost_timer();
        self.overlay.hide();
    }

    fn show_pending_ghost(&mut self) {
        self.ghost_timer = None;
        let Some(text) = &self.pending_ghost else {
            return;
        };
        match self.focus.caret() {
            Some(caret) => self.overlay.show(text, caret),
            None => self.overlay.hide(),
        }
    }

    fn on_menu_item(&mut self, tag: isize) -> Option<TrayOutcome> {
        let command = menu_bar::command_for_tag(&self.app.tray_view().items, tag)?;
        let outcome = self.app.on_tray_command(command);
        if outcome == TrayOutcome::Updated {
            self.hide_ghost();
            self.menu_bar.update(&self.app.tray_view());
        }
        Some(outcome)
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
            let outcome = with_runtime(|runtime| runtime.on_menu_item(tag)).flatten();
            if outcome == Some(TrayOutcome::Quit) {
                NSApplication::sharedApplication(self.mtm()).terminate(None);
            }
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(showPendingGhost:))]
        fn show_pending_ghost(&self, _timer: &NSTimer) {
            with_runtime(Runtime::show_pending_ghost);
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(injectPendingText:))]
        fn inject_pending_text(&self, _timer: &NSTimer) {
            let text = with_runtime(|runtime| runtime.pending_injection.take()).flatten();
            if let Some(text) = text
                && let Err(error) = injection::type_text(&text)
            {
                tracing::warn!(%error, "failed to type accepted text");
            }
        }

        // SAFETY: matches the notification observer signature.
        #[unsafe(method(activeSpaceDidChange:))]
        fn active_space_did_change(&self, _notification: &NSNotification) {
            with_runtime(Runtime::on_context_lost);
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
