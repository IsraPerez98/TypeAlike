//! The message loop, and the one place Win32 callbacks reach GhostType's state.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use ghosttype_core::app::{BadgeView, Field, TrayCommand, TrayView};
use ghosttype_core::controller::{BackendChoice, Controller, Delivery, FocusChange, Services, Shell, Timer};
use ghosttype_core::desktop::{Desktop, Surroundings};
use ghosttype_core::key::{Disposition, ReleaseFilter};
use ghosttype_core::settings::{ClipboardAccess, Settings};
use ghosttype_core::settings_view::{ControlId, SettingsInput, SettingsView};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_TAB};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DefWindowProcW, DispatchMessageW, EN_CHANGE, EN_KILLFOCUS, EN_SETFOCUS, GetMessageW,
    HC_ACTION, HHOOK, HOOKPROC, IDCANCEL, IDOK, IsDialogMessageW, KBDLLHOOKSTRUCT, KillTimer, MSG,
    PostMessageW, PostQuitMessage, RegisterWindowMessageW, SPI_SETHIGHCONTRAST, SW_HIDE, SetTimer,
    SetWindowsHookExW, ShowWindow, TranslateMessage, WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOW_EX_STYLE,
    WINDOWS_HOOK_ID, WM_APP, WM_CAPTURECHANGED, WM_CLIPBOARDUPDATE, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU,
    WM_CTLCOLOREDIT, WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP,
    WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SETTINGCHANGE, WM_SIZE,
    WM_SYSKEYDOWN, WM_SYSKEYUP, WM_TIMER, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASS_STYLES, WS_OVERLAPPED,
};
use windows::core::{Error, Owned, PCWSTR, w};

use crate::RunError;
use crate::clipboard::ClipboardWatch;
use crate::focus::{self, FocusTracker};
use crate::injection::{self, INJECTION_TAG};
use crate::keys;
use crate::mailbox::{DELIVERY_MESSAGE, Mailbox};
use crate::menu::{self, MenuPlacement};
use crate::overlay::Overlay;
use crate::settings::{self, Appearance, Pointer, Procedures, SettingsWindow, WindowInput};
use crate::system_theme::{self, SystemTheme};
use crate::tray::{self, MenuRequest, TRAY_CALLBACK_MESSAGE, Tray};
use crate::window::{Window, WindowSpec};

const CARET_TIMER_ID: usize = 1;
const SETTLE_TIMER_ID: usize = 2;
const INJECT_MESSAGE: u32 = WM_APP + 2;
const PICKER_MENU_MESSAGE: u32 = WM_APP + 4;
const COLOR_SET_CHANGE: &str = "ImmersiveColorSet";
const SWALLOW_EVENT: LRESULT = LRESULT(1);
const HANDLED: LRESULT = LRESULT(0);
const DRAWN: LRESULT = LRESULT(1);
const CURSOR_SET: LRESULT = LRESULT(1);
const QUIT_RECEIVED: i32 = 0;
const READ_FAILED: i32 = -1;
const SETTINGS_PROCEDURES: Procedures = Procedures {
    window: Some(settings_window_procedure),
    page: Some(settings_page_procedure),
};

/// Everything Win32 callbacks on this thread can reach. The appearance has its own cell because
/// controls ask to draw while the runtime is busy changing them.
struct ThreadState {
    runtime: RefCell<Option<Runtime>>,
    appearance: RefCell<Option<Rc<Appearance>>>,
}

thread_local! {
    static STATE: ThreadState = const {
        ThreadState {
            runtime: RefCell::new(None),
            appearance: RefCell::new(None),
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
    let (mailbox, deliveries) = Mailbox::new(window.handle());
    let clipboard = ClipboardWatch::start(window.handle(), settings.clipboard_access);
    let controller = Controller::new(settings, services, BackendChoice::User, &mailbox.into_sink());
    let tray =
        Tray::add(window.handle(), &controller.tray_view()).map_err(RunError::during("add the tray icon"))?;
    // SAFETY: plain call with a static, nul-terminated name.
    let taskbar_created_message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    let _keyboard_hook = install_hook(WH_KEYBOARD_LL, Some(keyboard_procedure))
        .map_err(RunError::during("install the keyboard hook"))?;
    let _mouse_hook = install_hook(WH_MOUSE_LL, Some(mouse_procedure))
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
                settings: None,
            },
            release_filter: ReleaseFilter::default(),
            taskbar_created_message,
            deliveries,
            pending_menu: None,
        }))
    });
    tracing::info!("running in the tray");
    let result = pump_messages();
    STATE.with(|state| {
        state.runtime.take();
        state.appearance.take();
    });
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

