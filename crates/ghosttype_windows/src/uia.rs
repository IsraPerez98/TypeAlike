//! The focused control as UI Automation (UIA) tells it: the text around its caret, its labels, the
//! web page that holds it, and its caret for apps that draw a caret of their own and have no
//! system caret and no caret object (such as WinUI apps).

use std::fmt;
use std::marker::PhantomData;
use std::ptr;
use std::time::Duration;

use ghosttype_core::desktop::{
    Confidence, FieldLabels, FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET, ScreenLine,
    ScreenRect, ScreenText, TextColor,
};
use windows::Foundation::Uri;
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, SAFEARRAY,
};
use windows::Win32::System::Ole::{SafeArrayAccessData, SafeArrayDestroy, SafeArrayUnaccessData};
use windows::Win32::System::Variant::{VARIANT, VT_BSTR};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomation2, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationElement6, IUIAutomationTextPattern, IUIAutomationTextRange, IUIAutomationTreeWalker,
    TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character, TreeScope_Descendants,
    UIA_BackgroundColorAttributeId, UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId,
    UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_ForegroundColorAttributeId,
    UIA_FrameworkIdPropertyId, UIA_IsOffscreenPropertyId, UIA_NamePropertyId, UIA_TextControlTypeId,
    UIA_TextPatternId, UIA_ValueValuePropertyId,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::{BOOL, BSTR, HSTRING, Interface, Result};

use crate::caret::{self, Caret};
use crate::focus::whole_chars;
use crate::pixels::Rgb;

/// The numbers that describe one bounding rectangle: left, top, width and height.
const VALUES_PER_RECTANGLE: usize = 4;
const CHARACTERS_BEFORE_CARET: i32 = -1;
/// The length that asks `GetText` for all the text of a range.
const WHOLE_RANGE: i32 = -1;
/// The most text elements that one read of the screen keeps.
const MAX_TEXT_ELEMENTS: i32 = 2000;
/// The frameworks of web pages in UIA: Chromium (Chrome, Edge, Electron apps and `WebView2`) and
/// Gecko (Firefox). The document of a web page gives the URL of the page as its value.
const WEB_FRAMEWORKS: [&str; 2] = ["Chrome", "Gecko"];
/// The class of the hidden input box of a terminal of xterm.js, such as the terminal of VS Code.
const XTERM_INPUT_CLASS: &str = "xterm-helper-textarea";
/// The class of the screen of a terminal of xterm.js, which holds its rows.
const XTERM_SCREEN_CLASS: &str = "xterm-screen";

/// What UIA tells about the focused control.
#[derive(Debug)]
pub(crate) enum UiaField {
    /// The control hides its text, as a password field does.
    Password,
    /// The control shows its text.
    Shown(ShownField),
}

/// What a field that shows its text tells about itself. `Debug` shows only what is known, so logs
/// never hold the text, the labels or the web page.
#[derive(Clone, PartialEq, Eq, Default)]
pub(crate) struct ShownField {
    /// The text around the caret, or `None` when GhostType cannot read it.
    pub(crate) text: Option<FieldText>,
    /// The labels of the field.
    pub(crate) labels: FieldLabels,
    /// The host of the web page that holds the field, or `None` outside a web page.
    pub(crate) domain: Option<String>,
    /// Whether the field hides the text on the line of its caret, as the input box of a terminal
    /// of xterm.js does.
    pub(crate) is_caret_line_hidden: bool,
}

