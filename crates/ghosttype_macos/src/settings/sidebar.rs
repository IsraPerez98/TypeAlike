//! The sidebar of the settings window: a source list with a row for each page, like the sidebar of
//! System Settings. Each row shows the symbol of its page on a colored tile, then the page title. A
//! pick in the list shows its page in the tab view controller, and names the window after it.

use std::fmt;

use ghosttype_core::settings_window::view::PageIcon;
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSBox, NSBoxType, NSColor, NSControlTextEditingDelegate, NSFontWeightSemibold,
    NSImageSymbolConfiguration, NSImageView, NSLayoutConstraint, NSScrollView, NSTabViewController,
    NSTableCellView, NSTableColumn, NSTableView, NSTableViewDataSource, NSTableViewDelegate,
    NSTableViewRowSizeStyle, NSTableViewStyle, NSTextField, NSTitlePosition,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSIndexSet, NSInteger, NSNotification, NSSize, NSString};

use crate::settings::controls::{body_font, set_text, symbol, truncating_label};
use crate::settings::layout::{NO_INSETS, stack};
use crate::settings::show_selected_pane_title;

const ROW_HEIGHT: CGFloat = 32.0;
const TILE_SIDE: CGFloat = 22.0;
const TILE_CORNER_RADIUS: CGFloat = 6.0;
const TILE_SYMBOL_SIZE: CGFloat = 12.0;
const TILE_TITLE_SPACING: CGFloat = 8.0;
const PAGE_COLUMN: &str = "page";

/// The list of pages at the leading edge of the settings window.
#[derive(Debug)]
pub(super) struct Sidebar {
    pub(super) view: Retained<NSScrollView>,
    list: Retained<PageList>,
}

impl Sidebar {
    /// Makes a row for each of `icons`, top to bottom, with the first row picked. A pick shows the
    /// page of the same index in `tabs`.
    pub(super) fn new(
        icons: &[PageIcon],
        tabs: Retained<NSTabViewController>,
        main_thread: MainThreadMarker,
    ) -> Self {
        let rows = icons
            .iter()
            .map(|&icon| PageRow::new(icon, main_thread))
            .collect();
        let list = PageList::new(PageListIvars { rows, tabs }, main_thread);
        let table = NSTableView::new(main_thread);
        table.setStyle(NSTableViewStyle::SourceList);
        table.setHeaderView(None);
        table.setRowSizeStyle(NSTableViewRowSizeStyle::Custom);
        table.setRowHeight(ROW_HEIGHT);
        table.setAllowsEmptySelection(false);
        let column = NSTableColumn::initWithIdentifier(
            NSTableColumn::alloc(main_thread),
            &NSString::from_str(PAGE_COLUMN),
        );
        table.addTableColumn(&column);
        // SAFETY: the table keeps a weak reference to its data source and delegate, and `Sidebar`
        // keeps the list alive as long as the table.
        unsafe {
            table.setDataSource(Some(ProtocolObject::from_ref(&*list)));
            table.setDelegate(Some(ProtocolObject::from_ref(&*list)));
        }
        table.selectRowIndexes_byExtendingSelection(&NSIndexSet::indexSetWithIndex(0), false);
        let view = NSScrollView::new(main_thread);
        view.setDrawsBackground(false);
        view.setHasVerticalScroller(true);
        view.setAutohidesScrollers(true);
        view.setDocumentView(Some(&table));
        Self { view, list }
    }

    /// Shows `titles` in the rows, top to bottom.
    pub(super) fn render<'a>(&self, titles: impl IntoIterator<Item = &'a str>) {
        for (row, title) in self.list.ivars().rows.iter().zip(titles) {
            set_text(&row.title, title);
        }
    }
}

/// The views of one row of the sidebar.
#[derive(Debug)]
struct PageRow {
    cell: Retained<NSTableCellView>,
    title: Retained<NSTextField>,
}

impl PageRow {
    fn new(icon: PageIcon, main_thread: MainThreadMarker) -> Self {
        let icon_view = icon_tile(icon, main_thread);
        let title = truncating_label(&body_font(), main_thread);
        let content = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            TILE_TITLE_SPACING,
            NO_INSETS,
            main_thread,
        );
        content.addArrangedSubview(&icon_view);
        content.addArrangedSubview(&title);
        let cell = NSTableCellView::new(main_thread);
        // SAFETY: the cell keeps the label as one of its subviews, so the label outlives the
        // reference.
        unsafe { cell.setTextField(Some(&title)) };
        cell.addSubview(&content);
        content.setTranslatesAutoresizingMaskIntoConstraints(false);
        NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
            content
                .leadingAnchor()
                .constraintEqualToAnchor(&cell.leadingAnchor()),
            content
                .trailingAnchor()
                .constraintLessThanOrEqualToAnchor(&cell.trailingAnchor()),
            content
                .centerYAnchor()
                .constraintEqualToAnchor(&cell.centerYAnchor()),
        ]));
        Self { cell, title }
    }
}

