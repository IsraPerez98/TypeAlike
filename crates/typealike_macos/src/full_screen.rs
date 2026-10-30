//! Tells whether the app in front shows in full screen: the presentation options that the active
//! app puts into effect hide the menu bar. macOS sets them for a window in a full-screen space, and
//! a game that fills the display hides the menu bar itself.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationPresentationOptions};
use typealike_core::desktop::FullScreenActivity;

/// Reads whether the app in front shows in full screen.
pub(crate) fn read(main_thread: MainThreadMarker) -> FullScreenActivity {
    let options = NSApplication::sharedApplication(main_thread).currentSystemPresentationOptions();
    activity_of(options)
}

fn activity_of(options: NSApplicationPresentationOptions) -> FullScreenActivity {
    let hides_menu_bar =
        NSApplicationPresentationOptions::FullScreen | NSApplicationPresentationOptions::HideMenuBar;
    if options.intersects(hides_menu_bar) {
        FullScreenActivity::InFront
    } else {
        FullScreenActivity::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_screen_space_and_hidden_menu_bar_are_full_screen() {
        let options = [
            NSApplicationPresentationOptions::FullScreen
                | NSApplicationPresentationOptions::AutoHideMenuBar
                | NSApplicationPresentationOptions::AutoHideDock,
            NSApplicationPresentationOptions::HideMenuBar | NSApplicationPresentationOptions::HideDock,
        ];

        let activities = options.map(activity_of);

        assert_eq!(
            activities,
            [FullScreenActivity::InFront, FullScreenActivity::InFront]
        );
    }

    #[test]
    fn menu_bar_that_shows_or_hides_itself_is_not_full_screen() {
        let options = [
            NSApplicationPresentationOptions::Default,
            NSApplicationPresentationOptions::AutoHideMenuBar,
        ];

        let activities = options.map(activity_of);

        assert_eq!(activities, [FullScreenActivity::None, FullScreenActivity::None]);
    }
}
