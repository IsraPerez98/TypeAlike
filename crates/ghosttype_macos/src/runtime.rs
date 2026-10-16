//! The AppKit run loop, and the one place macOS callbacks reach GhostType's state.

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::time::Duration;

use dispatch2::DispatchQueue;
use ghosttype_core::app::{BadgeView, Field, TrayView};
use ghosttype_core::controller::{
    BackendChoice, Controller, Delivery, DeliverySink, FocusChange, PaneAction, Services, SettingsView,
    Shell, Timer,
};
use ghosttype_core::desktop::{Desktop, Surroundings};
use ghosttype_core::general_pane::GeneralAction;
use ghosttype_core::inference::SystemPrompt;
use ghosttype_core::key::{Disposition, ReleaseFilter};
use ghosttype_core::models_pane::ModelsAction;
use ghosttype_core::playground::PlaygroundAction;
use ghosttype_core::settings::{ClipboardAccess, Settings};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSButton, NSControl, NSControlStateValueOn, NSMenuItem,
    NSPopUpButton, NSSearchField, NSSwitch, NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification,
    NSWorkspaceDidActivateApplicationNotification,
};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventTapProxy, CGEventType, CGKeyCode};
use objc2_foundation::{NSNotification, NSTimeInterval, NSTimer};

use crate::RunError;
use crate::badge::Badge;
use crate::event_tap::EventTap;
use crate::focus::{self, FocusTracker};
use crate::injection::{self, INJECTION_TAG};
use crate::keys::{self, Chord};
use crate::main_menu;
use crate::menu_bar::{self, MenuBarItem};
use crate::overlay::Overlay;
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
        DispatchQueue::main().exec_async(move || {
            with_runtime(|runtime| runtime.on_delivery(delivery));
        });
    });
    let controller = Controller::new(settings, services, BackendChoice::Runner, &deliver);
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
            menu_bar,
            overlay: Overlay::new(main_thread),
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
        let key = code.map(|code| keys::translate(code, chord, keys::typed_char(event)));
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

    fn on_pane_action(&mut self, action: PaneAction) {
        self.controller.on_pane_action(action, &mut self.shell);
    }

    /// Runs the action of the menu entry that `choose` reads from the settings window. For an
    /// entry with no action, renders the panes again, so that the menu shows its selected entry.
    fn on_menu_pick(&mut self, choose: impl FnOnce(&SettingsWindow) -> Option<PaneAction>) {
        match self.shell.settings.as_ref().and_then(choose) {
            Some(action) => self.on_pane_action(action),
            None => self.controller.render_settings(&mut self.shell),
        }
    }

    fn on_settings_button(&mut self, tag: isize) {
        let action = self
            .shell
            .settings
            .as_ref()
            .and_then(|settings| settings.action_for_tag(tag));
        if let Some(action) = action {
            self.on_pane_action(PaneAction::Models(action));
        }
    }

    fn on_playground_complete(&mut self) {
        let prompt = self
            .shell
            .settings
            .as_ref()
            .map(SettingsWindow::playground_prompt)
            .unwrap_or_default();
        self.on_pane_action(PaneAction::Playground(PlaygroundAction::Complete(prompt)));
    }

    fn on_system_prompt_edit(&mut self) {
        let Some(text) = self.shell.settings.as_ref().map(SettingsWindow::system_prompt) else {
            return;
        };
        self.on_pane_action(PaneAction::General(GeneralAction::EditSystemPrompt(
            SystemPrompt(text),
        )));
    }
}

/// What the controller reads and does through Accessibility and AppKit.
struct AppKitShell {
    focus: FocusTracker,
    menu_bar: MenuBarItem,
    overlay: Overlay,
    badge: Badge,
    target: Retained<RuntimeTarget>,
    caret_timer: Option<Retained<NSTimer>>,
    settle_timer: Option<Retained<NSTimer>>,
    pending_injection: Option<String>,
    settings: Option<SettingsWindow>,
}

impl AppKitShell {
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

    fn clipboard_text(&self) -> Option<String> {
        self.focus.clipboard_text()
    }
}

impl Shell for AppKitShell {
    fn refresh_focus(&mut self) -> FocusChange {
        self.focus.refresh()
    }

    fn render_at_caret(&mut self, ghost: Option<&str>, badge: Option<BadgeView>) {
        let caret = self.focus.caret();
        match (badge, caret) {
            (Some(badge), Some(caret)) => self.badge.show(&badge, caret),
            _ => self.badge.hide(),
        }
        match (ghost, caret) {
            (Some(text), Some(caret)) => self.overlay.show(text, caret),
            _ => self.overlay.hide(),
        }
    }

    fn hide_ghost(&mut self) {
        self.overlay.hide();
    }

    fn hide_badge(&mut self) {
        self.badge.hide();
    }

    fn type_text(&mut self, text: String) {
        self.pending_injection = Some(text);
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
            render_panes(settings, view);
        }
    }

    fn show_settings(&mut self, view: &SettingsView) {
        let target = &self.target;
        let settings = self.settings.get_or_insert_with(|| {
            let action_target = ActionTarget {
                target: Retained::into_super(target.clone()),
                button_action: sel!(pickSettingsButton:),
                search_action: sel!(searchModels:),
                model_picker_action: sel!(pickPlaygroundModel:),
                complete_action: sel!(completePlaygroundText:),
                autostart_action: sel!(switchAutostart:),
                clipboard_action: sel!(switchClipboard:),
                preparation_action: sel!(switchPreparation:),
                display_action: sel!(switchDisplay:),
                suggestion_length_action: sel!(pickSuggestionLength:),
                suggestion_model_action: sel!(pickSuggestionModel:),
                system_prompt_action: sel!(systemPromptDidChange:),
                reset_system_prompt_action: sel!(resetSystemPrompt:),
            };
            SettingsWindow::new(action_target, target.mtm())
        });
        render_panes(settings, view);
        settings.show();
    }

    /// macOS reads the clipboard only when core asks for its text, so it keeps no copied text.
    fn use_clipboard_access(&mut self, _access: ClipboardAccess) {}

    /// Ends the app from a timer, outside the borrow of the runtime that the end of the app drops.
    fn quit(&mut self) {
        self.schedule(NEXT_RUN_LOOP_PASS, sel!(quitApplication:));
    }
}