impl fmt::Debug for ShownField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShownField")
            .field("text", &self.text)
            .field("labels", &self.labels)
            .field("has_domain", &self.domain.is_some())
            .field("is_caret_line_hidden", &self.is_caret_line_hidden)
            .finish()
    }
}

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

    /// Locates the caret of the focused control: the system caret, else the caret that the control
    /// draws itself. The caret has the color of the text before it, when the control tells it.
    pub(crate) fn read(&self) -> Option<Caret> {
        let element = self.focused_element();
        let caret = caret::system_caret().or_else(|| self.drawn_caret(element.as_ref()?))?;
        Some(Caret {
            text_color: element.as_ref().and_then(text_color),
            ..caret
        })
    }

    /// Locates the caret that `element`, the focused control, draws itself: the caret of the caret
    /// object of its window, else the right edge of the character before the caret. In a terminal
    /// of xterm.js, the caret moves up to the row that holds it.
    fn drawn_caret(&self, element: &IUIAutomationElement) -> Option<Caret> {
        let caret = caret::accessible_caret().or_else(|| caret_of(element))?;
        Some(match self.terminal_input(element) {
            Some(input) => input.align(caret),
            None => caret,
        })
    }

    /// Reads the hidden input box of a terminal of xterm.js and the top of its screen, when
    /// `element` is that input box.
    fn terminal_input(&self, element: &IUIAutomationElement) -> Option<TerminalInput> {
        if !is_terminal_input(element) {
            return None;
        }
        let screen = self.terminal_screen(element)?;
        // SAFETY: plain calls on interfaces of this thread.
        let (frame, screen_frame) = unsafe {
            (
                element.CurrentBoundingRectangle().ok()?,
                screen.CachedBoundingRectangle().ok()?,
            )
        };
        Some(TerminalInput {
            frame,
            screen_top: screen_frame.top,
        })
    }

    /// Finds the screen of the terminal of xterm.js that holds `element`, with its bounds in the
    /// cache.
    fn terminal_screen(&self, element: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        // SAFETY: plain calls on interfaces of this thread.
        unsafe {
            let is_screen = self
                .automation
                .CreatePropertyCondition(UIA_ClassNamePropertyId, &VARIANT::from(XTERM_SCREEN_CLASS))
                .ok()?;
            let walker = self.automation.CreateTreeWalker(&is_screen).ok()?;
            let cache = self.automation.CreateCacheRequest().ok()?;
            cache.AddProperty(UIA_ClassNamePropertyId).ok()?;
            cache.AddProperty(UIA_BoundingRectanglePropertyId).ok()?;
            let screen = walker.NormalizeElementBuildCache(element, &cache).ok()?;
            // UIA gives the root of the tree when no element above matches.
            (screen.CachedClassName().ok()? == XTERM_SCREEN_CLASS).then_some(screen)
        }
    }

    /// Reads the focused control: whether it hides its text, and else the text around its
    /// selection, its labels and the host of its web page. `None` when UIA finds no focused
    /// control.
    pub(crate) fn field(&self) -> Option<UiaField> {
        let element = self.focused_element()?;
        if is_password(&element) {
            return Some(UiaField::Password);
        }
        Some(UiaField::Shown(ShownField {
            text: text_around_selection(&element),
            labels: labels_of(&element),
            domain: self.web_domain(&element),
            is_caret_line_hidden: is_terminal_input(&element),
        }))
    }

    /// Reads the host of the web page that holds `element`, from the URL of the nearest web
    /// document at or above it. Only a web document counts, because the value of another
    /// document, such as a page of a word processor, can be all of its text.
    fn web_domain(&self, element: &IUIAutomationElement) -> Option<String> {
        let walker = self.web_document_walker()?;
        // SAFETY: plain calls on interfaces of this thread.
        let url = unsafe {
            let cache = self.automation.CreateCacheRequest().ok()?;
            cache.AddProperty(UIA_ControlTypePropertyId).ok()?;
            cache.AddProperty(UIA_ValueValuePropertyId).ok()?;
            let document = walker.NormalizeElementBuildCache(element, &cache).ok()?;
            // UIA gives the root of the tree when no element above matches.
            if document.CachedControlType().ok()? != UIA_DocumentControlTypeId {
                return None;
            }
            document.GetCachedPropertyValue(UIA_ValueValuePropertyId).ok()?
        };
        url_host(&text_of(&url)?)
    }

    /// A walker that sees only the documents of web pages.
    fn web_document_walker(&self) -> Option<IUIAutomationTreeWalker> {
        // SAFETY: plain calls on an interface of this thread.
        unsafe {
            let is_document = self
                .automation
                .CreatePropertyCondition(
                    UIA_ControlTypePropertyId,
                    &VARIANT::from(UIA_DocumentControlTypeId.0),
                )
                .ok()?;
            let [chromium, gecko] = WEB_FRAMEWORKS.map(|framework| {
                self.automation
                    .CreatePropertyCondition(UIA_FrameworkIdPropertyId, &VARIANT::from(framework))
            });
            let is_web = self
                .automation
                .CreateOrCondition(&chromium.ok()?, &gecko.ok()?)
                .ok()?;
            let condition = self.automation.CreateAndCondition(&is_document, &is_web).ok()?;
            self.automation.CreateTreeWalker(&condition).ok()
        }
    }

    /// Reads the visible text elements of the foreground window, and the frames of the focused
    /// control and its caret. `None` for a password field.
    pub(crate) fn screen_text(&self) -> Option<ScreenText> {
        let field = self.focused_field()?;
        // SAFETY: plain calls. UIA reads the window of another process without messages to it.
        let window = unsafe { self.automation.ElementFromHandle(GetForegroundWindow()) }.ok()?;
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

/// Where the focused control, which does not hide its text, and its caret are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FocusedField {
    pub(crate) frame: ScreenRect,
    pub(crate) caret: Option<ScreenRect>,
}

