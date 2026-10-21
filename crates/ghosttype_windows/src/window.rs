//! Windows owned by GhostType, each with a class of its own.
//!
//! It also gives the id of a window for another thread, posts messages to a window, and reads the
//! words that a message packs in its parameters.

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_TOPMOST, IDC_ARROW, IsWindowVisible, LoadCursorW,
    PostMessageW, RegisterClassExW, SW_HIDE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    SetWindowPos, ShowWindow, UnregisterClassW, WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW, WNDPROC,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{Error, PCWSTR, Result};

use crate::struct_size;

/// A window, as a value that can go to another thread, which an `HWND` cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WindowId(isize);

impl WindowId {
    /// The handle of the window, for a Win32 call.
    pub(crate) fn to_hwnd(self) -> HWND {
        HWND(std::ptr::with_exposed_provenance_mut(self.0.cast_unsigned()))
    }
}

impl From<HWND> for WindowId {
    fn from(window: HWND) -> Self {
        Self(window.0.expose_provenance().cast_signed())
    }
}

/// Posts `message`, without parameters, to `window`, which can be a window of another thread.
pub(crate) fn post_message(window: HWND, message: u32) -> Result<()> {
    // SAFETY: the message carries no pointer, and the call fails safely when the window is gone.
    unsafe { PostMessageW(Some(window), message, WPARAM(0), LPARAM(0)) }
}

/// The low word of `value`: its 16 lowest bits.
pub(crate) fn low_word(value: usize) -> u16 {
    u16::try_from(value & usize::from(u16::MAX)).expect("masked to 16 bits")
}

/// The high word of `value`: the 16 bits above its low word.
pub(crate) fn high_word(value: usize) -> u16 {
    low_word(value >> u16::BITS)
}

/// What a window looks like to the system.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowSpec {
    pub(crate) class_name: PCWSTR,
    pub(crate) procedure: WNDPROC,
    pub(crate) style: WINDOW_STYLE,
    pub(crate) extended_style: WINDOW_EX_STYLE,
}

impl WindowSpec {
    /// A popup that floats above other windows, never takes focus and lets clicks through. It is
    /// not in the taskbar or in Alt+Tab.
    pub(crate) fn floating(class_name: PCWSTR) -> Self {
        Self {
            class_name,
            procedure: Some(default_procedure),
            style: WS_POPUP,
            extended_style: WS_EX_NOACTIVATE
                | WS_EX_TRANSPARENT
                | WS_EX_LAYERED
                | WS_EX_TOOLWINDOW
                | WS_EX_TOPMOST,
        }
    }
}

/// A top-level window, destroyed on drop. Its class goes away with it.
#[derive(Debug)]
pub(crate) struct Window {
    handle: HWND,
    /// Held to unregister the class after the window is destroyed.
    _class: WindowClass,
}

impl Window {
    /// Registers a class of its own and creates a hidden window of it.
    pub(crate) fn create(spec: WindowSpec) -> Result<Self> {
        let class = WindowClass::register(&spec)?;
        // SAFETY: the class is registered while `class` lives; the static names outlive the call.
        let handle = unsafe {
            CreateWindowExW(
                spec.extended_style,
                spec.class_name,
                spec.class_name,
                spec.style,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(class.instance),
                None,
            )
        }?;
        Ok(Self {
            handle,
            _class: class,
        })
    }

    /// Raw window handle, valid while `self` lives.
    pub(crate) fn handle(&self) -> HWND {
        self.handle
    }

    /// Shows the window above every other window, without focus.
    pub(crate) fn show_on_top(&self) -> Result<()> {
        let placement = SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW;
        // Raises the window above always-on-top hosts, which ShowWindow alone would leave it under.
        // SAFETY: plain call on a window this type owns.
        unsafe { SetWindowPos(self.handle, Some(HWND_TOPMOST), 0, 0, 0, 0, placement) }
    }

    pub(crate) fn hide(&self) {
        // Returns the previous visibility, not an error.
        // SAFETY: plain call on a window this type owns.
        let _ = unsafe { ShowWindow(self.handle, SW_HIDE) };
    }

    pub(crate) fn is_visible(&self) -> bool {
        // SAFETY: plain call on a window this type owns.
        unsafe { IsWindowVisible(self.handle) }.as_bool()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: the window was created by this type on this thread and is destroyed once.
        if let Err(error) = unsafe { DestroyWindow(self.handle) } {
            tracing::warn!(%error, "failed to destroy window");
        }
    }
}

/// A registered window class, unregistered on drop.
#[derive(Debug)]
struct WindowClass {
    name: PCWSTR,
    instance: HINSTANCE,
}

impl WindowClass {
    /// Registers the class of `spec`.
    fn register(spec: &WindowSpec) -> Result<Self> {
        // SAFETY: plain call.
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }?.into();
        // SAFETY: plain call that loads a shared system cursor, which is never freed.
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }?;
        let class = WNDCLASSEXW {
            cbSize: struct_size::<WNDCLASSEXW>(),
            lpfnWndProc: spec.procedure,
            hInstance: instance,
            hCursor: cursor,
            lpszClassName: spec.class_name,
            ..WNDCLASSEXW::default()
        };
        // SAFETY: the class description and its static name outlive the call.
        if unsafe { RegisterClassExW(&raw const class) } == 0 {
            return Err(Error::from_thread());
        }
        Ok(Self {
            name: spec.class_name,
            instance,
        })
    }
}

impl Drop for WindowClass {
    fn drop(&mut self) {
        // SAFETY: the class was registered by this type, and its window is destroyed before it.
        if let Err(error) = unsafe { UnregisterClassW(self.name, Some(self.instance)) } {
            tracing::warn!(%error, "failed to unregister window class");
        }
    }
}

extern "system" fn default_procedure(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // SAFETY: forwards the system's own arguments unchanged.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}
