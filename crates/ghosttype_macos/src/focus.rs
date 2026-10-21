//! What has keyboard focus: whether GhostType may suggest there, where its caret is, and the text
//! around the caret.

use std::iter;
use std::ptr::{self, NonNull};

use ghosttype_core::controller::FocusChange;
use ghosttype_core::desktop::{
    Field, FieldLabels, FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET, Surroundings, TextColor,
};
use objc2_app_kit::{NSColor, NSColorSpace, NSRunningApplication, NSWorkspace};
use objc2_application_services::{
    AXError, AXIsProcessTrustedWithOptions, AXUIElement, AXValue, AXValueType, kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{
    CFArray, CFAttributedString, CFBoolean, CFDictionary, CFNumber, CFRange, CFRetained, CFString, CFType,
    CFURL, CGFloat, CGPoint, CGRect, CGSize, Type,
};
use objc2_core_graphics::CGColor;

pub(crate) const FOCUSED_ELEMENT_ATTRIBUTE: &str = "AXFocusedUIElement";
const SUBROLE_ATTRIBUTE: &str = "AXSubrole";
const SECURE_TEXT_FIELD_SUBROLE: &str = "AXSecureTextField";
const SELECTED_TEXT_RANGE_ATTRIBUTE: &str = "AXSelectedTextRange";
const BOUNDS_FOR_RANGE_ATTRIBUTE: &str = "AXBoundsForRange";
const STRING_FOR_RANGE_ATTRIBUTE: &str = "AXStringForRange";
const ATTRIBUTED_STRING_FOR_RANGE_ATTRIBUTE: &str = "AXAttributedStringForRange";
const FOREGROUND_COLOR_TEXT_ATTRIBUTE: &str = "AXForegroundColor";
/// The least opacity of text that shows. A web editor takes keys in a hidden input box whose text
/// is transparent.
const MIN_SHOWN_TEXT_ALPHA: CGFloat = 0.5;
const NUMBER_OF_CHARACTERS_ATTRIBUTE: &str = "AXNumberOfCharacters";
const VALUE_ATTRIBUTE: &str = "AXValue";
pub(crate) const WINDOW_ATTRIBUTE: &str = "AXWindow";
const POSITION_ATTRIBUTE: &str = "AXPosition";
const SIZE_ATTRIBUTE: &str = "AXSize";
const TITLE_ATTRIBUTE: &str = "AXTitle";
const PLACEHOLDER_ATTRIBUTE: &str = "AXPlaceholderValue";
const DESCRIPTION_ATTRIBUTE: &str = "AXDescription";
const HELP_ATTRIBUTE: &str = "AXHelp";
const TITLE_UI_ELEMENT_ATTRIBUTE: &str = "AXTitleUIElement";
pub(crate) const ROLE_ATTRIBUTE: &str = "AXRole";
pub(crate) const PARENT_ATTRIBUTE: &str = "AXParent";
const URL_ATTRIBUTE: &str = "AXURL";
const WEB_AREA_ROLE: &str = "AXWebArea";
pub(crate) const WINDOW_ROLE: &str = "AXWindow";
/// The most elements that GhostType reads on the way up from the focused field to its web area.
const MAX_WEB_AREA_DISTANCE: usize = 64;
const DOM_CLASS_LIST_ATTRIBUTE: &str = "AXDOMClassList";
/// The class of the hidden input box of a terminal of xterm.js, such as the terminal of VS Code.
const XTERM_INPUT_CLASS: &str = "xterm-helper-textarea";
/// The class of the screen of a terminal of xterm.js, which holds its rows.
const XTERM_SCREEN_CLASS: &str = "xterm-screen";
/// The most elements that GhostType reads on the way up from the input box of a terminal to its
/// screen, which holds the box two levels up.
const MAX_TERMINAL_SCREEN_DISTANCE: usize = 4;
const MANUAL_ACCESSIBILITY_ATTRIBUTE: &str = "AXManualAccessibility";
const ENHANCED_USER_INTERFACE_ATTRIBUTE: &str = "AXEnhancedUserInterface";
const CHROMIUM_BROWSER_BUNDLE_PREFIXES: [&str; 7] = [
    "com.google.chrome",
    "org.chromium.chromium",
    "com.brave.browser",
    "com.microsoft.edgemac",
    "company.thebrowser.",
    "com.vivaldi.vivaldi",
    "com.operasoftware.opera",
];
pub(crate) const ACCESSIBILITY_QUERY_TIMEOUT_SECONDS: f32 = 0.1;

/// Where ghost text starts: just right of the caret, in global points with a top-left origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Caret {
    pub(crate) left: CGFloat,
    pub(crate) top: CGFloat,
    pub(crate) height: CGFloat,
}

