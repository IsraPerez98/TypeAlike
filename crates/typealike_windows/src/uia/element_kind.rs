//! The kind of the focused element: the UI framework that made it and its control type.
//!
//! The kind holds no text of the element, so a record of the log can name it.

use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, UIA_ControlTypePropertyId, UIA_FrameworkIdPropertyId,
};

use super::Uia;

/// The focused element of a read, for a later request about it on the thread of the read.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FocusedElement(pub(super) IUIAutomationElement);

/// The kind of a UIA element. It holds no text of the element.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ElementKind {
    /// The UIA framework id, such as `Win32`, `WPF`, `XAML` or `Chrome`.
    pub(crate) framework: String,
    /// The UIA control type id, such as 50004 for an edit control.
    pub(crate) control_type: i32,
}

impl Uia {
    /// Reads the kind of `element` in one request to its app. `None` when the request fails.
    pub(crate) fn kind_of(&self, element: &FocusedElement) -> Option<ElementKind> {
        let cache = self.cache_request(&[UIA_FrameworkIdPropertyId, UIA_ControlTypePropertyId])?;
        // SAFETY: plain calls on interfaces of this thread.
        unsafe {
            let cached = element.0.BuildUpdatedCache(&cache).ok()?;
            Some(ElementKind {
                framework: cached.CachedFrameworkId().ok()?.to_string(),
                control_type: cached.CachedControlType().ok()?.0,
            })
        }
    }
}
