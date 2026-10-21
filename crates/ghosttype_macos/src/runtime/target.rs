//! The Objective-C object that passes the events of AppKit to the runtime.
//!
//! It receives the actions of the controls, the timers and the notifications of the workspace.

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSButton, NSControl, NSMenuItem, NSPopUpButton, NSSwitch, NSWorkspace,
    NSWorkspaceActiveSpaceDidChangeNotification, NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::{NSNotification, NSTimer};

use crate::injection;
use crate::runtime::{Runtime, with_runtime};

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class does not implement Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[name = "GhostTypeRuntimeTarget"]
    pub(super) struct RuntimeTarget;

    impl RuntimeTarget {
        // SAFETY: matches the action signature, called with the picked menu item.
        #[unsafe(method(pickMenuItem:))]
        fn pick_menu_item(&self, sender: &NSMenuItem) {
            let tag = sender.tag();
            with_runtime(|runtime| runtime.on_menu_item(tag));
        }

        // SAFETY: matches the action signature, called with the picked item of the badge menu.
        #[unsafe(method(pickBadgeMenuItem:))]
        fn pick_badge_menu_item(&self, sender: &NSMenuItem) {
            let tag = sender.tag();
            with_runtime(|runtime| runtime.on_badge_menu_item(tag));
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(showBadgeMenu:))]
        fn show_badge_menu(&self, _timer: &NSTimer) {
            // The menu runs an event loop of its own, so it must run outside the borrow of the runtime.
            if let Some((menu, anchor)) = with_runtime(Runtime::build_badge_menu).flatten() {
                menu.popUpMenuPositioningItem_atLocation_inView(None, anchor, None);
            }
        }

        // SAFETY: matches the action signature, called with a switch of a settings page.
        #[unsafe(method(toggleSetting:))]
        fn toggle_setting(&self, sender: &NSSwitch) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.toggled(sender)));
        }

        // SAFETY: matches the action signature, called with a menu of a settings page.
        #[unsafe(method(pickSetting:))]
        fn pick_setting(&self, sender: &NSPopUpButton) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.picked(sender)));
        }

        // SAFETY: matches the action signature, called with a button of a settings page.
        #[unsafe(method(pressSetting:))]
        fn press_setting(&self, sender: &NSButton) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.pressed(sender)));
        }

        // SAFETY: matches the notification observer signature, called for each change of the text
        // of a text view.
        #[unsafe(method(editSetting:))]
        fn edit_setting(&self, notification: &NSNotification) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.edited(notification)));
        }

        // SAFETY: matches the action signature, called with a text field of a settings page or its
        // button.
        #[unsafe(method(submitSetting:))]
        fn submit_setting(&self, sender: &NSControl) {
            with_runtime(|runtime| runtime.on_settings_control(|settings| settings.submitted(sender)));
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(caretDidSettle:))]
        fn caret_did_settle(&self, _timer: &NSTimer) {
            with_runtime(Runtime::on_caret_settled);
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(inputDidSettle:))]
        fn input_did_settle(&self, _timer: &NSTimer) {
            with_runtime(Runtime::on_input_settled);
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(injectPendingText:))]
        fn inject_pending_text(&self, _timer: &NSTimer) {
            let replacement = with_runtime(|runtime| runtime.shell.pending_injection.take()).flatten();
            if let Some(replacement) = replacement
                && let Err(error) = injection::replace_text(&replacement)
            {
                tracing::warn!(%error, "failed to type accepted text");
            }
        }

        // SAFETY: matches the timer action signature.
        #[unsafe(method(quitApplication:))]
        fn quit_application(&self, _timer: &NSTimer) {
            NSApplication::sharedApplication(self.mtm()).terminate(None);
        }

        // SAFETY: matches the notification observer signature.
        #[unsafe(method(activeSpaceDidChange:))]
        fn active_space_did_change(&self, _notification: &NSNotification) {
            with_runtime(Runtime::on_space_change);
        }

        // SAFETY: matches the notification observer signature.
        #[unsafe(method(applicationDidActivate:))]
        fn application_did_activate(&self, _notification: &NSNotification) {
            with_runtime(Runtime::on_app_activation);
        }
    }
);

impl RuntimeTarget {
    pub(super) fn new(main_thread: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(());
        // SAFETY: NSObject's init takes no arguments and returns an initialized object.
        unsafe { msg_send![super(this), init] }
    }
}

pub(super) fn observe_workspace(target: &RuntimeTarget) {
    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    // SAFETY: the target implements each selector, and the center drops deallocated observers itself.
    unsafe {
        center.addObserver_selector_name_object(
            target,
            sel!(activeSpaceDidChange:),
            Some(NSWorkspaceActiveSpaceDidChangeNotification),
            None,
        );
        center.addObserver_selector_name_object(
            target,
            sel!(applicationDidActivate:),
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
        );
    }
}