impl Caret {
    /// Builds a caret from the bounds of the text that ends at it; `None` when it has no height.
    pub(crate) fn from_bounds(bounds: CGRect) -> Option<Self> {
        (bounds.size.height > 0.0).then_some(Self {
            left: bounds.origin.x + bounds.size.width,
            top: bounds.origin.y,
            height: bounds.size.height,
        })
    }

    /// A caret at the top-left corner of `field`, as high as the field, for a field that gives no
    /// caret; `None` when the field has no height.
    pub(crate) fn at_start_of(field: CGRect) -> Option<Self> {
        Self::from_bounds(CGRect::new(field.origin, CGSize::new(0.0, field.size.height)))
    }
}

/// Follows the frontmost app and classifies whether GhostType may observe and type into it.
#[derive(Debug)]
pub(crate) struct FocusTracker {
    system_wide: CFRetained<AXUIElement>,
    frontmost_process: Option<i32>,
    frontmost_name: Option<String>,
    frontmost_app_id: Option<String>,
    is_frontmost_eligible: bool,
}

impl FocusTracker {
    pub(crate) fn new() -> Self {
        // SAFETY: plain call.
        let system_wide = unsafe { AXUIElement::new_system_wide() };
        // SAFETY: plain call on a valid element.
        let error = unsafe { system_wide.set_messaging_timeout(ACCESSIBILITY_QUERY_TIMEOUT_SECONDS) };
        if error != AXError::Success {
            tracing::warn!(code = error.0, "failed to bound accessibility queries");
        }
        Self {
            system_wide,
            frontmost_process: None,
            frontmost_name: None,
            frontmost_app_id: None,
            is_frontmost_eligible: false,
        }
    }

    /// Observes the frontmost app; returns whether it changed since the last call.
    pub(crate) fn refresh(&mut self) -> FocusChange {
        let frontmost = NSWorkspace::sharedWorkspace().frontmostApplication();
        let frontmost_process = frontmost
            .as_ref()
            .map(|application| application.processIdentifier());
        if frontmost_process == self.frontmost_process {
            return FocusChange::Unchanged;
        }
        self.frontmost_process = frontmost_process;
        self.frontmost_name = frontmost
            .as_ref()
            .and_then(|application| application.localizedName())
            .map(|name| name.to_string());
        self.frontmost_app_id = frontmost
            .as_ref()
            .and_then(|application| application.bundleIdentifier())
            .map(|identifier| identifier.to_string());
        self.is_frontmost_eligible = frontmost_process.is_some_and(|process| !is_own_process(process));
        if let Some(application) = frontmost.filter(|_| self.is_frontmost_eligible) {
            wake_web_accessibility(&application);
        }
        FocusChange::Changed
    }

    /// The process of the frontmost app when [`refresh`](Self::refresh) looked last.
    pub(crate) fn frontmost_process(&self) -> Option<i32> {
        self.frontmost_process
    }

    /// Locates the caret of the focused control, when it exposes one through Accessibility.
    pub(crate) fn caret(&self) -> Option<Caret> {
        caret_bounds(&*self.focused_element()?).and_then(Caret::from_bounds)
    }

    /// The color of the text before the caret of the focused control, when the control tells it.
    pub(crate) fn text_color(&self) -> Option<TextColor> {
        text_color(&*self.focused_element()?)
    }

