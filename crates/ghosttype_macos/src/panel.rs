//! The floating panels of GhostType, which never take focus.
//!
//! It also gives the height of the primary screen, which places a panel in AppKit coordinates.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSBackingStoreType, NSBox, NSBoxType, NSColor, NSPanel, NSScreen, NSStatusWindowLevel, NSTitlePosition,
    NSView, NSWindowAnimationBehavior, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSRect, NSSize};

/// The width of the border of a rounded panel, in points.
pub(crate) const ROUNDED_PANEL_BORDER_WIDTH: CGFloat = 1.0;

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

/// Builds a floating panel that shows `content` in a box with round corners of `corner_radius`
/// and a hairline border, with `padding` points between the border and `content`.
pub(crate) fn rounded_panel(
    content: &NSView,
    corner_radius: CGFloat,
    padding: CGFloat,
    main_thread: MainThreadMarker,
) -> Retained<NSPanel> {
    let background = NSBox::new(main_thread);
    background.setBoxType(NSBoxType::Custom);
    background.setTitlePosition(NSTitlePosition::NoTitle);
    background.setCornerRadius(corner_radius);
    background.setBorderWidth(ROUNDED_PANEL_BORDER_WIDTH);
    background.setBorderColor(&NSColor::separatorColor());
    background.setFillColor(&NSColor::windowBackgroundColor());
    background.setContentViewMargins(NSSize::new(padding, padding));
    background.setContentView(Some(content));
    let panel = floating_panel(main_thread);
    panel.setContentView(Some(&background));
    panel
}

/// The height of the primary screen, which turns the top-left global coordinates of the caret
/// into the bottom-left ones of AppKit.
pub(crate) fn primary_screen_height(main_thread: MainThreadMarker) -> Option<CGFloat> {
    let primary_screen = NSScreen::screens(main_thread).firstObject()?;
    Some(primary_screen.frame().size.height)
}
