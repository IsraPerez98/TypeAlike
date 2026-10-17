//! Hands messages from one thread to the message loop of another: a channel holds them and a
//! posted window message wakes the loop to take them.

use std::sync::{Arc, mpsc};

use ghosttype_core::controller::{Delivery, DeliverySink};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

/// The sending end of the channel.
#[derive(Debug)]
pub(crate) struct Mailbox<Message> {
    sender: mpsc::Sender<Message>,
    window: usize,
    wake_message: u32,
}

impl<Message> Mailbox<Message> {
    /// Creates a mailbox that posts `wake_message` to `window` for each message, and the receiver
    /// that the thread of `window` drains.
    pub(crate) fn new(window: HWND, wake_message: u32) -> (Self, mpsc::Receiver<Message>) {
        let (sender, receiver) = mpsc::channel();
        let mailbox = Self {
            sender,
            window: window.0.expose_provenance(),
            wake_message,
        };
        (mailbox, receiver)
    }

    /// Sends `message` and wakes the receiving thread. Does nothing when the receiver is gone.
    pub(crate) fn send(&self, message: Message) {
        if self.sender.send(message).is_err() {
            return;
        }
        let window = HWND(std::ptr::with_exposed_provenance_mut(self.window));
        // SAFETY: plain call; posting to a window of another thread is allowed and fails safely when it is gone.
        if let Err(error) = unsafe { PostMessageW(Some(window), self.wake_message, WPARAM(0), LPARAM(0)) } {
            tracing::warn!(%error, "failed to wake a message loop");
        }
    }
}

impl Mailbox<Delivery> {
    /// Returns a sink, callable from any thread, that sends each delivery to the receiving thread.
    pub(crate) fn into_sink(self) -> DeliverySink {
        Arc::new(move |delivery| self.send(delivery))
    }
}
