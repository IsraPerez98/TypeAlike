//! The settings window: a sidebar with a row for each page of the core settings view, and the
//! panes that render the pages and report user actions back. The setup window shows its rows with
//! the same views.
//!
//! Layout follows System Settings: a source list of the pages at the leading edge, and grouped
//! lists in rounded boxes with hairline separators between rows, under a pinned header with the
//! search field on a page that has one.

mod controls;
mod layout;
mod page;
mod rows;
mod setup;
mod sidebar;

use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSButton, NSControl, NSControlStateValueOn,
    NSControlTextDidChangeNotification, NSObjectNSKeyValueBindingCreation as _, NSPopUpButton,
    NSSplitViewController, NSSplitViewItem, NSSwitch, NSTabViewController, NSTabViewControllerTabStyle,
    NSTabViewItem, NSTextDidChangeNotification, NSTitleBinding, NSToolbar,
    NSToolbarSidebarTrackingSeparatorItemIdentifier, NSViewController, NSViewControllerTransitionOptions,
    NSWindow, NSWindowStyleMask, NSWindowToolbarStyle,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize, NSString};
use typealike_core::localization::UiLanguage;
use typealike_core::settings_window::view::{self as view, ControlId, SettingsInput, SettingsView};

pub(crate) use crate::settings::controls::control_state;
use crate::settings::layout::below_toolbar;
use crate::settings::page::PageView;
pub(crate) use crate::settings::setup::{SetupActions, SetupWindow};
use crate::settings::sidebar::Sidebar;

const WINDOW_SIZE: NSSize = NSSize::new(820.0, 680.0);
const SIDEBAR_WIDTH: CGFloat = 220.0;

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

/// An action that a control of a settings page sends to the action target.
#[derive(Debug, Clone, Copy)]
enum ControlAction {
    Toggle,
    Pick,
    Submit,
}

impl ActionTarget {
    /// Makes `control` send `action` to the target.
    fn connect(&self, control: &NSControl, action: ControlAction) {
        let selector = match action {
            ControlAction::Toggle => self.toggle_action,
            ControlAction::Pick => self.pick_action,
            ControlAction::Submit => self.submit_action,
        };
        // SAFETY: the target implements each of these actions, taking the control, and
        // `SettingsWindow` keeps the target alive as long as its controls.
        unsafe {
            control.setTarget(Some(&self.target));
            control.setAction(Some(selector));
        }
    }
}

/// Creates the controls of the settings pages, which report to the action target, and tags each
/// control with the index of its id.
struct Binder<'a> {
    target: &'a ActionTarget,
    ids: &'a mut Vec<ControlId>,
    /// The language of the texts, which gives the case of the labels of menus and buttons.
    language: UiLanguage,
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
    sidebar: Sidebar,
    panes: Vec<Pane>,
    control_ids: Vec<ControlId>,
    target: ActionTarget,
    main_thread: MainThreadMarker,
}

impl SettingsWindow {
    /// Builds the window with a pane and a sidebar row for each page of `view`, hidden until
    /// [`Self::show`].
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
        let icons: Vec<_> = view.pages.iter().map(|page| page.icon).collect();
        let sidebar = Sidebar::new(&icons, tabs.clone(), main_thread);
        let split = split_view(&sidebar, &tabs, main_thread);
        let window = settings_window(&split, &tabs, main_thread);
        observe_text_changes(&target);
        Self {
            window,
            tabs,
            sidebar,
            panes,
            control_ids: Vec::new(),
            target,
            main_thread,
        }
    }

    /// Brings the window and the app to the front. With a `page`, it shows the page of that index.
    pub(crate) fn show(&self, page: Option<usize>) {
        if let Some(index) = page {
            self.sidebar.select(index);
        }
        NSApplication::sharedApplication(self.main_thread).activate();
        self.window.makeKeyAndOrderFront(None);
    }

    /// Shows `view` in the panes, each page in the pane made for it.
    pub(crate) fn render(&mut self, view: &SettingsView) {
        self.control_ids.clear();
        let mut binder = Binder {
            target: &self.target,
            ids: &mut self.control_ids,
            language: view.language,
            main_thread: self.main_thread,
        };
        for (pane, page) in self.panes.iter_mut().zip(&view.pages) {
            pane.set_title(&page.title);
            pane.page.render(page, &mut binder);
        }
        self.sidebar
            .render(view.pages.iter().map(|page| page.title.as_str()));
        show_selected_pane_title(&self.tabs);
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
}

