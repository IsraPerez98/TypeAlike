//! Windows owned by GhostType, each with its own registered class.

use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, IDC_ARROW, LoadCursorW, RegisterClassExW, UnregisterClassW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW, WNDPROC,
};
use windows::core::{Error, PCWSTR, Result};

/// What a window looks like to the system.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowSpec {
    pub(crate) class_name: PCWSTR,
    pub(crate) procedure: WNDPROC,
    pub(crate) style: WINDOW_STYLE,
    pub(crate) extended_style: WINDOW_EX_STYLE,
    /// The window that contains this one, for a child window.
    pub(crate) parent: Option<HWND>,
}

/// A window and its class, destroyed and unregistered on drop.
#[derive(Debug)]
pub(crate) struct Window {
    handle: HWND,
    #[expect(
        dead_code,
        reason = "held to unregister the class after the window is destroyed"
    )]
    class: WindowClass,
}

impl Window {
    /// Registers the class and creates a hidden window of it.
    pub(crate) fn create(spec: WindowSpec) -> Result<Self> {
        let class = WindowClass::register(spec)?;
        // SAFETY: the class was just registered; the static names outlive the call.
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
                spec.parent,
                None,
                Some(class.instance),
                None,
            )
        }?;
        Ok(Self { handle, class })
    }

    /// Raw window handle, valid while `self` lives.
    pub(crate) fn handle(&self) -> HWND {
        self.handle
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

#[derive(Debug)]
struct WindowClass {
    name: PCWSTR,
    instance: HINSTANCE,
}

impl WindowClass {
    fn register(spec: WindowSpec) -> Result<Self> {
        // SAFETY: plain call.
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }?.into();
        // SAFETY: plain call that loads a shared system cursor, which is never freed.
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>()
                .try_into()
                .expect("class size fits in u32"),
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
        // SAFETY: the class was registered by this type and its window is already destroyed.
        if let Err(error) = unsafe { UnregisterClassW(self.name, Some(self.instance)) } {
            tracing::warn!(%error, "failed to unregister window class");
        }
    }
}
