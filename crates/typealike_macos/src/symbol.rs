//! The SF Symbols of TypeAlike, such as the symbol of its status on the badge.

use objc2::rc::Retained;
use objc2_app_kit::NSImage;
use objc2_foundation::NSString;
use typealike_core::app::Status;

const ACTIVE_SYMBOL: &str = "character.cursor.ibeam";
const PAUSED_SYMBOL: &str = "pause.circle";

/// The SF Symbol `name`, which assistive technology reads as `description`, or `None` when macOS
/// has no symbol of that name.
pub(crate) fn system_symbol(name: &str, description: &str) -> Option<Retained<NSImage>> {
    NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        Some(&NSString::from_str(description)),
    )
}

/// The SF Symbol that shows `status`, which assistive technology reads as `description`.
pub(crate) fn status_symbol(status: Status, description: &str) -> Option<Retained<NSImage>> {
    let name = match status {
        Status::Active => ACTIVE_SYMBOL,
        Status::Paused => PAUSED_SYMBOL,
    };
    system_symbol(name, description)
}
