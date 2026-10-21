//! The floating panels of GhostType: borderless panels over every app that never take focus,
//! and the height of the primary screen that places them.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSPanel, NSScreen, NSStatusWindowLevel, NSWindowAnimationBehavior,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::NSRect;

/// Builds a borderless panel that floats over every app on every Space. It never takes focus,
/// and clicks go through it.
pub(crate) fn floating_panel(main_thread: MainThreadMarker) -> Retained<NSPanel> {
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        NSPanel::alloc(main_thread),
        NSRect::ZERO,
        NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
        NSBackingStoreType::Buffered,
        false,
    );
    // SAFETY: the caller owns the panel, so AppKit must not release it on close.
    unsafe { panel.setReleasedWhenClosed(false) };
    panel.setIgnoresMouseEvents(true);
    panel.setOpaque(false);
    panel.setBackgroundColor(Some(&NSColor::clearColor()));
    panel.setHasShadow(false);
    panel.setHidesOnDeactivate(false);
    panel.setLevel(NSStatusWindowLevel);
    panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::Stationary,
    );
    panel
}

/// The height of the primary screen, which turns the top-left global coordinates of the caret
/// into the bottom-left ones of AppKit.
pub(crate) fn primary_screen_height(main_thread: MainThreadMarker) -> Option<CGFloat> {
    let primary_screen = NSScreen::screens(main_thread).firstObject()?;
    Some(primary_screen.frame().size.height)
}
