//! The message loop, and the one place Win32 callbacks reach GhostType's state.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use ghosttype_core::app::{App, INPUT_SETTLE_DELAY, TrayCommand, TrayOutcome};
use ghosttype_core::engine::Reaction;
use ghosttype_core::general_pane::GeneralPane;
use ghosttype_core::hub::{HubEventSink, ModelHub};
use ghosttype_core::inference::{InferenceEvent, InferenceEventSink, InferenceRequest, ModelRunner};
use ghosttype_core::models_pane::ModelsPane;
use ghosttype_core::playground::PlaygroundPane;
use ghosttype_core::settings::{Settings, SettingsStore};
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

use crate::clipboard::ClipboardWatch;
use crate::desktop::ForegroundDesktop;
use crate::focus::{self, FocusChange, FocusTracker};
use crate::injection::{self, INJECTION_TAG};
use crate::keys::{self, Disposition, ReleaseFilter};
use crate::mailbox::{DELIVERY_MESSAGE, Delivery, Mailbox};
use crate::menu::{self, MenuPlacement};
use crate::overlay::Overlay;
use crate::settings::{self, Appearance, MenuPicker, Pointer, Procedures, SettingsInput, SettingsWindow};
use crate::system_theme::{self, SystemTheme};
use crate::tray::{self, MenuRequest, TRAY_CALLBACK_MESSAGE, Tray};
use crate::window::{Window, WindowSpec};
use crate::{RunError, Services};

