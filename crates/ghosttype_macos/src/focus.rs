//! What has keyboard focus: whether GhostType may suggest there, where its caret is, and the text
//! around the caret.

use std::ptr::{self, NonNull};

use ghosttype_core::app::Field;
use ghosttype_core::desktop::{
    Desktop, FieldText, MAX_CHARS_AFTER_CARET, MAX_CHARS_BEFORE_CARET, Surroundings,
};
use objc2_app_kit::NSWorkspace;
use objc2_application_services::{
    AXError, AXIsProcessTrustedWithOptions, AXUIElement, AXValue, AXValueType, kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFNumber, CFRange, CFRetained, CFString, CFType, CGFloat, CGRect,
};

use crate::clipboard;

const FOCUSED_ELEMENT_ATTRIBUTE: &str = "AXFocusedUIElement";
const SUBROLE_ATTRIBUTE: &str = "AXSubrole";
const SECURE_TEXT_FIELD_SUBROLE: &str = "AXSecureTextField";
const SELECTED_TEXT_RANGE_ATTRIBUTE: &str = "AXSelectedTextRange";
const BOUNDS_FOR_RANGE_ATTRIBUTE: &str = "AXBoundsForRange";
const STRING_FOR_RANGE_ATTRIBUTE: &str = "AXStringForRange";
const NUMBER_OF_CHARACTERS_ATTRIBUTE: &str = "AXNumberOfCharacters";
const VALUE_ATTRIBUTE: &str = "AXValue";
const WINDOW_ATTRIBUTE: &str = "AXWindow";
const TITLE_ATTRIBUTE: &str = "AXTitle";
const ACCESSIBILITY_QUERY_TIMEOUT_SECONDS: f32 = 0.1;

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
}

/// Whether the frontmost app changed since it was last observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FocusChange {
    Unchanged,
    Changed,
}

/// Follows the frontmost app and classifies whether GhostType may observe and type into it.
#[derive(Debug)]
pub(crate) struct FocusTracker {
    system_wide: CFRetained<AXUIElement>,
    frontmost_process: Option<i32>,
    frontmost_name: Option<String>,
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
            .and_then(|application| application.localizedName())
            .map(|name| name.to_string());
        self.is_frontmost_eligible = frontmost_process.is_some_and(|process| !is_own_process(process));
        FocusChange::Changed
    }

    /// Locates the caret of the focused control, when it exposes one through Accessibility.
    pub(crate) fn caret(&self) -> Option<Caret> {
        let element = self.focused_element()?;
        let selection = selected_range(&element)?;
        let caret_index = selection.location.checked_add(selection.length)?;
        // TextEdit reports the empty range at the caret one line too high, so measure the text before it.
        bounds_for_range(&element, previous_unit_range(caret_index)?).and_then(Caret::from_bounds)
    }

    fn is_secure_text_focused(&self) -> bool {
        self.focused_element()
            .and_then(|element| copy_attribute(&element, SUBROLE_ATTRIBUTE))
            .and_then(|subrole| subrole.downcast::<CFString>().ok())
            .is_some_and(|subrole| subrole.to_string() == SECURE_TEXT_FIELD_SUBROLE)
    }

    fn focused_element(&self) -> Option<CFRetained<AXUIElement>> {
        copy_attribute(&self.system_wide, FOCUSED_ELEMENT_ATTRIBUTE)?
            .downcast::<AXUIElement>()
            .ok()
    }
}

impl Desktop for FocusTracker {
    /// Classifies the focused control right now: excluded for secure text and ineligible apps.
    fn field(&self) -> Field {
        if self.is_frontmost_eligible && !self.is_secure_text_focused() {
            Field::Open
        } else {
            Field::Excluded
        }
    }

    fn surroundings(&self) -> Surroundings {
        let element = self.focused_element();
        Surroundings {
            app_name: self.frontmost_name.clone(),
            window_title: element.as_deref().and_then(window_title),
            field_text: element.as_deref().and_then(field_text),
        }
    }

    fn clipboard_text(&self) -> Option<String> {
        clipboard::copied_text()
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

fn copy_string(element: &AXUIElement, attribute: &'static str) -> Option<String> {
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

fn copy_attribute(element: &AXUIElement, attribute: &'static str) -> Option<CFRetained<CFType>> {
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

fn read_rect(value: &AXValue) -> Option<CGRect> {
    let mut rect = CGRect::ZERO;
    // SAFETY: `rect` is a local CGRect, which is what the CGRect value type describes.
    let is_read = unsafe { value.value(AXValueType::CGRect, NonNull::from(&mut rect).cast()) };
    is_read.then_some(rect)
}

#[cfg(test)]
mod tests {
    use objc2_core_foundation::{CGPoint, CGSize};

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

    fn range(location: isize, length: isize) -> CFRange {
        CFRange { location, length }
    }

    #[test]
    fn ranges_reach_the_limits_around_the_caret() {
        let ranges = SurroundingRanges::new(range(1500, 0), Some(2000));

        assert_eq!(
            ranges,
            Some(SurroundingRanges {
                before: range(1500 - 1000, 1000),
                after: range(1500, 300),
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
    fn this_process_is_own_process() {
        let process = i32::try_from(std::process::id()).unwrap();

        assert!(is_own_process(process));
    }
}
