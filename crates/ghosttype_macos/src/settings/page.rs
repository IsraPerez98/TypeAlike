//! A settings page, rendered from its core tree: the summary and the search field, then each
//! section under its heading. Switches, menus, lines of text, items and model output sit in
//! rounded groups like the grouped lists of System Settings. Text areas, text fields and status
//! notes sit between the groups.
//!
//! A page with a search field pins the summary and the search field above the sections, which
//! scroll.

use ghosttype_core::settings_window::view::{self as view, ControlId, Row, SettingsInput};
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSButton, NSCellImagePosition, NSControlSize, NSFont, NSSearchField, NSStackView, NSTextField,
    NSUserInterfaceLayoutOrientation, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSEdgeInsets, NSString};

use crate::settings::controls::{
    heading, icon_button, secondary_wrapping_label, set_text, show_back_button, small_secondary_label,
};
use crate::settings::layout::{
    ACCESSORY_SPACING, LIST_PART_SPACING, NO_INSETS, fill_width, pinned_root, rounded_group, scroll_view,
    stack,
};
use crate::settings::rows::RowView;
use crate::settings::{ActionTarget, Binder, ControlAction};

const CONTENT_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 20.0,
    left: 24.0,
    bottom: 24.0,
    right: 24.0,
};
const SUMMARY_FONT_SIZE: CGFloat = 12.0;
const SECTION_SPACING: CGFloat = 28.0;
const HEADER_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 20.0,
    left: 24.0,
    bottom: 16.0,
    right: 24.0,
};
const HEADER_SPACING: CGFloat = 4.0;
const SEARCH_FIELD_TOP_SPACING: CGFloat = 14.0;

/// The views of a settings page, kept to update them on each render. The page keeps the search
/// field that it was made with.
#[derive(Debug)]
pub(super) struct PageView {
    pub(super) root: Retained<NSView>,
    content: Retained<NSStackView>,
    summary: Retained<NSTextField>,
    search: Option<SearchView>,
    sections: Vec<SectionView>,
}

impl PageView {
    /// Makes the views of `page`.
    pub(super) fn new(page: &view::Page, target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let content = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            SECTION_SPACING,
            CONTENT_INSETS,
            main_thread,
        );
        let summary = secondary_wrapping_label(main_thread);
        summary.setFont(Some(&NSFont::systemFontOfSize(SUMMARY_FONT_SIZE)));
        let (root, search) = if page.search.is_some() {
            let search = SearchView::new(target, main_thread);
            let header = stack(
                NSUserInterfaceLayoutOrientation::Vertical,
                HEADER_SPACING,
                HEADER_INSETS,
                main_thread,
            );
            let header_views: [&NSView; 2] = [&summary, &search.field];
            for view in header_views {
                header.addArrangedSubview(view);
                fill_width(view, &header);
            }
            header.setCustomSpacing_afterView(SEARCH_FIELD_TOP_SPACING, &summary);
            let root = pinned_root(&header, &content, main_thread);
            (Retained::into_super(root), Some(search))
        } else {
            content.addArrangedSubview(&summary);
            fill_width(&summary, &content);
            (Retained::into_super(scroll_view(&content, main_thread)), None)
        };
        Self {
            root,
            content,
            summary,
            search,
            sections: Vec::new(),
        }
    }

    /// Shows `page`. A section keeps its views while its rows keep their kinds and controls, so
    /// that an open menu, the focus, the typed text and a click in progress stay.
    pub(super) fn render(&mut self, page: &view::Page, binder: &mut Binder<'_>) {
        set_text(&self.summary, page.summary);
        if let (Some(search), Some(field)) = (&mut self.search, &page.search) {
            search.render(field, binder);
        }
        let kept = page.sections.len().min(self.sections.len());
        for section in self.sections.drain(kept..) {
            section.container.removeFromSuperview();
        }
        while self.sections.len() < page.sections.len() {
            self.sections.push(SectionView::new(&self.content, binder));
        }
        for (section, shown) in self.sections.iter_mut().zip(&page.sections) {
            section.render(shown, binder);
        }
    }

    /// The input of a change of the text in `view`, when it is a text area or a line of text of
    /// the page.
    pub(super) fn edited(&self, view: &AnyObject) -> Option<SettingsInput> {
        self.rows().find_map(|row| row.edited(view))
    }

    /// The text in the text field or the search field `id`, when it is on the page.
    pub(super) fn field_text(&self, id: &ControlId) -> Option<String> {
        let search = self
            .search
            .as_ref()
            .filter(|search| search.id.as_ref() == Some(id))
            .map(|search| search.field.stringValue().to_string());
        search.or_else(|| self.rows().find_map(|row| row.field_text(id)))
    }

    fn rows(&self) -> impl Iterator<Item = &RowView> {
        self.sections.iter().flat_map(|section| &section.rows)
    }
}