fn render_panes(settings: &mut SettingsWindow, view: &SettingsView) {
    settings.render_general(&view.general);
    settings.render_models(&view.models);
    settings.render_playground(&view.playground);
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

        // SAFETY: matches the action signature, called with the clicked settings button.
        #[unsafe(method(pickSettingsButton:))]
        fn pick_settings_button(&self, sender: &NSButton) {
            let tag = sender.tag();
            with_runtime(|runtime| runtime.on_settings_button(tag));
        }

        // SAFETY: matches the action signature, called with the submitted search field.
        #[unsafe(method(searchModels:))]
        fn search_models(&self, sender: &NSSearchField) {
            let action = PaneAction::Models(ModelsAction::Search(sender.stringValue().to_string()));
            with_runtime(|runtime| runtime.on_pane_action(action));
        }

        // SAFETY: matches the action signature, called with the playground's model menu.
        #[unsafe(method(pickPlaygroundModel:))]
        fn pick_playground_model(&self, sender: &NSPopUpButton) {
            let index = sender.indexOfSelectedItem();
            with_runtime(|runtime| {
                runtime.on_menu_pick(|settings| {
                    settings.playground_choice(index).map(PaneAction::Playground)
                });
            });
        }

        // SAFETY: matches the action signature, called with the General pane's autostart switch.
        #[unsafe(method(switchAutostart:))]
        fn switch_autostart(&self, sender: &NSSwitch) {
            let action = if sender.state() == NSControlStateValueOn {
                GeneralAction::EnableAutostart
            } else {
                GeneralAction::DisableAutostart
            };
            with_runtime(|runtime| runtime.on_pane_action(PaneAction::General(action)));
        }

        // SAFETY: matches the action signature, called with the General pane's clipboard switch.
        #[unsafe(method(switchClipboard:))]
        fn switch_clipboard(&self, sender: &NSSwitch) {
            let action = if sender.state() == NSControlStateValueOn {
                GeneralAction::AllowClipboard
            } else {
                GeneralAction::DenyClipboard
            };
            with_runtime(|runtime| runtime.on_pane_action(PaneAction::General(action)));
        }

        // SAFETY: matches the action signature, called with the General pane's prompt preparation switch.
        #[unsafe(method(switchPreparation:))]
        fn switch_preparation(&self, sender: &NSSwitch) {
            let action = if sender.state() == NSControlStateValueOn {
                GeneralAction::PrepareOnFocus
            } else {
                GeneralAction::PrepareOnFirstKey
            };
            with_runtime(|runtime| runtime.on_pane_action(PaneAction::General(action)));
        }

        // SAFETY: matches the action signature, called with the General pane's suggestion display switch.
        #[unsafe(method(switchDisplay:))]
        fn switch_display(&self, sender: &NSSwitch) {
            let action = if sender.state() == NSControlStateValueOn {
                GeneralAction::ShowWordByWord
            } else {
                GeneralAction::ShowWhenComplete
            };
            with_runtime(|runtime| runtime.on_pane_action(PaneAction::General(action)));
        }

        // SAFETY: matches the action signature, called with the General pane's suggestion length menu.
        #[unsafe(method(pickSuggestionLength:))]
        fn pick_suggestion_length(&self, sender: &NSPopUpButton) {
            let index = sender.indexOfSelectedItem();
            with_runtime(|runtime| {
                runtime.on_menu_pick(|settings| {
                    settings.suggestion_length_choice(index).map(PaneAction::General)
                });
            });
        }

        // SAFETY: matches the action signature, called with the General pane's suggestion model menu.
        #[unsafe(method(pickSuggestionModel:))]
        fn pick_suggestion_model(&self, sender: &NSPopUpButton) {
            let index = sender.indexOfSelectedItem();
            with_runtime(|runtime| {
                runtime.on_menu_pick(|settings| {
                    settings.suggestion_model_choice(index).map(PaneAction::Playground)
                });
            });
        }

        // SAFETY: matches the notification observer signature, called for each edit of the system prompt.
        #[unsafe(method(systemPromptDidChange:))]
        fn system_prompt_did_change(&self, _notification: &NSNotification) {
            with_runtime(Runtime::on_system_prompt_edit);
        }

        // SAFETY: matches the action signature, called with the General pane's reset button.
        #[unsafe(method(resetSystemPrompt:))]
        fn reset_system_prompt(&self, _sender: &NSButton) {
            with_runtime(|runtime| {
                runtime.on_pane_action(PaneAction::General(GeneralAction::ResetSystemPrompt));
            });
        }

        // SAFETY: matches the action signature, called with the playground's button or prompt field.
        #[unsafe(method(completePlaygroundText:))]
        fn complete_playground_text(&self, _sender: &NSControl) {
            with_runtime(Runtime::on_playground_complete);
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
            let text = with_runtime(|runtime| runtime.shell.pending_injection.take()).flatten();
            if let Some(text) = text
                && let Err(error) = injection::type_text(&text)
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
