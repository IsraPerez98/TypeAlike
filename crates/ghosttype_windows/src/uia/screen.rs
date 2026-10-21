//! The screen text that UIA reads.
//!
//! It is the visible text elements of the foreground window, and where the focused field and its
//! caret are.

use ghosttype_core::desktop::{Confidence, ScreenLine, ScreenRect, ScreenText};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    IUIAutomationCondition, IUIAutomationElement, TreeScope_Descendants, UIA_BoundingRectanglePropertyId,
    UIA_ControlTypePropertyId, UIA_IsOffscreenPropertyId, UIA_NamePropertyId, UIA_TextControlTypeId,
};

use super::{Uia, is_password};
use crate::caret::{self, Caret};
use crate::display::screen_rect;
use crate::focus::foreground_hwnd;

/// The most text elements that one read of the screen keeps.
const MAX_TEXT_ELEMENTS: i32 = 2000;

impl Uia {
    /// Reads the visible text elements of the foreground window, and the frames of the focused
    /// control and its caret. `None` for a password field.
    pub(crate) fn screen_text(&self) -> Option<ScreenText> {
        let field = self.focused_field()?;
        // SAFETY: plain call. UIA reads the window of another process without messages to it.
        let window = unsafe { self.automation.ElementFromHandle(foreground_hwnd()) }.ok()?;
        Some(ScreenText {
            lines: self.visible_texts(&window)?,
            caret: field.caret,
            field: field.frame,
        })
    }

    /// Reads where the focused control and its caret are. `None` for a password field.
    pub(crate) fn focused_field(&self) -> Option<FocusedField> {
        let focused = self.focused_element()?;
        if is_password(&focused) {
            return None;
        }
        // SAFETY: plain call on an interface of this thread.
        let frame = unsafe { focused.CurrentBoundingRectangle() }.ok()?;
        let caret = caret::system_caret().or_else(|| self.drawn_caret(&focused));
        Some(FocusedField {
            frame: screen_rect(frame),
            caret: caret.as_ref().map(caret_frame),
        })
    }

    /// Reads each text element of `window` that shows on the screen, in one request to its app.
    fn visible_texts(&self, window: &IUIAutomationElement) -> Option<Vec<ScreenLine>> {
        let condition = self.visible_text_condition()?;
        // SAFETY: plain calls on interfaces of this thread.
        let elements = unsafe {
            let cache = self.cache_request(&[UIA_NamePropertyId, UIA_BoundingRectanglePropertyId])?;
            window
                .FindAllBuildCache(TreeScope_Descendants, &condition, &cache)
                .ok()?
        };
        // SAFETY: plain call on an interface of this thread.
        let count = unsafe { elements.Length() }.ok()?.min(MAX_TEXT_ELEMENTS);
        let lines = (0..count)
            .filter_map(|index| {
                // SAFETY: plain calls on interfaces of this thread, with an index below the length.
                unsafe {
                    let element = elements.GetElement(index).ok()?;
                    Some(ScreenLine {
                        text: element.CachedName().ok()?.to_string(),
                        confidence: Confidence::CERTAIN,
                        frame: screen_rect(element.CachedBoundingRectangle().ok()?),
                    })
                }
            })
            .collect();
        Some(lines)
    }

    /// The condition of a text element that shows on the screen.
    fn visible_text_condition(&self) -> Option<IUIAutomationCondition> {
        // SAFETY: plain calls on an interface of this thread.
        unsafe {
            let is_text = self
                .automation
                .CreatePropertyCondition(UIA_ControlTypePropertyId, &VARIANT::from(UIA_TextControlTypeId.0))
                .ok()?;
            let is_on_screen = self
                .automation
                .CreatePropertyCondition(UIA_IsOffscreenPropertyId, &VARIANT::from(false))
                .ok()?;
            self.automation.CreateAndCondition(&is_text, &is_on_screen).ok()
        }
    }
}

/// Where the focused control, which does not hide its text, and its caret are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusedField {
    pub(crate) frame: ScreenRect,
    pub(crate) caret: Option<ScreenRect>,
}

/// The frame of `caret`: a line of one pixel at the right edge of the text before it.
fn caret_frame(caret: &Caret) -> ScreenRect {
    ScreenRect {
        left: caret.origin.x,
        top: caret.origin.y,
        right: caret.origin.x.saturating_add(1),
        bottom: caret.origin.y.saturating_add(caret.height),
    }
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::POINT;

    use super::*;

    #[test]
    fn caret_frame_is_one_pixel_wide_at_the_caret() {
        let caret = Caret {
            origin: POINT { x: 107, y: 40 },
            height: 18,
            font_face: None,
            text_color: None,
        };

        let frame = caret_frame(&caret);

        assert_eq!(
            frame,
            ScreenRect {
                left: 107,
                top: 40,
                right: 108,
                bottom: 58,
            }
        );
    }
}
