//! The message loop of the thread that runs the hooks, the overlay, the tray icon and the
//! controller, and the one place Win32 callbacks on that thread reach GhostType's state. The
//! settings window runs on a thread of its own, so that this loop never waits for drawing.

use std::cell::RefCell;
use std::sync::mpsc;
use std::time::Duration;

use ghosttype_core::app::{BadgeView, Field, TrayCommand, TrayView};
use ghosttype_core::controller::{BackendChoice, Controller, Delivery, FocusChange, Services, Shell, Timer};
use ghosttype_core::desktop::{Desktop, Surroundings};
use ghosttype_core::key::{Disposition, ReleaseFilter};
use ghosttype_core::settings::{ClipboardAccess, Settings};
use ghosttype_core::settings_view::{SettingsInput, SettingsView};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DefWindowProcW, HC_ACTION, HHOOK, HOOKPROC, KBDLLHOOKSTRUCT, KillTimer, PostMessageW,
    PostQuitMessage, RegisterWindowMessageW, SetTimer, SetWindowsHookExW, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WINDOW_EX_STYLE, WINDOWS_HOOK_ID, WM_APP, WM_CLIPBOARDUPDATE, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETTINGCHANGE,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASS_STYLES, WS_OVERLAPPED,
};
use windows::core::{Error, Owned, w};

use crate::RunError;
use crate::clipboard::ClipboardWatch;
use crate::focus::{self, FocusTracker};
use crate::injection::{self, INJECTION_TAG};
use crate::keys;
use crate::mailbox::Mailbox;
use crate::overlay::Overlay;
#[cfg(feature = "reactor")]
use crate::reactor_settings::SettingsThread;
#[cfg(not(feature = "reactor"))]
use crate::settings_thread::SettingsThread;
use crate::system_theme::{self, SystemTheme};
use crate::tray::{self, MenuRequest, TRAY_CALLBACK_MESSAGE, Tray};
use crate::window::{self, Window, WindowSpec};

const CARET_TIMER_ID: usize = 1;
const SETTLE_TIMER_ID: usize = 2;
const INJECT_MESSAGE: u32 = WM_APP + 2;
const DELIVERY_MESSAGE: u32 = WM_APP + 3;
const SETTINGS_INPUT_MESSAGE: u32 = WM_APP + 5;
const SWALLOW_EVENT: LRESULT = LRESULT(1);
const HANDLED: LRESULT = LRESULT(0);

/// Everything Win32 callbacks on this thread can reach.
struct ThreadState {
    runtime: RefCell<Option<Runtime>>,
}

thread_local! {
    static STATE: ThreadState = const {
        ThreadState {
            runtime: RefCell::new(None),
        }
    };
}

/// Shows the tray icon and offers suggestions from the runner of `services` until the user quits.
/// GhostType starts with the saved `settings`.
pub(crate) fn run(settings: Settings, services: Services) -> Result<(), RunError> {
    let window = Window::create(WindowSpec {
        class_name: w!("GhostType.Main"),
        procedure: Some(main_window_procedure),
        class_style: WNDCLASS_STYLES::default(),
        style: WS_OVERLAPPED,
        extended_style: WINDOW_EX_STYLE::default(),
        parent: None,
    })
    .map_err(RunError::during("create the main window"))?;
    let overlay = Overlay::create().map_err(RunError::during("create the ghost text overlay"))?;
    system_theme::match_menus_to(SystemTheme::read());
    let (mailbox, deliveries) = Mailbox::new(window.handle(), DELIVERY_MESSAGE);
    let (settings_mailbox, settings_inputs) = Mailbox::new(window.handle(), SETTINGS_INPUT_MESSAGE);
    let settings_thread =
        SettingsThread::start(settings_mailbox).map_err(RunError::during("start the settings window"))?;
    let clipboard = ClipboardWatch::start(window.handle(), settings.clipboard_access);
    let controller = Controller::new(settings, services, BackendChoice::User, &mailbox.into_sink());
    let tray =
        Tray::add(window.handle(), &controller.tray_view()).map_err(RunError::during("add the tray icon"))?;
    // SAFETY: plain call with a static, nul-terminated name.
    let taskbar_created_message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    let keyboard_hook = install_hook(WH_KEYBOARD_LL, Some(keyboard_procedure))
        .map_err(RunError::during("install the keyboard hook"))?;
    let mouse_hook = install_hook(WH_MOUSE_LL, Some(mouse_procedure))
        .map_err(RunError::during("install the mouse hook"))?;
    STATE.with(|state| {
        state.runtime.replace(Some(Runtime {
            controller,
            shell: Win32Shell {
                focus: FocusTracker::new(),
                clipboard,
                overlay,
                tray,
                window,
                pending_injection: None,
                settings: settings_thread,
            },
            release_filter: ReleaseFilter::default(),
            taskbar_created_message,
            deliveries,
            settings_inputs,
        }))
    });
    tracing::info!("running in the tray");
    let result = window::pump_messages(|_| false).map_err(RunError::during("read the message queue"));
    drop((keyboard_hook, mouse_hook));
    STATE.with(|state| state.runtime.take());
    result
}

