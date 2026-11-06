//! What has keyboard focus: whether TypeAlike may suggest there, where its caret is, and the text
//! around the caret.

mod caret;
mod surroundings;

use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_application_services::AXUIElement;
use objc2_core_foundation::{CFBoolean, CFRetained, CFString, CGRect};
use typealike_core::controller::FocusChange;
use typealike_core::desktop::{Field, Surroundings};

use crate::accessibility::{
    FOCUSED_ELEMENT_ATTRIBUTE, bound_queries, can_focused_element_hide_text, copy_element, frame,
    is_search_field, is_secure_text, selected_range,
};
pub(crate) use crate::focus::caret::{FontFace, TextStyle};
use crate::focus::caret::{caret_bounds, is_terminal_input, text_style};
use crate::focus::surroundings::{field_labels, field_text, has_unreadable_text, web_domain, window_title};
use crate::geometry::{Caret, is_finite_frame};

const MANUAL_ACCESSIBILITY_ATTRIBUTE: &str = "AXManualAccessibility";
const ENHANCED_USER_INTERFACE_ATTRIBUTE: &str = "AXEnhancedUserInterface";
const CHROMIUM_BROWSER_BUNDLE_PREFIXES: [&str; 7] = [
    "com.google.chrome",
    "org.chromium.chromium",
    "com.brave.browser",
    "com.microsoft.edgemac",
    "company.thebrowser.",
    "com.vivaldi.vivaldi",
    "com.operasoftware.opera",
];
const GHOSTTY_BUNDLE_PREFIX: &str = "com.mitchellh.ghostty";

/// Follows the frontmost app and classifies whether TypeAlike may observe and type into it.
#[derive(Debug)]
pub(crate) struct FocusTracker {
    system_wide: CFRetained<AXUIElement>,
    frontmost: Option<FrontmostApp>,
}

/// The frontmost app when [`FocusTracker::refresh`] looked last.
#[derive(Debug)]
struct FrontmostApp {
    process: i32,
    name: Option<String>,
    app_id: Option<String>,
    /// Whether TypeAlike may observe and type into the app, which is not TypeAlike itself.
    is_eligible: bool,
}

impl FocusTracker {
    pub(crate) fn new() -> Self {
        // SAFETY: plain call.
        let system_wide = unsafe { AXUIElement::new_system_wide() };
        bound_queries(&system_wide);
        Self {
            system_wide,
            frontmost: None,
        }
    }

    /// Observes the frontmost app; returns whether it changed since the last call.
    pub(crate) fn refresh(&mut self) -> FocusChange {
        let application = NSWorkspace::sharedWorkspace().frontmostApplication();
        let process = application
            .as_ref()
            .map(|application| application.processIdentifier());
        if process == self.frontmost_process() {
            return FocusChange::Unchanged;
        }
        self.frontmost = application
            .as_ref()
            .zip(process)
            .map(|(application, process)| FrontmostApp {
                process,
                name: application.localizedName().map(|name| name.to_string()),
                app_id: application
                    .bundleIdentifier()
                    .map(|identifier| identifier.to_string()),
                is_eligible: !is_own_process(process),
            });
        if let Some(application) = application.filter(|_| self.is_frontmost_eligible()) {
            wake_web_accessibility(&application);
        }
        FocusChange::Changed
    }

    /// The process of the frontmost app when [`refresh`](Self::refresh) looked last.
    pub(crate) fn frontmost_process(&self) -> Option<i32> {
        self.frontmost.as_ref().map(|app| app.process)
    }

    fn is_frontmost_eligible(&self) -> bool {
        self.frontmost.as_ref().is_some_and(|app| app.is_eligible)
    }

    /// Locates the caret of the focused control, when it exposes one through Accessibility.
    pub(crate) fn caret(&self) -> Option<Caret> {
        caret_bounds(&*self.focused_element()?).and_then(Caret::from_bounds)
    }

    /// The color and the font face of the text before the caret of the focused control, as far as
    /// the control tells them.
    pub(crate) fn text_style(&self) -> TextStyle {
        self.focused_element()
            .map(|element| text_style(&element))
            .unwrap_or_default()
    }

    /// The frame of the focused control when it holds text, as a field does, even when it gives
    /// no caret.
    pub(crate) fn field_frame(&self) -> Option<CGRect> {
        let element = self.focused_element()?;
        selected_range(&element)?;
        frame(&element).filter(is_finite_frame)
    }

    fn focused_element(&self) -> Option<CFRetained<AXUIElement>> {
        copy_element(&self.system_wide, FOCUSED_ELEMENT_ATTRIBUTE)
    }

    /// Classifies the focused control right now: excluded for ineligible apps and for a control
    /// that can hide its text.
    pub(crate) fn field(&self) -> Field {
        if self.is_frontmost_eligible() && !can_focused_element_hide_text(&self.system_wide) {
            Field::Open
        } else {
            Field::Excluded
        }
    }