/// A side of the selection.
#[derive(Debug, Clone, Copy)]
enum Side {
    Before,
    After,
}

/// Reads the text around the selection of `element`, which shows its text: before the start of the
/// selection and after its end.
fn text_around_selection(element: &IUIAutomationElement) -> Option<FieldText> {
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
    // Many apps give no bounds for the empty range at the caret, so measure the character before it.
    caret_from_rectangles(&bounding_rectangles(&character_before_caret(element)?)?)
}

fn character_before_caret(element: &IUIAutomationElement) -> Option<IUIAutomationTextRange> {
    let range = caret_range(element)?;
    // SAFETY: plain call on an interface of this thread.
    unsafe {
        range.MoveEndpointByUnit(
            TextPatternRangeEndpoint_Start,
            TextUnit_Character,
            CHARACTERS_BEFORE_CARET,
        )
    }
    .ok()?;
    Some(range)
}

/// Reads the color of the character before the caret of `element`. `None` for the hidden input box
/// of a terminal of xterm.js, which does not show its text.
fn text_color(element: &IUIAutomationElement) -> Option<TextColor> {
    if is_terminal_input(element) {
        return None;
    }
    let range = character_before_caret(element)?;
    // SAFETY: plain calls on an interface of this thread.
    let (text, background) = unsafe {
        (
            range.GetAttributeValue(UIA_ForegroundColorAttributeId).ok()?,
            range.GetAttributeValue(UIA_BackgroundColorAttributeId).ok(),
        )
    };
    shown_text_color(colorref(&text)?, background.as_ref().and_then(colorref))
}

/// Reads a `COLORREF` attribute value. `None` when UIA gives a value of another type: the value
/// for an attribute that the control does not support, or for colors that change in the range.
fn colorref(value: &VARIANT) -> Option<u32> {
    i32::try_from(value).ok().map(i32::cast_unsigned)
}

/// The color of text in the `COLORREF` `text` on the `COLORREF` `background`. `None` when both
/// are the same: Chromium drops the alpha of the transparent text of a hidden input box.
fn shown_text_color(text: u32, background: Option<u32>) -> Option<TextColor> {
    (background != Some(text)).then(|| Rgb::from_colorref(text).into())
}

fn is_terminal_input(element: &IUIAutomationElement) -> bool {
    // SAFETY: plain call on an interface of this thread.
    unsafe { element.CurrentClassName() }.is_ok_and(|class| class == XTERM_INPUT_CLASS)
}

/// Reads the labels of `element`. UIA has no placeholder property, so the placeholder stays `None`.
fn labels_of(element: &IUIAutomationElement) -> FieldLabels {
    // SAFETY: plain calls on interfaces of this thread.
    let (caption, name, help_text) = unsafe {
        (
            element.CurrentLabeledBy().and_then(|label| label.CurrentName()),
            element.CurrentName(),
            element.CurrentHelpText(),
        )
    };
    // SAFETY: plain calls on interfaces of this thread.
    let full_description = || unsafe {
        element
            .cast::<IUIAutomationElement6>()
            .and_then(|element| element.CurrentFullDescription())
    };
    FieldLabels {
        caption: label_text(caption),
        name: label_text(name),
        placeholder: None,
        help: label_text(help_text).or_else(|| label_text(full_description())),
    }
}

