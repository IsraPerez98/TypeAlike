//! The session event tap that sees key presses and clicks before any app, removed on drop.

use std::ptr;

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventMask, CGEventTapCallBack, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventType,
};

const WATCHED_EVENTS: [CGEventType; 5] = [
    CGEventType::KeyDown,
    CGEventType::KeyUp,
    CGEventType::LeftMouseDown,
    CGEventType::RightMouseDown,
    CGEventType::OtherMouseDown,
];
const SOURCE_ORDER: isize = 0;

#[derive(Debug)]
pub(crate) struct EventTap {
    port: CFRetained<CFMachPort>,
    source: CFRetained<CFRunLoopSource>,
}

impl EventTap {
    /// Installs `callback` ahead of every app in this session, on the main run loop.
    ///
    /// Returns `None` when macOS refuses the tap, which happens without Accessibility access.
    pub(crate) fn install(callback: CGEventTapCallBack) -> Option<Self> {
        // SAFETY: the callback lives for the whole program and ignores its user info, passed null.
        let port = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                event_mask(&WATCHED_EVENTS),
                callback,
                ptr::null_mut(),
            )
        }?;
        let source = CFMachPort::new_run_loop_source(None, Some(&port), SOURCE_ORDER)?;
        // SAFETY: reads an immutable system constant.
        let common_modes = unsafe { kCFRunLoopCommonModes };
        CFRunLoop::main()?.add_source(Some(&source), common_modes);
        Some(Self { port, source })
    }

    /// Turns the tap back on after macOS disabled it for a slow callback.
    pub(crate) fn enable(&self) {
        CGEvent::tap_enable(&self.port, true);
    }
}

impl Drop for EventTap {
    fn drop(&mut self) {
        self.source.invalidate();
        self.port.invalidate();
    }
}

fn event_mask(events: &[CGEventType]) -> CGEventMask {
    events.iter().fold(0, |mask, event| mask | (1 << event.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_has_one_bit_per_event_type() {
        let mask = event_mask(&[CGEventType::LeftMouseDown, CGEventType::KeyDown]);

        assert_eq!(mask, 0b100_0000_0010);
    }
}
