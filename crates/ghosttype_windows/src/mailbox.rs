//! Hands messages from one thread to the message loop of another: a channel holds them and a
//! posted window message wakes the loop to take them.

use std::sync::{Arc, mpsc};

use ghosttype_core::controller::{Delivery, DeliverySink};
use windows::Win32::Foundation::HWND;

use crate::window::{WindowId, post_message};

/// The sending end of the channel.
#[derive(Debug)]
pub(crate) struct Mailbox<Message> {
    sender: mpsc::Sender<Message>,
    window: WindowId,
    wake_message: u32,
}

impl<Message> Mailbox<Message> {
    /// Creates a mailbox that posts `wake_message` to `window` for each message, and the receiver
    /// that the thread of `window` drains.
    pub(crate) fn new(window: HWND, wake_message: u32) -> (Self, mpsc::Receiver<Message>) {
        let (sender, receiver) = mpsc::channel();
        let mailbox = Self {
            sender,
            window: WindowId::from(window),
            wake_message,
        };
        (mailbox, receiver)
    }

    /// Sends `message` and wakes the receiving thread. Does nothing when the receiver is gone.
    pub(crate) fn send(&self, message: Message) {
        if self.sender.send(message).is_err() {
            return;
        }
        if let Err(error) = post_message(self.window.to_hwnd(), self.wake_message) {
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
