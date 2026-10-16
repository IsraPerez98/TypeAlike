//! The settings window: a toolbar of panes that render the core General, Models and Playground
//! panes and report user actions back.
//!
//! Layout follows System Settings: a pinned header with the search field, then grouped lists in
//! rounded boxes with hairline separators between rows.

mod general;
mod playground;

use ghosttype_core::general_pane::{GeneralAction, GeneralView};
use ghosttype_core::models_pane::{
    ListContent, ModelList, ModelRow, ModelsAction, ModelsView, RowAccessory, RowButton,
};
use ghosttype_core::playground::{ModelPicker, Note, NoteTone, PlaygroundAction, PlaygroundView};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAccessibility as _, NSApplication, NSBackingStoreType, NSBox, NSBoxType, NSButton, NSCellImagePosition,
    NSColor, NSControl, NSControlSize, NSFont, NSFontWeightRegular, NSImage, NSImageView, NSLayoutAttribute,
    NSLayoutConstraint, NSLayoutConstraintOrientation, NSLayoutPriorityDefaultLow, NSLineBreakMode,
    NSMenuItem, NSObjectNSKeyValueBindingCreation as _, NSPopUpButton, NSProgressIndicator,
    NSProgressIndicatorStyle, NSResponder, NSScrollView, NSSearchField, NSStackView, NSStackViewDistribution,
    NSStackViewGravity, NSTabViewController, NSTabViewControllerTabStyle, NSTabViewItem, NSTextField,
    NSTitleBinding, NSTitlePosition, NSUserInterfaceLayoutOrientation, NSView, NSViewController, NSWindow,
    NSWindowStyleMask, NSWindowToolbarStyle,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};

use crate::labels::title_case;
use crate::settings::general::GeneralPage;
use crate::settings::playground::PlaygroundPage;

const WINDOW_SIZE: NSSize = NSSize::new(680.0, 680.0);
const HEADER_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 20.0,
    left: 24.0,
    bottom: 16.0,
    right: 24.0,
};
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

const MESSAGE_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 28.0,
    left: 16.0,
    bottom: 28.0,
    right: 16.0,
};
const SUMMARY_FONT_SIZE: CGFloat = 12.0;
const DETAIL_FONT_SIZE: CGFloat = 12.0;
const HEADER_SPACING: CGFloat = 4.0;
const SEARCH_FIELD_TOP_SPACING: CGFloat = 14.0;
const SECTION_SPACING: CGFloat = 28.0;
const LIST_PART_SPACING: CGFloat = 8.0;
const MIN_ROW_HEIGHT: CGFloat = 52.0;
const ROW_TEXT_PADDING: CGFloat = 8.0;
const ROW_COLUMN_SPACING: CGFloat = 12.0;
const ROW_TEXT_SPACING: CGFloat = 2.0;
const ACCESSORY_SPACING: CGFloat = 8.0;
const MESSAGE_SPACING: CGFloat = 8.0;
const ICON_BUTTON_SIDE: CGFloat = 24.0;
const GROUP_CORNER_RADIUS: CGFloat = 10.0;
const GROUP_BORDER_WIDTH: CGFloat = 1.0;
const PROGRESS_FULL: f64 = 1000.0;
const OPEN_SYMBOL: &str = "chevron.right";
const BACK_SYMBOL: &str = "chevron.left";
const CANCEL_SYMBOL: &str = "xmark.circle.fill";
const DELETE_SYMBOL: &str = "trash";
const INSTALLED_SYMBOL: &str = "checkmark.circle.fill";
const INSTALLED_DESCRIPTION: &str = "Downloaded";
const UNAVAILABLE_SYMBOL: &str = "lock.fill";
const UNAVAILABLE_DESCRIPTION: &str = "Unavailable";
const GENERAL_SYMBOL: &str = "gearshape";
const MODELS_SYMBOL: &str = "cube";
const PLAYGROUND_SYMBOL: &str = "text.cursor";

/// The object and selectors every settings control reports to.
#[derive(Debug)]
pub(crate) struct ActionTarget {
    pub(crate) target: Retained<NSObject>,
    pub(crate) button_action: Sel,
    pub(crate) search_action: Sel,
    pub(crate) model_picker_action: Sel,
    pub(crate) complete_action: Sel,
    pub(crate) autostart_action: Sel,
    pub(crate) clipboard_action: Sel,
    pub(crate) preparation_action: Sel,
    pub(crate) display_action: Sel,
    pub(crate) suggestion_length_action: Sel,
    pub(crate) suggestion_model_action: Sel,
    pub(crate) system_prompt_action: Sel,
    pub(crate) reset_system_prompt_action: Sel,
}

