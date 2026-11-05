//! The SF Symbols of TypeAlike.

use objc2::rc::Retained;
use objc2_app_kit::NSImage;
use objc2_foundation::NSString;

/// The SF Symbol `name`, which assistive technology reads as `description`, or `None` when macOS
/// has no symbol of that name.
pub(crate) fn system_symbol(name: &str, description: &str) -> Option<Retained<NSImage>> {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        Some(&NSString::from_str(description)),
    )
}