fn pump_messages() -> Result<(), RunError> {
    let mut message = MSG::default();
    loop {
        // SAFETY: `message` is a local the call fills in.
        match unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 {
            QUIT_RECEIVED => return Ok(()),
            READ_FAILED => return Err(RunError::during("read the message queue")(Error::from_thread())),
            _ => {}
        }
        if is_settings_dialog_message(&message) {
            continue;
        }
        // SAFETY: `message` was just filled in by GetMessageW.
        unsafe {
            // Returns whether a character message was produced, not an error.
            let _ = TranslateMessage(&raw const message);
            DispatchMessageW(&raw const message);
        }
    }
}

/// Gives the settings window keyboard navigation (Tab, arrow keys, Enter, Escape) like a dialog.
fn is_settings_dialog_message(message: &MSG) -> bool {
    let Some(settings) =
        with_runtime(|runtime| runtime.shell.settings.as_ref().map(SettingsWindow::handle)).flatten()
    else {
        return false;
    };
    // SAFETY: `message` was filled in by GetMessageW and the settings window is alive.
    if !unsafe { IsDialogMessageW(settings, &raw const *message) }.as_bool() {
        return false;
    }
    if message.message == WM_KEYDOWN && message.wParam.0 == usize::from(VK_TAB.0) {
        with_runtime(|runtime| runtime.shell.settings.as_mut().map(SettingsWindow::reveal_focus));
    }
    true
}

fn with_runtime<Output>(action: impl FnOnce(&mut Runtime) -> Output) -> Option<Output> {
    STATE.with(|state| state.runtime.try_borrow_mut().ok()?.as_mut().map(action))
}

fn with_appearance<Output>(action: impl FnOnce(&Appearance) -> Output) -> Option<Output> {
    let appearance = STATE.with(|state| state.appearance.try_borrow().ok()?.clone())?;
    Some(action(&appearance))
}

fn publish_appearance(appearance: Rc<Appearance>) {
    STATE.with(|state| state.appearance.replace(Some(appearance)));
}

struct Runtime {
    controller: Controller,
    shell: Win32Shell,
    release_filter: ReleaseFilter<VIRTUAL_KEY>,
    taskbar_created_message: u32,
    deliveries: mpsc::Receiver<Delivery>,
    /// The picker whose menu must show once the runtime is free.
    pending_menu: Option<ControlId>,
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
            WM_SETTINGCHANGE if is_appearance_change(wparam, lparam) => self.on_appearance_change(),
            DELIVERY_MESSAGE => self.take_deliveries(),
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

    fn on_appearance_change(&mut self) {
        system_theme::match_menus_to(SystemTheme::read());
        self.shell.render_tray(&self.controller.tray_view());
        let Some(settings) = &mut self.shell.settings else {
            return;
        };
        match settings.change_theme() {
            Ok(()) => publish_appearance(settings.appearance()),
            Err(error) => tracing::warn!(%error, "failed to redraw settings for the new theme"),
        }
    }

    fn take_deliveries(&mut self) {
        while let Ok(delivery) = self.deliveries.try_recv() {
            self.controller.on_delivery(delivery, &mut self.shell);
        }
    }

    fn on_settings_input(&mut self, input: WindowInput) {
        match input {
            WindowInput::Settings(input) => self.controller.on_settings_input(&input, &mut self.shell),
            WindowInput::OpenMenu(picker) => self.post_picker_menu(picker),
        }
    }

    fn post_picker_menu(&mut self, picker: ControlId) {
        let Some(settings) = &self.shell.settings else {
            return;
        };
        // SAFETY: plain call on our own window.
        let posted =
            unsafe { PostMessageW(Some(settings.handle()), PICKER_MENU_MESSAGE, WPARAM(0), LPARAM(0)) };
        match posted {
            Ok(()) => self.pending_menu = Some(picker),
            Err(error) => tracing::warn!(%error, "failed to open a picker menu"),
        }
    }

    fn on_menu_pick(&mut self, picker: ControlId, index: usize) {
        let input = SettingsInput::Picked { id: picker, index };
        self.controller.on_settings_input(&input, &mut self.shell);
    }

