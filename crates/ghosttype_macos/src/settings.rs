//! The settings window: a toolbar of panes, one for each page of the core settings view, that
//! render the pages and report user actions back.
//!
//! Layout follows System Settings: grouped lists in rounded boxes with hairline separators
//! between rows, under a pinned header with the search field on a page that has one.

mod page;

use ghosttype_core::settings_window::view::{
    self as view, ControlId, Note, NoteTone, PageIcon, SettingsInput, SettingsView,
};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSBox, NSBoxType, NSButton, NSColor, NSControl, NSControlSize,
    NSControlStateValueOn, NSControlTextDidChangeNotification, NSFont, NSImage, NSImageView,
    NSLayoutAttribute, NSLayoutConstraint, NSLayoutConstraintOrientation, NSLayoutPriorityDefaultLow,
    NSLineBreakMode, NSMenuItem, NSObjectNSKeyValueBindingCreation as _, NSPopUpButton, NSProgressIndicator,
    NSProgressIndicatorStyle, NSResponder, NSScrollView, NSStackView, NSStackViewDistribution, NSSwitch,
    NSTabViewController, NSTabViewControllerTabStyle, NSTabViewItem, NSTextDidChangeNotification,
    NSTextField, NSTitleBinding, NSTitlePosition, NSUserInterfaceLayoutOrientation, NSView, NSViewController,
    NSWindow, NSWindowStyleMask, NSWindowToolbarStyle,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{
    NSArray, NSEdgeInsets, NSNotification, NSNotificationCenter, NSPoint, NSRect, NSSize, NSString,
};

use crate::settings::page::{Binder, PageView};

const WINDOW_SIZE: NSSize = NSSize::new(680.0, 680.0);
const CONTENT_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 20.0,
    left: 24.0,
    bottom: 24.0,
    right: 24.0,
};
const NO_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 0.0,
    left: 0.0,
    bottom: 0.0,
    right: 0.0,
};
const ROW_CONTAINER_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 0.0,
    left: 12.0,
    bottom: 0.0,
    right: 12.0,
};
const SUMMARY_FONT_SIZE: CGFloat = 12.0;
const DETAIL_FONT_SIZE: CGFloat = 12.0;
const SECTION_SPACING: CGFloat = 28.0;
const LIST_PART_SPACING: CGFloat = 8.0;
const MIN_ROW_HEIGHT: CGFloat = 52.0;
const ROW_TEXT_PADDING: CGFloat = 8.0;
const ROW_COLUMN_SPACING: CGFloat = 12.0;
const ROW_TEXT_SPACING: CGFloat = 2.0;
const MESSAGE_SPACING: CGFloat = 8.0;
const GROUP_CORNER_RADIUS: CGFloat = 10.0;
const GROUP_BORDER_WIDTH: CGFloat = 1.0;
const PROGRESS_FULL: f64 = 1000.0;
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

/// Adds a row of `texts`, then `controls`, left to right, at the bottom of the rounded group
/// `rows_stack`, with a separator above it. The row grows past its minimum height to keep a
/// padding around `texts` when they wrap. The first row of a group has no separator above it.
fn add_grouped_row(
    rows_stack: &NSStackView,
    texts: &NSView,
    controls: &[&NSView],
    main_thread: MainThreadMarker,
) {
    let content = NSStackView::stackViewWithViews(&NSArray::from_slice(&[texts]), main_thread);
    for control in controls {
        content.addArrangedSubview(control);
    }
    content.setSpacing(ROW_COLUMN_SPACING);
    content.setDistribution(NSStackViewDistribution::Fill);
    // NSStackView does not keep its edge insets on the cross axis, so the padding is explicit.
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        content
            .heightAnchor()
            .constraintGreaterThanOrEqualToConstant(MIN_ROW_HEIGHT),
        texts
            .topAnchor()
            .constraintGreaterThanOrEqualToAnchor_constant(&content.topAnchor(), ROW_TEXT_PADDING),
        texts
            .bottomAnchor()
            .constraintLessThanOrEqualToAnchor_constant(&content.bottomAnchor(), -ROW_TEXT_PADDING),
    ]));
    let separator = separator(main_thread);
    let container = stack(
        NSUserInterfaceLayoutOrientation::Vertical,
        0.0,
        ROW_CONTAINER_INSETS,
        main_thread,
    );
    container.addArrangedSubview(&separator);
    container.addArrangedSubview(&content);
    rows_stack.addArrangedSubview(&container);
    fill_width(&container, rows_stack);
    fill_width(&separator, &container);
    fill_width(&content, &container);
    separator.setHidden(rows_stack.arrangedSubviews().count() == 1);
}

/// Returns a pop-up button that sends `action` to the target of `target` when the user picks an
/// entry.
fn pop_up_button(
    target: &ActionTarget,
    action: Sel,
    main_thread: MainThreadMarker,
) -> Retained<NSPopUpButton> {
    let button = NSPopUpButton::new(main_thread);
    button.setPullsDown(false);
    // SAFETY: the target implements `action`, taking the pop-up button, and `SettingsWindow`
    // keeps the target alive as long as the button.
    unsafe {
        button.setTarget(Some(&target.target));
        button.setAction(Some(action));
    }
    button
}

