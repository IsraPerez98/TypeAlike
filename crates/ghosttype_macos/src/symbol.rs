//! The SF Symbols of GhostType, such as the symbol of its status in the menu bar and on the badge.

use ghosttype_core::app::Status;
use objc2::rc::Retained;
use objc2_app_kit::NSImage;
use objc2_foundation::NSString;

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