/// The settings window, kept alive and reused after the user closes it.
#[derive(Debug)]
pub(crate) struct SettingsWindow {
    window: Retained<NSWindow>,
    tabs: Retained<NSTabViewController>,
    general_pane: Pane,
    models_pane: Pane,
    playground_pane: Pane,
    general: GeneralPage,
    summary: Retained<NSTextField>,
    search_field: Retained<NSSearchField>,
    browse: ListView,
    downloaded: ListView,
    playground: PlaygroundPage,
    actions: Vec<ModelsAction>,
    target: ActionTarget,
    main_thread: MainThreadMarker,
}

impl SettingsWindow {
    /// Builds the window, hidden until [`Self::show`].
    ///
    /// A Models button sends `target.button_action` with its tag; see [`Self::action_for_tag`]. The
    /// search field sends `target.search_action` when the user submits or clears it. The Playground
    /// model menu sends `target.model_picker_action`, see [`Self::playground_choice`]; its complete
    /// button and prompt field send `target.complete_action`. The General autostart switch sends
    /// `target.autostart_action`, its clipboard switch sends `target.clipboard_action`, its prompt
    /// preparation switch sends `target.preparation_action`, its suggestion display switch sends
    /// `target.display_action`, its suggestion length menu sends `target.suggestion_length_action`,
    /// see [`Self::suggestion_length_choice`], and its suggestion model menu sends
    /// `target.suggestion_model_action`, see [`Self::suggestion_model_choice`]. Each edit of the
    /// General system prompt sends `target.system_prompt_action`, see [`Self::system_prompt`], and
    /// its reset button sends `target.reset_system_prompt_action`.
    pub(crate) fn new(target: ActionTarget, main_thread: MainThreadMarker) -> Self {
        let summary = wrapping_label(main_thread);
        summary.setFont(Some(&NSFont::systemFontOfSize(SUMMARY_FONT_SIZE)));
        summary.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let search_field = search_field(&target, main_thread);
        let header = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            HEADER_SPACING,
            HEADER_INSETS,
            main_thread,
        );
        let header_views: [&NSView; 2] = [&summary, &search_field];
        for view in header_views {
            header.addArrangedSubview(view);
        }
        header.setCustomSpacing_afterView(SEARCH_FIELD_TOP_SPACING, &summary);
        fill_width(&summary, &header);
        fill_width(&search_field, &header);
        let content = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            SECTION_SPACING,
            CONTENT_INSETS,
            main_thread,
        );
        let browse = ListView::new(&content, &target, main_thread);
        let downloaded = ListView::new(&content, &target, main_thread);
        let playground = PlaygroundPage::new(&target, main_thread);
        let general = GeneralPage::new(&target, main_thread);
        let general_pane = Pane::new(&general.root, GENERAL_SYMBOL, main_thread);
        let models_pane = Pane::new(
            &models_root(&header, &content, main_thread),
            MODELS_SYMBOL,
            main_thread,
        );
        let playground_pane = Pane::new(&playground.root, PLAYGROUND_SYMBOL, main_thread);
        let tabs = pane_tabs(&[&general_pane, &models_pane, &playground_pane], main_thread);
        let window = settings_window(&tabs, main_thread);
        Self {
            window,
            tabs,
            general_pane,
            models_pane,
            playground_pane,
            general,
            summary,
            search_field,
            browse,
            downloaded,
            playground,
            actions: Vec::new(),
            target,
            main_thread,
        }
    }

    /// Brings the window and the app to the front.
    pub(crate) fn show(&self) {
        NSApplication::sharedApplication(self.main_thread).activate();
        self.window.makeKeyAndOrderFront(None);
    }

    /// Shows `view` in the General pane.
    pub(crate) fn render_general(&mut self, view: &GeneralView) {
        self.general_pane.set_title(view.title);
        self.show_selected_pane_title();
        self.general.render(view);
    }

    /// Returns the action of the General suggestion model menu entry at `index` in the last render.
    pub(crate) fn suggestion_model_choice(&self, index: isize) -> Option<PlaygroundAction> {
        self.general.suggestion_model_menu.action_at(index)
    }

    /// Returns the action of the General suggestion length menu entry at `index` in the last render.
    pub(crate) fn suggestion_length_choice(&self, index: isize) -> Option<GeneralAction> {
        self.general.suggestion_length_menu.action_at(index)
    }

    /// Returns the text in the General system prompt field.
    pub(crate) fn system_prompt(&self) -> String {
        self.general.system_prompt()
    }

    /// Shows `view` in the Models pane, reusing row views so clicks in progress are not lost.
    pub(crate) fn render_models(&mut self, view: &ModelsView) {
        self.models_pane.set_title(view.title);
        self.show_selected_pane_title();
        self.summary.setStringValue(&NSString::from_str(view.summary));
        self.search_field
            .setPlaceholderString(Some(&NSString::from_str(view.search_placeholder)));
        self.actions.clear();
        let mut renderer = Renderer {
            actions: &mut self.actions,
            target: &self.target,
            main_thread: self.main_thread,
        };
        renderer.render_list(&mut self.browse, Some(&view.browse));
        renderer.render_list(&mut self.downloaded, view.downloaded.as_ref());
    }

    /// Returns the action of the Models button that carries `tag` in the last render.
    pub(crate) fn action_for_tag(&self, tag: isize) -> Option<ModelsAction> {
        let index = usize::try_from(tag).ok()?;
        self.actions.get(index).cloned()
    }

    /// Shows `view` in the Playground pane.
    pub(crate) fn render_playground(&mut self, view: &PlaygroundView) {
        self.playground_pane.set_title(view.title);
        self.show_selected_pane_title();
        self.playground.render(view);
    }

    /// Names the window after the selected pane, which the tab controller does only on a selection change.
    fn show_selected_pane_title(&self) {
        if let Some(selected) = self.tabs.tabView().selectedTabViewItem() {
            self.tabs.setTitle(Some(&selected.label()));
        }
    }

    /// Returns the action of the Playground model menu entry at `index` in the last render.
    pub(crate) fn playground_choice(&self, index: isize) -> Option<PlaygroundAction> {
        self.playground.model_menu.action_at(index)
    }

    /// Returns the text in the Playground prompt field.
    pub(crate) fn playground_prompt(&self) -> String {
        self.playground.prompt()
    }
}

