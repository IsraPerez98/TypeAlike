//! The focused control as UI Automation (UIA) tells it: the text around its caret, its labels, the
//! web page that holds it, and its caret for apps that draw a caret of their own and have no
//! system caret and no caret object (such as WinUI apps).

mod com;
mod drawn_caret;
mod field;
mod screen;

use std::time::Duration;

use windows::Win32::Foundation::RECT;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::Variant::{VARIANT, VT_BSTR};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationCacheRequest, IUIAutomationElement,
    IUIAutomationTextPattern, IUIAutomationTextRange, UIA_PROPERTY_ID, UIA_TextPatternId,
};
use windows::core::{BOOL, Interface, Result};

use self::com::ComApartment;
pub(crate) use self::drawn_caret::FocusedCaret;
pub(crate) use self::field::{ShownField, UiaField};
pub(crate) use self::screen::FocusedField;
use crate::display;

/// The class of the hidden input box of a terminal of xterm.js, such as the terminal of VS Code.
const XTERM_INPUT_CLASS: &str = "xterm-helper-textarea";
/// The length that asks `GetText` for all the text of a range.
const WHOLE_RANGE: i32 = -1;

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

    /// A request that caches `properties` of each element that a call with it finds.
    fn cache_request(&self, properties: &[UIA_PROPERTY_ID]) -> Option<IUIAutomationCacheRequest> {
        // SAFETY: plain calls on interfaces of this thread.
        unsafe {
            let cache = self.automation.CreateCacheRequest().ok()?;
            for &property in properties {
                cache.AddProperty(property).ok()?;
            }
            Some(cache)
        }
    }
}

fn is_terminal_input(element: &IUIAutomationElement) -> bool {
    // SAFETY: plain call on an interface of this thread.
    unsafe { element.CurrentClassName() }.is_ok_and(|class| class == XTERM_INPUT_CLASS)
}

/// Reads the frame of `element` in physical screen pixels. `None` when it has no area.
fn frame_with_area(element: &IUIAutomationElement) -> Option<RECT> {
    // SAFETY: plain call on an interface of this thread.
    let frame = unsafe { element.CurrentBoundingRectangle() }.ok()?;
    display::has_area(frame).then_some(frame)
}

/// Whether `element` hides its text. An element that does not tell counts as a password field.
fn is_password(element: &IUIAutomationElement) -> bool {
    // SAFETY: plain call on an interface of this thread.
    unsafe { element.CurrentIsPassword() }.map_or(true, BOOL::as_bool)
}

/// Returns the text that `value` holds, or `None` when it holds no text.
fn text_of(value: &VARIANT) -> Option<String> {
    (value.vt() == VT_BSTR).then(|| {
        // SAFETY: the type of `value` tells that its union holds a BSTR.
        unsafe { value.Anonymous.Anonymous.Anonymous.bstrVal.to_string() }
    })
}

/// The selection of `element`, which is the empty range at the caret when nothing is selected.
fn caret_range(element: &IUIAutomationElement) -> Option<IUIAutomationTextRange> {
    // SAFETY: plain calls on interfaces of this thread.
    unsafe {
        let pattern: IUIAutomationTextPattern = element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        pattern.GetSelection().ok()?.GetElement(0).ok()
    }
}