    /// The frame of the focused control when it holds text, as a field does, even when it gives
    /// no caret.
    pub(crate) fn field_frame(&self) -> Option<CGRect> {
        let element = self.focused_element()?;
        selected_range(&element)?;
        frame(&element)
    }

    fn is_secure_text_focused(&self) -> bool {
        self.focused_element()
            .is_some_and(|element| is_secure_text(&element))
    }

    fn focused_element(&self) -> Option<CFRetained<AXUIElement>> {
        copy_attribute(&self.system_wide, FOCUSED_ELEMENT_ATTRIBUTE)?
            .downcast::<AXUIElement>()
            .ok()
    }

    /// Classifies the focused control right now: excluded for secure text and ineligible apps.
    pub(crate) fn field(&self) -> Field {
        if self.is_frontmost_eligible && !self.is_secure_text_focused() {
            Field::Open
        } else {
            Field::Excluded
        }
    }

    pub(crate) fn surroundings(&self) -> Surroundings {
        let element = self.focused_element();
        Surroundings {
            app_name: self.frontmost_name.clone(),
            app_id: self.frontmost_app_id.clone(),
            app_path: None,
            window_title: element.as_deref().and_then(window_title),
            field_labels: element.as_deref().map(field_labels).unwrap_or_default(),
            domain: element.as_deref().and_then(web_domain),
            field_text: element.as_deref().and_then(field_text),
            is_caret_line_hidden: element
                .as_deref()
                .is_some_and(|element| has_dom_class(element, XTERM_INPUT_CLASS)),
        }
    }
}

/// Returns whether GhostType may use Accessibility, asking macOS to prompt the user when not.
pub(crate) fn request_accessibility_access() -> bool {
    // SAFETY: reads an immutable system constant.
    let prompt_key = unsafe { kAXTrustedCheckOptionPrompt };
    let options = CFDictionary::from_slices(&[prompt_key], &[CFBoolean::new(true)]);
    // SAFETY: the options map a CFString key to a CFBoolean value, as the call expects.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_ref())) }
}

/// Asks a Chromium or Electron app to build the accessibility tree of its web content. These apps
/// skip that tree until an assistive app sets an attribute on them. Other apps ignore the request.
fn wake_web_accessibility(application: &NSRunningApplication) {
    let bundle_identifier = application
        .bundleIdentifier()
        .map(|identifier| identifier.to_string());
    let attribute = web_accessibility_attribute(bundle_identifier.as_deref());
    // SAFETY: plain call.
    let element = unsafe { AXUIElement::new_application(application.processIdentifier()) };
    // SAFETY: plain call on a valid element, with a CFBoolean value as both attributes expect.
    let error =
        unsafe { element.set_attribute_value(&CFString::from_static_str(attribute), CFBoolean::new(true)) };
    tracing::debug!(
        attribute,
        code = error.0,
        "asked the frontmost app for its web content"
    );
}

/// Picks the attribute that wakes the web content tree of the app with `bundle_identifier`.
fn web_accessibility_attribute(bundle_identifier: Option<&str>) -> &'static str {
    let is_chromium_browser = bundle_identifier.is_some_and(|identifier| {
        let identifier = identifier.to_ascii_lowercase();
        CHROMIUM_BROWSER_BUNDLE_PREFIXES
            .iter()
            .any(|prefix| identifier.starts_with(prefix))
    });
    // Window managers misplace the windows of apps with the enhanced user interface, so only these get it.
    if is_chromium_browser {
        ENHANCED_USER_INTERFACE_ATTRIBUTE
    } else {
        MANUAL_ACCESSIBILITY_ATTRIBUTE
    }
}

/// Whether `element` hides its text, as a password field does.
pub(crate) fn is_secure_text(element: &AXUIElement) -> bool {
    copy_string(element, SUBROLE_ATTRIBUTE).is_some_and(|subrole| subrole == SECURE_TEXT_FIELD_SUBROLE)
}

