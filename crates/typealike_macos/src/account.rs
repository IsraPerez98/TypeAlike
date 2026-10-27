//! The account of the user who runs TypeAlike.

use objc2_foundation::NSProcessInfo;
use typealike_core::settings::WriterName;

/// The full name of the user account, such as "Sara Novak", for the name of the writer.
#[must_use]
pub fn account_name() -> WriterName {
    WriterName(
        NSProcessInfo::processInfo()
            .fullUserName()
            .to_string()
            .trim()
            .to_owned(),
    )
}