/// The symbol of `icon` in white on a rounded tile of its color, like the icons of the panes of
/// System Settings.
fn icon_tile(icon: PageIcon, main_thread: MainThreadMarker) -> Retained<NSBox> {
    let (symbol_name, color) = page_look(icon);
    let image = NSImageView::new(main_thread);
    image.setImage(symbol(symbol_name, "").as_deref());
    // SAFETY: reads an AppKit constant that is initialized before any code runs.
    let weight = unsafe { NSFontWeightSemibold };
    image.setSymbolConfiguration(Some(
        &NSImageSymbolConfiguration::configurationWithPointSize_weight(TILE_SYMBOL_SIZE, weight),
    ));
    image.setContentTintColor(Some(&NSColor::whiteColor()));
    let tile = NSBox::new(main_thread);
    tile.setBoxType(NSBoxType::Custom);
    tile.setTitlePosition(NSTitlePosition::NoTitle);
    tile.setContentViewMargins(NSSize::ZERO);
    tile.setBorderWidth(0.0);
    tile.setCornerRadius(TILE_CORNER_RADIUS);
    tile.setFillColor(&color);
    tile.setContentView(Some(&image));
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        tile.widthAnchor().constraintEqualToConstant(TILE_SIDE),
        tile.heightAnchor().constraintEqualToConstant(TILE_SIDE),
    ]));
    tile
}

/// The SF Symbol and the tile color that show `icon`.
fn page_look(icon: PageIcon) -> (&'static str, Retained<NSColor>) {
    match icon {
        PageIcon::Gear => ("gearshape.fill", NSColor::systemGrayColor()),
        PageIcon::TextViewfinder => ("text.viewfinder", NSColor::systemBlueColor()),
        PageIcon::PersonText => ("person.text.rectangle.fill", NSColor::systemOrangeColor()),
        PageIcon::Cube => ("cube.fill", NSColor::systemPurpleColor()),
        PageIcon::TextCursor => ("text.cursor", NSColor::systemGreenColor()),
    }
}

#[derive(Debug)]
struct PageListIvars {
    rows: Vec<PageRow>,
    tabs: Retained<NSTabViewController>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class does not implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "GhostTypeSettingsPageList"]
    #[ivars = PageListIvars]
    /// The data source and the delegate of the sidebar table: it gives the row of each page and
    /// shows the page of the picked row.
    struct PageList;

    unsafe impl NSObjectProtocol for PageList {}

    unsafe impl NSControlTextEditingDelegate for PageList {}

    unsafe impl NSTableViewDataSource for PageList {
        // SAFETY: matches the data source method signature.
        #[unsafe(method(numberOfRowsInTableView:))]
        fn number_of_rows(&self, _table: &NSTableView) -> NSInteger {
            NSInteger::try_from(self.ivars().rows.len()).expect("the sidebar never holds isize::MAX pages")
        }
    }

    unsafe impl NSTableViewDelegate for PageList {
        // SAFETY: matches the delegate method signature.
        #[unsafe(method_id(tableView:viewForTableColumn:row:))]
        fn view_for_row(
            &self,
            _table: &NSTableView,
            _column: Option<&NSTableColumn>,
            row: NSInteger,
        ) -> Option<Retained<NSView>> {
            self.row_view(row)
        }

        // SAFETY: matches the delegate method signature.
        #[unsafe(method(tableViewSelectionDidChange:))]
        fn selection_did_change(&self, notification: &NSNotification) {
            let picked = notification
                .object()
                .and_then(|object| object.downcast::<NSTableView>().ok())
                .map(|table| table.selectedRow());
            if let Some(index) = picked.filter(|&index| index >= 0) {
                let tabs = &self.ivars().tabs;
                tabs.setSelectedTabViewItemIndex(index);
                show_selected_pane_title(tabs);
            }
        }
    }
);

impl fmt::Debug for PageList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PageList")
            .field("rows", &self.ivars().rows)
            .finish_non_exhaustive()
    }
}

impl PageList {
    fn new(ivars: PageListIvars, main_thread: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(main_thread).set_ivars(ivars);
        // SAFETY: NSObject's init takes no arguments and returns an initialized object.
        unsafe { msg_send![super(this), init] }
    }

    fn row_view(&self, row: NSInteger) -> Option<Retained<NSView>> {
        let row = self.ivars().rows.get(usize::try_from(row).ok()?)?;
        Some(Retained::into_super(row.cell.clone()))
    }
}