    pub(crate) fn surroundings(&self) -> Surroundings {
        let element = self.focused_element();
        let app_id = self.frontmost.as_ref().and_then(|app| app.app_id.clone());
        let hides_caret_in_text = gives_mouse_selection_only(app_id.as_deref())
            && element
                .as_deref()
                .is_none_or(|element| caret_bounds(element).is_none());
        Surroundings {
            app_name: self.frontmost.as_ref().and_then(|app| app.name.clone()),
            app_id,
            app_path: None,
            window_title: element.as_deref().and_then(window_title),
            field_labels: element.as_deref().map(field_labels).unwrap_or_default(),
            domain: element.as_deref().and_then(web_domain),
            field_text: element
                .as_deref()
                .filter(|_| !hides_caret_in_text)
                .and_then(field_text),
            is_caret_line_hidden: element.as_deref().is_some_and(is_terminal_input),
            is_search_field: element.as_deref().is_some_and(is_search_field),
            has_unreadable_text: hides_caret_in_text || element.as_deref().is_some_and(has_unreadable_text),
        }
    }
}

/// Asks a Chromium or Electron app to build the accessibility tree of its web content. These apps
/// skip that tree until an assistive app sets an attribute on them. Other apps ignore the request.
fn wake_web_accessibility(application: &NSRunningApplication) {
    let bundle_identifier = application
        .bundleIdentifier()
        .map(|identifier| identifier.to_string());
    let attribute = web_accessibility_attribute(bundle_identifier.as_deref());
    // SAFETY: plain call.
    let element = unsafe { AXUIElement::new_application(application.processIdentifier()) };
    // SAFETY: plain call on a valid element, with a CFBoolean value as both attributes expect.
    let error =
        unsafe { element.set_attribute_value(&CFString::from_static_str(attribute), CFBoolean::new(true)) };
    tracing::debug!(
        attribute,
        code = error.0,
        "asked the frontmost app for its web content"
    );
}

/// Picks the attribute that wakes the web content tree of the app with `bundle_identifier`.
fn web_accessibility_attribute(bundle_identifier: Option<&str>) -> &'static str {
    let is_chromium_browser = bundle_identifier.is_some_and(|identifier| {
        let identifier = identifier.to_ascii_lowercase();
        CHROMIUM_BROWSER_BUNDLE_PREFIXES
            .iter()
            .any(|prefix| identifier.starts_with(prefix))
    });
    // Window managers misplace the windows of apps with the enhanced user interface, so only these get it.
    if is_chromium_browser {
        ENHANCED_USER_INTERFACE_ATTRIBUTE
    } else {
        MANUAL_ACCESSIBILITY_ATTRIBUTE
    }
}

/// Whether the app with `bundle_identifier` gives the mouse selection of its focused field through
/// Accessibility, never its caret, as the terminal of Ghostty does. Without a caret, the text of
/// such a field does not tell where the caret is, so TypeAlike reads it from the screen.
fn gives_mouse_selection_only(bundle_identifier: Option<&str>) -> bool {
    bundle_identifier
        .is_some_and(|identifier| identifier.to_ascii_lowercase().starts_with(GHOSTTY_BUNDLE_PREFIX))
}

fn is_own_process(process: i32) -> bool {
    u32::try_from(process).is_ok_and(|process| process == std::process::id())
}

/// The focused field of an app, which does not hide its text.
pub(crate) struct FocusedField {
    pub(crate) element: CFRetained<AXUIElement>,
    pub(crate) frame: CGRect,
    pub(crate) caret: Option<CGRect>,
}

/// Returns the focused field of the app of `process`, or `None` for a password field.
pub(crate) fn focused_field(process: i32) -> Option<FocusedField> {
    // SAFETY: plain call.
    let app = unsafe { AXUIElement::new_application(process) };
    bound_queries(&app);
    let element = copy_element(&app, FOCUSED_ELEMENT_ATTRIBUTE)?;
    if is_secure_text(&element) {
        return None;
    }
    Some(FocusedField {
        frame: frame(&element)?,
        caret: caret_bounds(&element),
        element,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_browsers_get_the_enhanced_user_interface_switch() {
        let bundle_identifiers = [
            Some("com.google.Chrome"),
            Some("com.google.Chrome.canary"),
            Some("com.brave.Browser"),
            Some("com.microsoft.edgemac"),
            Some("company.thebrowser.Browser"),
        ];

        let attributes = bundle_identifiers.map(web_accessibility_attribute);

        assert_eq!(attributes, [ENHANCED_USER_INTERFACE_ATTRIBUTE; 5]);
    }

    #[test]
    fn other_apps_get_the_manual_accessibility_switch() {
        let bundle_identifiers = [
            Some("com.microsoft.VSCode"),
            Some("com.tinyspeck.slackmacgap"),
            Some("com.apple.TextEdit"),
            None,
        ];

        let attributes = bundle_identifiers.map(web_accessibility_attribute);

        assert_eq!(attributes, [MANUAL_ACCESSIBILITY_ATTRIBUTE; 4]);
    }

    #[test]
    fn ghostty_gives_only_its_mouse_selection() {
        let bundle_identifiers = [Some("com.mitchellh.ghostty"), Some("com.mitchellh.ghostty.debug")];

        let answers = bundle_identifiers.map(gives_mouse_selection_only);

        assert_eq!(answers, [true; 2]);
    }

    #[test]
    fn other_apps_give_their_caret_as_the_selection() {
        let bundle_identifiers = [Some("com.apple.Terminal"), Some("com.microsoft.VSCode"), None];

        let answers = bundle_identifiers.map(gives_mouse_selection_only);

        assert_eq!(answers, [false; 3]);
    }

    #[test]
    fn this_process_is_own_process() {
        let process = i32::try_from(std::process::id()).unwrap();

        assert!(is_own_process(process));
    }
}
