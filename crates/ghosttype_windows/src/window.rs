//! Windows owned by GhostType, and the classes that GhostType registers for them.

use std::rc::Rc;

use windows::Win32::Foundation::{HINSTANCE, HWND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, IDC_ARROW, LoadCursorW, RegisterClassExW, UnregisterClassW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASS_STYLES, WNDCLASSEXW, WNDPROC,
};
use windows::core::{Error, PCWSTR, Result};

/// What a window looks like to the system.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowSpec {
    pub(crate) class_name: PCWSTR,
    pub(crate) procedure: WNDPROC,
    pub(crate) class_style: WNDCLASS_STYLES,
    pub(crate) style: WINDOW_STYLE,
    pub(crate) extended_style: WINDOW_EX_STYLE,
    /// The window that contains this one, for a child window.
    pub(crate) parent: Option<HWND>,
}

/// A window, destroyed on drop. Its class goes away with the last window of it.
#[derive(Debug)]
pub(crate) struct Window {
    handle: HWND,
    #[expect(
        dead_code,
        reason = "held to unregister the class after the window is destroyed"
    )]
    class: Rc<WindowClass>,
}

impl Window {
    /// Registers a class of its own and creates a hidden window of it.
    pub(crate) fn create(spec: WindowSpec) -> Result<Self> {
        WindowClass::register(spec)?.create_window()
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

/// A registered class, and how its windows look to the system. It is unregistered when it and
/// all of its windows are gone.
#[derive(Debug)]
pub(crate) struct WindowClass {
    spec: WindowSpec,
    instance: HINSTANCE,
}

impl WindowClass {
    /// Registers the class of `spec`.
    pub(crate) fn register(spec: WindowSpec) -> Result<Rc<Self>> {
        // SAFETY: plain call.
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }?.into();
        // SAFETY: plain call that loads a shared system cursor, which is never freed.
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }?;
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>()
                .try_into()
                .expect("class size fits in u32"),
            style: spec.class_style,
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
        Ok(Rc::new(Self { spec, instance }))
    }

    /// Creates a hidden window of this class, with the styles and the parent of its spec.
    pub(crate) fn create_window(self: &Rc<Self>) -> Result<Window> {
        let spec = self.spec;
        // SAFETY: the class is registered while `self` lives; the static names outlive the call.
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
                Some(self.instance),
                None,
            )
        }?;
        Ok(Window {
            handle,
            class: Rc::clone(self),
        })
    }
}

impl Drop for WindowClass {
    fn drop(&mut self) {
        // SAFETY: the class was registered by this type, and each of its windows holds it, so
        // they are all destroyed.
        if let Err(error) = unsafe { UnregisterClassW(self.spec.class_name, Some(self.instance)) } {
            tracing::warn!(%error, "failed to unregister window class");
        }
    }
}