/// The bounds of the text that ends at the caret of `element`.
pub(crate) fn caret_bounds(element: &AXUIElement) -> Option<CGRect> {
    if has_dom_class(element, XTERM_INPUT_CLASS) {
        return terminal_input(element)?.caret_bounds();
    }
    // TextEdit reports the empty range at the caret one line too high, so measure the text before it.
    bounds_for_range(element, unit_before_caret(element)?)
}

fn unit_before_caret(element: &AXUIElement) -> Option<CFRange> {
    let selection = selected_range(element)?;
    previous_unit_range(selection.location.checked_add(selection.length)?)
}

/// Reads the color of the character before the caret of `element`. `None` for the hidden input box
/// of a terminal of xterm.js, which does not show its text.
fn text_color(element: &AXUIElement) -> Option<TextColor> {
    if has_dom_class(element, XTERM_INPUT_CLASS) {
        return None;
    }
    let text = copy_attribute_for_range(
        element,
        ATTRIBUTED_STRING_FOR_RANGE_ATTRIBUTE,
        unit_before_caret(element)?,
    )?
    .downcast::<CFAttributedString>()
    .ok()
    .filter(|text| text.length() > 0)?;
    let attribute = CFString::from_static_str(FOREGROUND_COLOR_TEXT_ATTRIBUTE);
    // SAFETY: the location is inside the string, and a null pointer asks for no effective range.
    let color = unsafe { text.attribute(0, Some(&attribute), ptr::null_mut()) }?
        .downcast::<CGColor>()
        .ok()?;
    let color = NSColor::colorWithCGColor(&color)?.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    text_color_of_components(
        [
            color.redComponent(),
            color.greenComponent(),
            color.blueComponent(),
        ],
        color.alphaComponent(),
    )
}

/// The color of text with the sRGB `components` and `alpha`, each from 0 to 1. `None` for text
/// that is mostly transparent, which does not show.
fn text_color_of_components(components: [CGFloat; 3], alpha: CGFloat) -> Option<TextColor> {
    if alpha < MIN_SHOWN_TEXT_ALPHA {
        return None;
    }
    let [red, green, blue] = components.map(channel);
    Some(TextColor { red, green, blue })
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "no conversion from f64 to u8 exists without `as`, and `as` saturates"
)]
fn channel(component: CGFloat) -> u8 {
    (component * CGFloat::from(u8::MAX)).round() as u8
}

/// The hidden input box of a terminal of xterm.js, which the terminal moves to the cell of its
/// cursor, and the screen of the terminal. The text in the box is not the text of the terminal:
/// xterm.js keeps the text of each input event there until Enter.
#[derive(Debug, Clone, Copy, PartialEq)]
struct TerminalInput {
    /// The frame of the input box, which is one row high.
    frame: CGRect,
    /// The top of the screen, where the first row starts.
    screen_top: CGFloat,
}

impl TerminalInput {
    /// The left edge of the cell of the cursor, as bounds without width, at the top of the row
    /// that holds the input box. VS Code draws the input box below the top of that row.
    fn caret_bounds(self) -> Option<CGRect> {
        let row_height = self.frame.size.height;
        if row_height <= 0.0 {
            return None;
        }
        let below_row = (self.frame.origin.y - self.screen_top).rem_euclid(row_height);
        Some(CGRect::new(
            CGPoint::new(self.frame.origin.x, self.frame.origin.y - below_row),
            CGSize::new(0.0, row_height),
        ))
    }
}

/// Reads the input box of a terminal of xterm.js, `element`, and the top of its screen.
fn terminal_input(element: &AXUIElement) -> Option<TerminalInput> {
    let screen = iter::successors(parent(element), |current| parent(current))
        .take(MAX_TERMINAL_SCREEN_DISTANCE)
        .find(|ancestor| has_dom_class(ancestor, XTERM_SCREEN_CLASS))?;
    Some(TerminalInput {
        frame: frame(element)?,
        screen_top: frame(&screen)?.origin.y,
    })
}

