//! The thread that runs the settings window, and the one place its Win32 callbacks reach the
//! window's state. The window shows the views that the controller sends and sends back what the
//! user does, so that work in the window never delays the keyboard and mouse hooks.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use ghosttype_core::settings_view::{ControlId, SettingsInput, SettingsView};
use windows::Win32::Foundation::{E_UNEXPECTED, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_TAB;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, EN_CHANGE, EN_KILLFOCUS, EN_SETFOCUS, HWND_MESSAGE, IDCANCEL, IDOK, IsDialogMessageW,
    MSG, PostMessageW, PostQuitMessage, SW_HIDE, ShowWindow, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP,
    WM_CAPTURECHANGED, WM_CLOSE, WM_COMMAND, WM_CONTEXTMENU, WM_CTLCOLOREDIT, WM_DPICHANGED, WM_DRAWITEM,
    WM_ERASEBKGND, WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETCURSOR, WM_SETTINGCHANGE, WM_SIZE, WNDCLASS_STYLES,
};
use windows::core::{Error, Result, w};

use crate::mailbox::Mailbox;
use crate::menu::{self, MenuPlacement};
use crate::settings::{self, Appearance, Pointer, Procedures, SettingsWindow, WindowInput};
use crate::system_theme;
use crate::window::{self, Window, WindowSpec};

const THREAD_NAME: &str = "settings window";
const COMMAND_MESSAGE: u32 = WM_APP + 1;
const PICKER_MENU_MESSAGE: u32 = WM_APP + 4;
const HANDLED: LRESULT = LRESULT(0);
const DRAWN: LRESULT = LRESULT(1);
const CURSOR_SET: LRESULT = LRESULT(1);
const SETTINGS_PROCEDURES: Procedures = Procedures {
    window: Some(settings_window_procedure),
    page: Some(settings_page_procedure),
};

/// What the controller asks of the settings window.
#[derive(Debug)]
enum Command {
    Render(SettingsView),
    Show(SettingsView),
    Quit,
}

/// Everything Win32 callbacks on the settings thread can reach. The appearance has its own cell
/// because controls ask to draw while the runtime is busy changing them.
struct ThreadState {
    runtime: RefCell<Option<SettingsRuntime>>,
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

/// The settings window on a thread of its own, from the side of the controller. Drop ends the
/// thread and closes the window.
#[derive(Debug)]
pub(crate) struct SettingsThread {
    commands: Mailbox<Command>,
    thread: Option<JoinHandle<()>>,
}

impl SettingsThread {
    /// Starts the thread. The window sends each input of the user to `inputs`.
    pub(crate) fn start(inputs: Mailbox<SettingsInput>) -> Result<Self> {
        let (started, start) = mpsc::channel();
        let thread = thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || run(inputs, &started))?;
        let commands = start
            .recv()
            .map_err(|error| Error::new(E_UNEXPECTED, error.to_string()))??;
        Ok(Self {
            commands,
            thread: Some(thread),
        })
    }

    /// Shows `view` in the settings window, when the window exists.
    pub(crate) fn render(&self, view: SettingsView) {
        self.commands.send(Command::Render(view));
    }

    /// Shows the settings window with `view` and brings it to the front. Makes the window the
    /// first time.
    pub(crate) fn show(&self, view: SettingsView) {
        self.commands.send(Command::Show(view));
    }
}

impl Drop for SettingsThread {
    fn drop(&mut self) {
        self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::warn!("the settings window thread panicked");
        }
    }
}