/// One pane of the settings toolbar: its view controller and toolbar item.
#[derive(Debug)]
struct Pane {
    controller: Retained<NSViewController>,
    tab: Retained<NSTabViewItem>,
}

impl Pane {
    fn new(view: &NSView, symbol_name: &str, main_thread: MainThreadMarker) -> Self {
        let controller = NSViewController::new(main_thread);
        controller.setView(view);
        controller.setPreferredContentSize(WINDOW_SIZE);
        let tab = NSTabViewItem::tabViewItemWithViewController(&controller);
        tab.setImage(symbol(symbol_name, "").as_deref());
        Self { controller, tab }
    }

    /// Names the pane in its toolbar item, and in the window title while the pane is selected.
    fn set_title(&self, title: &str) {
        let title = NSString::from_str(title);
        self.controller.setTitle(Some(&title));
        self.tab.setLabel(&title);
    }
}

/// A heading, a rounded group of rows or a message, and a footer.
#[derive(Debug)]
struct ListView {
    container: Retained<NSStackView>,
    back: Retained<NSButton>,
    heading: Retained<NSTextField>,
    rows_stack: Retained<NSStackView>,
    message: MessageView,
    footer: Retained<NSTextField>,
    rows: Vec<RowView>,
}

impl ListView {
    fn new(content: &NSStackView, target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let back = icon_button(target, main_thread);
        back.setImagePosition(NSCellImagePosition::ImageLeading);
        let heading = label(
            &NSFont::boldSystemFontOfSize(NSFont::systemFontSize()),
            main_thread,
        );
        let title_bar = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            ACCESSORY_SPACING,
            NO_INSETS,
            main_thread,
        );
        title_bar.addArrangedSubview(&back);
        title_bar.addArrangedSubview(&heading);
        let rows_stack = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            0.0,
            NO_INSETS,
            main_thread,
        );
        let message = MessageView::new(&rows_stack, target, main_thread);
        let group = rounded_group(&rows_stack, main_thread);
        let footer = wrapping_label(main_thread);
        footer.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
        footer.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let container = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            NO_INSETS,
            main_thread,
        );
        let parts: [&NSView; 3] = [&title_bar, &group, &footer];
        for part in parts {
            container.addArrangedSubview(part);
        }
        content.addArrangedSubview(&container);
        fill_width(&container, content);
        fill_width(&group, &container);
        fill_width(&footer, &container);
        fill_width(&message.container, &rows_stack);
        Self {
            container,
            back,
            heading,
            rows_stack,
            message,
            footer,
            rows: Vec::new(),
        }
    }

    fn resize_rows(&mut self, count: usize, target: &ActionTarget, main_thread: MainThreadMarker) {
        if self.rows.len() == count {
            return;
        }
        for row_view in self.rows.drain(..) {
            row_view.container.removeFromSuperview();
        }
        let rows_stack = &self.rows_stack;
        self.rows
            .extend((0..count).map(|_| RowView::new(rows_stack, target, main_thread)));
    }
}

