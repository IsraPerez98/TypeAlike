//! The SF Symbols that show the status of GhostType in the menu bar and on the badge.

use ghosttype_core::app::Status;
use objc2::rc::Retained;
use objc2_app_kit::NSImage;
use objc2_foundation::NSString;

const ACTIVE_SYMBOL: &str = "character.cursor.ibeam";
const PAUSED_SYMBOL: &str = "pause.circle";

/// The SF Symbol that shows `status`, which assistive technology reads as `description`.
pub(crate) fn symbol_image(status: Status, description: &str) -> Option<Retained<NSImage>> {
    let symbol = match status {
        Status::Active => ACTIVE_SYMBOL,
        Status::Paused => PAUSED_SYMBOL,
    };
    NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(symbol),
        Some(&NSString::from_str(description)),
    )
}
