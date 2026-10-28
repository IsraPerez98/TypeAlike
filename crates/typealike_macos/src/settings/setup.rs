//! The setup window, which shows the steps of the core setup view: a scrolled step with the
//! picture, the title and the body of the step, its demo, its rows as in the settings window and
//! its numbered instructions, above a fixed footer with the progress and the buttons.
//!
//! The window has a transparent title bar without a title and only a close button, like the
//! assistants of macOS. A new step fades and slides in, and the demo plays each time the first
//! step shows.

mod demo;
mod footer;
mod motion;
mod step;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSBox, NSBoxType, NSButton, NSControl, NSControlStateValueOn,
    NSLayoutConstraint, NSPopUpButton, NSSwitch, NSTitlePosition, NSView, NSWindow, NSWindowButton,
    NSWindowStyleMask, NSWindowTabbingMode, NSWindowTitleVisibility, NSWindowWillCloseNotification,
};
use objc2_foundation::{
    NSArray, NSEdgeInsets, NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize, NSString, NSTimer,
};
use typealike_core::settings_window::view::{ControlId, SettingsInput};
use typealike_core::setup::{SetupStep, SetupView};

use crate::labels::menu_label;
use crate::settings::setup::footer::Footer;
use crate::settings::setup::motion::{Motion, animate, step_transition};
use crate::settings::setup::step::StepView;
use crate::settings::{ActionTarget, Binder, observe_text_changes};

const WINDOW_SIZE: NSSize = NSSize::new(620.0, 580.0);

/// The actions of the setup window that no control sends.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SetupActions {
    /// Sent with the notification when the user closes the window, see
    /// [`SetupWindow::on_closed`].
    pub(crate) close: Sel,
    /// Sent with its timer at each frame of the demo, see [`SetupWindow::advance_demo`].
    pub(crate) demo_frame: Sel,
}

/// The setup window, kept alive and reused after it closes.
#[derive(Debug)]
pub(crate) struct SetupWindow {
    window: Retained<NSWindow>,
    step: StepView,
    footer: Footer,
    control_ids: Vec<ControlId>,
    target: ActionTarget,
    demo_frame_action: Sel,
    /// The step that the window shows, or `None` until the first render after the window opens.
    shown_step: Option<SetupStep>,
    main_thread: MainThreadMarker,
}

impl SetupWindow {
    /// Builds the window, hidden until [`Self::show`].
    ///
    /// Its controls send the actions of `target` as the controls of the settings window do. When
    /// the user closes the window, the window sends `actions.close`.
    pub(crate) fn new(target: ActionTarget, actions: SetupActions, main_thread: MainThreadMarker) -> Self {
        let step = StepView::new(main_thread);
        let footer = Footer::new(&target, main_thread);
        let window = setup_window(&step.root, &footer.root, main_thread);
        observe_text_changes(&target);
        // SAFETY: AppKit defines the notification name before any code runs. The target implements
        // the close action, taking the notification, and the app keeps the target alive while it
        // runs.
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &target.target,
                actions.close,
                Some(NSWindowWillCloseNotification),
                Some(&window),
            );
        }
        Self {
            window,
            step,
            footer,
            control_ids: Vec::new(),
            target,
            demo_frame_action: actions.demo_frame,
            shown_step: None,
            main_thread,
        }
    }

    /// Brings the window and the app to the front.
    pub(crate) fn show(&self) {
        NSApplication::sharedApplication(self.main_thread).activate();
        self.window.makeKeyAndOrderFront(None);
    }

    /// Shows `view`. When the step changes, the new step fades and slides in, from the top of its
    /// content, and the demo of the step plays.
    pub(crate) fn render(&mut self, view: &SetupView) {
        let motion = Motion::of_system();
        let is_new_step = self.shown_step != Some(view.step);
        if let Some(transition) = step_transition(self.shown_step, view.step, motion) {
            animate(self.step.animated_view(), transition);
        }
        let title = menu_label(&view.window_title, view.language);
        self.window.setTitle(&NSString::from_str(&title));
        self.control_ids.clear();
        let mut binder = Binder {
            target: &self.target,
            ids: &mut self.control_ids,
            language: view.language,
            main_thread: self.main_thread,
        };
        self.step.render(view, &mut binder);
        self.footer.render(view, &mut binder);
        if is_new_step {
            self.step.scroll_to_top();
            let target = &self.target.target;
            let action = self.demo_frame_action;
            self.step
                .demo
                .play(view.demo.as_ref(), motion, || demo_timer(target, action));
        }
        self.shown_step = Some(view.step);
    }

    /// Closes the window, without the close action.
    pub(crate) fn close(&mut self) {
        self.window.orderOut(None);
        self.on_closed();
    }

    /// Forgets the step that the window showed, after the window closed, so that the next render
    /// shows its step without a transition and plays its demo again.
    pub(crate) fn on_closed(&mut self) {
        self.step.demo.stop();
        self.shown_step = None;
    }

    /// Shows the next frame of the demo.
    pub(crate) fn advance_demo(&mut self) {
        self.step.demo.advance();
    }

    /// Reads the input of `switch`, a switch of the window.
    pub(crate) fn toggled(&self, switch: &NSSwitch) -> Option<SettingsInput> {
        Some(SettingsInput::Toggled {
            id: self.control_id(switch)?,
            is_on: switch.state() == NSControlStateValueOn,
        })
    }

    /// Reads the input of `menu`, a menu of the window.
    pub(crate) fn picked(&self, menu: &NSPopUpButton) -> Option<SettingsInput> {
        Some(SettingsInput::Picked {
            id: self.control_id(menu)?,
            index: usize::try_from(menu.indexOfSelectedItem()).ok()?,
        })
    }

    /// Reads the input of `button`, a button of the window.
    pub(crate) fn pressed(&self, button: &NSButton) -> Option<SettingsInput> {
        self.control_id(button).map(SettingsInput::Pressed)
    }

    /// Reads the input of `control`: a text field of the window, or the button that submits its
    /// text.
    pub(crate) fn submitted(&self, control: &NSControl) -> Option<SettingsInput> {
        let id = self.control_id(control)?;
        let text = self.step.field_text(&id)?;
        Some(SettingsInput::Submitted { id, text })
    }

    /// Reads the input of a change of the text in a text view or a text field, when it is a text
    /// area or a line of text of the window.
    pub(crate) fn edited(&self, notification: &NSNotification) -> Option<SettingsInput> {
        let object = notification.object()?;
        self.step.edited(&object)
    }

    /// The id that `control` carries in its tag since the last render.
    fn control_id(&self, control: &NSControl) -> Option<ControlId> {
        let index = usize::try_from(control.tag()).ok()?;
        self.control_ids.get(index).cloned()
    }
}