fn install_hook(kind: WINDOWS_HOOK_ID, procedure: HOOKPROC) -> windows::core::Result<Owned<HHOOK>> {
    // SAFETY: plain call.
    let module = unsafe { GetModuleHandleW(None) }?;
    // SAFETY: the procedure lives for the whole program; low-level hooks run on this thread.
    let hook = unsafe { SetWindowsHookExW(kind, procedure, Some(module.into()), 0) }?;
    // SAFETY: SetWindowsHookExW returned a valid hook that nothing else owns.
    Ok(unsafe { Owned::new(hook) })
}

fn with_runtime<Output>(action: impl FnOnce(&mut Runtime) -> Output) -> Option<Output> {
    STATE.with(|state| state.runtime.try_borrow_mut().ok()?.as_mut().map(action))
}

struct Runtime {
    controller: Controller,
    shell: Win32Shell,
    release_filter: ReleaseFilter<VIRTUAL_KEY>,
    taskbar_created_message: u32,
    deliveries: mpsc::Receiver<Delivery>,
    settings_inputs: mpsc::Receiver<SettingsInput>,
}

impl Runtime {
    fn on_keyboard(&mut self, message: u32, event: &KBDLLHOOKSTRUCT) -> Disposition {
        if event.dwExtraInfo == INJECTION_TAG {
            return Disposition::Pass;
        }
        self.controller.on_input(&mut self.shell);
        let Ok(key) = u16::try_from(event.vkCode).map(VIRTUAL_KEY) else {
            return Disposition::Pass;
        };
        match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => self.on_key_down(key, event.scanCode),
            WM_KEYUP | WM_SYSKEYUP => self.release_filter.on_release(key),
            _ => Disposition::Pass,
        }
    }

    fn on_key_down(&mut self, key: VIRTUAL_KEY, scan_code: u32) -> Disposition {
        let core_key = keys::translate(key, keys::current_chord(), keys::typed_char(key, scan_code));
        let disposition = self.controller.on_key_down(core_key, &mut self.shell);
        self.release_filter.on_press(key, disposition);
        disposition
    }

    fn on_click(&mut self) {
        self.controller.on_context_lost(&mut self.shell);
    }

    fn on_click_release(&mut self) {
        self.controller.on_input(&mut self.shell);
    }

    /// Window timers repeat, and the controller asks for one call for each start.
    fn on_timer(&mut self, timer: Timer) {
        self.shell.stop_timer(timer);
        self.controller.on_timer(timer, &mut self.shell);
    }

    fn on_tray_command(&mut self, command: TrayCommand) {
        self.controller.on_tray_command(command, &mut self.shell);
    }

    fn on_window_message(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match message {
            WM_TIMER if wparam.0 == CARET_TIMER_ID => self.on_timer(Timer::CaretSettle),
            WM_TIMER if wparam.0 == SETTLE_TIMER_ID => self.on_timer(Timer::InputSettle),
            WM_SETTINGCHANGE if system_theme::is_appearance_change(wparam, lparam) => {
                self.on_appearance_change();
            }
            DELIVERY_MESSAGE => self.take_deliveries(),
            SETTINGS_INPUT_MESSAGE => self.take_settings_inputs(),
            WM_CLIPBOARDUPDATE => self.shell.clipboard.on_update(),
            _ if message == self.taskbar_created_message => {
                if let Err(error) = self.shell.tray.register() {
                    tracing::warn!(%error, "failed to re-add tray icon");
                }
            }
            _ => return None,
        }
        Some(HANDLED)
    }

    /// Makes the menus and the tray icon follow the new theme. The settings window follows it on
    /// its own thread.
    fn on_appearance_change(&mut self) {
        system_theme::match_menus_to(SystemTheme::read());
        self.shell.render_tray(&self.controller.tray_view());
    }

    fn take_deliveries(&mut self) {
        while let Ok(delivery) = self.deliveries.try_recv() {
            self.controller.on_delivery(delivery, &mut self.shell);
        }
    }

    fn take_settings_inputs(&mut self) {
        while let Ok(input) = self.settings_inputs.try_recv() {
            self.controller.on_settings_input(&input, &mut self.shell);
        }
    }
}