    fn on_settings_message(
        &mut self,
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        let settings = self.shell.settings.as_mut()?;
        match message {
            WM_SIZE if window == settings.handle() => settings.layout(),
            WM_DPICHANGED => {
                // SAFETY: for WM_DPICHANGED the lparam points to the suggested RECT, valid during the message.
                let suggested =
                    unsafe { *std::ptr::with_exposed_provenance::<RECT>(lparam.0.cast_unsigned()) };
                match settings.change_dpi(u32::from(low_word(wparam.0)), suggested) {
                    Ok(()) => publish_appearance(settings.appearance()),
                    Err(error) => tracing::warn!(%error, "failed to rescale settings"),
                }
            }
            WM_MOUSEWHEEL => settings.scroll_by_wheel(i32::from(high_word(wparam.0).cast_signed())),
            WM_MOUSEMOVE => {
                settings.on_page_pointer(window, Pointer::Move(settings::pointer_position(lparam)));
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => {
                settings.on_page_pointer(window, Pointer::Press(settings::pointer_position(lparam)));
            }
            WM_LBUTTONUP => settings.on_page_pointer(window, Pointer::Release),
            WM_MOUSELEAVE => settings.on_page_pointer(window, Pointer::Leave),
            WM_CAPTURECHANGED => settings.on_page_pointer(window, Pointer::CaptureLost),
            WM_COMMAND => self.on_settings_command(wparam, lparam),
            _ => return None,
        }
        Some(HANDLED)
    }

    fn on_settings_command(&mut self, wparam: WPARAM, lparam: LPARAM) {
        let Some(settings) = &mut self.shell.settings else {
            return;
        };
        let command = i32::from(low_word(wparam.0));
        let input = if lparam.0 != 0 {
            let control = HWND(std::ptr::with_exposed_provenance_mut(lparam.0.cast_unsigned()));
            settings.input_for(control, u32::from(high_word(wparam.0)))
        } else if command == IDOK.0 {
            settings.input_for_enter()
        } else if command == IDCANCEL.0 {
            settings.hide();
            None
        } else {
            None
        };
        if let Some(input) = input {
            self.on_settings_input(input);
        }
    }

    fn paint_settings_page(&self, page: HWND) -> Option<LRESULT> {
        self.shell.settings.as_ref()?.paint_page(page);
        Some(HANDLED)
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
    settings: Option<SettingsWindow>,
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
        let Some(settings) = &mut self.settings else {
            return;
        };
        if let Err(error) = settings.render(view) {
            tracing::warn!(%error, "failed to render settings");
        }
    }

    fn show_settings(&mut self, view: &SettingsView) {
        if self.settings.is_none() {
            match SettingsWindow::create(SETTINGS_PROCEDURES) {
                Ok(settings) => {
                    publish_appearance(settings.appearance());
                    self.settings = Some(settings);
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to create the settings window");
                    return;
                }
            }
        }
        self.render_settings(view);
        if let Some(settings) = &mut self.settings {
            settings.layout();
            settings.show();
        }
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

fn low_word(value: usize) -> u16 {
    u16::try_from(value & usize::from(u16::MAX)).expect("masked to 16 bits")
}

fn high_word(value: usize) -> u16 {
    low_word(value >> u16::BITS)
}

fn is_appearance_change(wparam: WPARAM, lparam: LPARAM) -> bool {
    u32::try_from(wparam.0) == Ok(SPI_SETHIGHCONTRAST.0) || is_color_set_change(lparam)
}

fn is_color_set_change(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    let area = PCWSTR(std::ptr::with_exposed_provenance(lparam.0.cast_unsigned()));
    // SAFETY: for WM_SETTINGCHANGE a non-zero lparam is a nul-terminated string valid for the message.
    unsafe { area.to_string() }.is_ok_and(|area| area == COLOR_SET_CHANGE)
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

extern "system" fn settings_window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled = match message {
        WM_CLOSE => {
            // Returns the previous visibility, not an error.
            // SAFETY: plain call on the window that got the message.
            let _ = unsafe { ShowWindow(window, SW_HIDE) };
            Some(HANDLED)
        }
        WM_ERASEBKGND => Some(DRAWN),
        WM_PAINT => with_appearance(|appearance| SettingsWindow::paint_background(window, appearance))
            .map(|()| HANDLED),
        WM_GETMINMAXINFO => {
            SettingsWindow::limit_size(window, lparam);
            Some(HANDLED)
        }
        WM_DRAWITEM => draw_settings_item(lparam),
        PICKER_MENU_MESSAGE => Some(show_picker_menu(window)),
        _ => with_runtime(|runtime| runtime.on_settings_message(window, message, wparam, lparam)).flatten(),
    };
    // SAFETY: forwards the system's own arguments unchanged.
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(window, message, wparam, lparam) })
}

extern "system" fn settings_page_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled = match message {
        WM_ERASEBKGND => Some(DRAWN),
        WM_CTLCOLOREDIT => with_appearance(|appearance| appearance.color_field(wparam)),
        WM_DRAWITEM => draw_settings_item(lparam),
        WM_PAINT => with_runtime(|runtime| runtime.paint_settings_page(window))
            .flatten()
            .or_else(|| paint_page_later(window)),
        WM_COMMAND => {
            if is_field_change(wparam) {
                repaint_field_frames(window);
            }
            with_runtime(|runtime| runtime.on_settings_message(window, message, wparam, lparam)).flatten()
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_CONTEXTMENU => give_to_field(window, message, wparam, lparam)
            .or_else(|| {
                with_runtime(|runtime| runtime.on_settings_message(window, message, wparam, lparam)).flatten()
            }),
        WM_SETCURSOR => show_field_cursor(window),
        WM_MOUSEMOVE | WM_LBUTTONUP | WM_MOUSELEAVE | WM_CAPTURECHANGED => {
            with_runtime(|runtime| runtime.on_settings_message(window, message, wparam, lparam)).flatten()
        }
        _ => None,
    };
    // SAFETY: forwards the system's own arguments unchanged.
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(window, message, wparam, lparam) })
}

