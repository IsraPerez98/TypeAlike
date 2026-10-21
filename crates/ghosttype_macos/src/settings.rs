//! The settings window: a toolbar of panes, one for each page of the core settings view, that
//! render the pages and report user actions back.
//!
//! Layout follows System Settings: grouped lists in rounded boxes with hairline separators
//! between rows, under a pinned header with the search field on a page that has one.

mod controls;
mod layout;
mod page;
mod rows;

use ghosttype_core::settings_window::view::{self as view, ControlId, PageIcon, SettingsInput, SettingsView};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSButton, NSControl, NSControlStateValueOn,
    NSControlTextDidChangeNotification, NSObjectNSKeyValueBindingCreation as _, NSPopUpButton, NSSwitch,
    NSTabViewController, NSTabViewControllerTabStyle, NSTabViewItem, NSTextDidChangeNotification,
    NSTitleBinding, NSViewController, NSWindow, NSWindowStyleMask, NSWindowToolbarStyle,
};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize, NSString};

use crate::settings::controls::symbol;
use crate::settings::page::PageView;

const WINDOW_SIZE: NSSize = NSSize::new(680.0, 680.0);
const GEAR_SYMBOL: &str = "gearshape";
const CUBE_SYMBOL: &str = "cube";
const TEXT_CURSOR_SYMBOL: &str = "text.cursor";
const TEXT_VIEWFINDER_SYMBOL: &str = "text.viewfinder";
const PERSON_TEXT_SYMBOL: &str = "person.text.rectangle";

/// The object and selectors every settings control reports to.
#[derive(Debug)]
pub(crate) struct ActionTarget {
    pub(crate) target: Retained<NSObject>,
    pub(crate) toggle_action: Sel,
    pub(crate) pick_action: Sel,
    pub(crate) press_action: Sel,
    pub(crate) submit_action: Sel,
    pub(crate) edit_action: Sel,
}

/// Creates the controls of the settings pages, which report to the action target, and tags each
/// control with the index of its id.
struct Binder<'a> {
    target: &'a ActionTarget,
    ids: &'a mut Vec<ControlId>,
    main_thread: MainThreadMarker,
}

impl Binder<'_> {
    /// Tags `control` with `id`, so that its action reads `id` back.
    fn bind(&mut self, control: &NSControl, id: &ControlId) {
        let tag = isize::try_from(self.ids.len()).expect("a window never holds isize::MAX controls");
        self.ids.push(id.clone());
        control.setTag(tag);
    }
}

/// The settings window, kept alive and reused after the user closes it. It keeps the pages of
/// the view that it was made with.
#[derive(Debug)]
pub(crate) struct SettingsWindow {
    window: Retained<NSWindow>,
    tabs: Retained<NSTabViewController>,
    panes: Vec<Pane>,
    control_ids: Vec<ControlId>,
    target: ActionTarget,
    main_thread: MainThreadMarker,
}

impl SettingsWindow {
    /// Builds the window with a pane for each page of `view`, hidden until [`Self::show`].
    ///
    /// A switch sends `target.toggle_action`, see [`Self::toggled`], a menu sends
    /// `target.pick_action`, see [`Self::picked`], and a button sends `target.press_action`, see
    /// [`Self::pressed`]. A text field, its button and a search field send `target.submit_action`,
    /// see [`Self::submitted`]. Each change of the text of a text view or a text field sends
    /// `target.edit_action` with the notification, see [`Self::edited`].
    pub(crate) fn new(target: ActionTarget, view: &SettingsView, main_thread: MainThreadMarker) -> Self {
        let panes: Vec<Pane> = view
            .pages
            .iter()
            .map(|page| Pane::new(page, &target, main_thread))
            .collect();
        let tabs = pane_tabs(&panes, main_thread);
        let window = settings_window(&tabs, main_thread);
        // SAFETY: AppKit defines the notification names before any code runs. The target implements
        // the edit action, taking the notification, and the app keeps the target alive while it runs.
        unsafe {
            let center = NSNotificationCenter::defaultCenter();
            for name in [NSTextDidChangeNotification, NSControlTextDidChangeNotification] {
                center.addObserver_selector_name_object(&target.target, target.edit_action, Some(name), None);
            }
        }
        Self {
            window,
            tabs,
            panes,
            control_ids: Vec::new(),
            target,
            main_thread,
        }
    }

    /// Brings the window and the app to the front.
    pub(crate) fn show(&self) {
        NSApplication::sharedApplication(self.main_thread).activate();
        self.window.makeKeyAndOrderFront(None);
    }

    /// Shows `view` in the panes, each page in the pane made for it.
    pub(crate) fn render(&mut self, view: &SettingsView) {
        self.control_ids.clear();
        let mut binder = Binder {
            target: &self.target,
            ids: &mut self.control_ids,
            main_thread: self.main_thread,
        };
        for (pane, page) in self.panes.iter_mut().zip(&view.pages) {
            pane.set_title(page.title);
            pane.page.render(page, &mut binder);
        }
        self.show_selected_pane_title();
    }