fn parent(element: &AXUIElement) -> Option<CFRetained<AXUIElement>> {
    copy_attribute(element, PARENT_ATTRIBUTE)?.downcast().ok()
}

/// Whether the web element `element` has the HTML class `class`.
fn has_dom_class(element: &AXUIElement, class: &str) -> bool {
    let Some(classes) =
        copy_attribute(element, DOM_CLASS_LIST_ATTRIBUTE).and_then(|value| value.downcast::<CFArray>().ok())
    else {
        return false;
    };
    // SAFETY: the array of an Accessibility attribute holds CF objects.
    let classes = unsafe { classes.cast_unchecked::<CFType>() };
    classes
        .iter()
        .filter_map(|name| name.downcast::<CFString>().ok())
        .any(|name| name.to_string() == class)
}

fn previous_unit_range(caret_index: isize) -> Option<CFRange> {
    (caret_index > 0).then(|| CFRange {
        location: caret_index - 1,
        length: 1,
    })
}

fn is_own_process(process: i32) -> bool {
    u32::try_from(process).is_ok_and(|process| process == std::process::id())
}

fn bounds_for_range(element: &AXUIElement, range: CFRange) -> Option<CGRect> {
    let bounds = copy_attribute_for_range(element, BOUNDS_FOR_RANGE_ATTRIBUTE, range)?
        .downcast::<AXValue>()
        .ok()?;
    read_rect(&bounds)
}

fn window_title(element: &AXUIElement) -> Option<String> {
    let window = copy_attribute(element, WINDOW_ATTRIBUTE)?
        .downcast::<AXUIElement>()
        .ok()?;
    copy_string(&window, TITLE_ATTRIBUTE)
}

/// Reads the labels of `element`. Its caption is its title, else the text of the element that
/// gives its title, as a text label next to a field does.
fn field_labels(element: &AXUIElement) -> FieldLabels {
    FieldLabels {
        caption: first_text(element, &[TITLE_ATTRIBUTE]).or_else(|| title_element_text(element)),
        name: first_text(element, &[DESCRIPTION_ATTRIBUTE]),
        placeholder: first_text(element, &[PLACEHOLDER_ATTRIBUTE]),
        help: first_text(element, &[HELP_ATTRIBUTE]),
    }
}

fn title_element_text(element: &AXUIElement) -> Option<String> {
    let title_element = copy_attribute(element, TITLE_UI_ELEMENT_ATTRIBUTE)?
        .downcast::<AXUIElement>()
        .ok()?;
    first_text(&title_element, &[VALUE_ATTRIBUTE, TITLE_ATTRIBUTE])
}

/// Reads the first of `attributes` of `element` that holds text that is not empty.
fn first_text(element: &AXUIElement, attributes: &[&'static str]) -> Option<String> {
    attributes
        .iter()
        .filter_map(|attribute| copy_string(element, attribute))
        .find(|text| !text.trim().is_empty())
}

/// Reads the host of the web page that holds `element`, from the URL of the nearest web area
/// above it in its window.
fn web_domain(element: &AXUIElement) -> Option<String> {
    let mut current = element.retain();
    for _ in 0..MAX_WEB_AREA_DISTANCE {
        match copy_string(&current, ROLE_ATTRIBUTE).as_deref() {
            Some(WEB_AREA_ROLE) => return url_host(&current),
            Some(WINDOW_ROLE) | None => return None,
            Some(_) => current = copy_attribute(&current, PARENT_ATTRIBUTE)?.downcast().ok()?,
        }
    }
    None
}

fn url_host(element: &AXUIElement) -> Option<String> {
    let url = copy_attribute(element, URL_ATTRIBUTE)?.downcast::<CFURL>().ok()?;
    url.host_name().map(|host| host.to_string())
}

/// Reads the text around the selection of `element`, by range, or from its whole value when it
/// cannot read a range.
fn field_text(element: &AXUIElement) -> Option<FieldText> {
    let ranges = SurroundingRanges::new(selected_range(element)?, character_count(element))?;
    field_text_by_range(element, &ranges).or_else(|| {
        let value = copy_string(element, VALUE_ATTRIBUTE)?;
        ranges.cut(&value)
    })
}

fn field_text_by_range(element: &AXUIElement, ranges: &SurroundingRanges) -> Option<FieldText> {
    let string_for_range = |range| {
        copy_attribute_for_range(element, STRING_FOR_RANGE_ATTRIBUTE, range)?
            .downcast::<CFString>()
            .ok()
            .map(|text| text.to_string())
    };
    Some(FieldText {
        before_caret: string_for_range(ranges.before)?,
        after_caret: string_for_range(ranges.after)?,
    })
}

fn selected_range(element: &AXUIElement) -> Option<CFRange> {
    let selection = copy_attribute(element, SELECTED_TEXT_RANGE_ATTRIBUTE)?
        .downcast::<AXValue>()
        .ok()?;
    read_range(&selection)
}

fn character_count(element: &AXUIElement) -> Option<isize> {
    copy_attribute(element, NUMBER_OF_CHARACTERS_ATTRIBUTE)?
        .downcast::<CFNumber>()
        .ok()?
        .as_isize()
}

/// The UTF-16 ranges of the text that a suggestion reads before and after a selection. When the
/// length of the text is unknown, the range after the selection has the most length it can have.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SurroundingRanges {
    before: CFRange,
    after: CFRange,
}