/// What shows in a group instead of rows: a spinner or a message, with an optional retry.
#[derive(Debug)]
struct MessageView {
    container: Retained<NSStackView>,
    spinner: Retained<NSProgressIndicator>,
    text: Retained<NSTextField>,
    retry: Retained<NSButton>,
}

impl MessageView {
    fn new(rows_stack: &NSStackView, target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let spinner = spinner(main_thread);
        let text = wrapping_label(main_thread);
        text.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let retry = push_button(target, main_thread);
        let container = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            MESSAGE_SPACING,
            MESSAGE_INSETS,
            main_thread,
        );
        let parts: [&NSView; 3] = [&spinner, &text, &retry];
        for part in parts {
            container.addView_inGravity(part, NSStackViewGravity::Center);
        }
        rows_stack.addArrangedSubview(&container);
        Self {
            container,
            spinner,
            text,
            retry,
        }
    }
}

/// One row: a separator above it, texts, a trailing detail and the accessory controls.
#[derive(Debug)]
struct RowView {
    container: Retained<NSStackView>,
    separator: Retained<NSBox>,
    title: Retained<NSTextField>,
    subtitle: Retained<NSTextField>,
    detail: Retained<NSTextField>,
    progress: Retained<NSProgressIndicator>,
    status: Retained<NSImageView>,
    primary: Retained<NSButton>,
    icon: Retained<NSButton>,
}

impl RowView {
    fn new(rows_stack: &NSStackView, target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let title = truncating_label(&NSFont::systemFontOfSize(NSFont::systemFontSize()), main_thread);
        let subtitle = truncating_label(
            &NSFont::systemFontOfSize(NSFont::smallSystemFontSize()),
            main_thread,
        );
        let texts = row_texts(&title, &subtitle, main_thread);
        // SAFETY: reads an AppKit constant that is initialized before any code runs.
        let regular_weight = unsafe { NSFontWeightRegular };
        let detail = label(
            &NSFont::monospacedDigitSystemFontOfSize_weight(DETAIL_FONT_SIZE, regular_weight),
            main_thread,
        );
        detail.setTextColor(Some(&NSColor::secondaryLabelColor()));
        let progress = spinner(main_thread);
        let status = NSImageView::new(main_thread);
        let primary = push_button(target, main_thread);
        let icon = icon_button(target, main_thread);
        let controls: [&NSView; 5] = [&detail, &progress, &status, &primary, &icon];
        let GroupedRow { container, separator } = GroupedRow::add(rows_stack, &texts, &controls, main_thread);
        Self {
            container,
            separator,
            title,
            subtitle,
            detail,
            progress,
            status,
            primary,
            icon,
        }
    }

    fn update(&self, row: &ModelRow, renderer: &mut Renderer<'_>) {
        set_text(&self.title, &row.title);
        set_text(&self.subtitle, &row.subtitle);
        set_text(&self.detail, &row.detail);
        let subtitle_color = match row.accessory {
            RowAccessory::Failed { .. } => NSColor::systemRedColor(),
            _ => NSColor::secondaryLabelColor(),
        };
        self.subtitle.setTextColor(Some(&subtitle_color));
        let controls: [&NSView; 4] = [&self.progress, &self.status, &self.primary, &self.icon];
        for control in controls {
            control.setHidden(true);
        }
        stop_spinning(&self.progress);
        self.show_accessory(&row.accessory, &row.title, renderer);
    }

