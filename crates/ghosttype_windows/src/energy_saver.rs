//! The energy saver of Windows (battery saver on Windows 10): its state, and a watch that reports
//! each change of it to the thread of the controller.

use ghosttype_core::desktop::EnergySaver;
use windows::Foundation::EventHandler;
use windows::System::Power::{EnergySaverStatus, PowerManager};

use crate::mailbox::Mailbox;

/// Reads the state of the energy saver. A failed read counts as off.
pub(crate) fn read() -> EnergySaver {
    if PowerManager::EnergySaverStatus().is_ok_and(|status| status == EnergySaverStatus::On) {
        EnergySaver::On
    } else {
        EnergySaver::Off
    }
}

/// Sends the state of the energy saver to its mailbox after each change, until it is dropped.
#[derive(Debug)]
pub(crate) struct EnergySaverWatch {
    token: i64,
}

impl EnergySaverWatch {
    /// Starts to send each new state of the energy saver to `mailbox`. Windows calls the handler on
    /// a thread of its pool.
    pub(crate) fn start(mailbox: Mailbox<EnergySaver>) -> windows::core::Result<Self> {
        let handler = EventHandler::new(move |_, _| {
            mailbox.send(read());
            Ok(())
        });
        let token = PowerManager::EnergySaverStatusChanged(&handler)?;
        Ok(Self { token })
    }
}

impl Drop for EnergySaverWatch {
    fn drop(&mut self) {
        if let Err(error) = PowerManager::RemoveEnergySaverStatusChanged(self.token) {
            tracing::warn!(%error, "failed to stop the watch of the energy saver");
        }
    }
}
