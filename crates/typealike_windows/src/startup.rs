//! Starts TypeAlike at login with a value in the user's `Run` registry key.

use std::ffi::OsString;
use std::path::Path;

use typealike_core::settings::Autostart;
use windows::Win32::System::Registry::RRF_RT_REG_BINARY;
use windows::core::{PCWSTR, w};

use crate::registry;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const APPROVAL_KEY: PCWSTR =
    w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\StartupApproved\\Run");
const ENTRY_NAME: PCWSTR = w!("TypeAlike");
const APPROVAL_BYTES: usize = 12;
const STOPPED_FLAG: u8 = 0b1;

/// The value in the user's `Run` registry key that starts a program when the user logs in.
///
/// The user can also stop the entry in Task Manager or in the Startup apps page of Windows
/// Settings. The entry is then off, and to turn it on lets it start again.
#[derive(Debug)]
pub struct StartupEntry {
    command: OsString,
}

impl StartupEntry {
    /// Creates the entry that starts `program`.
    #[must_use]
    pub fn new(program: &Path) -> Self {
        Self {
            command: run_command(program),
        }
    }
}

impl Autostart for StartupEntry {
    fn is_enabled(&self) -> bool {
        registry::has_value(RUN_KEY, ENTRY_NAME) && is_approved_now()
    }

    fn enable(&mut self) {
        if let Err(error) = registry::write_string(RUN_KEY, ENTRY_NAME, &self.command) {
            tracing::warn!(%error, "failed to add the startup entry");
        }
        if let Err(error) = registry::delete_value(APPROVAL_KEY, ENTRY_NAME) {
            tracing::warn!(%error, "failed to let the startup entry start");
        }
    }

    fn disable(&mut self) {
        if let Err(error) = registry::delete_value(RUN_KEY, ENTRY_NAME) {
            tracing::warn!(%error, "failed to remove the startup entry");
        }
    }
}

/// The command line that starts `program`, in quotes, so that a path with spaces stays one
/// argument.
fn run_command(program: &Path) -> OsString {
    let mut command = OsString::from("\"");
    command.push(program);
    command.push("\"");
    command
}

fn is_approved_now() -> bool {
    let mut approval = [0_u8; APPROVAL_BYTES];
    let size = registry::read_value(APPROVAL_KEY, ENTRY_NAME, RRF_RT_REG_BINARY, &mut approval).unwrap_or(0);
    is_approved(approval.get(..size).unwrap_or_default())
}

/// Whether the `approval` bytes that Windows keeps for a startup entry let it start. Windows sets
/// the low bit of the first byte when the user stops the entry. It keeps no bytes for an entry
/// that the user did not change.
fn is_approved(approval: &[u8]) -> bool {
    approval.first().is_none_or(|flags| flags & STOPPED_FLAG == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_quotes_the_program_path() {
        let program = Path::new(r"C:\Program Files\TypeAlike\TypeAlike.exe");

        let command = run_command(program);

        assert_eq!(command, r#""C:\Program Files\TypeAlike\TypeAlike.exe""#);
    }

    #[test]
    fn entry_starts_unless_windows_marks_it_stopped() {
        let approvals: [&[u8]; 5] = [&[], &[0x02, 0x00], &[0x06, 0x00], &[0x03, 0x00], &[0x07, 0x00]];

        let is_approved = approvals.map(is_approved);

        assert_eq!(is_approved, [true, true, true, false, false]);
    }
}