/// Replaces the entries of `button` with one entry for each of `titles`.
fn rebuild_menu(button: &NSPopUpButton, titles: &[String]) {
    button.removeAllItems();
    let Some(menu) = button.menu() else {
        return;
    };
    for title in titles {
        // SAFETY: the item has no action, so the pop-up button sends its own action instead.
        let item = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(button.mtm()),
                &NSString::from_str(title),
                None,
                &NSString::new(),
            )
        };
        menu.addItem(&item);
    }
}

/// A spinner and a line of status text.
#[derive(Debug)]
struct NoteView {
    container: Retained<NSStackView>,
    spinner: Retained<NSProgressIndicator>,
    text: Retained<NSTextField>,
}

impl NoteView {
    fn new(main_thread: MainThreadMarker) -> Self {
        let spinner = spinner(main_thread);
        let text = secondary_wrapping_label(main_thread);
        text.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        let container = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            MESSAGE_SPACING,
            NO_INSETS,
            main_thread,
        );
        container.addArrangedSubview(&spinner);
        container.addArrangedSubview(&text);
        Self {
            container,
            spinner,
            text,
        }
    }

    fn render(&self, note: Option<&Note>) {
        stop_spinning(&self.spinner);
        self.container.setHidden(note.is_none());
        let Some(note) = note else {
            return;
        };
        set_text(&self.text, note.text);
        let color = match note.tone {
            NoteTone::Plain | NoteTone::Busy => NSColor::secondaryLabelColor(),
            NoteTone::Error => NSColor::systemRedColor(),
        };
        self.text.setTextColor(Some(&color));
        if note.tone == NoteTone::Busy {
            start_spinning(&self.spinner);
        }
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

fn scroll_view(content: &NSStackView, main_thread: MainThreadMarker) -> Retained<NSScrollView> {
    let document = FlippedView::new(main_thread);
    document.addSubview(content);
    let scroll_view = NSScrollView::new(main_thread);
    scroll_view.setHasVerticalScroller(true);
    scroll_view.setAutohidesScrollers(true);
    scroll_view.setDrawsBackground(false);
    scroll_view.setDocumentView(Some(&document));
    scroll_view.setContentHuggingPriority_forOrientation(1.0, NSLayoutConstraintOrientation::Vertical);
    let clip_view = scroll_view.contentView();
    document.setTranslatesAutoresizingMaskIntoConstraints(false);
    content.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        document
            .leadingAnchor()
            .constraintEqualToAnchor(&clip_view.leadingAnchor()),
        document
            .trailingAnchor()
            .constraintEqualToAnchor(&clip_view.trailingAnchor()),
        document
            .topAnchor()
            .constraintEqualToAnchor(&clip_view.topAnchor()),
        content
            .leadingAnchor()
            .constraintEqualToAnchor(&document.leadingAnchor()),
        content
            .trailingAnchor()
            .constraintEqualToAnchor(&document.trailingAnchor()),
        content.topAnchor().constraintEqualToAnchor(&document.topAnchor()),
        content
            .bottomAnchor()
            .constraintEqualToAnchor(&document.bottomAnchor()),
    ]));
    scroll_view
}

/// Wraps `content` in a rounded box like the grouped lists of System Settings.
fn rounded_group(content: &NSView, main_thread: MainThreadMarker) -> Retained<NSBox> {
    let group = NSBox::new(main_thread);
    group.setBoxType(NSBoxType::Custom);
    group.setTitlePosition(NSTitlePosition::NoTitle);
    group.setContentViewMargins(NSSize::ZERO);
    group.setCornerRadius(GROUP_CORNER_RADIUS);
    group.setBorderWidth(GROUP_BORDER_WIDTH);
    group.setBorderColor(&NSColor::separatorColor());
    group.setFillColor(&NSColor::quaternarySystemFillColor());
    group.setContentView(Some(content));
    content.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        content
            .leadingAnchor()
            .constraintEqualToAnchor(&group.leadingAnchor()),
        content
            .trailingAnchor()
            .constraintEqualToAnchor(&group.trailingAnchor()),
        content.topAnchor().constraintEqualToAnchor(&group.topAnchor()),
        content
            .bottomAnchor()
            .constraintEqualToAnchor(&group.bottomAnchor()),
    ]));
    group
}

fn spinner(main_thread: MainThreadMarker) -> Retained<NSProgressIndicator> {
    let spinner = NSProgressIndicator::new(main_thread);
    spinner.setStyle(NSProgressIndicatorStyle::Spinning);
    spinner.setControlSize(NSControlSize::Small);
    spinner.setMinValue(0.0);
    spinner.setMaxValue(PROGRESS_FULL);
    spinner.setHidden(true);
    spinner
}

fn start_spinning(spinner: &NSProgressIndicator) {
    spinner.setIndeterminate(true);
    spinner.setHidden(false);
    // SAFETY: AppKit accepts no sender.
    unsafe { spinner.startAnimation(None) };
}

fn stop_spinning(spinner: &NSProgressIndicator) {
    // SAFETY: AppKit accepts no sender.
    unsafe { spinner.stopAnimation(None) };
    spinner.setHidden(true);
}