    fn show_accessory(&self, accessory: &RowAccessory, subject: &str, renderer: &mut Renderer<'_>) {
        match accessory {
            RowAccessory::Download(button) | RowAccessory::Failed { retry: button } => {
                renderer.show_push_button(&self.primary, button, Some(subject));
            }
            RowAccessory::Open(button) => renderer.show_icon_button(&self.icon, OPEN_SYMBOL, button, subject),
            RowAccessory::Downloading { progress, cancel } => {
                self.progress.setIndeterminate(false);
                self.progress.setDoubleValue(f64::from(progress.0));
                self.progress.setHidden(false);
                renderer.show_icon_button(&self.icon, CANCEL_SYMBOL, cancel, subject);
            }
            RowAccessory::Canceling => start_spinning(&self.progress),
            RowAccessory::Installed { delete } => {
                show_status(
                    &self.status,
                    INSTALLED_SYMBOL,
                    INSTALLED_DESCRIPTION,
                    &NSColor::systemGreenColor(),
                );
                renderer.show_icon_button(&self.icon, DELETE_SYMBOL, delete, subject);
            }
            RowAccessory::Unavailable => show_status(
                &self.status,
                UNAVAILABLE_SYMBOL,
                UNAVAILABLE_DESCRIPTION,
                &NSColor::tertiaryLabelColor(),
            ),
        }
    }
}

/// A row of a rounded group: a separator above it and the row's columns.
#[derive(Debug)]
struct GroupedRow {
    container: Retained<NSStackView>,
    separator: Retained<NSBox>,
}

impl GroupedRow {
    /// Adds a row of `texts`, then `controls`, left to right, at the bottom of `rows_stack`. The
    /// row grows past its minimum height to keep a padding around `texts` when they wrap.
    fn add(
        rows_stack: &NSStackView,
        texts: &NSView,
        controls: &[&NSView],
        main_thread: MainThreadMarker,
    ) -> Self {
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
        Self { container, separator }
    }
}

/// A pop-up menu of models. It rebuilds its entries only when they change, so an open menu stays
/// open.
#[derive(Debug)]
struct ModelMenu {
    button: Retained<NSPopUpButton>,
    titles: Vec<String>,
    actions: Vec<Option<PlaygroundAction>>,
}

