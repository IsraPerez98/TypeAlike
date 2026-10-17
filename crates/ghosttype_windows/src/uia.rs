//! The caret of the focused control as UI Automation (UIA) tells it, for apps that draw a caret of
//! their own and have no system caret: browsers, Electron apps and WinUI apps.

use std::marker::PhantomData;
use std::ptr;

use windows::Win32::Foundation::POINT;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, SAFEARRAY,
};
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayUnaccessData};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern, IUIAutomationTextRange,
    TextPatternRangeEndpoint_Start, TextUnit_Character, UIA_TextPatternId,
};
use windows::core::{BOOL, Result};

use crate::focus::Caret;

/// The numbers that describe one bounding rectangle: left, top, width and height.
const VALUES_PER_RECTANGLE: usize = 4;
const CHARACTERS_BEFORE_CARET: i32 = -1;

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

    /// Locates the caret of the focused control from the bounds of the character before it.
    /// `None` for a password field and for a control without text.
    pub(crate) fn read(&self) -> Option<Caret> {
        // SAFETY: plain call on an interface of this thread.
        let element = unsafe { self.automation.GetFocusedElement() }.ok()?;
        caret_of(&element)
    }
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
    fn no_rectangle_gives_no_caret() {
        let caret = caret_from_rectangles(&[]);

        assert_eq!(caret, None);
    }
}
