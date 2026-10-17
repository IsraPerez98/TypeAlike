//! The focused control as UI Automation (UIA) tells it: the text around its caret, and its caret
//! for apps that draw a caret of their own and have no system caret (browsers, Electron apps and
//! WinUI apps).

use std::marker::PhantomData;
use std::ptr;
use std::time::Duration;

use ghosttype_core::desktop::{
    Confidence, FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET, ScreenLine, ScreenRect, ScreenText,
};
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, SAFEARRAY,
};
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayUnaccessData};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationTextPattern, IUIAutomationTextRange, TextPatternRangeEndpoint_End,
    TextPatternRangeEndpoint_Start, TextUnit_Character, TreeScope_Descendants,
    UIA_BoundingRectanglePropertyId, UIA_ControlTypePropertyId, UIA_IsOffscreenPropertyId,
    UIA_NamePropertyId, UIA_TextControlTypeId, UIA_TextPatternId,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::{BOOL, Interface, Result};

use crate::focus::{self, Caret, whole_chars};

/// The numbers that describe one bounding rectangle: left, top, width and height.
const VALUES_PER_RECTANGLE: usize = 4;
const CHARACTERS_BEFORE_CARET: i32 = -1;
/// The length that asks `GetText` for all the text of a range.
const WHOLE_RANGE: i32 = -1;
/// The most text elements that one read of the screen keeps.
const MAX_TEXT_ELEMENTS: i32 = 2000;

/// A connection to UIA, usable only on the thread that made it.
#[derive(Debug)]
pub(crate) struct UiaCaret {
    automation: IUIAutomation,
    /// Declared after `automation`, so that COM ends on this thread after the last interface goes.
    _apartment: ComApartment,
}

impl UiaCaret {
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

    /// Locates the caret of the focused control from the bounds of the character before it.
    /// `None` for a password field and for a control without text.
    pub(crate) fn read(&self) -> Option<Caret> {
        caret_of(&self.focused_element()?)
    }

    /// Reads the text around the selection of the focused control. `None` for a password field
    /// and for a control without text.
    pub(crate) fn field_text(&self) -> Option<FieldText> {
        field_text_of(&self.focused_element()?)
    }

    /// Reads the label of the focused control: its name, else its help text.
    pub(crate) fn field_label(&self) -> Option<String> {
        let element = self.focused_element()?;
        // SAFETY: plain calls on an interface of this thread.
        let labels = unsafe { [element.CurrentName(), element.CurrentHelpText()] };
        labels
            .into_iter()
            .filter_map(|label| label.ok().map(|label| label.to_string()))
            .find(|label| !label.trim().is_empty())
    }

    /// Reads the visible text elements of the foreground window, and the frames of the focused
    /// control and its caret. `None` for a password field.
    pub(crate) fn screen_text(&self) -> Option<ScreenText> {
        let focused = self.focused_element()?;
        if is_password(&focused) {
            return None;
        }
        // SAFETY: plain call on an interface of this thread.
        let field = unsafe { focused.CurrentBoundingRectangle() }.ok()?;
        let caret = focus::caret().or_else(|| caret_of(&focused));
        // SAFETY: plain calls; UIA reads the window of another process without messages to it.
        let window = unsafe { self.automation.ElementFromHandle(GetForegroundWindow()) }.ok()?;
        Some(ScreenText {
            lines: self.visible_texts(&window)?,
            caret: caret.as_ref().map(caret_frame),
            field: screen_rect(field),
        })
    }