/// Gives pointer input that a page got over one of its text fields to that field. The field
/// handles it outside the borrow of the runtime, because it tells the page while it does.
fn give_to_field(page: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    let input = with_runtime(|runtime| {
        runtime
            .shell
            .settings
            .as_ref()?
            .field_input(page, message, wparam, lparam)
    })
    .flatten()?;
    Some(input.send())
}

fn show_field_cursor(page: HWND) -> Option<LRESULT> {
    with_runtime(|runtime| {
        runtime
            .shell
            .settings
            .as_ref()
            .is_some_and(|settings| settings.show_field_cursor(page))
    })
    .filter(|is_shown| *is_shown)
    .map(|_| CURSOR_SET)
}

/// Draws a settings control with the current appearance; it needs no runtime state.
fn draw_settings_item(lparam: LPARAM) -> Option<LRESULT> {
    with_appearance(|appearance| SettingsWindow::draw_item(lparam, appearance))
        .filter(|is_drawn| *is_drawn)
        .map(|_| DRAWN)
}

/// Whether a `WM_COMMAND` tells that a text field gained or lost focus or changed its text,
/// which changes the frame and placeholder that its page draws.
fn is_field_change(wparam: WPARAM) -> bool {
    matches!(
        u32::from(high_word(wparam.0)),
        EN_SETFOCUS | EN_KILLFOCUS | EN_CHANGE
    )
}

fn repaint_field_frames(page: HWND) {
    // Returns whether it invalidated; a failure only delays the repaint.
    // SAFETY: plain call on the window that got the message.
    let _ = unsafe { InvalidateRect(Some(page), None, false) };
}

/// Shows the menu of the pending picker below it and applies the pick. The menu runs a message
/// loop of its own, so it shows outside any borrow of the runtime.
fn show_picker_menu(window: HWND) -> LRESULT {
    let pending = with_runtime(|runtime| {
        let picker = runtime.pending_menu.take()?;
        let menu = runtime.shell.settings.as_ref()?.picker_menu(&picker)?;
        Some((picker, menu))
    })
    .flatten();
    let Some((picker, picker_menu)) = pending else {
        return HANDLED;
    };
    let labels: Vec<&str> = picker_menu.titles.iter().map(String::as_str).collect();
    let placement = MenuPlacement::Below(picker_menu.anchor);
    match menu::pick(window, &labels, Some(picker_menu.selected), placement) {
        Ok(Some(index)) => {
            with_runtime(|runtime| runtime.on_menu_pick(picker, index));
        }
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "failed to show a picker menu"),
    }
    HANDLED
}

/// Paints the background of a page whose state is busy, and asks for a full paint once it is free.
fn paint_page_later(window: HWND) -> Option<LRESULT> {
    with_appearance(|appearance| SettingsWindow::paint_background(window, appearance))?;
    // Returns whether it invalidated; a failure only leaves the cards out until the next paint.
    // SAFETY: plain call on the window that got the message.
    let _ = unsafe { InvalidateRect(Some(window), None, false) };
    Some(HANDLED)
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