/// The search field of a page, which sends the submit action when the user submits or clears it.
#[derive(Debug)]
struct SearchView {
    field: Retained<NSSearchField>,
    id: Option<ControlId>,
}

impl SearchView {
    fn new(target: &ActionTarget, main_thread: MainThreadMarker) -> Self {
        let field = NSSearchField::new(main_thread);
        field.setControlSize(NSControlSize::Large);
        field.setSendsWholeSearchString(true);
        target.connect(&field, ControlAction::Submit);
        Self { field, id: None }
    }

    fn render(&mut self, shown: &view::SearchField, binder: &mut Binder<'_>) {
        self.field
            .setPlaceholderString(Some(&NSString::from_str(shown.placeholder)));
        binder.bind(&self.field, &shown.id);
        self.id = Some(shown.id.clone());
    }
}

/// The views of one section: the back button and the heading, the rows, then the footer.
#[derive(Debug)]
struct SectionView {
    container: Retained<NSStackView>,
    title_bar: Retained<NSStackView>,
    back: Retained<NSButton>,
    heading: Retained<NSTextField>,
    rows_container: Retained<NSStackView>,
    footer: Retained<NSTextField>,
    rows: Vec<RowView>,
}

impl SectionView {
    fn new(content: &NSStackView, binder: &Binder<'_>) -> Self {
        let main_thread = binder.main_thread;
        let back = icon_button(binder.target, main_thread);
        back.setImagePosition(NSCellImagePosition::ImageLeading);
        let heading = heading(main_thread);
        let title_bar = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            ACCESSORY_SPACING,
            NO_INSETS,
            main_thread,
        );
        title_bar.addArrangedSubview(&back);
        title_bar.addArrangedSubview(&heading);
        let rows_container = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            NO_INSETS,
            main_thread,
        );
        let footer = small_secondary_label(main_thread);
        let container = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            LIST_PART_SPACING,
            NO_INSETS,
            main_thread,
        );
        let parts: [&NSView; 3] = [&title_bar, &rows_container, &footer];
        for part in parts {
            container.addArrangedSubview(part);
            fill_width(part, &container);
        }
        content.addArrangedSubview(&container);
        fill_width(&container, content);
        Self {
            container,
            title_bar,
            back,
            heading,
            rows_container,
            footer,
            rows: Vec::new(),
        }
    }

    fn render(&mut self, section: &view::Section, binder: &mut Binder<'_>) {
        set_text(&self.heading, section.heading.as_deref().unwrap_or_default());
        match &section.back {
            Some(back) => show_back_button(&self.back, back, binder),
            None => self.back.setHidden(true),
        }
        self.title_bar
            .setHidden(section.heading.is_none() && section.back.is_none());
        set_text(&self.footer, section.footer.unwrap_or_default());
        let has_same_rows = self.rows.len() == section.rows.len()
            && self
                .rows
                .iter()
                .zip(&section.rows)
                .all(|(row_view, row)| row_view.shows(row));
        if !has_same_rows {
            self.rebuild(&section.rows, binder);
        }
        for (row_view, row) in self.rows.iter_mut().zip(&section.rows) {
            row_view.render(row, binder);
        }
    }

    /// Makes new views for `rows`. Grouped rows that follow each other share a rounded group.
    fn rebuild(&mut self, rows: &[Row], binder: &Binder<'_>) {
        for part in self.rows_container.arrangedSubviews().to_vec() {
            part.removeFromSuperview();
        }
        self.rows.clear();
        let mut group = None;
        for row in rows {
            let container: &NSStackView = if RowView::is_grouped(row) {
                group.get_or_insert_with(|| self.add_group(binder.main_thread))
            } else {
                group = None;
                &self.rows_container
            };
            let row_view = RowView::add(row, container, binder);
            self.rows.push(row_view);
        }
    }

    /// Adds a rounded group below the rows, and returns the stack of its rows.
    fn add_group(&self, main_thread: MainThreadMarker) -> Retained<NSStackView> {
        let rows_stack = stack(
            NSUserInterfaceLayoutOrientation::Vertical,
            0.0,
            NO_INSETS,
            main_thread,
        );
        let group = rounded_group(&rows_stack, main_thread);
        self.rows_container.addArrangedSubview(&group);
        fill_width(&group, &self.rows_container);
        rows_stack
    }
}