    /// Reads the input of `switch`, a switch of a page.
    pub(crate) fn toggled(&self, switch: &NSSwitch) -> Option<SettingsInput> {
        Some(SettingsInput::Toggled {
            id: self.control_id(switch)?,
            is_on: switch.state() == NSControlStateValueOn,
        })
    }

    /// Reads the input of `menu`, a menu of a page.
    pub(crate) fn picked(&self, menu: &NSPopUpButton) -> Option<SettingsInput> {
        Some(SettingsInput::Picked {
            id: self.control_id(menu)?,
            index: usize::try_from(menu.indexOfSelectedItem()).ok()?,
        })
    }

    /// Reads the input of `button`, a button of a page.
    pub(crate) fn pressed(&self, button: &NSButton) -> Option<SettingsInput> {
        self.control_id(button).map(SettingsInput::Pressed)
    }

    /// Reads the input of `control`: a text field or a search field of a page, or the button that
    /// submits the text of a text field.
    pub(crate) fn submitted(&self, control: &NSControl) -> Option<SettingsInput> {
        let id = self.control_id(control)?;
        let text = self.panes.iter().find_map(|pane| pane.page.field_text(&id))?;
        Some(SettingsInput::Submitted { id, text })
    }

    /// Reads the input of a change of the text in a text view or a text field, when it is a text
    /// area or a line of text of a page.
    pub(crate) fn edited(&self, notification: &NSNotification) -> Option<SettingsInput> {
        let object = notification.object()?;
        self.panes.iter().find_map(|pane| pane.page.edited(&object))
    }

    /// The id that `control` carries in its tag since the last render.
    fn control_id(&self, control: &NSControl) -> Option<ControlId> {
        let index = usize::try_from(control.tag()).ok()?;
        self.control_ids.get(index).cloned()
    }

    /// Names the window after the selected pane, which the tab controller does only on a selection change.
    fn show_selected_pane_title(&self) {
        if let Some(selected) = self.tabs.tabView().selectedTabViewItem() {
            self.tabs.setTitle(Some(&selected.label()));
        }
    }
}

/// One pane of the settings toolbar: its view controller, its toolbar item and its page.
#[derive(Debug)]
struct Pane {
    controller: Retained<NSViewController>,
    tab: Retained<NSTabViewItem>,
    page: PageView,
}

impl Pane {
    fn new(shown: &view::Page, target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let page = PageView::new(shown, target, main_thread);
        let controller = NSViewController::new(main_thread);
        controller.setView(&page.root);
        controller.setPreferredContentSize(WINDOW_SIZE);
        let tab = NSTabViewItem::tabViewItemWithViewController(&controller);
        tab.setImage(symbol(page_symbol(shown.icon), "").as_deref());
        Self {
            controller,
            tab,
            page,
        }
    }

    /// Names the pane in its toolbar item, and in the window title while the pane is selected.
    fn set_title(&self, title: &str) {
        let title = NSString::from_str(title);
        self.controller.setTitle(Some(&title));
        self.tab.setLabel(&title);
    }
}

/// The SF Symbol that shows `icon`.
fn page_symbol(icon: PageIcon) -> &'static str {
    match icon {
        PageIcon::Gear => GEAR_SYMBOL,
        PageIcon::Cube => CUBE_SYMBOL,
        PageIcon::TextCursor => TEXT_CURSOR_SYMBOL,
        PageIcon::TextViewfinder => TEXT_VIEWFINDER_SYMBOL,
        PageIcon::PersonText => PERSON_TEXT_SYMBOL,
    }
}

fn pane_tabs(panes: &[Pane], main_thread: MainThreadMarker) -> Retained<NSTabViewController> {
    let tabs = NSTabViewController::new(main_thread);
    tabs.setTabStyle(NSTabViewControllerTabStyle::Toolbar);
    for pane in panes {
        tabs.addTabViewItem(&pane.tab);
    }
    tabs
}

fn settings_window(tabs: &NSTabViewController, main_thread: MainThreadMarker) -> Retained<NSWindow> {
    // SAFETY: the window is released on close by default; the next statement turns that off
    // before the window can close.
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(main_thread),
            NSRect::new(NSPoint::ZERO, WINDOW_SIZE),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    // SAFETY: the window is owned by `SettingsWindow`, so AppKit must not release it on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setToolbarStyle(NSWindowToolbarStyle::Preference);
    window.setAutorecalculatesKeyViewLoop(true);
    window.setContentViewController(Some(tabs));
    // The binding below copies the title to the window, and a window title must not be nil.
    tabs.setTitle(Some(&NSString::new()));
    // SAFETY: `title` of the tab controller is a string the window can show, and the window keeps
    // the tab controller alive as its content view controller.
    unsafe {
        window.bind_toObject_withKeyPath_options(NSTitleBinding, tabs, &NSString::from_str("title"), None);
    }
    window.setContentSize(WINDOW_SIZE);
    window.center();
    window
}