/// A repeating timer that sends `action` to `target` at each frame of the demo.
fn demo_timer(target: &NSObject, action: Sel) -> Retained<NSTimer> {
    // SAFETY: the target implements the action, taking the timer, and the app keeps the target
    // alive while it runs.
    unsafe {
        NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            demo::FRAME_INTERVAL.as_secs_f64(),
            target,
            action,
            None,
            true,
        )
    }
}

/// The window with `content` above the `footer`, centered on the screen.
fn setup_window(content: &NSView, footer: &NSView, main_thread: MainThreadMarker) -> Retained<NSWindow> {
    // SAFETY: the window is released on close by default; the next statement turns that off
    // before the window can close.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(main_thread),
            NSRect::new(NSPoint::ZERO, WINDOW_SIZE),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::FullSizeContentView,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is owned by `SetupWindow`, so AppKit must not release it on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitlebarAppearsTransparent(true);
    window.setTitleVisibility(NSWindowTitleVisibility::Hidden);
    window.setTabbingMode(NSWindowTabbingMode::Disallowed);
    for button in [NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
        if let Some(button) = window.standardWindowButton(button) {
            button.setHidden(true);
        }
    }
    window.setAutorecalculatesKeyViewLoop(true);
    window.setContentView(Some(&root_view(content, footer, main_thread)));
    window.setContentSize(WINDOW_SIZE);
    window.center();
    window
}

/// `content` from below the title bar down to the `footer`, which stays at the bottom.
fn root_view(content: &NSView, footer: &NSView, main_thread: MainThreadMarker) -> Retained<NSView> {
    let root = NSView::new(main_thread);
    for part in [content, footer] {
        root.addSubview(part);
        part.setTranslatesAutoresizingMaskIntoConstraints(false);
    }
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        content
            .topAnchor()
            .constraintEqualToAnchor(&root.safeAreaLayoutGuide().topAnchor()),
        content
            .leadingAnchor()
            .constraintEqualToAnchor(&root.leadingAnchor()),
        content
            .trailingAnchor()
            .constraintEqualToAnchor(&root.trailingAnchor()),
        content
            .bottomAnchor()
            .constraintEqualToAnchor(&footer.topAnchor()),
        footer
            .leadingAnchor()
            .constraintEqualToAnchor(&root.leadingAnchor()),
        footer
            .trailingAnchor()
            .constraintEqualToAnchor(&root.trailingAnchor()),
        footer
            .bottomAnchor()
            .constraintEqualToAnchor(&root.bottomAnchor()),
    ]));
    root
}

/// A box without a title and a border, with `content` inside it at `insets` from its edges. The
/// caller gives it its look.
fn framed_box(content: &NSView, insets: NSEdgeInsets, main_thread: MainThreadMarker) -> Retained<NSBox> {
    let frame = NSBox::new(main_thread);
    frame.setBoxType(NSBoxType::Custom);
    frame.setTitlePosition(NSTitlePosition::NoTitle);
    frame.setBorderWidth(0.0);
    frame.setContentViewMargins(NSSize::ZERO);
    frame.setContentView(Some(content));
    content.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        content
            .leadingAnchor()
            .constraintEqualToAnchor_constant(&frame.leadingAnchor(), insets.left),
        content
            .trailingAnchor()
            .constraintEqualToAnchor_constant(&frame.trailingAnchor(), -insets.right),
        content
            .topAnchor()
            .constraintEqualToAnchor_constant(&frame.topAnchor(), insets.top),
        content
            .bottomAnchor()
            .constraintEqualToAnchor_constant(&frame.bottomAnchor(), -insets.bottom),
    ]));
    frame
}
