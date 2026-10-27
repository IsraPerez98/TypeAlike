//! The permissions of macOS that TypeAlike asks the user for: Accessibility, and Screen & System
//! Audio Recording for the OCR.

use std::process::Command;

use objc2_foundation::NSBundle;
use typealike_core::desktop::{Permission, PermissionState};

use crate::{accessibility, ocr};

const TCCUTIL: &str = "/usr/bin/tccutil";

/// Reads the state of each permission, in the order of the Permissions pane.
pub(crate) fn states() -> Vec<(Permission, PermissionState)> {
    let state = |is_granted| {
        if is_granted {
            PermissionState::Granted
        } else {
            PermissionState::Missing
        }
    };
    vec![
        (Permission::Accessibility, state(accessibility::is_trusted())),
        (Permission::ScreenRecording, state(ocr::is_permitted())),
    ]
}

/// Asks the user for `permission`. Each build has another ad-hoc signature, and macOS refuses the
/// grant of an older build while System Settings still shows it on. Thus this first removes the
/// grant of TypeAlike, and the prompt of macOS then adds TypeAlike to the list again.
pub(crate) fn ask(permission: Permission) {
    remove_grant(permission);
    match permission {
        Permission::Accessibility => {
            if !accessibility::request_access() {
                tracing::info!("asked for accessibility access");
            }
        }
        Permission::ScreenRecording => ocr::ask_for_permission(),
    }
}

/// Removes the grant of `permission` from the list of System Settings. A program without a bundle,
/// as from `cargo run`, has no entry of its own: the terminal app holds its permissions.
fn remove_grant(permission: Permission) {
    let Some(bundle_id) = NSBundle::mainBundle().bundleIdentifier() else {
        return;
    };
    let service = match permission {
        Permission::Accessibility => "Accessibility",
        Permission::ScreenRecording => "ScreenCapture",
    };
    match Command::new(TCCUTIL)
        .args(["reset", service, &bundle_id.to_string()])
        .status()
    {
        Ok(status) if status.success() => tracing::info!(service, "removed the old permission"),
        Ok(status) => tracing::warn!(service, %status, "tccutil failed to remove the old permission"),
        Err(error) => tracing::warn!(service, %error, "failed to run tccutil"),
    }
}
