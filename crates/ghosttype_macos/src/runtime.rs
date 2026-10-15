//! The AppKit run loop, and the one place macOS callbacks reach GhostType's state.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::Arc;

use dispatch2::DispatchQueue;
use ghosttype_core::app::{App, TrayOutcome};
use ghosttype_core::engine::Reaction;
use ghosttype_core::general_pane::{GeneralAction, GeneralPane};
use ghosttype_core::hub::{HubEvent, HubEventSink, ModelHub};
use ghosttype_core::inference::{InferenceEvent, InferenceEventSink, ModelRunner, Prompt};
use ghosttype_core::models_pane::{ModelsAction, ModelsPane};
use ghosttype_core::playground::{PlaygroundAction, PlaygroundPane};
use ghosttype_core::settings::{Settings, SettingsStore};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSButton, NSControl, NSControlStateValueOn, NSMenuItem,
    NSPopUpButton, NSSearchField, NSSwitch, NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification,
};
use objc2_core_graphics::{CGEvent, CGEventField, CGEventTapProxy, CGEventType};
use objc2_foundation::{NSNotification, NSTimeInterval, NSTimer};

use crate::event_tap::EventTap;
use crate::focus::{self, FocusChange, FocusTracker};
use crate::injection::{self, INJECTION_TAG};
use crate::keys::{self, Chord, Disposition, ReleaseFilter};
use crate::main_menu;
use crate::menu_bar::{self, MenuBarItem};
use crate::overlay::Overlay;
use crate::settings::{ActionTarget, SettingsWindow};
use crate::{RunError, Services};