/// Runs the thread: makes the window that takes the commands, gives its mailbox to `started`,
/// then handles messages until [`Command::Quit`].
fn run(inputs: Mailbox<SettingsInput>, started: &mpsc::Sender<Result<Mailbox<Command>>>) {
    let command_window = match Window::create(WindowSpec {
        class_name: w!("GhostType.SettingsCommands"),
        procedure: Some(command_window_procedure),
        class_style: WNDCLASS_STYLES::default(),
        style: WINDOW_STYLE::default(),
        extended_style: WINDOW_EX_STYLE::default(),
        parent: Some(HWND_MESSAGE),
    }) {
        Ok(window) => window,
        Err(error) => {
            // Fails only when the controller stopped to wait, which needs no handling.
            let _ = started.send(Err(error));
            return;
        }
    };
    let (commands, receiver) = Mailbox::new(command_window.handle(), COMMAND_MESSAGE);
    STATE.with(|state| {
        state.runtime.replace(Some(SettingsRuntime {
            window: None,
            commands: receiver,
            inputs,
            pending_menu: None,
        }))
    });
    if started.send(Ok(commands)).is_ok()
        && let Err(error) = window::pump_messages(is_settings_dialog_message)
    {
        tracing::warn!(%error, "the settings window stopped");
    }
    STATE.with(|state| {
        state.runtime.take();
        state.appearance.take();
    });
}

/// Gives the settings window keyboard navigation (Tab, arrow keys, Enter, Escape) like a dialog.
fn is_settings_dialog_message(message: &MSG) -> bool {
    let Some(settings) =
        with_runtime(|runtime| runtime.window.as_ref().map(SettingsWindow::handle)).flatten()
    else {
        return false;
    };
    // SAFETY: `message` was filled in by GetMessageW and the settings window is alive.
    if !unsafe { IsDialogMessageW(settings, &raw const *message) }.as_bool() {
        return false;
    }
    if message.message == WM_KEYDOWN && message.wParam.0 == usize::from(VK_TAB.0) {
        with_runtime(|runtime| runtime.window.as_mut().map(SettingsWindow::reveal_focus));
    }
    true
}

fn with_runtime<Output>(action: impl FnOnce(&mut SettingsRuntime) -> Output) -> Option<Output> {
    STATE.with(|state| state.runtime.try_borrow_mut().ok()?.as_mut().map(action))
}

fn with_appearance<Output>(action: impl FnOnce(&Appearance) -> Output) -> Option<Output> {
    let appearance = STATE.with(|state| state.appearance.try_borrow().ok()?.clone())?;
    Some(action(&appearance))
}

fn publish_appearance(appearance: Rc<Appearance>) {
    STATE.with(|state| state.appearance.replace(Some(appearance)));
}

/// The settings window and the channels to the controller.
struct SettingsRuntime {
    window: Option<SettingsWindow>,
    commands: mpsc::Receiver<Command>,
    inputs: Mailbox<SettingsInput>,
    /// The picker whose menu must show once the runtime is free.
    pending_menu: Option<ControlId>,
}