impl SurroundingRanges {
    fn new(selection: CFRange, text_length: Option<isize>) -> Option<Self> {
        let max_before = isize::try_from(MAX_CHARS_BEFORE_CARET).ok()?;
        let max_after = isize::try_from(MAX_CHARS_AFTER_CARET).ok()?;
        let before_start = selection.location.saturating_sub(max_before).max(0);
        let after_start = selection.location.checked_add(selection.length)?;
        Some(Self {
            before: CFRange {
                location: before_start,
                length: selection.location - before_start,
            },
            after: CFRange {
                location: after_start,
                length: text_length.map_or(max_after, |length| {
                    length.saturating_sub(after_start).clamp(0, max_after)
                }),
            },
        })
    }

    /// Cuts the text of the ranges from `value`, the whole text.
    fn cut(&self, value: &str) -> Option<FieldText> {
        let units: Vec<u16> = value.encode_utf16().collect();
        let slice = |start: isize, length: isize| {
            let start = usize::try_from(start).ok()?;
            let end = start.checked_add(usize::try_from(length).ok()?)?.min(units.len());
            units.get(start..end).map(String::from_utf16_lossy)
        };
        Some(FieldText {
            before_caret: slice(self.before.location, self.before.length)?,
            after_caret: slice(self.after.location, self.after.length).unwrap_or_default(),
        })
    }
}

pub(crate) fn copy_string(element: &AXUIElement, attribute: &'static str) -> Option<String> {
    copy_attribute(element, attribute)?
        .downcast::<CFString>()
        .ok()
        .map(|text| text.to_string())
}

fn copy_attribute_for_range(
    element: &AXUIElement,
    attribute: &'static str,
    range: CFRange,
) -> Option<CFRetained<CFType>> {
    // SAFETY: `range` is a local CFRange, which is what the CFRange value type describes.
    let range = unsafe { AXValue::new(AXValueType::CFRange, NonNull::from(&range).cast()) }?;
    let mut value: *const CFType = ptr::null();
    // SAFETY: the output pointer refers to a local.
    let error = unsafe {
        element.copy_parameterized_attribute_value(
            &CFString::from_static_str(attribute),
            &range,
            NonNull::from(&mut value),
        )
    };
    // SAFETY: on success the call stored a reference the caller owns.
    unsafe { take_copied_value(error, value) }
}

pub(crate) fn copy_attribute(element: &AXUIElement, attribute: &'static str) -> Option<CFRetained<CFType>> {
    let mut value: *const CFType = ptr::null();
    // SAFETY: the output pointer refers to a local.
    let error = unsafe {
        element.copy_attribute_value(&CFString::from_static_str(attribute), NonNull::from(&mut value))
    };
    // SAFETY: on success the call stored a reference the caller owns.
    unsafe { take_copied_value(error, value) }
}