/// Makes each change of the text in a text view or a text field send the edit action of `target`
/// with the notification.
fn observe_text_changes(target: &ActionTarget) {
    // SAFETY: AppKit defines the notification names before any code runs. The target implements
    // the edit action, taking the notification, and the app keeps the target alive while it runs.
    unsafe {
        let center = NSNotificationCenter::defaultCenter();
        for name in [NSTextDidChangeNotification, NSControlTextDidChangeNotification] {
            center.addObserver_selector_name_object(&target.target, target.edit_action, Some(name), None);
        }
    }
}

/// One pane of the settings window: its view controller, its tab and its page.
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
        controller.setView(&below_toolbar(&page.root, main_thread));
        let tab = NSTabViewItem::tabViewItemWithViewController(&controller);
        Self {
            controller,
            tab,
            page,
        }
    }

    /// Names the pane in its tab, and in the window title while the pane is selected.
    fn set_title(&self, title: &str) {
        let title = NSString::from_str(title);
        self.controller.setTitle(Some(&title));
        self.tab.setLabel(&title);
    }
}

/// Names the window after the selected pane of `tabs`, through the title of `tabs`.
fn show_selected_pane_title(tabs: &NSTabViewController) {
    if let Some(selected) = tabs.tabView().selectedTabViewItem() {
        tabs.setTitle(Some(&selected.label()));
    }
}

/// The panes without tabs: the sidebar picks the pane that shows.
fn pane_tabs(panes: &[Pane], main_thread: MainThreadMarker) -> Retained<NSTabViewController> {
    let tabs = NSTabViewController::new(main_thread);
    tabs.setTabStyle(NSTabViewControllerTabStyle::Unspecified);
    tabs.setTransitionOptions(NSViewControllerTransitionOptions::None);
    for pane in panes {
        tabs.addTabViewItem(&pane.tab);
    }
    tabs
}

/// The sidebar at a fixed width, then the panes.
fn split_view(
    sidebar: &Sidebar,
    tabs: &NSTabViewController,
    main_thread: MainThreadMarker,
) -> Retained<NSSplitViewController> {
    let sidebar_controller = NSViewController::new(main_thread);
    sidebar_controller.setView(&sidebar.view);
    let sidebar_item = NSSplitViewItem::sidebarWithViewController(&sidebar_controller);
    sidebar_item.setCanCollapse(false);
    sidebar_item.setMinimumThickness(SIDEBAR_WIDTH);
    sidebar_item.setMaximumThickness(SIDEBAR_WIDTH);
    let panes_item = NSSplitViewItem::splitViewItemWithViewController(tabs);
    let split = NSSplitViewController::new(main_thread);
    split.addSplitViewItem(&sidebar_item);
    split.addSplitViewItem(&panes_item);
    split
}

fn settings_window(
    split: &NSSplitViewController,
    tabs: &NSTabViewController,
    main_thread: MainThreadMarker,
) -> Retained<NSWindow> {
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
    // SAFETY: the window is owned by `SettingsWindow`, so AppKit must not release it on close.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setToolbar(Some(&title_toolbar(main_thread)));
    window.setToolbarStyle(NSWindowToolbarStyle::Unified);
    window.setAutorecalculatesKeyViewLoop(true);
    // The window binds its title to the title of its content view controller, and the binding
    // below to the title of the panes. A window title must not be nil.
    split.setTitle(Some(&NSString::new()));
    tabs.setTitle(Some(&NSString::new()));
    window.setContentViewController(Some(split));
    // SAFETY: `title` of the tab controller is a string the window can show, and the window keeps
    // the tab controller alive as its content view controller.
    unsafe {
        window.bind_toObject_withKeyPath_options(NSTitleBinding, tabs, &NSString::from_str("title"), None);
    }
    window.setContentSize(WINDOW_SIZE);
    window.center();
    window
}

/// A toolbar that only puts the window title after the sidebar, over the panes.
fn title_toolbar(main_thread: MainThreadMarker) -> Retained<NSToolbar> {
    let toolbar = NSToolbar::new(main_thread);
    // SAFETY: AppKit defines the identifier before any code runs, and the toolbar makes the
    // standard item itself.
    unsafe {
        toolbar.insertItemWithItemIdentifier_atIndex(NSToolbarSidebarTrackingSeparatorItemIdentifier, 0);
    }
    toolbar
}
