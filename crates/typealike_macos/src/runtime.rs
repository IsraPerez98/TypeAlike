//! The AppKit run loop, and the one place macOS callbacks reach TypeAlike's state.

mod target;

use std::cell::{OnceCell, RefCell};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dispatch2::DispatchQueue;
use objc2::rc::Retained;
use objc2::runtime::Sel;
use objc2::{MainThreadMarker, MainThreadOnly, sel};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy, NSMenu, NSWorkspace};
use objc2_core_foundation::CGRect;
use objc2_core_graphics::{CGEvent, CGEventType, CGKeyCode};
use objc2_foundation::{NSPoint, NSString, NSTimeInterval, NSTimer, NSURL};
use typealike_core::app::{BadgeMenu, BadgeView, TrayView};
use typealike_core::controller::{
    BackendChoice, CaretLook, CaretRead, Controller, Delivery, DeliverySink, FocusChange, Ghost,
    GhostPlacement, GhostView, PlaygroundPage, Services, Shell, Startup, Timer,
};
use typealike_core::desktop::{
    CopiedText, Desktop, EnergySaver, Field, OcrAvailability, OcrLevels, Permission, PermissionState,
    ScreenReadId, ScreenSource, Surroundings, Timestamp,
};
use typealike_core::inference::Backend;
use typealike_core::key::{BadgeClickFilter, Disposition, Key, ReleaseFilter};
use typealike_core::settings::{ClipboardAccess, Settings};
use typealike_core::settings_window::view::{SettingsInput, SettingsView};
use typealike_core::setup::SetupView;
use typealike_core::setup::TrayPlace;

use crate::RunError;
use crate::accessibility;
use crate::badge::Badge;
use crate::badge_menu;
use crate::clipboard::ClipboardWatch;
use crate::energy_saver::{self, EnergySaverObserver};
use crate::event_tap::EventTap;
use crate::focus::{FocusTracker, TextStyle};
use crate::full_screen;
use crate::game;
use crate::geometry::{Caret, screen_rect, whole_points_up};
use crate::injection::{self, Replacement};
use crate::keys;
use crate::language::ui_language;
use crate::main_menu;
use crate::menu_bar::{self, MenuBarItem};
use crate::mirror::Mirror;
use crate::ocr;
use crate::overlay::{self, Overlay};
use crate::permissions;
use crate::runtime::target::{RuntimeTarget, observe_workspace};
use crate::screen_text::ScreenTextReader;
use crate::settings::{ActionTarget, SettingsWindow, SetupActions, SetupWindow};

const NEXT_RUN_LOOP_PASS: NSTimeInterval = 0.0;
const ACCESSIBILITY_CHECK_INTERVAL: NSTimeInterval = 1.0;

thread_local! {
    static RUNTIME: RefCell<Option<Runtime>> = const { RefCell::new(None) };
}

/// Shows the menu bar extra and offers suggestions from the runner of `services` until the user
/// quits. TypeAlike starts with the saved `settings`.
pub(crate) fn run(settings: Settings, services: Services) -> Result<(), RunError> {
    let main_thread = MainThreadMarker::new().ok_or(RunError::NotMainThread)?;
    let application = NSApplication::sharedApplication(main_thread);
    application.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    let tap = EventTap::install(on_tap_event);
    let target = RuntimeTarget::new(main_thread);
    let deliver: DeliverySink = Arc::new(deliver_on_main_thread);
    let energy_saver_observer = EnergySaverObserver::start(on_energy_saver_change);
    let startup = Startup {
        backend_choice: BackendChoice::Platform(Backend::Metal),
        energy_saver: energy_saver::read(),
        ui_language: ui_language(),
        tray: TrayPlace::MenuBar,
        playground: PlaygroundPage::of_build(),
    };
    tracing::info!(energy_saver = ?startup.energy_saver, "read Low Power Mode");
    let controller = Controller::new(settings, services, startup, &deliver);
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
            caret: None,
            caret_text_style: OnceCell::new(),
            field_frame: None,
            target,
            caret_timer: None,
            settle_timer: None,
            tab_hold_timer: None,
            idle_timer: None,
            field_pause_timer: None,
            scroll_timer: None,
            update_check_timer: None,
            pending_injections: VecDeque::new(),
            settings: None,
            setup: None,
        },
        tap,
        release_filter: ReleaseFilter::default(),
        badge_click: BadgeClickFilter::default(),
        badge_menu: None,
    }));
    with_runtime(Runtime::read_game);
    with_runtime(Runtime::read_full_screen);
    with_runtime(|runtime| runtime.shell.schedule(NEXT_RUN_LOOP_PASS, sel!(finishLaunch:)));
    tracing::info!("running in the menu bar");
    application.run();
    drop(energy_saver_observer);
    RUNTIME.take();
    Ok(())
}