/// Takes ownership of the value an Accessibility copy call returned.
///
/// # Safety
///
/// When `error` is success, `value` must be null or a reference the caller owns.
unsafe fn take_copied_value(error: AXError, value: *const CFType) -> Option<CFRetained<CFType>> {
    if error != AXError::Success {
        return None;
    }
    // SAFETY: the caller guarantees `value` is an owned reference.
    NonNull::new(value.cast_mut()).map(|value| unsafe { CFRetained::from_raw(value) })
}

fn read_range(value: &AXValue) -> Option<CFRange> {
    let mut range = CFRange {
        location: 0,
        length: 0,
    };
    // SAFETY: `range` is a local CFRange, which is what the CFRange value type describes.
    let is_read = unsafe { value.value(AXValueType::CFRange, NonNull::from(&mut range).cast()) };
    is_read.then_some(range)
}

/// The frame of `element` on the screen, in global points with a top-left origin.
pub(crate) fn frame(element: &AXUIElement) -> Option<CGRect> {
    let position = copy_attribute(element, POSITION_ATTRIBUTE)?
        .downcast::<AXValue>()
        .ok()?;
    let size = copy_attribute(element, SIZE_ATTRIBUTE)?
        .downcast::<AXValue>()
        .ok()?;
    let mut origin = CGPoint::ZERO;
    let mut extent = CGSize::ZERO;
    // SAFETY: `origin` and `extent` are a local CGPoint and CGSize, which is what the value types describe.
    let is_read = unsafe {
        position.value(AXValueType::CGPoint, NonNull::from(&mut origin).cast())
            && size.value(AXValueType::CGSize, NonNull::from(&mut extent).cast())
    };
    is_read.then_some(CGRect::new(origin, extent))
}

