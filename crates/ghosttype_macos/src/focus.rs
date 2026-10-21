//! What has keyboard focus: whether GhostType may suggest there, where its caret is, and the text
//! around the caret.

mod caret;
mod surroundings;

use ghosttype_core::controller::FocusChange;
use ghosttype_core::desktop::{Field, Surroundings, TextColor};
use objc2_app_kit::{NSRunningApplication, NSWorkspace};
use objc2_application_services::{AXError, AXUIElement};
use objc2_core_foundation::{CFBoolean, CFRetained, CFString, CGRect};

use crate::accessibility::{
    ACCESSIBILITY_QUERY_TIMEOUT_SECONDS, FOCUSED_ELEMENT_ATTRIBUTE, copy_attribute, frame, has_dom_class,
    is_secure_text, selected_range,
};
use crate::focus::caret::{XTERM_INPUT_CLASS, caret_bounds, text_color};
use crate::focus::surroundings::{field_labels, field_text, web_domain, window_title};
use crate::geometry::Caret;

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

/// Follows the frontmost app and classifies whether GhostType may observe and type into it.
#[derive(Debug)]
pub(crate) struct FocusTracker {
    system_wide: CFRetained<AXUIElement>,
    frontmost_process: Option<i32>,
    frontmost_name: Option<String>,
    frontmost_app_id: Option<String>,
    is_frontmost_eligible: bool,
}

impl FocusTracker {
    pub(crate) fn new() -> Self {
        // SAFETY: plain call.
        let system_wide = unsafe { AXUIElement::new_system_wide() };
        // SAFETY: plain call on a valid element.
        let error = unsafe { system_wide.set_messaging_timeout(ACCESSIBILITY_QUERY_TIMEOUT_SECONDS) };
        if error != AXError::Success {
            tracing::warn!(code = error.0, "failed to bound accessibility queries");
        }
        Self {
            system_wide,
            frontmost_process: None,
            frontmost_name: None,
            frontmost_app_id: None,
            is_frontmost_eligible: false,
        }
    }

    /// Observes the frontmost app; returns whether it changed since the last call.
    pub(crate) fn refresh(&mut self) -> FocusChange {
        let frontmost = NSWorkspace::sharedWorkspace().frontmostApplication();
        let frontmost_process = frontmost
            .as_ref()
            .map(|application| application.processIdentifier());
        if frontmost_process == self.frontmost_process {
            return FocusChange::Unchanged;
        }
        self.frontmost_process = frontmost_process;
        self.frontmost_name = frontmost
            .as_ref()
            .and_then(|application| application.localizedName())
            .map(|name| name.to_string());
        self.frontmost_app_id = frontmost
            .as_ref()
            .and_then(|application| application.bundleIdentifier())
            .map(|identifier| identifier.to_string());
        self.is_frontmost_eligible = frontmost_process.is_some_and(|process| !is_own_process(process));
        if let Some(application) = frontmost.filter(|_| self.is_frontmost_eligible) {
            wake_web_accessibility(&application);
        }
        FocusChange::Changed
    }

    /// The process of the frontmost app when [`refresh`](Self::refresh) looked last.
    pub(crate) fn frontmost_process(&self) -> Option<i32> {
        self.frontmost_process
    }

    /// Locates the caret of the focused control, when it exposes one through Accessibility.
    pub(crate) fn caret(&self) -> Option<Caret> {
        caret_bounds(&*self.focused_element()?).and_then(Caret::from_bounds)
    }

    /// The color of the text before the caret of the focused control, when the control tells it.
    pub(crate) fn text_color(&self) -> Option<TextColor> {
        text_color(&*self.focused_element()?)
    }

    /// The frame of the focused control when it holds text, as a field does, even when it gives
    /// no caret.
    pub(crate) fn field_frame(&self) -> Option<CGRect> {
        let element = self.focused_element()?;
        selected_range(&element)?;
        frame(&element)
    }

    fn is_secure_text_focused(&self) -> bool {
        self.focused_element()
            .is_some_and(|element| is_secure_text(&element))
    }

    fn focused_element(&self) -> Option<CFRetained<AXUIElement>> {
        copy_attribute(&self.system_wide, FOCUSED_ELEMENT_ATTRIBUTE)?
            .downcast::<AXUIElement>()
            .ok()
    }

    /// Classifies the focused control right now: excluded for secure text and ineligible apps.
    pub(crate) fn field(&self) -> Field {
        if self.is_frontmost_eligible && !self.is_secure_text_focused() {
            Field::Open
        } else {
            Field::Excluded
        }
    }

    pub(crate) fn surroundings(&self) -> Surroundings {
        let element = self.focused_element();
        Surroundings {
            app_name: self.frontmost_name.clone(),
            app_id: self.frontmost_app_id.clone(),
            app_path: None,
            window_title: element.as_deref().and_then(window_title),
            field_labels: element.as_deref().map(field_labels).unwrap_or_default(),
            domain: element.as_deref().and_then(web_domain),
            field_text: element.as_deref().and_then(field_text),
            is_caret_line_hidden: element
                .as_deref()
                .is_some_and(|element| has_dom_class(element, XTERM_INPUT_CLASS)),
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
    // SAFETY: plain call on a valid element.
    let error = unsafe { app.set_messaging_timeout(ACCESSIBILITY_QUERY_TIMEOUT_SECONDS) };
    if error != AXError::Success {
        tracing::warn!(code = error.0, "failed to bound accessibility queries");
    }
    let element = copy_attribute(&app, FOCUSED_ELEMENT_ATTRIBUTE)?
        .downcast::<AXUIElement>()
        .ok()?;
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
    fn this_process_is_own_process() {
        let process = i32::try_from(std::process::id()).unwrap();

        assert!(is_own_process(process));
    }
}