fn show_status(view: &NSImageView, symbol_name: &str, description: &str, tint: &NSColor) {
    view.setImage(symbol(symbol_name, description).as_deref());
    view.setContentTintColor(Some(tint));
    view.setHidden(false);
}

fn symbol(name: &str, description: &str) -> Option<Retained<NSImage>> {
    let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str(name),
        Some(&NSString::from_str(description)),
    );
    if image.is_none() {
        tracing::warn!(name, "settings symbol is unavailable");
    }
    image
}

fn separator(main_thread: MainThreadMarker) -> Retained<NSBox> {
    let separator = NSBox::new(main_thread);
    separator.setBoxType(NSBoxType::Separator);
    separator
}

fn label(font: &NSFont, main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::new(), main_thread);
    label.setFont(Some(font));
    label
}

fn secondary_wrapping_label(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = wrapping_label(main_thread);
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label
}

/// Stacks `title` over `subtitle`, taking the free width of a row.
fn row_texts(title: &NSView, subtitle: &NSView, main_thread: MainThreadMarker) -> Retained<NSStackView> {
    let texts = stack(
        NSUserInterfaceLayoutOrientation::Vertical,
        ROW_TEXT_SPACING,
        NO_INSETS,
        main_thread,
    );
    texts.addArrangedSubview(title);
    texts.addArrangedSubview(subtitle);
    texts.setContentHuggingPriority_forOrientation(
        NSLayoutPriorityDefaultLow,
        NSLayoutConstraintOrientation::Horizontal,
    );
    texts
}

fn truncating_label(font: &NSFont, main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = label(font, main_thread);
    label.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    label.setContentCompressionResistancePriority_forOrientation(
        NSLayoutPriorityDefaultLow,
        NSLayoutConstraintOrientation::Horizontal,
    );
    label
}

/// Shows `text` in `label`, hiding the label when `text` is empty.
fn set_text(label: &NSTextField, text: &str) {
    label.setStringValue(&NSString::from_str(text));
    label.setHidden(text.is_empty());
}

fn stack(
    orientation: NSUserInterfaceLayoutOrientation,
    spacing: CGFloat,
    insets: NSEdgeInsets,
    main_thread: MainThreadMarker,
) -> Retained<NSStackView> {
    let stack = NSStackView::new(main_thread);
    stack.setOrientation(orientation);
    let alignment = if orientation == NSUserInterfaceLayoutOrientation::Vertical {
        NSLayoutAttribute::Leading
    } else {
        NSLayoutAttribute::CenterY
    };
    stack.setAlignment(alignment);
    stack.setSpacing(spacing);
    stack.setEdgeInsets(insets);
    stack
}

/// Makes `view` as wide as the inside of the vertical `stack` it is arranged in.
fn fill_width(view: &NSView, stack: &NSStackView) {
    let insets = stack.edgeInsets();
    view.widthAnchor()
        .constraintEqualToAnchor_constant(&stack.widthAnchor(), -(insets.left + insets.right))
        .setActive(true);
}

/// A wrapping, non-editable label in the system font.
fn wrapping_label(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = WrappingLabel::new(main_thread);
    label.setEditable(false);
    label.setSelectable(true);
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    label.setFont(Some(&NSFont::systemFontOfSize(NSFont::systemFontSize())));
    label.setTextColor(Some(&NSColor::labelColor()));
    Retained::into_super(label)
}

define_class!(
    // SAFETY: NSTextField has no subclassing requirements, and this class does not implement Drop.
    #[unsafe(super(NSTextField, NSControl, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GhostTypeWrappingLabel"]
    /// A text field that wraps at its laid-out width. AppKit sizes a wrapping field as one line
    /// unless its preferred maximum layout width is set, so its lines would overflow its row.
    struct WrappingLabel;

    impl WrappingLabel {
        // SAFETY: matches the NSView method signature.
        #[unsafe(method(layout))]
        fn layout(&self) {
            let width = self.bounds().size.width;
            if (self.preferredMaxLayoutWidth() - width).abs() > CGFloat::EPSILON {
                self.setPreferredMaxLayoutWidth(width);
                self.invalidateIntrinsicContentSize();
            }
            // SAFETY: calls the NSTextField layout that this method overrides.
            let () = unsafe { msg_send![super(self), layout] };
        }
    }
);

impl WrappingLabel {
    fn new(main_thread: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(());
        // SAFETY: NSTextField's initWithFrame takes a frame and returns an initialized field.
        unsafe { msg_send![super(this), initWithFrame: NSRect::ZERO] }
    }
}

define_class!(
    // SAFETY: NSView has no subclassing requirements, and this class does not implement Drop.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GhostTypeFlippedView"]
    /// A plain view with a top-left origin, so scrolled content starts at the top.
    struct FlippedView;

    impl FlippedView {
        // SAFETY: matches the NSView method signature.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }
    }
);

impl FlippedView {
    fn new(main_thread: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(());
        // SAFETY: NSView's init takes no arguments and returns an initialized view.
        unsafe { msg_send![super(this), init] }
    }
}
