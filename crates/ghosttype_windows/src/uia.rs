//! The focused control as UI Automation (UIA) tells it: the text around its caret, its labels, the
//! web page that holds it, and its caret for apps that draw a caret of their own and have no
//! system caret and no caret object (such as WinUI apps).

mod com;
mod drawn_caret;
mod field;
mod screen;

use std::time::Duration;

use ghosttype_core::desktop::ScreenRect;
use windows::Win32::Foundation::RECT;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationTextRange, UIA_TextPatternId,
};
use windows::core::{BOOL, Interface, Result};

use self::com::ComApartment;
pub(crate) use self::field::{ShownField, UiaField};
pub(crate) use self::screen::FocusedField;

/// The class of the hidden input box of a terminal of xterm.js, such as the terminal of VS Code.
const XTERM_INPUT_CLASS: &str = "xterm-helper-textarea";

/// A connection to UIA, usable only on the thread that made it.
#[derive(Debug)]
pub(crate) struct Uia {
    automation: IUIAutomation,
    /// Declared after `automation`, so that COM ends on this thread after the last interface goes.
    _apartment: ComApartment,
}

impl Uia {
    /// Joins the multithreaded COM apartment on this thread and connects to UIA.
    pub(crate) fn connect() -> Result<Self> {
        let apartment = ComApartment::join()?;
        // SAFETY: plain call; COM is initialized on this thread.
        let automation = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }?;
        Ok(Self {
            automation,
            _apartment: apartment,
        })
    }

    /// Makes each request to an app fail after `connection` without an answer to the connection,
    /// or after `transaction` without the answer to the request, so that a busy app does not
    /// hold this thread.
    pub(crate) fn limit_waits(&self, connection: Duration, transaction: Duration) {
        let to_ms = |wait: Duration| u32::try_from(wait.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: plain calls on interfaces of this thread.
        let limited = self
            .automation
            .cast::<IUIAutomation2>()
            .and_then(|automation| unsafe {
                automation.SetConnectionTimeout(to_ms(connection))?;
                automation.SetTransactionTimeout(to_ms(transaction))
            });
        if let Err(error) = limited {
            tracing::warn!(%error, "failed to limit the waits for UI Automation");
        }
    }

    fn focused_element(&self) -> Option<IUIAutomationElement> {
        // SAFETY: plain call on an interface of this thread.
        unsafe { self.automation.GetFocusedElement() }.ok()
    }
}

fn is_terminal_input(element: &IUIAutomationElement) -> bool {
    // SAFETY: plain call on an interface of this thread.
    unsafe { element.CurrentClassName() }.is_ok_and(|class| class == XTERM_INPUT_CLASS)
}

/// Whether `element` hides its text. An element that does not tell counts as a password field.
fn is_password(element: &IUIAutomationElement) -> bool {
    // SAFETY: plain call on an interface of this thread.
    unsafe { element.CurrentIsPassword() }.map_or(true, BOOL::as_bool)
}

/// The selection of `element`, which is the empty range at the caret when nothing is selected.
fn caret_range(element: &IUIAutomationElement) -> Option<IUIAutomationTextRange> {
    // SAFETY: plain calls on interfaces of this thread.
    unsafe {
        let pattern: IUIAutomationTextPattern = element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        pattern.GetSelection().ok()?.GetElement(0).ok()
    }
}

pub(crate) fn screen_rect(rect: RECT) -> ScreenRect {
    ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}