/// Gives the new state of Low Power Mode to the runtime, on the main thread.
fn on_energy_saver_change(energy_saver: EnergySaver) {
    with_runtime(|runtime| runtime.on_energy_saver(energy_saver));
}

/// Gives `delivery` to the runtime on the main thread, from any thread.
fn deliver_on_main_thread(delivery: Delivery) {
    let sent = Instant::now();
    DispatchQueue::main().exec_async(move || {
        if let Delivery::Screen(read) = &delivery {
            tracing::debug!(read = ?read.id, trip = ?sent.elapsed(), "screen read reached the main thread");
        }
        with_runtime(|runtime| runtime.on_delivery(delivery));
    });
}

fn with_runtime<Output>(action: impl FnOnce(&mut Runtime) -> Output) -> Option<Output> {
    RUNTIME.with(|cell| cell.try_borrow_mut().ok()?.as_mut().map(action))
}

struct Runtime {
    controller: Controller,
    shell: AppKitShell,
    /// The event tap, which macOS refuses until the user gives the Accessibility permission.
    tap: Option<EventTap>,
    release_filter: ReleaseFilter<CGKeyCode>,
    badge_click: BadgeClickFilter,
    /// The badge menu that shows now, whose entries a pick names by their index.
    badge_menu: Option<BadgeMenu>,
}