impl SettingsRuntime {
    fn take_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Render(view) => self.render(&view),
                Command::Show(view) => self.show(&view),
                // SAFETY: plain call.
                Command::Quit => unsafe { PostQuitMessage(0) },
            }
        }
    }

    fn render(&mut self, view: &SettingsView) {
        let Some(window) = &mut self.window else {
            return;
        };
        if let Err(error) = window.render(view) {
            tracing::warn!(%error, "failed to render settings");
        }
    }

    fn show(&mut self, view: &SettingsView) {
        if self.window.is_none() {
            match SettingsWindow::create(SETTINGS_PROCEDURES) {
                Ok(window) => {
                    publish_appearance(window.appearance());
                    self.window = Some(window);
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to create the settings window");
                    return;
                }
            }
        }
        self.render(view);
        if let Some(window) = &mut self.window {
            window.layout();
            window.show();
        }
    }

    fn on_input(&mut self, input: WindowInput) {
        match input {
            WindowInput::Settings(input) => self.inputs.send(input),
            WindowInput::OpenMenu(picker) => self.post_picker_menu(picker),
        }
    }

    fn post_picker_menu(&mut self, picker: ControlId) {
        let Some(window) = &self.window else {
            return;
        };
        // SAFETY: plain call on our own window.
        let posted =
            unsafe { PostMessageW(Some(window.handle()), PICKER_MENU_MESSAGE, WPARAM(0), LPARAM(0)) };
        match posted {
            Ok(()) => self.pending_menu = Some(picker),
            Err(error) => tracing::warn!(%error, "failed to open a picker menu"),
        }
    }

    fn on_menu_pick(&self, picker: ControlId, index: usize) {
        self.inputs.send(SettingsInput::Picked { id: picker, index });
    }

    fn on_message(&mut self, window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
        let settings = self.window.as_mut()?;
        match message {
            WM_SIZE if window == settings.handle() => settings.layout(),
            WM_SETTINGCHANGE if system_theme::is_appearance_change(wparam, lparam) => {
                match settings.change_theme() {
                    Ok(()) => publish_appearance(settings.appearance()),
                    Err(error) => tracing::warn!(%error, "failed to redraw settings for the new theme"),
                }
            }
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
            WM_COMMAND => self.on_command(wparam, lparam),
            _ => return None,
        }
        Some(HANDLED)
    }

    fn on_command(&mut self, wparam: WPARAM, lparam: LPARAM) {
        let Some(settings) = &mut self.window else {
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
            self.on_input(input);
        }
    }

    fn paint_page(&self, page: HWND) -> Option<LRESULT> {
        self.window.as_ref()?.paint_page(page);
        Some(HANDLED)
    }
}

fn low_word(value: usize) -> u16 {
    u16::try_from(value & usize::from(u16::MAX)).expect("masked to 16 bits")
}

fn high_word(value: usize) -> u16 {
    low_word(value >> u16::BITS)
}

extern "system" fn command_window_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == COMMAND_MESSAGE {
        with_runtime(SettingsRuntime::take_commands);
        return HANDLED;
    }
    // SAFETY: forwards the system's own arguments unchanged.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
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
        WM_DRAWITEM => draw_item(lparam),
        PICKER_MENU_MESSAGE => Some(show_picker_menu(window)),
        _ => with_runtime(|runtime| runtime.on_message(window, message, wparam, lparam)).flatten(),
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
        WM_DRAWITEM => draw_item(lparam),
        WM_PAINT => with_runtime(|runtime| runtime.paint_page(window))
            .flatten()
            .or_else(|| paint_page_later(window)),
        WM_COMMAND => {
            if is_field_change(wparam) {
                repaint_field_frames(window);
            }
            with_runtime(|runtime| runtime.on_message(window, message, wparam, lparam)).flatten()
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_CONTEXTMENU => give_to_field(window, message, wparam, lparam)
            .or_else(|| {
                with_runtime(|runtime| runtime.on_message(window, message, wparam, lparam)).flatten()
            }),
        WM_SETCURSOR => show_field_cursor(window),
        WM_MOUSEMOVE | WM_LBUTTONUP | WM_MOUSELEAVE | WM_CAPTURECHANGED => {
            with_runtime(|runtime| runtime.on_message(window, message, wparam, lparam)).flatten()
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
            .window
            .as_ref()?
            .field_input(page, message, wparam, lparam)
    })
    .flatten()?;
    Some(input.send())
}

fn show_field_cursor(page: HWND) -> Option<LRESULT> {
    with_runtime(|runtime| {
        runtime
            .window
            .as_ref()
            .is_some_and(|settings| settings.show_field_cursor(page))
    })
    .filter(|is_shown| *is_shown)
    .map(|_| CURSOR_SET)
}

/// Draws a settings control with the current appearance; it needs no runtime state.
fn draw_item(lparam: LPARAM) -> Option<LRESULT> {
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

/// Shows the menu of the pending picker below it and sends the pick. The menu runs a message
/// loop of its own, so it shows outside any borrow of the runtime.
fn show_picker_menu(window: HWND) -> LRESULT {
    let pending = with_runtime(|runtime| {
        let picker = runtime.pending_menu.take()?;
        let menu = runtime.window.as_ref()?.picker_menu(&picker)?;
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