const GHOST_TIMER_ID: usize = 1;
const SETTLE_TIMER_ID: usize = 2;
const CARET_SETTLE_DELAY_MS: u32 = 40;
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
    models_page: Some(settings_page_procedure),
    playground_page: Some(settings_page_procedure),
    general_page: Some(settings_page_procedure),
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
/// The settings window starts with `settings` and uses `services`.
pub(crate) fn run(mut app: App, settings: Settings, services: Services) -> Result<(), RunError> {
    let Services {
        hub,
        mut runner,
        settings_store,
        autostart,
    } = services;
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
    let tray = Tray::add(window.handle(), &app.tray_view()).map_err(RunError::during("add the tray icon"))?;
    // SAFETY: plain call with a static, nul-terminated name.
    let taskbar_created_message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    let _keyboard_hook = install_hook(WH_KEYBOARD_LL, Some(keyboard_procedure))
        .map_err(RunError::during("install the keyboard hook"))?;
    let _mouse_hook = install_hook(WH_MOUSE_LL, Some(mouse_procedure))
        .map_err(RunError::during("install the mouse hook"))?;
    let (mailbox, deliveries) = Mailbox::new(window.handle());
    app.use_settings(&settings);
    let mut playground = PlaygroundPane::with_backend(settings.backend);
    playground.use_settings(&settings);
    let clipboard = ClipboardWatch::start(window.handle(), settings.clipboard_access);
    let general = GeneralPane::new(settings).with_autostart(autostart);
    let runner_sink = mailbox.sink(Delivery::Playground);
    if let Some(request) = general.load_suggestion_model(&mut playground) {
        runner.submit(request, &runner_sink);
    }
    STATE.with(|state| {
        state.runtime.replace(Some(Runtime {
            app,
            focus: FocusTracker::new(),
            tray,
            overlay,
            window,
            taskbar_created_message,
            pending_ghost: None,
            is_ghost_timer_pending: false,
            pending_injection: None,
            release_filter: ReleaseFilter::default(),
            models: ModelsPane::new(),
            hub,
            hub_sink: mailbox.sink(Delivery::Hub),
            playground,
            general,
            settings_store,
            runner,
            runner_sink,
            clipboard,
            suggestion_sink: mailbox.sink(Delivery::Suggestion),
            deliveries,
            settings: None,
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
        with_runtime(|runtime| runtime.settings.as_ref().map(SettingsWindow::handle)).flatten()
    else {
        return false;
    };
    // SAFETY: `message` was filled in by GetMessageW and the settings window is alive.
    if !unsafe { IsDialogMessageW(settings, &raw const *message) }.as_bool() {
        return false;
    }
    if message.message == WM_KEYDOWN && message.wParam.0 == usize::from(VK_TAB.0) {
        with_runtime(|runtime| runtime.settings.as_mut().map(SettingsWindow::reveal_focus));
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
    app: App,
    focus: FocusTracker,
    tray: Tray,
    overlay: Overlay,
    window: Window,
    taskbar_created_message: u32,
    pending_ghost: Option<String>,
    is_ghost_timer_pending: bool,
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
    clipboard: ClipboardWatch,
    deliveries: mpsc::Receiver<Delivery>,
    settings: Option<SettingsWindow>,
    /// The picker whose menu must show once the runtime is free.
    pending_menu: Option<MenuPicker>,
}

impl Runtime {
    fn on_keyboard(&mut self, message: u32, event: &KBDLLHOOKSTRUCT) -> Disposition {
        if event.dwExtraInfo == INJECTION_TAG {
            return Disposition::Pass;
        }
        self.restart_settle_timer();
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
        if self.focus.refresh() == FocusChange::Changed {
            self.on_context_lost();
        }
        let Some(core_key) = keys::translate(key, keys::current_chord(), keys::typed_char(key, scan_code))
        else {
            return Disposition::Pass;
        };
        let desktop = ForegroundDesktop {
            focus: &self.focus,
            clipboard: &self.clipboard,
        };
        let reaction = self.app.on_key(core_key, &desktop);
        let disposition = self.apply(reaction);
        self.release_filter.on_press(key, disposition);
        disposition
    }

    fn on_context_lost(&mut self) {
        let reaction = self.app.on_context_lost();
        self.apply(reaction);
    }

    fn restart_settle_timer(&mut self) {
        let delay_ms = u32::try_from(INPUT_SETTLE_DELAY.as_millis()).expect("the settle delay fits in u32");
        if !self.start_timer(SETTLE_TIMER_ID, delay_ms) {
            tracing::warn!(error = %Error::from_thread(), "failed to schedule the prompt preparation");
        }
    }

    fn on_input_settled(&mut self) {
        self.stop_timer(SETTLE_TIMER_ID);
        if self.focus.refresh() == FocusChange::Changed {
            self.on_context_lost();
        }
        let desktop = ForegroundDesktop {
            focus: &self.focus,
            clipboard: &self.clipboard,
        };
        if let Some(request) = self.app.on_input_settled(&desktop) {
            self.runner.submit(request, &self.suggestion_sink);
        }
    }

    /// Starts the window timer `timer_id`, or restarts it when it runs. Returns whether it runs.
    fn start_timer(&self, timer_id: usize, delay_ms: u32) -> bool {
        // SAFETY: plain call on our own window.
        unsafe { SetTimer(Some(self.window.handle()), timer_id, delay_ms, None) != 0 }
    }

    fn stop_timer(&self, timer_id: usize) {
        // Fails only when no timer is pending, which needs no handling.
        // SAFETY: plain call on our own window.
        let _ = unsafe { KillTimer(Some(self.window.handle()), timer_id) };
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
                // SAFETY: plain call on our own window.
                let posted =
                    unsafe { PostMessageW(Some(self.window.handle()), INJECT_MESSAGE, WPARAM(0), LPARAM(0)) };
                if let Err(error) = posted {
                    tracing::warn!(%error, "failed to queue accepted text");
                }
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
        let reaction = match event {
            InferenceEvent::Partial(text) => self.app.on_partial_completion(&text),
            InferenceEvent::Completed(result) => Some(self.app.on_completion(result)),
            InferenceEvent::Loaded { .. } => None,
        };
        match reaction {
            // A completion does not move the caret, so it must not make the wait for the caret longer.
            Some(Reaction::Show(text)) if self.is_ghost_timer_pending => self.pending_ghost = Some(text),
            Some(reaction) => {
                self.apply(reaction);
            }
            None => {}
        }
    }

    fn show_ghost_after_caret_settles(&mut self, text: String) {
        self.pending_ghost = Some(text);
        self.restart_ghost_timer();
    }

    fn restart_ghost_timer(&mut self) {
        let is_started = self.start_timer(GHOST_TIMER_ID, CARET_SETTLE_DELAY_MS);
        if !is_started {
            tracing::warn!(error = %Error::from_thread(), "failed to schedule ghost text");
        }
        self.is_ghost_timer_pending = is_started;
    }

    fn hide_ghost(&mut self) {
        self.pending_ghost = None;
        self.stop_ghost_timer();
        self.overlay.hide();
    }

    fn stop_ghost_timer(&mut self) {
        self.is_ghost_timer_pending = false;
        self.stop_timer(GHOST_TIMER_ID);
    }

    fn show_pending_ghost(&mut self) {
        self.stop_ghost_timer();
        let Some(text) = &self.pending_ghost else {
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

    fn on_tray_command(&mut self, command: TrayCommand) {
        match self.app.on_tray_command(command) {
            TrayOutcome::Paused => {
                self.hide_ghost();
                let request = self.playground.unload_model();
                self.on_status_change(request);
            }
            TrayOutcome::Resumed => {
                let request = self.general.load_suggestion_model(&mut self.playground);
                self.on_status_change(request);
            }
            TrayOutcome::OpenSettings => self.open_settings(),
            // SAFETY: plain call.
            TrayOutcome::Quit => unsafe { PostQuitMessage(0) },
        }
    }

    fn on_status_change(&mut self, model_request: Option<InferenceRequest>) {
        if let Some(request) = model_request {
            self.runner.submit(request, &self.runner_sink);
        }
        self.refresh_tray();
        self.render_settings();
    }

    fn refresh_tray(&mut self) {
        let view = self.app.tray_view();
        if let Err(error) = self.tray.update(&view) {
            tracing::warn!(%error, "failed to update tray icon");
        }
    }

    fn on_window_message(&mut self, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        match message {
            WM_TIMER if wparam.0 == GHOST_TIMER_ID => self.show_pending_ghost(),
            WM_TIMER if wparam.0 == SETTLE_TIMER_ID => self.on_input_settled(),
            WM_SETTINGCHANGE if is_appearance_change(wparam, lparam) => self.on_appearance_change(),
            DELIVERY_MESSAGE => self.take_deliveries(),
            WM_CLIPBOARDUPDATE => self.clipboard.on_update(),
            _ if message == self.taskbar_created_message => {
                if let Err(error) = self.tray.register() {
                    tracing::warn!(%error, "failed to re-add tray icon");
                }
            }
            _ => return None,
        }
        Some(HANDLED)
    }

    fn on_appearance_change(&mut self) {
        system_theme::match_menus_to(SystemTheme::read());
        self.refresh_tray();
        let Some(settings) = &mut self.settings else {
            return;
        };
        match settings.change_theme() {
            Ok(()) => publish_appearance(settings.appearance()),
            Err(error) => tracing::warn!(%error, "failed to redraw settings for the new theme"),
        }
    }

    fn take_deliveries(&mut self) {
        let mut has_pane_changes = false;
        while let Ok(delivery) = self.deliveries.try_recv() {
            match delivery {
                Delivery::Hub(event) => {
                    self.models.on_event(event);
                    has_pane_changes = true;
                }
                Delivery::Playground(event) => {
                    self.playground.on_event(event);
                    if let Some(settings) = self.general.keep_suggestion_model(&self.playground) {
                        self.settings_store.save(&settings);
                    }
                    has_pane_changes = true;
                }
                Delivery::Suggestion(event) => self.on_suggestion_event(event),
            }
        }
        if has_pane_changes {
            self.render_settings();
        }
    }

    fn open_settings(&mut self) {
        let request = self.models.on_open();
        self.hub.submit(request, &self.hub_sink);
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
        self.render_settings();
        if let Some(settings) = &mut self.settings {
            settings.layout();
            settings.show();
        }
    }

    fn render_settings(&mut self) {
        let Some(settings) = &mut self.settings else {
            return;
        };
        let models = self.models.view();
        let installed = self.models.installed_files();
        let playground = self.playground.view(&installed);
        let general = self.general.view(&self.playground, &installed);
        if let Err(error) = settings.render(&models, &playground, &general) {
            tracing::warn!(%error, "failed to render settings");
        }
    }

    fn on_settings_input(&mut self, input: SettingsInput) {
        match input {
            SettingsInput::Models(action) => {
                if let Some(request) = self.models.on_action(action) {
                    self.hub.submit(request, &self.hub_sink);
                }
            }
            SettingsInput::Playground(action) => {
                if let Some(request) = self.playground.on_action(action) {
                    self.runner.submit(request, &self.runner_sink);
                }
            }
            SettingsInput::General(action) => {
                let outcome = self.general.on_action(action, &mut self.playground);
                if let Some(settings) = outcome.settings {
                    self.settings_store.save(&settings);
                    self.app.use_settings(&settings);
                    self.playground.use_settings(&settings);
                    self.clipboard.use_access(settings.clipboard_access);
                }
                if let Some(request) = outcome.request {
                    self.runner.submit(request, &self.runner_sink);
                }
            }
            SettingsInput::OpenMenu(picker) => {
                self.post_picker_menu(picker);
                return;
            }
        }
        self.render_settings();
    }

    fn post_picker_menu(&mut self, picker: MenuPicker) {
        let Some(settings) = &self.settings else {
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

    fn on_menu_pick(&mut self, picker: MenuPicker, index: usize) {
        let input = self
            .settings
            .as_ref()
            .and_then(|settings| settings.input_for_pick(picker, index));
        if let Some(input) = input {
            self.on_settings_input(input);
        }
    }

    fn on_settings_message(
        &mut self,
        window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        let settings = self.settings.as_mut()?;
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
        let Some(settings) = &mut self.settings else {
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
        self.settings.as_ref()?.paint_page(page);
        Some(HANDLED)
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
        let menu = runtime.settings.as_ref()?.picker_menu(picker);
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
    let Some(view) = with_runtime(|runtime| runtime.app.tray_view()) else {
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
    let text = with_runtime(|runtime| runtime.pending_injection.take()).flatten();
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
        with_runtime(Runtime::on_context_lost);
    }
    if code == HC_ACTION.cast_signed() && is_button_up {
        with_runtime(Runtime::restart_settle_timer);
    }
    // SAFETY: forwards the system's own arguments unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}