/// Returns the text that `value` holds, or `None` when it holds no text.
fn text_of(value: &VARIANT) -> Option<String> {
    (value.vt() == VT_BSTR).then(|| {
        // SAFETY: the type of `value` tells that its union holds a BSTR.
        unsafe { value.Anonymous.Anonymous.Anonymous.bstrVal.to_string() }
    })
}

/// Returns the host of `url`, or `None` for a URL without a host, such as the URL of a file.
fn url_host(url: &str) -> Option<String> {
    let host = Uri::CreateUri(&HSTRING::from(url)).ok()?.Host().ok()?.to_string();
    (!host.is_empty()).then_some(host)
}

/// Returns the text of `label`, or `None` when the control does not have this label. UIA gives an
/// empty string for a label that a control does not have.
fn label_text(label: Result<BSTR>) -> Option<String> {
    label
        .ok()
        .map(|label| label.to_string())
        .filter(|label| !label.trim().is_empty())
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

/// The hidden input box of a terminal of xterm.js, which the terminal moves to the cell of its
/// cursor, and the screen of the terminal.
#[derive(Debug, Clone, Copy)]
struct TerminalInput {
    /// The frame of the input box, which is one row high.
    frame: RECT,
    /// The top of the screen, where the first row starts.
    screen_top: i32,
}

impl TerminalInput {
    /// Moves `caret`, the caret in the input box, up to the row that holds the input box. VS Code
    /// draws the input box below the top of that row.
    fn align(self, caret: Caret) -> Caret {
        let row_height = self.frame.bottom.saturating_sub(self.frame.top);
        let below_row = self
            .frame
            .top
            .saturating_sub(self.screen_top)
            .checked_rem_euclid(row_height)
            .unwrap_or_default();
        Caret {
            origin: POINT {
                x: caret.origin.x,
                y: caret.origin.y.saturating_sub(below_row),
            },
            ..caret
        }
    }
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

pub(crate) fn screen_rect(rect: RECT) -> ScreenRect {
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
                text_color: None,
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
                text_color: None,
            })
        );
    }

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

    fn caret_at(y: i32) -> Caret {
        Caret {
            origin: POINT { x: 443, y },
            height: 16,
            font_face: None,
            text_color: None,
        }
    }

    fn terminal_input(top: i32, bottom: i32) -> TerminalInput {
        TerminalInput {
            frame: RECT {
                left: 442,
                top,
                right: 452,
                bottom,
            },
            screen_top: 538,
        }
    }

    #[test]
    fn caret_moves_up_to_the_row_of_the_terminal_input() {
        let input = terminal_input(1284, 1302);

        let caret = input.align(caret_at(1285));

        assert_eq!(caret, caret_at(1277));
    }

    #[test]
    fn caret_of_a_terminal_input_on_its_row_stays() {
        let input = terminal_input(1276, 1294);

        let caret = input.align(caret_at(1277));

        assert_eq!(caret, caret_at(1277));
    }

    #[test]
    fn terminal_input_without_height_leaves_the_caret() {
        let input = terminal_input(1284, 1284);

        let caret = input.align(caret_at(1285));

        assert_eq!(caret, caret_at(1285));
    }

    #[test]
    fn no_rectangle_gives_no_caret() {
        let caret = caret_from_rectangles(&[]);

        assert_eq!(caret, None);
    }

    #[test]
    fn text_color_comes_from_its_colorref() {
        let colors = [
            shown_text_color(0x0033_2211, Some(0x00FF_FFFF)),
            shown_text_color(0x0033_2211, None),
        ];

        let expected = Some(TextColor {
            red: 0x11,
            green: 0x22,
            blue: 0x33,
        });
        assert_eq!(colors, [expected; 2]);
    }

    #[test]
    fn text_in_the_color_of_its_background_has_no_color() {
        let color = shown_text_color(0, Some(0));

        assert_eq!(color, None);
    }

    #[test]
    fn host_comes_from_the_url_of_a_web_page() {
        let host = url_host("https://mail.google.com/mail/u/0/#inbox");

        assert_eq!(host.as_deref(), Some("mail.google.com"));
    }

    #[test]
    fn url_without_a_host_gives_no_host() {
        let hosts = ["file:///C:/notes.html", "about:blank", "not a url"].map(url_host);

        assert_eq!(hosts, [None, None, None]);
    }
}