/// What the controller reads and does through Win32.
struct Win32Shell {
    focus: FocusTracker,
    clipboard: ClipboardWatch,
    overlay: Overlay,
    tray: Tray,
    window: Window,
    pending_injection: Option<String>,
    settings: SettingsThread,
}

impl Desktop for Win32Shell {
    fn field(&self) -> Field {
        self.focus.field()
    }

    fn surroundings(&self) -> Surroundings {
        self.focus.surroundings()
    }

    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.copied_text().map(str::to_owned)
    }
}

impl Shell for Win32Shell {
    fn refresh_focus(&mut self) -> FocusChange {
        self.focus.refresh()
    }

    /// Shows only the ghost text: Windows has no badge yet.
    fn render_at_caret(&mut self, ghost: Option<&str>, _badge: Option<BadgeView>) {
        let Some(text) = ghost else {
            self.overlay.hide();
            return;
        };
        let Some(caret) = focus::caret() else {
            self.overlay.hide();
            return;
        };
        if let Err(error) = self.overlay.show(text, &caret) {
            tracing::warn!(%error, "failed to show ghost text");
        }
    }

    fn hide_ghost(&mut self) {
        self.overlay.hide();
    }

    /// Does nothing: Windows has no badge yet.
    fn hide_badge(&mut self) {}

    fn type_text(&mut self, text: String) {
        self.pending_injection = Some(text);
        // SAFETY: plain call on our own window.
        let posted =
            unsafe { PostMessageW(Some(self.window.handle()), INJECT_MESSAGE, WPARAM(0), LPARAM(0)) };
        if let Err(error) = posted {
            tracing::warn!(%error, "failed to queue accepted text");
        }
    }

    fn start_timer(&mut self, timer: Timer, delay: Duration) {
        let delay_ms = u32::try_from(delay.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: plain call on our own window.
        let is_started =
            unsafe { SetTimer(Some(self.window.handle()), timer_id(timer), delay_ms, None) } != 0;
        if !is_started {
            tracing::warn!(error = %Error::from_thread(), ?timer, "failed to start a timer");
        }
    }

    fn stop_timer(&mut self, timer: Timer) {
        // Fails only when no timer is pending, which needs no handling.
        // SAFETY: plain call on our own window.
        let _ = unsafe { KillTimer(Some(self.window.handle()), timer_id(timer)) };
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

extern "system" fn main_window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled = match message {
        TRAY_CALLBACK_MESSAGE => {
            tray::menu_request(wparam, lparam).map(|request| show_tray_menu(window, request))
        }
        INJECT_MESSAGE => Some(inject_pending_text()),
        _ => with_runtime(|runtime| runtime.on_window_message(message, wparam, lparam)).flatten(),
    };
    // SAFETY: forwards the system's own arguments unchanged.
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(window, message, wparam, lparam) })
}

fn show_tray_menu(window: HWND, request: MenuRequest) -> LRESULT {
    let Some(view) = with_runtime(|runtime| runtime.controller.tray_view()) else {
        return HANDLED;
    };
    match tray::pick_command(window, &view.items, request.anchor) {
        Ok(Some(command)) => {
            with_runtime(|runtime| runtime.on_tray_command(command));
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "failed to show tray menu"),
    }
    HANDLED
}

fn inject_pending_text() -> LRESULT {
    let text = with_runtime(|runtime| runtime.shell.pending_injection.take()).flatten();
    if let Some(text) = text
        && let Err(error) = injection::type_text(&text)
    {
        tracing::warn!(%error, "failed to type accepted text");
    }
    HANDLED
}

extern "system" fn keyboard_procedure(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION.cast_signed() {
        // SAFETY: for HC_ACTION, lparam points to this event's KBDLLHOOKSTRUCT, valid during the call.
        let event =
            unsafe { &*std::ptr::with_exposed_provenance::<KBDLLHOOKSTRUCT>(lparam.0.cast_unsigned()) };
        let message = u32::try_from(wparam.0).unwrap_or_default();
        if with_runtime(|runtime| runtime.on_keyboard(message, event)) == Some(Disposition::Swallow) {
            return SWALLOW_EVENT;
        }
    }
    // SAFETY: forwards the system's own arguments unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

extern "system" fn mouse_procedure(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let message = u32::try_from(wparam.0).unwrap_or_default();
    let is_button_down = matches!(
        message,
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
    );
    let is_button_up = matches!(message, WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP);
    if code == HC_ACTION.cast_signed() && is_button_down {
        with_runtime(Runtime::on_click);
    }
    if code == HC_ACTION.cast_signed() && is_button_up {
        with_runtime(Runtime::on_click_release);
    }
    // SAFETY: forwards the system's own arguments unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
