//! The Accessibility (AX) layer of GhostType.
//!
//! It names the AX attributes that GhostType reads, and reads the attributes, the ranges and the
//! frames of AX elements behind safe functions.

use std::ptr::{self, NonNull};

use objc2_application_services::{
    AXError, AXIsProcessTrustedWithOptions, AXUIElement, AXValue, AXValueType, kAXTrustedCheckOptionPrompt,
};
use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFRange, CFRetained, CFString, CFType, CGPoint, CGRect, CGSize,
};

pub(crate) const FOCUSED_ELEMENT_ATTRIBUTE: &str = "AXFocusedUIElement";
const SUBROLE_ATTRIBUTE: &str = "AXSubrole";
const SECURE_TEXT_FIELD_SUBROLE: &str = "AXSecureTextField";
const SELECTED_TEXT_RANGE_ATTRIBUTE: &str = "AXSelectedTextRange";
pub(crate) const WINDOW_ATTRIBUTE: &str = "AXWindow";
const POSITION_ATTRIBUTE: &str = "AXPosition";
const SIZE_ATTRIBUTE: &str = "AXSize";
pub(crate) const ROLE_ATTRIBUTE: &str = "AXRole";
pub(crate) const PARENT_ATTRIBUTE: &str = "AXParent";
pub(crate) const WINDOW_ROLE: &str = "AXWindow";
const DOM_CLASS_LIST_ATTRIBUTE: &str = "AXDOMClassList";
const CHILDREN_ATTRIBUTE: &str = "AXChildren";
pub(crate) const ACCESSIBILITY_QUERY_TIMEOUT_SECONDS: f32 = 0.1;

/// Returns whether GhostType may use Accessibility, asking macOS to prompt the user when not.
pub(crate) fn request_accessibility_access() -> bool {
    // SAFETY: reads an immutable system constant.
    let prompt_key = unsafe { kAXTrustedCheckOptionPrompt };
    let options = CFDictionary::from_slices(&[prompt_key], &[CFBoolean::new(true)]);
    // SAFETY: the options map a CFString key to a CFBoolean value, as the call expects.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_ref())) }
}

/// Whether `element` hides its text, as a password field does.
pub(crate) fn is_secure_text(element: &AXUIElement) -> bool {
    copy_string(element, SUBROLE_ATTRIBUTE).is_some_and(|subrole| subrole == SECURE_TEXT_FIELD_SUBROLE)
}

pub(crate) fn parent(element: &AXUIElement) -> Option<CFRetained<AXUIElement>> {
    copy_attribute(element, PARENT_ATTRIBUTE)?.downcast().ok()
}

/// Whether the web element `element` has the HTML class `class`.
pub(crate) fn has_dom_class(element: &AXUIElement, class: &str) -> bool {
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

pub(crate) fn children(element: &AXUIElement) -> Vec<CFRetained<AXUIElement>> {
    let Some(children) =
        copy_attribute(element, CHILDREN_ATTRIBUTE).and_then(|value| value.downcast::<CFArray>().ok())
    else {
        return Vec::new();
    };
    // SAFETY: the array of an Accessibility attribute holds CF objects.
    let children = unsafe { children.cast_unchecked::<CFType>() };
    children
        .iter()
        .filter_map(|child| child.downcast::<AXUIElement>().ok())
        .collect()
}

pub(crate) fn is_same(first: &AXUIElement, second: &AXUIElement) -> bool {
    let first: &CFType = first;
    let second: &CFType = second;
    first == second
}

pub(crate) fn selected_range(element: &AXUIElement) -> Option<CFRange> {
    let selection = copy_attribute(element, SELECTED_TEXT_RANGE_ATTRIBUTE)?
        .downcast::<AXValue>()
        .ok()?;
    read_range(&selection)
}

pub(crate) fn copy_string(element: &AXUIElement, attribute: &'static str) -> Option<String> {
    copy_attribute(element, attribute)?
        .downcast::<CFString>()
        .ok()
        .map(|text| text.to_string())
}

pub(crate) fn copy_attribute_for_range(
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

pub(crate) fn read_rect(value: &AXValue) -> Option<CGRect> {
    let mut rect = CGRect::ZERO;
    // SAFETY: `rect` is a local CGRect, which is what the CGRect value type describes.
    let is_read = unsafe { value.value(AXValueType::CGRect, NonNull::from(&mut rect).cast()) };
    is_read.then_some(rect)
}
