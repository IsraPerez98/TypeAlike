//! Low Power Mode of macOS: its state, and an observer that reports each change of it on the main
//! thread.

use std::ptr::NonNull;

use block2::RcBlock;
use ghosttype_core::desktop::EnergySaver;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_foundation::{
    NSNotification, NSNotificationCenter, NSObjectProtocol, NSOperationQueue, NSProcessInfo,
    NSProcessInfoPowerStateDidChangeNotification,
};

/// Reads whether Low Power Mode is on.
pub(crate) fn read() -> EnergySaver {
    if NSProcessInfo::processInfo().isLowPowerModeEnabled() {
        EnergySaver::On
    } else {
        EnergySaver::Off
    }
}

/// Gives the state of Low Power Mode to its handler after each change, until it is dropped.
pub(crate) struct EnergySaverObserver {
    observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
}

impl EnergySaverObserver {
    /// Starts to give each new state of Low Power Mode to `on_change`. macOS posts the change on a
    /// queue of its own, and the observer calls `on_change` on the main thread.
    pub(crate) fn start(on_change: fn(EnergySaver)) -> Self {
        let block = RcBlock::new(move |_notification: NonNull<NSNotification>| on_change(read()));
        // SAFETY: the observer takes no object, the main queue runs the block on the main thread, the
        // block holds only a function pointer, and drop removes the observer.
        let observer = unsafe {
            NSNotificationCenter::defaultCenter().addObserverForName_object_queue_usingBlock(
                Some(NSProcessInfoPowerStateDidChangeNotification),
                None,
                Some(&NSOperationQueue::mainQueue()),
                &block,
            )
        };
        Self { observer }
    }
}

impl Drop for EnergySaverObserver {
    fn drop(&mut self) {
        let observer: &AnyObject = self.observer.as_ref();
        // SAFETY: the observer is the object that the default center gave when it added the block.
        unsafe { NSNotificationCenter::defaultCenter().removeObserver(observer) };
    }
}
