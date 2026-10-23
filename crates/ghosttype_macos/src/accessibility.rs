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
const SEARCH_FIELD_SUBROLE: &str = "AXSearchField";
const SELECTED_TEXT_RANGE_ATTRIBUTE: &str = "AXSelectedTextRange";
pub(crate) const WINDOW_ATTRIBUTE: &str = "AXWindow";
const POSITION_ATTRIBUTE: &str = "AXPosition";
const SIZE_ATTRIBUTE: &str = "AXSize";
pub(crate) const ROLE_ATTRIBUTE: &str = "AXRole";
const PARENT_ATTRIBUTE: &str = "AXParent";
pub(crate) const VALUE_ATTRIBUTE: &str = "AXValue";
pub(crate) const WINDOW_ROLE: &str = "AXWindow";
const DOM_CLASS_LIST_ATTRIBUTE: &str = "AXDOMClassList";
const CHILDREN_ATTRIBUTE: &str = "AXChildren";
const QUERY_TIMEOUT_SECONDS: f32 = 0.1;

/// Returns whether GhostType may use Accessibility, asking macOS to prompt the user when not.
pub(crate) fn request_access() -> bool {
    // SAFETY: reads an immutable system constant.
    let prompt_key = unsafe { kAXTrustedCheckOptionPrompt };
    let options = CFDictionary::from_slices(&[prompt_key], &[CFBoolean::new(true)]);
    // SAFETY: the options map a CFString key to a CFBoolean value, as the call expects.
    unsafe { AXIsProcessTrustedWithOptions(Some(options.as_ref())) }
}

/// Bounds the Accessibility queries through `element` by a short timeout, so that an app that
/// does not answer cannot stop GhostType.
pub(crate) fn bound_queries(element: &AXUIElement) {
    // SAFETY: plain call on a valid element.
    let error = unsafe { element.set_messaging_timeout(QUERY_TIMEOUT_SECONDS) };
    if error != AXError::Success {
        tracing::warn!(code = error.0, "failed to bound accessibility queries");
    }
}

/// Whether `element` hides its text, as a password field does.
pub(crate) fn is_secure_text(element: &AXUIElement) -> bool {
    copy_string(element, SUBROLE_ATTRIBUTE).is_some_and(|subrole| subrole == SECURE_TEXT_FIELD_SUBROLE)
}

/// Whether `element` is a search box, as a native search field or an HTML `<input type="search">` is.
pub(crate) fn is_search_field(element: &AXUIElement) -> bool {
    copy_string(element, SUBROLE_ATTRIBUTE).is_some_and(|subrole| subrole == SEARCH_FIELD_SUBROLE)
}

/// Whether the focused element of `system_wide` can hide its text: a password field, or an
/// element whose app did not answer in time.
pub(crate) fn can_focused_element_hide_text(system_wide: &AXUIElement) -> bool {
    let subrole = focused_subrole(system_wide);
    if subrole == Err(AXError::CannotComplete) {
        tracing::debug!("the focused app did not tell in time whether the field hides its text");
    }
    can_hide_text(subrole)
}

/// Reads the subrole of the focused element of `system_wide`, and keeps the error of a read that
/// failed.
fn focused_subrole(system_wide: &AXUIElement) -> Result<Option<String>, AXError> {
    let Some(element) = read_attribute(system_wide, FOCUSED_ELEMENT_ATTRIBUTE)?
        .and_then(|value| value.downcast::<AXUIElement>().ok())
    else {
        return Ok(None);
    };
    Ok(read_attribute(&element, SUBROLE_ATTRIBUTE)?
        .and_then(|value| value.downcast::<CFString>().ok())
        .map(|subrole| subrole.to_string()))
}

/// Whether a field whose read of the subrole gave `subrole` can hide its text. A read that did
/// not finish in time tells nothing, so that field can.
fn can_hide_text(subrole: Result<Option<String>, AXError>) -> bool {
    match subrole {
        Ok(subrole) => subrole.is_some_and(|subrole| subrole == SECURE_TEXT_FIELD_SUBROLE),
        Err(error) => error == AXError::CannotComplete,
    }
}

