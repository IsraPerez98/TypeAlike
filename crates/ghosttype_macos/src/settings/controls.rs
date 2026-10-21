//! The controls of the settings pages.
//!
//! It gives the labels, buttons, spinners, symbols, pop-up buttons and text areas, and the fonts
//! of their text.

use ghosttype_core::settings_window::view;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAccessibility as _, NSBorderType, NSButton, NSColor, NSControl, NSControlSize, NSControlStateValue,
    NSControlStateValueOff, NSControlStateValueOn, NSFont, NSFontWeightRegular, NSImage, NSImageView,
    NSLayoutConstraint, NSLayoutConstraintOrientation, NSLayoutPriorityDefaultLow, NSLineBreakMode,
    NSMenuItem, NSPopUpButton, NSProgressIndicator, NSProgressIndicatorStyle, NSResponder, NSScrollView,
    NSTextField, NSTextView, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSRect, NSSize, NSString};

use crate::labels::title_case;
use crate::settings::{ActionTarget, Binder, ControlAction};
use crate::symbol::system_symbol;

const DETAIL_FONT_SIZE: CGFloat = 12.0;
const TEXT_AREA_HEIGHT: CGFloat = 120.0;
const TEXT_AREA_INSETS: NSSize = NSSize::new(4.0, 6.0);
const ICON_BUTTON_SIDE: CGFloat = 24.0;
const BACK_SYMBOL: &str = "chevron.left";

/// The pop-up menu of a row, which picks one of its entries. The menu rebuilds its entries only
/// when they change, so that an open menu stays open.
#[derive(Debug)]
pub(super) struct MenuChoices {
    pub(super) menu: Retained<NSPopUpButton>,
    titles: Vec<String>,
}

impl MenuChoices {
    /// Makes a menu without entries, which sends the pick action to the target.
    pub(super) fn new(target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        Self {
            menu: pop_up_button(target, main_thread),
            titles: Vec::new(),
        }
    }

    /// Shows an entry for each of `titles`, with the entry at `selected` picked.
    pub(super) fn render(&mut self, titles: Vec<String>, selected: usize) {
        if titles != self.titles {
            rebuild_menu(&self.menu, &titles);
            self.titles = titles;
        }
        let selected = isize::try_from(selected).expect("a menu never holds isize::MAX entries");
        self.menu.selectItemAtIndex(selected);
    }
}

