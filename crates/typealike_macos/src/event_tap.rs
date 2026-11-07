//! The session event tap that sees key presses, clicks and scrolls before any app, removed on drop.

use std::cell::OnceCell;
use std::ffi::c_void;
use std::ptr::{self, NonNull};

use objc2_core_foundation::{CFMachPort, CFRetained, CFRunLoop, CFRunLoopSource, kCFRunLoopCommonModes};
use objc2_core_graphics::{
    CGEvent, CGEventMask, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventTapProxy,
    CGEventType,
};
use typealike_core::key::Disposition;

const WATCHED_EVENTS: [CGEventType; 7] = [
    CGEventType::KeyDown,
    CGEventType::KeyUp,
    CGEventType::LeftMouseDown,
    CGEventType::LeftMouseUp,
    CGEventType::RightMouseDown,
    CGEventType::OtherMouseDown,
    CGEventType::ScrollWheel,
];
const SOURCE_ORDER: isize = 0;

/// Decides whether the tap passes or swallows a watched event.
pub(crate) type EventHandler = fn(CGEventType, &CGEvent) -> Disposition;

#[derive(Debug)]
pub(crate) struct EventTap {
    source: CFRetained<CFRunLoopSource>,
    /// The user info of the tap. The box keeps it at one address while the tap lives.
    context: Box<TapContext>,
}

#[derive(Debug)]
struct TapContext {
    handler: EventHandler,
    /// The tap itself, empty only while [`EventTap::install`] creates it.
    port: OnceCell<CFRetained<CFMachPort>>,
}

impl EventTap {
    /// Installs `handler` ahead of every app in this session, on the main run loop. When macOS
    /// turns the tap off, for a slow handler or for user input, the tap turns itself on again.
    ///
    /// Returns `None` when macOS refuses the tap, which happens without Accessibility access.
    pub(crate) fn install(handler: EventHandler) -> Option<Self> {
        let context = Box::new(TapContext {
            handler,
            port: OnceCell::new(),
        });
        // SAFETY: the user info is the context, which lives until `drop` invalidates the port.
        let port = unsafe {
            CGEvent::tap_create(
                CGEventTapLocation::SessionEventTap,
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::Default,
                event_mask(&WATCHED_EVENTS),
                Some(forward_event),
                ptr::from_ref(&*context).cast_mut().cast(),
            )
        }?;
        let port = context.port.get_or_init(|| port);
        let Some(source) = add_to_main_run_loop(port) else {
            port.invalidate();
            return None;
        };
        Some(Self { source, context })
    }
}

impl Drop for EventTap {
    fn drop(&mut self) {
        self.source.invalidate();
        if let Some(port) = self.context.port.get() {
            port.invalidate();
        }
    }
}

fn add_to_main_run_loop(port: &CFMachPort) -> Option<CFRetained<CFRunLoopSource>> {
    let source = CFMachPort::new_run_loop_source(None, Some(port), SOURCE_ORDER)?;
    // SAFETY: reads an immutable system constant.
    let common_modes = unsafe { kCFRunLoopCommonModes };
    CFRunLoop::main()?.add_source(Some(&source), common_modes);
    Some(source)
}

extern "C-unwind" fn forward_event(
    _proxy: CGEventTapProxy,
    kind: CGEventType,
    event: NonNull<CGEvent>,
    context: *mut c_void,
) -> *mut CGEvent {
    // SAFETY: the user info of the tap is its context, which outlives the tap.
    let context = unsafe { &*context.cast::<TapContext>() };
    if is_disable_notice(kind) {
        if let Some(port) = context.port.get() {
            CGEvent::tap_enable(port, true);
        }
        return event.as_ptr();
    }
    // SAFETY: the tap passes an event that stays valid for the whole callback.
    match (context.handler)(kind, unsafe { event.as_ref() }) {
        Disposition::Swallow => ptr::null_mut(),
        Disposition::Pass => event.as_ptr(),
    }
}

/// Whether `kind` tells that macOS turned the tap off.
fn is_disable_notice(kind: CGEventType) -> bool {
    matches!(
        kind,
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput
    )
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

    #[test]
    fn both_kinds_of_disable_notice_turn_the_tap_on_again() {
        let kinds = [
            CGEventType::TapDisabledByTimeout,
            CGEventType::TapDisabledByUserInput,
            CGEventType::KeyDown,
        ];

        let notices = kinds.map(is_disable_notice);

        assert_eq!(notices, [true, true, false]);
    }
}
