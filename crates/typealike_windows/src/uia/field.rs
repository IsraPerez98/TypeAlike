//! The focused field as UIA tells it.
//!
//! UIA tells whether the field hides its text, and else the text around its selection, its labels
//! and the host of its web page.

use std::fmt;

use typealike_core::desktop::{
    AfterCaret, FieldLabels, FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET,
};
use windows::Foundation::Uri;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationElement6, IUIAutomationTextRange, IUIAutomationTreeWalker,
    TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start, TextUnit_Character,
    UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_FrameworkIdPropertyId,
    UIA_ValueValuePropertyId,
};
use windows::core::{BSTR, HSTRING, Interface, Result};

use super::{Uia, WHOLE_RANGE, caret_range, has_unreadable_text, is_password, is_terminal_input, text_of};
use crate::utf16::whole_chars;

/// The frameworks of web pages in UIA: Chromium (Chrome, Edge, Electron apps and `WebView2`) and
/// Gecko (Firefox). The document of a web page gives the URL of the page as its value.
const WEB_FRAMEWORKS: [&str; 2] = ["Chrome", "Gecko"];
/// The ARIA role of a search box, which a browser also gives an HTML `<input type="search">`.
const SEARCH_BOX_ARIA_ROLE: &str = "searchbox";
/// The characters of the selection that a look reads. One character tells that text is selected.
const CHARS_TO_FIND_SELECTION: i32 = 1;
/// The characters after the caret that a look reads to find text on its line and on the lines
/// below. It is far less than the text of a read of the field, which a slow app can need seconds
/// to give. A terminal pads each line with spaces up to the width of its window, so it holds the
/// padding of a line of a terminal on a wide monitor, and a line that has only spaces counts as
/// empty.
const CHARS_TO_FIND_LINE_TEXT: usize = 400;

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
    /// The text around the caret, or `None` when TypeAlike cannot read it.
    pub(crate) text: Option<FieldText>,
    /// The labels of the field.
    pub(crate) labels: FieldLabels,
    /// The host of the web page that holds the field, or `None` outside a web page.
    pub(crate) domain: Option<String>,
    /// Whether the field hides the text on the line of its caret, as the input box of a terminal
    /// of xterm.js does.
    pub(crate) is_caret_line_hidden: bool,
    /// Whether the ARIA role of the field marks it as a search box.
    pub(crate) is_search_field: bool,
    /// Whether the field can show text that UIA does not give, as the editor of VS Code does.
    pub(crate) has_unreadable_text: bool,
}

impl fmt::Debug for ShownField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShownField")
            .field("text", &self.text)
            .field("labels", &self.labels)
            .field("has_domain", &self.domain.is_some())
            .field("is_caret_line_hidden", &self.is_caret_line_hidden)
            .field("is_search_field", &self.is_search_field)
            .field("has_unreadable_text", &self.has_unreadable_text)
            .finish()
    }
}

impl Uia {
    /// Reads the focused control: whether it hides its text, and else the text around its
    /// selection, its labels and the host of its web page. `None` when UIA gives no focused
    /// control, for example after a request that failed or did not finish in time.
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
            is_search_field: is_search_field(&element),
            has_unreadable_text: has_unreadable_text(&element),
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
        before_caret: text_beside(&selection, Side::Before, MAX_CHARS_BEFORE_CARET)?,
        after_caret: text_beside(&selection, Side::After, MAX_CHARS_AFTER_CARET).unwrap_or_default(),
    })
}

/// Reads what follows the caret of `element` now: selected text, else the text after the caret on
/// its line. `Unknown` when the field hides its text, or when UIA gives no selection or no text,
/// so that the text that the session read at its start counts.
pub(super) fn after_caret(element: &IUIAutomationElement) -> AfterCaret {
    if is_password(element) {
        return AfterCaret::Unknown;
    }
    let Some(selection) = caret_range(element) else {
        return AfterCaret::Unknown;
    };
    // SAFETY: plain call on an interface of this thread.
    let selected = unsafe { selection.GetText(CHARS_TO_FIND_SELECTION) }.ok();
    let after = text_beside(&selection, Side::After, CHARS_TO_FIND_LINE_TEXT);
    match (selected, after) {
        (Some(selected), Some(after)) => AfterCaret::seen(&selected.to_string(), &after),
        _ => AfterCaret::Unknown,
    }
}

/// Reads the text next to `selection` on `side`: at most `max_chars` characters.
fn text_beside(selection: &IUIAutomationTextRange, side: Side, max_chars: usize) -> Option<String> {
    let characters = i32::try_from(max_chars).ok()?;
    let (collapsed_end, growing_end, characters) = match side {
        Side::Before => (
            TextPatternRangeEndpoint_End,
            TextPatternRangeEndpoint_Start,
            -characters,
        ),
        Side::After => (
            TextPatternRangeEndpoint_Start,
            TextPatternRangeEndpoint_End,
            characters,
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

/// Whether the ARIA role of `element` marks it as a search box. Only web pages give ARIA roles, so
/// this misses a native search box.
fn is_search_field(element: &IUIAutomationElement) -> bool {
    // SAFETY: plain call on an interface of this thread.
    unsafe { element.CurrentAriaRole() }.is_ok_and(|role| is_search_role(&role.to_string()))
}

/// Whether `aria_role`, the value of an ARIA role, marks a search box. The first role of a list
/// counts, as in a browser.
fn is_search_role(aria_role: &str) -> bool {
    aria_role
        .split_whitespace()
        .next()
        .is_some_and(|role| role.eq_ignore_ascii_case(SEARCH_BOX_ARIA_ROLE))
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
    fn searchbox_role_marks_a_search_field() {
        let roles = ["searchbox", "SearchBox", "searchbox textbox", " searchbox"];

        assert_eq!(roles.map(is_search_role), [true; 4]);
    }

    #[test]
    fn other_role_or_no_role_marks_no_search_field() {
        let roles = ["textbox", "search", "combobox", "textbox searchbox", ""];

        assert_eq!(roles.map(is_search_role), [false; 5]);
    }

    #[test]
    fn url_without_a_host_gives_no_host() {
        let hosts = ["file:///C:/notes.html", "about:blank", "not a url"].map(url_host);

        assert_eq!(hosts, [None, None, None]);
    }
}