/// Returns a pop-up button that sends the pick action to the target when the user picks an entry.
fn pop_up_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSPopUpButton> {
    let button = NSPopUpButton::new(main_thread);
    button.setPullsDown(false);
    target.connect(&button, ControlAction::Pick);
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

/// The state of a switch or a menu item that is on when `is_on` is true.
pub(crate) fn control_state(is_on: bool) -> NSControlStateValue {
    if is_on {
        NSControlStateValueOn
    } else {
        NSControlStateValueOff
    }
}

pub(super) fn spinner(main_thread: MainThreadMarker) -> Retained<NSProgressIndicator> {
    let spinner = NSProgressIndicator::new(main_thread);
    spinner.setStyle(NSProgressIndicatorStyle::Spinning);
    spinner.setControlSize(NSControlSize::Small);
    spinner.setMinValue(0.0);
    spinner.setMaxValue(f64::from(view::Permille::FULL.0));
    spinner.setHidden(true);
    spinner
}

pub(super) fn start_spinning(spinner: &NSProgressIndicator) {
    spinner.setIndeterminate(true);
    spinner.setHidden(false);
    // SAFETY: AppKit accepts no sender.
    unsafe { spinner.startAnimation(None) };
}

pub(super) fn stop_spinning(spinner: &NSProgressIndicator) {
    // SAFETY: AppKit accepts no sender.
    unsafe { spinner.stopAnimation(None) };
    spinner.setHidden(true);
}

pub(super) fn show_status(view: &NSImageView, symbol_name: &str, description: &str, tint: &NSColor) {
    view.setImage(symbol(symbol_name, description).as_deref());
    view.setContentTintColor(Some(tint));
    view.setHidden(false);
}

pub(super) fn symbol(name: &str, description: &str) -> Option<Retained<NSImage>> {
    let image = system_symbol(name, description);
    if image.is_none() {
        tracing::warn!(name, "settings symbol is unavailable");
    }
    image
}

pub(super) fn label(font: &NSFont, main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = NSTextField::labelWithString(&NSString::new(), main_thread);
    label.setFont(Some(font));
    label
}

pub(super) fn secondary_wrapping_label(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = wrapping_label(main_thread);
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label
}

/// A wrapping label in the secondary label color and the small system font, for the text below
/// a label or a section.
pub(super) fn small_secondary_label(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = secondary_wrapping_label(main_thread);
    label.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
    label
}

pub(super) fn truncating_label(font: &NSFont, main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = label(font, main_thread);
    label.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    label.setContentCompressionResistancePriority_forOrientation(
        NSLayoutPriorityDefaultLow,
        NSLayoutConstraintOrientation::Horizontal,
    );
    label
}

/// Shows `text` in `label`, hiding the label when `text` is empty.
pub(super) fn set_text(label: &NSTextField, text: &str) {
    label.setStringValue(&NSString::from_str(text));
    label.setHidden(text.is_empty());
}

/// A wrapping, non-editable label in the system font.
pub(super) fn wrapping_label(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = WrappingLabel::new(main_thread);
    label.setEditable(false);
    label.setSelectable(true);
    label.setBezeled(false);
    label.setDrawsBackground(false);
    label.setLineBreakMode(NSLineBreakMode::ByWordWrapping);
    label.setFont(Some(&body_font()));
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

/// Shows `button` in `view`, a push button with a title. The screen reader names it after its
/// `subject` when there is one.
pub(super) fn show_push_button(
    view: &NSButton,
    button: &view::Button,
    subject: Option<&str>,
    binder: &mut Binder<'_>,
) {
    view.setTitle(&NSString::from_str(&title_case(button.label)));
    let spoken_label = subject.map(|subject| NSString::from_str(&format!("{}, {subject}", button.label)));
    view.setAccessibilityLabel(spoken_label.as_deref());
    view.setEnabled(button.is_enabled);
    binder.bind(view, &button.id);
    view.setHidden(false);
}

/// Shows `button` in `view`, the back button of a section title bar.
pub(super) fn show_back_button(view: &NSButton, button: &view::Button, binder: &mut Binder<'_>) {
    view.setImage(symbol(BACK_SYMBOL, button.label).as_deref());
    view.setTitle(&NSString::from_str(&title_case(button.label)));
    view.setEnabled(button.is_enabled);
    binder.bind(view, &button.id);
    view.setHidden(false);
}

/// Returns a hidden push button that sends the press action to the target.
pub(super) fn push_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSButton> {
    let button = button(target, target.press_action, main_thread);
    button.setHidden(true);
    button
}

/// Returns a hidden borderless button for a symbol, which sends the press action to the target.
pub(super) fn icon_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSButton> {
    let button = push_button(target, main_thread);
    button.setBordered(false);
    button.setContentTintColor(Some(&NSColor::secondaryLabelColor()));
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        button
            .widthAnchor()
            .constraintGreaterThanOrEqualToConstant(ICON_BUTTON_SIDE),
        button
            .heightAnchor()
            .constraintGreaterThanOrEqualToConstant(ICON_BUTTON_SIDE),
    ]));
    button
}

/// Returns a push button that sends `action` to the target of `target`.
pub(super) fn button(
    target: &ActionTarget,
    action: Sel,
    main_thread: MainThreadMarker,
) -> Retained<NSButton> {
    // SAFETY: the target implements `action`, taking the button, and `SettingsWindow` keeps the
    // target alive as long as the button.
    unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::new(),
            Some(&target.target),
            Some(action),
            main_thread,
        )
    }
}

/// Returns a bold label that names what follows it.
pub(super) fn heading(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    label(
        &NSFont::boldSystemFontOfSize(NSFont::systemFontSize()),
        main_thread,
    )
}

/// The system font at the size of body text.
pub(super) fn body_font() -> Retained<NSFont> {
    NSFont::systemFontOfSize(NSFont::systemFontSize())
}

/// The font of short facts, such as sizes and timings, with digits of one width.
pub(super) fn detail_font() -> Retained<NSFont> {
    // SAFETY: reads an AppKit constant that is initialized before any code runs.
    let regular_weight = unsafe { NSFontWeightRegular };
    NSFont::monospacedDigitSystemFontOfSize_weight(DETAIL_FONT_SIZE, regular_weight)
}

/// Returns a plain text view with a bezel, in the scroll view that holds it.
pub(super) fn text_area(main_thread: MainThreadMarker) -> (Retained<NSScrollView>, Retained<NSTextView>) {
    let text_scroll_view = NSTextView::scrollableTextView(main_thread);
    text_scroll_view.setBorderType(NSBorderType::BezelBorder);
    text_scroll_view
        .heightAnchor()
        .constraintEqualToConstant(TEXT_AREA_HEIGHT)
        .setActive(true);
    let text_view = text_scroll_view
        .documentView()
        .and_then(|view| view.downcast::<NSTextView>().ok())
        .expect("a scrollable text view holds a text view");
    text_view.setRichText(false);
    text_view.setAllowsUndo(true);
    text_view.setFont(Some(&body_font()));
    text_view.setTextContainerInset(TEXT_AREA_INSETS);
    (text_scroll_view, text_view)
}
