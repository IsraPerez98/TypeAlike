//! What has keyboard focus: whether GhostType may suggest there, and where its caret is.

use std::ptr::{self, NonNull};

use ghosttype_core::app::Field;
use objc2_app_kit::NSWorkspace;
use objc2_application_services::{
    AXError, AXIsProcessTrustedWithOptions, AXUIElement, AXValue, AXValueType, kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFRange, CFRetained, CFString, CFType, CGFloat, CGRect,
};

const FOCUSED_ELEMENT_ATTRIBUTE: &str = "AXFocusedUIElement";
const SUBROLE_ATTRIBUTE: &str = "AXSubrole";
const SECURE_TEXT_FIELD_SUBROLE: &str = "AXSecureTextField";
const SELECTED_TEXT_RANGE_ATTRIBUTE: &str = "AXSelectedTextRange";
const BOUNDS_FOR_RANGE_ATTRIBUTE: &str = "AXBoundsForRange";
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
            is_frontmost_eligible: false,
        }
    }

    /// Observes the frontmost app; returns whether it changed since the last call.
    pub(crate) fn refresh(&mut self) -> FocusChange {
        let frontmost_process = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|application| application.processIdentifier());
        if frontmost_process == self.frontmost_process {
            return FocusChange::Unchanged;
        }
        self.frontmost_process = frontmost_process;
        self.is_frontmost_eligible = frontmost_process.is_some_and(|process| !is_own_process(process));
        FocusChange::Changed
    }

    /// Classifies the focused control right now: excluded for secure text and ineligible apps.
    pub(crate) fn field(&self) -> Field {
        if self.is_frontmost_eligible && !self.is_secure_text_focused() {
            Field::Open
        } else {
            Field::Excluded
        }
    }

    /// Locates the caret of the focused control, when it exposes one through Accessibility.
    pub(crate) fn caret(&self) -> Option<Caret> {
        let element = self.focused_element()?;
        let selection = copy_attribute(&element, SELECTED_TEXT_RANGE_ATTRIBUTE)?
            .downcast::<AXValue>()
            .ok()?;
        let selection = read_range(&selection)?;
        let caret_index = selection.location.checked_add(selection.length)?;
        let at_caret = CFRange {
            location: caret_index,
            length: 0,
        };
        bounds_for_range(&element, at_caret)
            .and_then(Caret::from_bounds)
            .or_else(|| {
                bounds_for_range(&element, previous_unit_range(caret_index)?).and_then(Caret::from_bounds)
            })
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
    // SAFETY: `range` is a local CFRange, which is what the CFRange value type describes.
    let range = unsafe { AXValue::new(AXValueType::CFRange, NonNull::from(&range).cast()) }?;
    let mut bounds: *const CFType = ptr::null();
    // SAFETY: the output pointer refers to a local.
    let error = unsafe {
        element.copy_parameterized_attribute_value(
            &CFString::from_static_str(BOUNDS_FOR_RANGE_ATTRIBUTE),
            &range,
            NonNull::from(&mut bounds),
        )
    };
    // SAFETY: on success the call stored a reference the caller owns.
    let bounds = unsafe { take_copied_value(error, bounds) }?
        .downcast::<AXValue>()
        .ok()?;
    read_rect(&bounds)
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

    #[test]
    fn this_process_is_own_process() {
        let process = i32::try_from(std::process::id()).unwrap();

        assert!(is_own_process(process));
    }
}