pub(crate) fn parent(element: &AXUIElement) -> Option<CFRetained<AXUIElement>> {
    copy_element(element, PARENT_ATTRIBUTE)
}

/// Whether the web element `element` has the HTML class `class`.
pub(crate) fn has_dom_class(element: &AXUIElement, class: &str) -> bool {
    copy_array(element, DOM_CLASS_LIST_ATTRIBUTE).is_some_and(|classes| {
        classes
            .iter()
            .filter_map(|name| name.downcast::<CFString>().ok())
            .any(|name| name.to_string() == class)
    })
}

pub(crate) fn children(element: &AXUIElement) -> Vec<CFRetained<AXUIElement>> {
    copy_array(element, CHILDREN_ATTRIBUTE)
        .map(|children| {
            children
                .iter()
                .filter_map(|child| child.downcast::<AXUIElement>().ok())
                .collect()
        })
        .unwrap_or_default()
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

pub(crate) fn copy_element(
    element: &AXUIElement,
    attribute: &'static str,
) -> Option<CFRetained<AXUIElement>> {
    copy_attribute(element, attribute)?.downcast().ok()
}

fn copy_array(element: &AXUIElement, attribute: &'static str) -> Option<CFRetained<CFArray<CFType>>> {
    let array = copy_attribute(element, attribute)?.downcast::<CFArray>().ok()?;
    // SAFETY: the array of an Accessibility attribute holds CF objects.
    Some(unsafe { CFRetained::cast_unchecked(array) })
}

pub(crate) fn copy_attribute_for_range(
    element: &AXUIElement,
    attribute: &'static str,
    range: CFRange,
) -> Option<CFRetained<CFType>> {
    // SAFETY: `range` is a local CFRange, which is what the CFRange value type describes.
    let range = unsafe { AXValue::new(AXValueType::CFRange, NonNull::from(&range).cast()) }?;
    copy_attribute_for_parameter(element, attribute, &range)
}

pub(crate) fn copy_attribute_for_parameter(
    element: &AXUIElement,
    attribute: &'static str,
    parameter: &CFType,
) -> Option<CFRetained<CFType>> {
    let mut value: *const CFType = ptr::null();
    // SAFETY: the output pointer refers to a local.
    let error = unsafe {
        element.copy_parameterized_attribute_value(
            &CFString::from_static_str(attribute),
            parameter,
            NonNull::from(&mut value),
        )
    };
    // SAFETY: on success the call stored a reference the caller owns.
    unsafe { take_copied_value(error, value) }.ok().flatten()
}

pub(crate) fn copy_attribute(element: &AXUIElement, attribute: &'static str) -> Option<CFRetained<CFType>> {
    read_attribute(element, attribute).ok().flatten()
}

/// Reads `attribute` of `element`, and keeps the error of a read that failed.
fn read_attribute(
    element: &AXUIElement,
    attribute: &'static str,
) -> Result<Option<CFRetained<CFType>>, AXError> {
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
unsafe fn take_copied_value(
    error: AXError,
    value: *const CFType,
) -> Result<Option<CFRetained<CFType>>, AXError> {
    if error != AXError::Success {
        return Err(error);
    }
    // SAFETY: the caller guarantees `value` is an owned reference.
    Ok(NonNull::new(value.cast_mut()).map(|value| unsafe { CFRetained::from_raw(value) }))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_field_and_read_that_did_not_finish_in_time_can_hide_text() {
        let reads = [
            Ok(Some(SECURE_TEXT_FIELD_SUBROLE.to_owned())),
            Err(AXError::CannotComplete),
        ];

        assert_eq!(reads.map(can_hide_text), [true; 2]);
    }

    #[test]
    fn field_with_another_subrole_or_without_one_shows_its_text() {
        let reads = [
            Ok(Some(SEARCH_FIELD_SUBROLE.to_owned())),
            Ok(None),
            Err(AXError::NoValue),
            Err(AXError::AttributeUnsupported),
        ];

        assert_eq!(reads.map(can_hide_text), [false; 4]);
    }
}