    /// Reads each text element of `window` that shows on the screen, in one request to its app.
    fn visible_texts(&self, window: &IUIAutomationElement) -> Option<Vec<ScreenLine>> {
        let condition = self.visible_text_condition()?;
        // SAFETY: plain calls on interfaces of this thread.
        let elements = unsafe {
            let cache = self.automation.CreateCacheRequest().ok()?;
            cache.AddProperty(UIA_NamePropertyId).ok()?;
            cache.AddProperty(UIA_BoundingRectanglePropertyId).ok()?;
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

    fn focused_element(&self) -> Option<IUIAutomationElement> {
        // SAFETY: plain call on an interface of this thread.
        unsafe { self.automation.GetFocusedElement() }.ok()
    }
}

/// A side of the selection.
#[derive(Debug, Clone, Copy)]
enum Side {
    Before,
    After,
}

/// Reads the text around the selection of `element`: before its start and after its end.
fn field_text_of(element: &IUIAutomationElement) -> Option<FieldText> {
    if is_password(element) {
        return None;
    }
    let selection = caret_range(element)?;
    Some(FieldText {
        before_caret: text_beside(&selection, Side::Before)?,
        after_caret: text_beside(&selection, Side::After).unwrap_or_default(),
    })
}

/// Reads the text next to `selection` on `side`: at most [`MAX_CHARS_BEFORE_CARET`] characters
/// before it, or at most [`MAX_CHARS_AFTER_CARET`] after it.
fn text_beside(selection: &IUIAutomationTextRange, side: Side) -> Option<String> {
    let (collapsed_end, growing_end, characters) = match side {
        Side::Before => (
            TextPatternRangeEndpoint_End,
            TextPatternRangeEndpoint_Start,
            -i32::try_from(MAX_CHARS_BEFORE_CARET).ok()?,
        ),
        Side::After => (
            TextPatternRangeEndpoint_Start,
            TextPatternRangeEndpoint_End,
            i32::try_from(MAX_CHARS_AFTER_CARET).ok()?,
        ),
    };
    // SAFETY: plain calls on interfaces of this thread.
    let text = unsafe {
        let range = selection.Clone().ok()?;
        range
            .MoveEndpointByRange(collapsed_end, selection, growing_end)
            .ok()?;
        range
            .MoveEndpointByUnit(growing_end, TextUnit_Character, characters)
            .ok()?;
        range.GetText(WHOLE_RANGE).ok()?
    };
    Some(whole_chars(&text))
}

/// Locates the caret in `element` from the bounds of the character before it.
fn caret_of(element: &IUIAutomationElement) -> Option<Caret> {
    if is_password(element) {
        return None;
    }
    let range = caret_range(element)?;
    // Many apps give no bounds for the empty range at the caret, so measure the character before it.
    // SAFETY: plain call on an interface of this thread.
    unsafe {
        range.MoveEndpointByUnit(
            TextPatternRangeEndpoint_Start,
            TextUnit_Character,
            CHARACTERS_BEFORE_CARET,
        )
    }
    .ok()?;
    caret_from_rectangles(&bounding_rectangles(&range)?)
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

fn bounding_rectangles(range: &IUIAutomationTextRange) -> Option<Vec<f64>> {
    // SAFETY: plain call on an interface of this thread; the array is owned right after.
    let array = SafeArray(unsafe { range.GetBoundingRectangles() }.ok()?);
    array.doubles()
}

/// Builds the caret at the right edge of the last of `rectangles`, each given as left, top, width
/// and height in physical screen pixels.
fn caret_from_rectangles(rectangles: &[f64]) -> Option<Caret> {
    let [left, top, width, height] = *rectangles.as_chunks::<VALUES_PER_RECTANGLE>().0.last()?;
    let top_left = POINT {
        x: to_pixel(left),
        y: to_pixel(top),
    };
    let bottom_right = POINT {
        x: to_pixel(left + width),
        y: to_pixel(top + height),
    };
    Caret::from_corners(top_left, bottom_right, None)
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

fn screen_rect(rect: RECT) -> ScreenRect {
    ScreenRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    }
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "no conversion from f64 to i32 exists without `as`, and `as` saturates"
)]
fn to_pixel(coordinate: f64) -> i32 {
    coordinate.round() as i32
}

/// Membership of this thread in the multithreaded COM apartment, which ends on drop. It cannot go
/// to another thread, because COM ends only on the thread that joined it.
#[derive(Debug)]
struct ComApartment(PhantomData<*const ()>);

impl ComApartment {
    fn join() -> Result<Self> {
        // SAFETY: plain call; drop balances it on this thread.
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok()?;
        Ok(Self(PhantomData))
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        // SAFETY: balances the successful CoInitializeEx of `join`, on the same thread.
        unsafe { CoUninitialize() };
    }
}

/// A SAFEARRAY that this thread owns, destroyed on drop.
struct SafeArray(*mut SAFEARRAY);

impl SafeArray {
    /// Copies the elements of a one-dimensional array of doubles. `None` for another array.
    fn doubles(&self) -> Option<Vec<f64>> {
        // SAFETY: a non-null pointer refers to the array that `self` owns.
        let header = unsafe { self.0.as_ref() }?;
        let holds_doubles = header.cDims == 1 && usize::try_from(header.cbElements).ok()? == size_of::<f64>();
        if !holds_doubles {
            return None;
        }
        let count = usize::try_from(header.rgsabound[0].cElements).ok()?;
        if count == 0 {
            return Some(Vec::new());
        }
        let mut data = ptr::null_mut();
        // SAFETY: the array is valid; `data` gets its elements until the access ends.
        unsafe { SafeArrayAccessData(self.0, &raw mut data) }.ok()?;
        // SAFETY: the array holds `count` doubles at `data` while the access lasts.
        let values = unsafe { std::slice::from_raw_parts(data.cast::<f64>(), count) }.to_vec();
        // SAFETY: ends the access above; the array stays valid.
        unsafe { SafeArrayUnaccessData(self.0) }.ok()?;
        Some(values)
    }
}

impl Drop for SafeArray {
    fn drop(&mut self) {
        if self.0.is_null() {
            return;
        }
        // A failed destroy during drop leaves nothing to recover.
        // SAFETY: `self` owns the array and nothing accesses it now.
        let _ = unsafe { SafeArrayDestroy(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_starts_at_right_edge_of_last_rectangle() {
        let rectangles = [10.0, 20.0, 5.0, 16.0, 100.0, 40.0, 7.0, 18.0];

        let caret = caret_from_rectangles(&rectangles);

        assert_eq!(
            caret,
            Some(Caret {
                origin: POINT { x: 107, y: 40 },
                height: 18,
                font_face: None,
            })
        );
    }

    #[test]
    fn fractional_bounds_round_to_pixels() {
        let rectangles = [100.4, 40.6, 7.2, 18.0];

        let caret = caret_from_rectangles(&rectangles);

        assert_eq!(
            caret,
            Some(Caret {
                origin: POINT { x: 108, y: 41 },
                height: 18,
                font_face: None,
            })
        );
    }

    #[test]
    fn caret_frame_is_one_pixel_wide_at_the_caret() {
        let caret = Caret {
            origin: POINT { x: 107, y: 40 },
            height: 18,
            font_face: None,
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

    #[test]
    fn no_rectangle_gives_no_caret() {
        let caret = caret_from_rectangles(&[]);

        assert_eq!(caret, None);
    }
}