impl ModelMenu {
    /// Builds a menu that sends `action` to the target of `target` when the user picks a model.
    fn new(target: &ActionTarget, action: Sel, main_thread: MainThreadMarker) -> Self {
        Self {
            button: pop_up_button(target, action, main_thread),
            titles: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// Shows `picker`, and keeps the actions of its entries for [`Self::action_at`].
    fn render(&mut self, picker: &ModelPicker) {
        self.button
            .setAccessibilityLabel(Some(&NSString::from_str(picker.label)));
        let titles: Vec<String> = picker.choices.iter().map(|choice| choice.title.clone()).collect();
        if titles != self.titles {
            rebuild_menu(&self.button, &titles);
            self.titles = titles;
        }
        self.actions = picker
            .choices
            .iter()
            .map(|choice| choice.action.clone())
            .collect();
        let selected = isize::try_from(picker.selected).expect("a menu never holds isize::MAX entries");
        self.button.selectItemAtIndex(selected);
        self.button.setEnabled(picker.is_enabled);
    }

    /// Returns the action of the entry at `index` in the last render.
    fn action_at(&self, index: isize) -> Option<PlaygroundAction> {
        let index = usize::try_from(index).ok()?;
        self.actions.get(index).cloned().flatten()
    }
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

struct Renderer<'a> {
    actions: &'a mut Vec<ModelsAction>,
    target: &'a ActionTarget,
    main_thread: MainThreadMarker,
}

impl Renderer<'_> {
    fn render_list(&mut self, view: &mut ListView, list: Option<&ModelList>) {
        view.container.setHidden(list.is_none());
        let Some(list) = list else {
            return;
        };
        set_text(&view.heading, &list.heading);
        set_text(&view.footer, list.footer.unwrap_or_default());
        match &list.back {
            Some(back) => self.show_back_button(&view.back, back),
            None => view.back.setHidden(true),
        }
        let rows = match &list.content {
            ListContent::Rows(rows) => rows.as_slice(),
            ListContent::Loading(_) | ListContent::Message { .. } => &[],
        };
        self.render_message(&view.message, &list.content);
        view.resize_rows(rows.len(), self.target, self.main_thread);
        for (index, (row_view, row)) in view.rows.iter().zip(rows).enumerate() {
            row_view.separator.setHidden(index == 0);
            row_view.update(row, self);
        }
    }

    fn render_message(&mut self, view: &MessageView, content: &ListContent) {
        stop_spinning(&view.spinner);
        view.retry.setHidden(true);
        match content {
            ListContent::Rows(_) => view.container.setHidden(true),
            ListContent::Loading(text) => {
                view.container.setHidden(false);
                set_text(&view.text, text);
                start_spinning(&view.spinner);
            }
            ListContent::Message { text, retry } => {
                view.container.setHidden(false);
                set_text(&view.text, text);
                if let Some(retry) = retry {
                    self.show_push_button(&view.retry, retry, None);
                }
            }
        }
    }

    fn show_push_button(&mut self, view: &NSButton, button: &RowButton, subject: Option<&str>) {
        view.setTitle(&NSString::from_str(&title_case(button.label)));
        let spoken_label = subject.map(|subject| NSString::from_str(&format!("{}, {subject}", button.label)));
        view.setAccessibilityLabel(spoken_label.as_deref());
        self.attach(view, button);
    }

    fn show_icon_button(&mut self, view: &NSButton, symbol_name: &str, button: &RowButton, subject: &str) {
        let spoken_label = format!("{}, {subject}", button.label);
        view.setImage(symbol(symbol_name, &spoken_label).as_deref());
        view.setToolTip(Some(&NSString::from_str(button.label)));
        view.setAccessibilityLabel(Some(&NSString::from_str(&spoken_label)));
        self.attach(view, button);
    }

    fn show_back_button(&mut self, view: &NSButton, button: &RowButton) {
        view.setImage(symbol(BACK_SYMBOL, button.label).as_deref());
        view.setTitle(&NSString::from_str(&title_case(button.label)));
        self.attach(view, button);
    }

    fn attach(&mut self, view: &NSButton, button: &RowButton) {
        let tag = isize::try_from(self.actions.len()).expect("a window never holds isize::MAX buttons");
        self.actions.push(button.action.clone());
        view.setTag(tag);
        view.setHidden(false);
    }
}

fn pane_tabs(panes: &[&Pane], main_thread: MainThreadMarker) -> Retained<NSTabViewController> {
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

/// The Models pane: the pinned `header`, a divider, and `content` that scrolls.
fn models_root(
    header: &NSView,
    content: &NSStackView,
    main_thread: MainThreadMarker,
) -> Retained<NSStackView> {
    let scroll_view = scroll_view(content, main_thread);
    let divider = separator(main_thread);
    let root = stack(
        NSUserInterfaceLayoutOrientation::Vertical,
        0.0,
        NO_INSETS,
        main_thread,
    );
    let root_views: [&NSView; 3] = [header, &divider, &scroll_view];
    for view in root_views {
        root.addArrangedSubview(view);
        fill_width(view, &root);
    }
    root.setDistribution(NSStackViewDistribution::Fill);
    root
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

fn search_field(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSSearchField> {
    let field = NSSearchField::new(main_thread);
    field.setControlSize(NSControlSize::Large);
    field.setSendsWholeSearchString(true);
    // SAFETY: the target implements the search action, taking the field, and `SettingsWindow`
    // keeps the target alive as long as the field.
    unsafe {
        field.setTarget(Some(&target.target));
        field.setAction(Some(target.search_action));
    }
    field
}

fn push_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSButton> {
    // SAFETY: the target implements the button action, taking the button, and `SettingsWindow`
    // keeps the target alive as long as the button.
    let button = unsafe {
        NSButton::buttonWithTitle_target_action(
            &NSString::new(),
            Some(&target.target),
            Some(target.button_action),
            main_thread,
        )
    };
    button.setHidden(true);
    button
}

fn icon_button(target: &ActionTarget, main_thread: MainThreadMarker) -> Retained<NSButton> {
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