const CARET_SETTLE_DELAY: NSTimeInterval = 0.04;
const NEXT_RUN_LOOP_PASS: NSTimeInterval = 0.0;

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Shows the menu bar extra and offers suggestions from the runner of `services` until the user
/// quits. The settings window starts with `settings` and uses `services`.
pub(crate) fn run(app: App, settings: Settings, services: Services) -> Result<(), RunError> {
    let Services {
        hub,
        mut runner,
        settings_store,
        autostart,
    } = services;
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
    main_menu::install(main_thread);
    let general = GeneralPane::new(settings)
        .without_backend_menu()
        .with_autostart(autostart);
    let mut playground = PlaygroundPane::new();
    let runner_sink = main_thread_sink(Runtime::on_inference_event);
    if let Some(request) = general.load_suggestion_model(&mut playground) {
        runner.submit(request, &runner_sink);
    }
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
        models: ModelsPane::new(),
        hub,
        hub_sink: main_thread_sink(Runtime::on_hub_event),
        playground,
        general,
        settings_store,
        runner,
        runner_sink,
        suggestion_sink: main_thread_sink(Runtime::on_suggestion_event),
        settings: None,
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

/// Returns a sink that hands each event to `handle` on the main thread.
fn main_thread_sink<Event: Send + 'static>(
    handle: fn(&mut Runtime, Event),
) -> Arc<dyn Fn(Event) + Send + Sync> {
    Arc::new(move |event: Event| {
        DispatchQueue::main().exec_async(move || {
            with_runtime(|runtime| handle(runtime, event));
        });
    })
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
    models: ModelsPane,
    hub: Box<dyn ModelHub>,
    hub_sink: HubEventSink,
    playground: PlaygroundPane,
    general: GeneralPane,
    settings_store: Box<dyn SettingsStore>,
    runner: Box<dyn ModelRunner>,
    runner_sink: InferenceEventSink,
    suggestion_sink: InferenceEventSink,
    settings: Option<SettingsWindow>,
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
                self.show_ghost_after_caret_settles(text);
                Disposition::Pass
            }
            Reaction::Hide => {
                self.hide_ghost();
                Disposition::Pass
            }
            Reaction::Accept { text, rest } => {
                self.hide_ghost();
                self.pending_injection = Some(text);
                self.schedule(NEXT_RUN_LOOP_PASS, sel!(injectPendingText:));
                if let Some(rest) = rest {
                    self.show_ghost_after_caret_settles(rest);
                }
                Disposition::Swallow
            }
            Reaction::Request(request) => {
                self.hide_ghost();
                self.runner.submit(request, &self.suggestion_sink);
                Disposition::Pass
            }
        }
    }

    fn on_suggestion_event(&mut self, event: InferenceEvent) {
        if let InferenceEvent::Completed(result) = event {
            let reaction = self.app.on_completion(result);
            self.apply(reaction);
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

    fn show_ghost_after_caret_settles(&mut self, text: String) {
        self.pending_ghost = Some(text);
        self.restart_ghost_timer();
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
        match outcome {
            TrayOutcome::Updated => {
                self.hide_ghost();
                self.menu_bar.update(&self.app.tray_view());
            }
            TrayOutcome::OpenSettings => self.open_settings(),
            TrayOutcome::Quit => {}
        }
        Some(outcome)
    }

    fn open_settings(&mut self) {
        let request = self.models.on_open();
        self.hub.submit(request, &self.hub_sink);
        let target = &self.target;
        let settings = self.settings.get_or_insert_with(|| {
            let action_target = ActionTarget {
                target: Retained::into_super(target.clone()),
                button_action: sel!(pickSettingsButton:),
                search_action: sel!(searchModels:),
                model_picker_action: sel!(pickPlaygroundModel:),
                complete_action: sel!(completePlaygroundText:),
                autostart_action: sel!(switchAutostart:),
                suggestion_model_action: sel!(pickSuggestionModel:),
            };
            SettingsWindow::new(action_target, target.mtm())
        });
        render_panes(settings, &self.models, &self.playground, &self.general);
        settings.show();
    }

    fn on_settings_button(&mut self, tag: isize) {
        let action = self
            .settings
            .as_ref()
            .and_then(|settings| settings.action_for_tag(tag));
        if let Some(action) = action {
            self.on_models_action(action);
        }
    }

    fn on_models_action(&mut self, action: ModelsAction) {
        if let Some(request) = self.models.on_action(action) {
            self.hub.submit(request, &self.hub_sink);
        }
        self.render_settings();
    }

    fn on_hub_event(&mut self, event: HubEvent) {
        self.models.on_event(event);
        self.render_settings();
    }

    fn on_playground_model_picked(&mut self, index: isize) {
        let action = self
            .settings
            .as_ref()
            .and_then(|settings| settings.playground_choice(index));
        match action {
            Some(action) => self.on_playground_action(action),
            None => self.render_settings(),
        }
    }

    fn on_playground_complete(&mut self) {
        let prompt = self
            .settings
            .as_ref()
            .map(SettingsWindow::playground_prompt)
            .unwrap_or_default();
        self.on_playground_action(PlaygroundAction::Complete(Prompt(prompt)));
    }

    fn on_playground_action(&mut self, action: PlaygroundAction) {
        if let Some(request) = self.playground.on_action(action) {
            self.runner.submit(request, &self.runner_sink);
        }
        self.render_settings();
    }

    fn on_suggestion_model_picked(&mut self, index: isize) {
        let action = self
            .settings
            .as_ref()
            .and_then(|settings| settings.suggestion_model_choice(index));
        match action {
            Some(action) => self.on_playground_action(action),
            None => self.render_settings(),
        }
    }

    fn on_general_action(&mut self, action: GeneralAction) {
        let outcome = self.general.on_action(action, &mut self.playground);
        if let Some(settings) = outcome.settings {
            self.settings_store.save(&settings);
        }
        if let Some(request) = outcome.request {
            self.runner.submit(request, &self.runner_sink);
        }
        self.render_settings();
    }

    fn on_inference_event(&mut self, event: InferenceEvent) {
        self.playground.on_event(event);
        if let Some(settings) = self.general.keep_suggestion_model(&self.playground) {
            self.settings_store.save(&settings);
        }
        self.render_settings();
    }

    fn render_settings(&mut self) {
        if let Some(settings) = &mut self.settings {
            render_panes(settings, &self.models, &self.playground, &self.general);
        }
    }
}

fn render_panes(
    settings: &mut SettingsWindow,
    models: &ModelsPane,
    playground: &PlaygroundPane,
    general: &GeneralPane,
) {
    let installed = models.installed_files();
    settings.render_general(&general.view(playground, &installed));
    settings.render_models(&models.view());
    settings.render_playground(&playground.view(&installed));
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

        // SAFETY: matches the action signature, called with the clicked settings button.
        #[unsafe(method(pickSettingsButton:))]
        fn pick_settings_button(&self, sender: &NSButton) {
            let tag = sender.tag();
            with_runtime(|runtime| runtime.on_settings_button(tag));
        }

        // SAFETY: matches the action signature, called with the submitted search field.
        #[unsafe(method(searchModels:))]
        fn search_models(&self, sender: &NSSearchField) {
            let query = sender.stringValue().to_string();
            with_runtime(|runtime| runtime.on_models_action(ModelsAction::Search(query)));
        }

        // SAFETY: matches the action signature, called with the playground's model menu.
        #[unsafe(method(pickPlaygroundModel:))]
        fn pick_playground_model(&self, sender: &NSPopUpButton) {
            let index = sender.indexOfSelectedItem();
            with_runtime(|runtime| runtime.on_playground_model_picked(index));
        }

        // SAFETY: matches the action signature, called with the General pane's autostart switch.
        #[unsafe(method(switchAutostart:))]
        fn switch_autostart(&self, sender: &NSSwitch) {
            let action = if sender.state() == NSControlStateValueOn {
                GeneralAction::EnableAutostart
            } else {
                GeneralAction::DisableAutostart
            };
            with_runtime(|runtime| runtime.on_general_action(action));
        }

        // SAFETY: matches the action signature, called with the General pane's suggestion model menu.
        #[unsafe(method(pickSuggestionModel:))]
        fn pick_suggestion_model(&self, sender: &NSPopUpButton) {
            let index = sender.indexOfSelectedItem();
            with_runtime(|runtime| runtime.on_suggestion_model_picked(index));
        }

        // SAFETY: matches the action signature, called with the playground's button or prompt field.
        #[unsafe(method(completePlaygroundText:))]
        fn complete_playground_text(&self, _sender: &NSControl) {
            with_runtime(Runtime::on_playground_complete);
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