fn read_rect(value: &AXValue) -> Option<CGRect> {
    let mut rect = CGRect::ZERO;
    // SAFETY: `rect` is a local CGRect, which is what the CGRect value type describes.
    let is_read = unsafe { value.value(AXValueType::CGRect, NonNull::from(&mut rect).cast()) };
    is_read.then_some(rect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_starts_at_right_edge_of_bounds() {
        let bounds = CGRect::new(CGPoint::new(100.0, 40.0), CGSize::new(7.0, 18.0));

        assert_eq!(
            Caret::from_bounds(bounds),
            Some(Caret {
                left: 107.0,
                top: 40.0,
                height: 18.0
            })
        );
    }

    #[test]
    fn bounds_without_height_have_no_caret() {
        let bounds = CGRect::new(CGPoint::new(100.0, 40.0), CGSize::new(0.0, 0.0));

        assert_eq!(Caret::from_bounds(bounds), None);
    }

    #[test]
    fn caret_at_the_start_of_a_field_is_at_its_left_edge_and_as_high_as_the_field() {
        let field = CGRect::new(CGPoint::new(511.0, 101.0), CGSize::new(860.0, 18.0));

        assert_eq!(
            Caret::at_start_of(field),
            Some(Caret {
                left: 511.0,
                top: 101.0,
                height: 18.0
            })
        );
    }

    #[test]
    fn previous_unit_ends_at_caret() {
        assert_eq!(
            previous_unit_range(5),
            Some(CFRange {
                location: 4,
                length: 1
            })
        );
    }

    #[test]
    fn caret_at_start_has_no_previous_unit() {
        assert_eq!(previous_unit_range(0), None);
    }

    #[test]
    fn opaque_color_components_round_to_channels() {
        let color = text_color_of_components([1.0, 0.5, 0.0], 1.0);

        assert_eq!(
            color,
            Some(TextColor {
                red: 255,
                green: 128,
                blue: 0
            })
        );
    }

    #[test]
    fn mostly_transparent_text_has_no_color() {
        let color = text_color_of_components([1.0, 1.0, 1.0], 0.0);

        assert_eq!(color, None);
    }

    fn terminal_input(top: CGFloat, height: CGFloat) -> TerminalInput {
        TerminalInput {
            frame: CGRect::new(CGPoint::new(645.0, top), CGSize::new(8.0, height)),
            screen_top: 262.0,
        }
    }

    #[test]
    fn terminal_caret_moves_up_to_the_row_of_the_input_box() {
        let input = terminal_input(810.0, 15.0);

        let bounds = input.caret_bounds();

        assert_eq!(
            bounds,
            Some(CGRect::new(CGPoint::new(645.0, 802.0), CGSize::new(0.0, 15.0)))
        );
    }

    #[test]
    fn terminal_caret_of_an_input_box_on_its_row_stays() {
        let input = terminal_input(802.0, 15.0);

        let bounds = input.caret_bounds();

        assert_eq!(
            bounds,
            Some(CGRect::new(CGPoint::new(645.0, 802.0), CGSize::new(0.0, 15.0)))
        );
    }

    #[test]
    fn terminal_input_box_without_height_has_no_caret() {
        let input = terminal_input(810.0, 0.0);

        assert_eq!(input.caret_bounds(), None);
    }

    fn range(location: isize, length: isize) -> CFRange {
        CFRange { location, length }
    }

    #[test]
    fn ranges_reach_the_limits_around_the_caret() {
        let max_before = isize::try_from(MAX_CHARS_BEFORE_CARET).unwrap();
        let max_after = isize::try_from(MAX_CHARS_AFTER_CARET).unwrap();
        let caret = max_before + 500;

        let ranges = SurroundingRanges::new(range(caret, 0), Some(caret + max_after + 500));

        assert_eq!(
            ranges,
            Some(SurroundingRanges {
                before: range(500, max_before),
                after: range(caret, max_after),
            })
        );
    }

    #[test]
    fn ranges_stop_at_the_ends_of_the_text_and_skip_the_selection() {
        let ranges = SurroundingRanges::new(range(5, 3), Some(10));

        assert_eq!(
            ranges,
            Some(SurroundingRanges {
                before: range(0, 5),
                after: range(8, 2),
            })
        );
    }

    #[test]
    fn text_of_unknown_length_reads_the_most_after_the_selection() {
        let ranges = SurroundingRanges::new(range(5, 2), None);

        assert_eq!(ranges.map(|ranges| ranges.after), Some(range(7, 300)));
    }

    #[test]
    fn whole_value_is_cut_around_the_selection() {
        let ranges = SurroundingRanges::new(range(5, 1), None).expect("the selection is valid");

        let field_text = ranges.cut("Hello world");

        assert_eq!(
            field_text,
            Some(FieldText {
                before_caret: "Hello".to_owned(),
                after_caret: "world".to_owned(),
            })
        );
    }

    #[test]
    fn chromium_browsers_get_the_enhanced_user_interface_switch() {
        let bundle_identifiers = [
            Some("com.google.Chrome"),
            Some("com.google.Chrome.canary"),
            Some("com.brave.Browser"),
            Some("com.microsoft.edgemac"),
            Some("company.thebrowser.Browser"),
        ];

        let attributes = bundle_identifiers.map(web_accessibility_attribute);

        assert_eq!(attributes, [ENHANCED_USER_INTERFACE_ATTRIBUTE; 5]);
    }

    #[test]
    fn other_apps_get_the_manual_accessibility_switch() {
        let bundle_identifiers = [
            Some("com.microsoft.VSCode"),
            Some("com.tinyspeck.slackmacgap"),
            Some("com.apple.TextEdit"),
            None,
        ];

        let attributes = bundle_identifiers.map(web_accessibility_attribute);

        assert_eq!(attributes, [MANUAL_ACCESSIBILITY_ATTRIBUTE; 4]);
    }

    #[test]
    fn this_process_is_own_process() {
        let process = i32::try_from(std::process::id()).unwrap();

        assert!(is_own_process(process));
    }
}
