//! The message loop of the thread that runs the hooks, the overlay, the tray icon and the
//! controller, and the one place Win32 callbacks on that thread reach GhostType's state. The
//! settings window runs on a thread of its own, so that this loop never waits for drawing.

mod shell;

use std::cell::RefCell;
use std::mem;
use std::sync::mpsc;

use ghosttype_core::app::BadgeMenu;
use ghosttype_core::controller::{BackendChoice, Controller, Delivery, Services, Shell, Timer};
use ghosttype_core::key::{Disposition, ReleaseFilter};
use ghosttype_core::settings::Settings;
use ghosttype_core::settings_window::view::SettingsInput;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DefWindowProcW, DispatchMessageW, EVENT_OBJECT_FOCUS, GetMessageW, HC_ACTION, HHOOK,
    HOOKPROC, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, RegisterWindowMessageW, SetWindowsHookExW,
    TranslateMessage, WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOW_EX_STYLE, WINDOWS_HOOK_ID, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP, WM_CLIPBOARDUPDATE, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETTINGCHANGE, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_TIMER, WM_XBUTTONDOWN, WM_XBUTTONUP, WS_OVERLAPPED,
};
use windows::core::{Error, Owned, w};

use self::shell::Win32Shell;
use crate::RunError;
use crate::badge::Badge;
use crate::caret_reader::{CaretRead, CaretReader, FieldRead, KeptFieldText, PendingView};
use crate::clipboard::ClipboardWatch;
use crate::focus::FocusTracker;
use crate::injection::{self, INJECTION_TAG};
use crate::keys;
use crate::mailbox::Mailbox;
use crate::menu;
use crate::ocr::OcrSupport;
use crate::overlay::Overlay;
use crate::screen_text::{ScreenAnswer, ScreenTextReader};
use crate::settings::SettingsThread;
use crate::system_theme::{self, SystemTheme};
use crate::tray::{self, MenuRequest, TRAY_CALLBACK_MESSAGE, Tray};
use crate::window::{Window, WindowSpec};

const CARET_TIMER_ID: usize = 1;
const SETTLE_TIMER_ID: usize = 2;
const FIELD_TIMER_ID: usize = 3;
const INJECT_MESSAGE: u32 = WM_APP + 2;
const DELIVERY_MESSAGE: u32 = WM_APP + 3;
const CARET_READ_MESSAGE: u32 = WM_APP + 4;
const SETTINGS_INPUT_MESSAGE: u32 = WM_APP + 5;
const BADGE_CLICK_MESSAGE: u32 = WM_APP + 6;
const FIELD_READ_MESSAGE: u32 = WM_APP + 7;
const SCREEN_READ_MESSAGE: u32 = WM_APP + 8;
const SWALLOW_EVENT: LRESULT = LRESULT(1);
const HANDLED: LRESULT = LRESULT(0);
const QUIT_RECEIVED: i32 = 0;
const READ_FAILED: i32 = -1;

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
        style: WS_OVERLAPPED,
        extended_style: WINDOW_EX_STYLE::default(),
    })
    .map_err(RunError::during("create the main window"))?;
    let overlay = Overlay::create().map_err(RunError::during("create the ghost text overlay"))?;
    let badge = Badge::create().map_err(RunError::during("create the badge"))?;
    system_theme::match_menus_to(SystemTheme::read());
    let (caret_mailbox, caret_reads) = Mailbox::new(window.handle(), CARET_READ_MESSAGE);
    let (field_mailbox, field_reads) = Mailbox::new(window.handle(), FIELD_READ_MESSAGE);
    let caret_reader = CaretReader::start(caret_mailbox, field_mailbox)
        .map_err(RunError::during("start the caret reader"))?;
    let (screen_mailbox, screen_reads) = Mailbox::new(window.handle(), SCREEN_READ_MESSAGE);
    let screen_text =
        ScreenTextReader::start(screen_mailbox).map_err(RunError::during("start the screen text reader"))?;
    let ocr = OcrSupport::detect();
    tracing::info!(?ocr, "found the OCR support");
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
    let focus_hook = watch_focus()
        .inspect_err(|error| tracing::warn!(%error, "failed to watch the focus"))
        .ok();
    STATE.with(|state| {
        state.runtime.replace(Some(Runtime {
            controller,
            shell: Win32Shell {
                focus: FocusTracker::new(),
                clipboard,
                overlay,
                badge,
                caret_reader,
                screen_text,
                ocr,
                pending_view: PendingView::default(),
                kept_field_text: KeptFieldText::default(),
                tray,
                window,
                pending_injection: None,
                settings: settings_thread,
            },
            release_filter: ReleaseFilter::default(),
            is_badge_pressed: false,
            taskbar_created_message,
            deliveries,
            settings_inputs,
            caret_reads,
            field_reads,
            screen_reads,
        }))
    });
    // The focused field counts as a password field until a read of it tells otherwise.
    with_runtime(|runtime| runtime.shell.read_field_text());
    tracing::info!("running in the tray");
    let result = pump_messages();
    drop((keyboard_hook, mouse_hook, focus_hook));
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

