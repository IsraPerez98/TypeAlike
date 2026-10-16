//! Hands events from worker threads to the main thread: a channel holds them and a posted window
//! message wakes the message loop to take them.

use std::sync::{Arc, mpsc};

use ghosttype_core::controller::{Delivery, DeliverySink};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

/// Message posted to the main window when deliveries wait in the channel.
pub(crate) const DELIVERY_MESSAGE: u32 = WM_APP + 3;

/// The sending end of the channel.
#[derive(Debug)]
pub(crate) struct Mailbox {
    sender: mpsc::Sender<Delivery>,
    window: usize,
}

impl Mailbox {
    /// Creates a mailbox that wakes `window`, and the receiver the main thread drains.
    pub(crate) fn new(window: HWND) -> (Self, mpsc::Receiver<Delivery>) {
        let (sender, receiver) = mpsc::channel();
        let mailbox = Self {
            sender,
            window: window.0.expose_provenance(),
        };
        (mailbox, receiver)
    }

    /// Returns a sink, callable from any thread, that sends each delivery to the main thread.
    pub(crate) fn into_sink(self) -> DeliverySink {
        Arc::new(move |delivery| self.deliver(delivery))
    }

    fn deliver(&self, delivery: Delivery) {
        if self.sender.send(delivery).is_err() {
            return;
        }
        let window = HWND(std::ptr::with_exposed_provenance_mut(self.window));
        // SAFETY: plain call; posting to a window of another thread is allowed and fails safely when it is gone.
        if let Err(error) = unsafe { PostMessageW(Some(window), DELIVERY_MESSAGE, WPARAM(0), LPARAM(0)) } {
            tracing::warn!(%error, "failed to wake the message loop");
        }
    }
}