impl Runtime {
    fn on_event(&mut self, kind: CGEventType, event: &CGEvent) -> Disposition {
        if injection::is_injected(event) {
            return Disposition::Pass;
        }
        if kind == CGEventType::ScrollWheel {
            self.controller.on_scroll(&mut self.shell);
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
                let disposition = self.badge_click.on_badge_press();
                self.shell.schedule(NEXT_RUN_LOOP_PASS, sel!(showBadgeMenu:));
                disposition
            }
            CGEventType::LeftMouseUp => self.badge_click.on_release(),
            CGEventType::LeftMouseDown | CGEventType::RightMouseDown | CGEventType::OtherMouseDown => {
                self.controller.on_context_lost(&mut self.shell);
                Disposition::Pass
            }
            _ => Disposition::Pass,
        }
    }

    fn on_key_down(&mut self, event: &CGEvent) -> Disposition {
        let code = keys::key_code(event);
        let chord = keys::chord_from_flags(CGEvent::flags(Some(event)));
        let key = code.map(|code| Key::from_press(keys::kind_of(code), chord, keys::typed(event)));
        let disposition = self
            .controller
            .on_key_down(key, keys::press(event), &mut self.shell);
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

    fn on_tab_hold_ended(&mut self) {
        self.shell.tab_hold_timer = None;
        self.controller.on_timer(Timer::TabHold, &mut self.shell);
    }

    fn on_idle_timeout(&mut self) {
        self.shell.idle_timer = None;
        self.controller.on_timer(Timer::Idle, &mut self.shell);
    }

    fn on_field_pause_ended(&mut self) {
        self.shell.field_pause_timer = None;
        self.controller.on_timer(Timer::FieldPause, &mut self.shell);
    }

    fn on_scroll_settled(&mut self) {
        self.shell.scroll_timer = None;
        self.controller.on_timer(Timer::ScrollSettle, &mut self.shell);
    }

    fn on_update_check_time(&mut self) {
        self.shell.update_check_timer = None;
        self.controller.on_timer(Timer::UpdateCheck, &mut self.shell);
    }

    fn on_energy_saver(&mut self, energy_saver: EnergySaver) {
        tracing::info!(?energy_saver, "Low Power Mode changed");
        self.controller.on_energy_saver(energy_saver, &mut self.shell);
    }

    fn on_space_change(&mut self) {
        self.read_full_screen();
        self.controller.on_context_lost(&mut self.shell);
        self.controller.on_input(&mut self.shell);
    }

    fn on_app_activation(&mut self) {
        self.read_game();
        self.read_full_screen();
        self.controller.on_input(&mut self.shell);
        if NSApplication::sharedApplication(self.shell.target.mtm()).isActive() {
            self.controller.on_permission_change(&mut self.shell);
        }
    }

    /// Shows the setup while the user did not finish it. Then, when macOS refused the event tap,
    /// waits for the Accessibility permission. It runs in the first pass of the run loop, where
    /// AppKit can show windows.
    fn on_launch(&mut self) {
        self.controller.on_launch(&mut self.shell);
        if self.tap.is_none() {
            tracing::warn!("no accessibility access, so the keyboard is not watched yet");
            self.wait_for_accessibility();
        }
    }

    /// Opens the setup or the Permissions pane, and looks for the Accessibility permission until
    /// the user gives it.
    fn wait_for_accessibility(&mut self) {
        self.controller.on_missing_permission(&mut self.shell);
        self.shell
            .schedule(ACCESSIBILITY_CHECK_INTERVAL, sel!(checkAccessibility:));
    }

    /// Starts the event tap when the user gave the Accessibility permission, else looks again later.
    fn on_accessibility_check(&mut self) {
        if accessibility::is_trusted() {
            self.tap = EventTap::install(on_tap_event);
        }
        if self.tap.is_none() {
            self.shell
                .schedule(ACCESSIBILITY_CHECK_INTERVAL, sel!(checkAccessibility:));
            return;
        }
        tracing::info!("accessibility access given, watching the keyboard");
        self.controller.on_permission_change(&mut self.shell);
    }

    /// Gives the controller whether the app in front is a game.
    fn read_game(&mut self) {
        let game = game::read();
        tracing::debug!(?game, "read the game in front");
        self.controller.on_game(game, &mut self.shell);
    }

    /// Gives the controller whether the app in front shows in full screen. A window goes into and
    /// out of full screen in a space of its own, so each change of the space reads it again.
    fn read_full_screen(&mut self) {
        let full_screen = full_screen::read(self.shell.target.mtm());
        tracing::debug!(?full_screen, "read the full screen in front");
        self.controller.on_full_screen(full_screen, &mut self.shell);
    }

    fn on_menu_item(&mut self, tag: isize) {
        if let Some(command) = menu_bar::command_for_tag(&self.controller.tray_view().items, tag) {
            self.controller.on_tray_command(command, &mut self.shell);
        }
    }

    /// Builds the menu of the badge, and keeps its view for the pick. Returns the menu and where
    /// it starts, or `None` while no badge shows.
    fn build_badge_menu(&mut self) -> Option<(Retained<NSMenu>, NSPoint)> {
        let anchor = self.shell.badge.menu_anchor()?;
        let view = self.controller.badge_menu()?;
        let target = &self.shell.target;
        let menu = badge_menu::build(&view, target, sel!(pickBadgeMenuItem:), target.mtm());
        self.badge_menu = Some(view);
        Some((menu, anchor))
    }

    fn on_badge_menu_item(&mut self, tag: isize) {
        let command = usize::try_from(tag)
            .ok()
            .and_then(|index| self.badge_menu.as_ref()?.command(index).cloned());
        if let Some(command) = command {
            self.controller.on_badge_command(command, &mut self.shell);
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

    /// Handles the input that `read` reads from the setup window, if any.
    fn on_setup_control(&mut self, read: impl FnOnce(&SetupWindow) -> Option<SettingsInput>) {
        if let Some(input) = self.shell.setup.as_ref().and_then(read) {
            self.controller.on_settings_input(&input, &mut self.shell);
        }
    }

    /// Handles a close of the setup window by the user.
    fn on_setup_closed(&mut self) {
        if let Some(setup) = &mut self.shell.setup {
            setup.on_closed();
        }
        self.controller.on_setup_closed();
    }

    fn advance_setup_demo(&mut self) {
        if let Some(setup) = &mut self.shell.setup {
            setup.advance_demo();
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
    /// The caret at the last look, or `None` when the focused field gave no caret.
    caret: Option<Caret>,
    /// The color and the font face of the text before the caret of the last look. The shell reads
    /// them at their first use after the look, so that a look that shows no ghost text after the
    /// caret adds no Accessibility read.
    caret_text_style: OnceCell<TextStyle>,
    /// The frame of the focused field at the last look, when the field holds text.
    field_frame: Option<CGRect>,
    target: Retained<RuntimeTarget>,
    caret_timer: Option<Retained<NSTimer>>,
    settle_timer: Option<Retained<NSTimer>>,
    tab_hold_timer: Option<Retained<NSTimer>>,
    idle_timer: Option<Retained<NSTimer>>,
    field_pause_timer: Option<Retained<NSTimer>>,
    scroll_timer: Option<Retained<NSTimer>>,
    update_check_timer: Option<Retained<NSTimer>>,
    /// The replacements that Tab accepted, oldest first. Each scheduled inject pass types one.
    pending_injections: VecDeque<Replacement>,
    settings: Option<SettingsWindow>,
    setup: Option<SetupWindow>,
}

impl AppKitShell {
    fn show_at_caret(&mut self, view: &GhostView, caret: Caret) {
        if view.placement == GhostPlacement::BelowCaret {
            self.overlay.hide();
            self.mirror.show_below_caret(view, caret);
            return;
        }
        self.mirror.hide();
        let style = self.caret_text_style();
        let wrapped = view.wrapped.as_ref();
        match &view.ghost {
            Ghost::Text(text) => self.overlay.show(text, wrapped, caret, style),
            Ghost::Fix(fix) => self.overlay.show_fix(fix, wrapped, caret, style),
        }
    }

    /// The color and the font face of the text before the caret of the last look, so that the
    /// overlay measures and draws in the same font.
    fn caret_text_style(&self) -> &TextStyle {
        self.caret_text_style.get_or_init(|| self.focus.text_style())
    }

    /// Shows `view` in the mirror next to the focused field, which gives no caret. Hides it when
    /// the focused control holds no text.
    fn show_mirror(&self, view: &GhostView) {
        self.overlay.hide();
        match self.field_frame {
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
            Timer::TabHold => &mut self.tab_hold_timer,
            Timer::Idle => &mut self.idle_timer,
            Timer::FieldPause => &mut self.field_pause_timer,
            Timer::ScrollSettle => &mut self.scroll_timer,
            Timer::UpdateCheck => &mut self.update_check_timer,
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

    fn permissions(&self) -> Vec<(Permission, PermissionState)> {
        permissions::states()
    }

    /// Vision reads at the level `.fast` or `.accurate`.
    fn ocr_levels(&self) -> OcrLevels {
        OcrLevels::Selectable
    }

    fn now(&self) -> Option<Timestamp> {
        let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
        Some(Timestamp(since_epoch.as_secs()))
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

    /// Reads the caret and the frame of the focused field through Accessibility, and keeps them
    /// for the draw. The style of the text before the caret waits for its first use.
    fn read_caret(&mut self) -> CaretLook {
        self.caret = self.focus.caret();
        self.caret_text_style = OnceCell::new();
        self.field_frame = self.focus.field_frame();
        CaretLook::Now(CaretRead {
            caret: self.caret.map(Caret::to_screen_rect),
            field: self.field_frame.map(screen_rect),
        })
    }

    /// Measures `ghost` in the font of the overlay at the caret of the last look.
    fn ghost_width(&self, ghost: &Ghost) -> i32 {
        self.caret.map_or(0, |caret| {
            let face = self.caret_text_style().face.as_ref();
            whole_points_up(overlay::inline_width(ghost, caret, face))
        })
    }

    fn render_at_caret(&mut self, ghost: Option<&GhostView>, badge: Option<BadgeView>) {
        let caret = self.caret;
        let badge_caret = caret.or_else(|| self.field_frame.and_then(Caret::at_start_of));
        match (badge, badge_caret) {
            (Some(badge), Some(badge_caret)) => self.badge.show(&badge, badge_caret),
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

    fn replace_before_caret(&mut self, deleted: usize, text: String) {
        self.pending_injections.push_back(Replacement { deleted, text });
        self.schedule(NEXT_RUN_LOOP_PASS, sel!(injectPendingText:));
    }

    fn start_timer(&mut self, timer: Timer, delay: Duration) {
        self.stop_timer(timer);
        let action = match timer {
            Timer::CaretSettle => sel!(caretDidSettle:),
            Timer::InputSettle => sel!(inputDidSettle:),
            Timer::TabHold => sel!(tabHoldDidEnd:),
            Timer::Idle => sel!(idleTimeoutDidEnd:),
            Timer::FieldPause => sel!(fieldPauseDidEnd:),
            Timer::ScrollSettle => sel!(scrollDidSettle:),
            Timer::UpdateCheck => sel!(updateCheckDidCome:),
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

    fn show_setup(&mut self, view: &SetupView) {
        let target = &self.target;
        let setup = self.setup.get_or_insert_with(|| {
            let action_target = ActionTarget {
                target: Retained::into_super(target.clone()),
                submit_action: sel!(submitSetupControl:),
                toggle_action: sel!(toggleSetupControl:),
                pick_action: sel!(pickSetupControl:),
                press_action: sel!(pressSetupControl:),
                edit_action: sel!(editSetupControl:),
            };
            let actions = SetupActions {
                close: sel!(setupWindowWillClose:),
                demo_frame: sel!(advanceSetupDemo:),
            };
            SetupWindow::new(action_target, actions, target.mtm())
        });
        setup.render(view);
        setup.show();
    }

    fn render_setup(&mut self, view: &SetupView) {
        if let Some(setup) = &mut self.setup {
            setup.render(view);
        }
    }

    /// Hides the setup window, which stays for the next setup.
    fn close_setup(&mut self) {
        if let Some(setup) = &mut self.setup {
            setup.close();
        }
    }

    fn show_settings(&mut self, view: &SettingsView, page: Option<usize>) {
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
        settings.show(page);
    }

    /// macOS reads the clipboard only when core asks for its text, so it keeps no copied text.
    fn use_clipboard_access(&mut self, _access: ClipboardAccess) {}

    fn read_screen(&mut self, read: ScreenReadId, source: ScreenSource) {
        self.screen_text
            .read(read, source, self.focus.frontmost_process());
    }

    fn ask_for_permission(&mut self, permission: Permission) {
        permissions::ask(permission);
    }

    fn open_web_page(&mut self, url: &str) {
        let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) else {
            tracing::warn!("refused a malformed web address");
            return;
        };
        if !NSWorkspace::sharedWorkspace().openURL(&url) {
            tracing::warn!("failed to open a web page");
        }
    }

    /// Ends the app from a timer, outside the borrow of the runtime that the end of the app drops.
    fn quit(&mut self) {
        self.schedule(NEXT_RUN_LOOP_PASS, sel!(quitApplication:));
    }
}

fn on_tap_event(kind: CGEventType, event: &CGEvent) -> Disposition {
    with_runtime(|runtime| runtime.on_event(kind, event)).unwrap_or(Disposition::Pass)
}