/// Watches the focus in other apps: each move of the focus reaches `focus_event_procedure`.
fn watch_focus() -> windows::core::Result<Owned<HWINEVENTHOOK>> {
    // SAFETY: plain call. The procedure lives for the whole program, and the events come through
    // the message loop of this thread.
    let hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_FOCUS,
            EVENT_OBJECT_FOCUS,
            None,
            Some(focus_event_procedure),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if hook.is_invalid() {
        return Err(Error::from_thread());
    }
    // SAFETY: SetWinEventHook returned a valid hook that nothing else owns.
    Ok(unsafe { Owned::new(hook) })
}

fn pump_messages() -> Result<(), RunError> {
    let mut message = MSG::default();
    loop {
        // SAFETY: `message` is a local the call fills in.
        match unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 {
            QUIT_RECEIVED => return Ok(()),
            READ_FAILED => return Err(RunError::during("read the message queue")(Error::from_thread())),
            _ => {}
        }
        // SAFETY: `message` was just filled in by GetMessageW.
        unsafe {
            // Returns whether a character message was produced, not an error.
            let _ = TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
}

fn with_runtime<Output>(action: impl FnOnce(&mut Runtime) -> Output) -> Option<Output> {
    STATE.with(|state| state.runtime.try_borrow_mut().ok()?.as_mut().map(action))
}

struct Runtime {
    controller: Controller,
    shell: Win32Shell,
    release_filter: ReleaseFilter<VIRTUAL_KEY>,
    /// Whether the user pressed the left button on the badge and did not release it yet.
    is_badge_pressed: bool,
    taskbar_created_message: u32,
    deliveries: mpsc::Receiver<Delivery>,
    settings_inputs: mpsc::Receiver<SettingsInput>,
    caret_reads: mpsc::Receiver<CaretRead>,
    field_reads: mpsc::Receiver<FieldRead>,
    screen_reads: mpsc::Receiver<ScreenAnswer>,
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

    /// The controller reads the field text as it is before the key, so the key makes that text
    /// stale only after the controller. A modifier alone does not change the field.
    fn on_key_down(&mut self, key: VIRTUAL_KEY, scan_code: u32) -> Disposition {
        let core_key = keys::translate(key, keys::current_chord(), keys::typed(key, scan_code));
        let disposition = self.controller.on_key_down(core_key, &mut self.shell);
        self.release_filter.on_press(key, disposition);
        if core_key.is_some() {
            self.shell.forget_field_text();
        }
        disposition
    }

    /// Takes the clicks on the badge, so that the app under it never gets them. The badge menu
    /// shows after the hook returns, outside the borrow of the runtime.
    fn on_mouse(&mut self, message: u32, point: POINT) -> Disposition {
        match message {
            WM_LBUTTONDOWN if self.shell.badge.contains(point) => {
                self.is_badge_pressed = true;
                if let Err(error) = self.shell.post(BADGE_CLICK_MESSAGE) {
                    tracing::warn!(%error, "failed to queue the badge click");
                }
                Disposition::Swallow
            }
            WM_LBUTTONUP if mem::take(&mut self.is_badge_pressed) => {
                self.controller.on_input(&mut self.shell);
                Disposition::Swallow
            }
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN => {
                self.controller.on_context_lost(&mut self.shell);
                self.shell.forget_focused_field();
                Disposition::Pass
            }
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP | WM_XBUTTONUP => {
                self.controller.on_input(&mut self.shell);
                self.shell.forget_focused_field();
                Disposition::Pass
            }
            _ => Disposition::Pass,
        }
    }

    /// Window timers repeat, and the controller asks for one call for each start.
    fn on_timer(&mut self, timer: Timer) {
        self.shell.stop_timer(timer);
        self.controller.on_timer(timer, &mut self.shell);
    }

    /// The menu of the badge and where it starts, or `None` while no badge shows.
    fn badge_menu(&self) -> Option<(BadgeMenu, POINT)> {
        let anchor = self.shell.badge.menu_anchor()?;
        Some((self.controller.badge_menu()?, anchor))
    }

    fn on_window_message(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match message {
            WM_TIMER if wparam.0 == CARET_TIMER_ID => self.on_timer(Timer::CaretSettle),
            WM_TIMER if wparam.0 == SETTLE_TIMER_ID => self.on_timer(Timer::InputSettle),
            WM_TIMER if wparam.0 == FIELD_TIMER_ID => self.shell.read_field_text(),
            WM_SETTINGCHANGE if system_theme::is_appearance_change(wparam, lparam) => {
                self.on_appearance_change();
            }
            DELIVERY_MESSAGE => self.take_deliveries(),
            SETTINGS_INPUT_MESSAGE => self.take_settings_inputs(),
            CARET_READ_MESSAGE => self.take_caret_reads(),
            FIELD_READ_MESSAGE => self.take_field_reads(),
            SCREEN_READ_MESSAGE => self.take_screen_reads(),
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
        for delivery in self.deliveries.try_iter() {
            self.controller.on_delivery(delivery, &mut self.shell);
        }
    }

    fn take_settings_inputs(&mut self) {
        for input in self.settings_inputs.try_iter() {
            self.controller.on_settings_input(&input, &mut self.shell);
        }
    }

    fn take_caret_reads(&mut self) {
        for read in self.caret_reads.try_iter() {
            self.shell.show_at_caret(&read);
        }
    }

    fn take_field_reads(&mut self) {
        for read in self.field_reads.try_iter() {
            self.shell.kept_field_text.keep(read);
        }
    }

    fn take_screen_reads(&mut self) {
        for answer in self.screen_reads.try_iter() {
            tracing::debug!(read = ?answer.read.id, trip = ?answer.sent.elapsed(), "screen read reached the main thread");
            self.controller
                .on_delivery(Delivery::Screen(answer.read), &mut self.shell);
        }
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
        BADGE_CLICK_MESSAGE => Some(show_badge_menu(window)),
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
            with_runtime(|runtime| runtime.controller.on_tray_command(command, &mut runtime.shell));
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "failed to show tray menu"),
    }
    HANDLED
}

fn show_badge_menu(window: HWND) -> LRESULT {
    let Some((view, anchor)) = with_runtime(|runtime| runtime.badge_menu()).flatten() else {
        return HANDLED;
    };
    match menu::pick_badge_item(window, &view, anchor) {
        Ok(picked) => {
            if let Some(command) = picked.and_then(|index| view.command(index).cloned()) {
                with_runtime(|runtime| runtime.controller.on_badge_command(command, &mut runtime.shell));
            }
        }
        Err(error) => tracing::warn!(%error, "failed to show the badge menu"),
    }
    HANDLED
}

fn inject_pending_text() -> LRESULT {
    let replacement = with_runtime(|runtime| runtime.shell.pending_injection.take()).flatten();
    if let Some(replacement) = replacement
        && let Err(error) = injection::replace_text(&replacement)
    {
        tracing::warn!(%error, "failed to type accepted text");
    }
    HANDLED
}

extern "system" fn focus_event_procedure(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _window: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time_ms: u32,
) {
    with_runtime(|runtime| runtime.shell.forget_focused_field());
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
    if code == HC_ACTION.cast_signed() {
        // SAFETY: for HC_ACTION, lparam points to this event's MSLLHOOKSTRUCT, valid during the call.
        let event =
            unsafe { &*std::ptr::with_exposed_provenance::<MSLLHOOKSTRUCT>(lparam.0.cast_unsigned()) };
        let message = u32::try_from(wparam.0).unwrap_or_default();
        if with_runtime(|runtime| runtime.on_mouse(message, event.pt)) == Some(Disposition::Swallow) {
            return SWALLOW_EVENT;
        }
    }
    // SAFETY: forwards the system's own arguments unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
