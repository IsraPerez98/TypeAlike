//! The focused field as UIA tells it.
//!
//! UIA tells whether the field hides its text, and else the text around its selection, its labels
//! and the host of its web page.

use std::fmt;

use ghosttype_core::desktop::{FieldLabels, FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET};
use windows::Foundation::Uri;
use windows::Win32::System::Variant::{VARIANT, VT_BSTR};
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationElement6, IUIAutomationTextRange, IUIAutomationTreeWalker,
    TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character,
    UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_FrameworkIdPropertyId,
    UIA_ValueValuePropertyId,
};
use windows::core::{BSTR, HSTRING, Interface, Result};

use super::{Uia, caret_range, is_password, is_terminal_input};
use crate::utf16::whole_chars;

/// The length that asks `GetText` for all the text of a range.
const WHOLE_RANGE: i32 = -1;
/// The frameworks of web pages in UIA: Chromium (Chrome, Edge, Electron apps and `WebView2`) and
/// Gecko (Firefox). The document of a web page gives the URL of the page as its value.
const WEB_FRAMEWORKS: [&str; 2] = ["Chrome", "Gecko"];

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

impl Uia {
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
            let cache = self.cache_request(&[UIA_ControlTypePropertyId, UIA_ValueValuePropertyId])?;
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

#[cfg(test)]
mod tests {
    use super::*;

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
