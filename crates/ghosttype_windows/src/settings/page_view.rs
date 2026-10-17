//! Renders a core settings page in a page window: the header and the search field, then each
//! section as a heading over cards. Each kind of row has one card. A card stays from one render to
//! the next while its row keeps its kind and control, so that the focus and the typed text stay.

use std::mem;
use std::rc::Rc;

use ghosttype_core::settings_view::{self as view, Accessory, ControlId, Row, SettingsInput};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{BN_CLICKED, EN_CHANGE};
use windows::core::Result;

use crate::settings::WindowInput;
use crate::settings::appearance::Appearance;
use crate::settings::controls::{Control, ControlKind, PickerMenu};
use crate::settings::form::{
    self, CARD_DETAIL, CARD_GAP, CARD_PADDING_X, CARD_PADDING_Y, CARD_TEXT, CONTROL_GAP, CONTROL_HEIGHT,
    HEADING_GAP, Header, PAGE_DETAIL, PAGE_TEXT, Placer, SECTION_GAP, SettingCard, enable_if, show_note,
};
use crate::settings::layout::{Bounds, Column};
use crate::settings::page::{Page, Shape};
use crate::settings::typography::TextRole;
use crate::window::WindowClass;

const PICKER_WIDTH: i32 = 280;
const SWITCH_WIDTH: i32 = 88;
const SWITCH_ON_TEXT: &str = "On";
const SWITCH_OFF_TEXT: &str = "Off";
const TEXT_AREA_LINES: i32 = 6;
const EDIT_LINE_BREAK: &str = "\r\n";
const LINE_BREAK: &str = "\n";
const OUTPUT_DETAIL_GAP: i32 = 4;
const SEARCH_GAP: i32 = 16;
const PROGRESS_WIDTH: i32 = 120;
const PROGRESS_HEIGHT: i32 = 3;

/// A page window that shows a core settings page.
#[derive(Debug)]
pub(crate) struct PageView {
    header: Header,
    search: Option<SearchView>,
    sections: Vec<SectionView>,
    page: Page,
}

impl PageView {
    /// Creates a hidden page window of `class`.
    pub(crate) fn create(class: &Rc<WindowClass>) -> Result<Self> {
        let page = Page::create(class)?;
        Ok(Self {
            header: Header::create(page.handle())?,
            search: None,
            sections: Vec::new(),
            page,
        })
    }

    pub(crate) fn page(&self) -> &Page {
        &self.page
    }

    pub(crate) fn page_mut(&mut self) -> &mut Page {
        &mut self.page
    }

    /// Shows `shown`, then places its controls.
    pub(crate) fn render(&mut self, shown: &view::Page, appearance: &Appearance) -> Result<()> {
        self.page.set_title(shown.title);
        self.header.render(shown.title, shown.summary);
        let host = self.page.handle();
        match (&shown.search, &mut self.search) {
            (None, search) => *search = None,
            (Some(field), Some(search)) => search.render(field),
            (Some(field), search @ None) => *search = Some(SearchView::create(host, field, appearance)?),
        }
        self.sections.truncate(shown.sections.len());
        while self.sections.len() < shown.sections.len() {
            self.sections.push(SectionView::create(host, appearance)?);
        }
        for (section, shown_section) in self.sections.iter_mut().zip(&shown.sections) {
            section.render(shown_section, host, appearance)?;
        }
        self.layout(appearance);
        Ok(())
    }

    /// Places every control for the page's size and scroll offset.
    pub(crate) fn layout(&mut self, appearance: &Appearance) {
        loop {
            let mut column = form::content_column(self.page.viewport(), appearance);
            let mut placer = Placer::new(self.page.offset());
            let mut shapes = Vec::new();
            self.header.layout(&mut column, &mut placer, appearance);
            if let Some(search) = &self.search {
                column.skip(appearance.px(SEARCH_GAP));
                let field = column.take(appearance.px(CONTROL_HEIGHT));
                placer.place_field(&search.field, field, appearance, &mut shapes);
            }
            for section in &self.sections {
                column.skip(appearance.px(SECTION_GAP));
                section.layout(&mut column, &mut placer, appearance, &mut shapes);
            }
            let content_height = column.top() + appearance.px(form::PAGE_PADDING);
            if !self.page.finish_layout(content_height, shapes) {
                return;
            }
        }
    }

    /// The input of a `WM_COMMAND` with `notification` from `control`, if `control` is on this
    /// page.
    pub(crate) fn input_for(&self, control: HWND, notification: u32) -> Option<WindowInput> {
        self.sections
            .iter()
            .find_map(|section| section.input_for(control, notification))
    }

    /// The input of the return key in `focus`, when `focus` is a text field of this page.
    pub(crate) fn submit_for(&self, focus: HWND) -> Option<WindowInput> {
        let search = self
            .search
            .as_ref()
            .filter(|search| search.field.handle() == focus)
            .map(SearchView::submitted);
        search.or_else(|| self.rows().find_map(|row| row.submit_for(focus)))
    }

    /// The menu that the picker `id` opens, below the picker on the screen.
    pub(crate) fn picker_menu(&self, id: &ControlId) -> Option<PickerMenu> {
        self.rows().find_map(|row| row.picker_menu(id))
    }

    pub(crate) fn apply(&self, appearance: &Appearance) {
        self.page.apply();
        for control in self.header.controls() {
            control.apply(appearance);
        }
        if let Some(search) = &self.search {
            search.field.apply(appearance);
        }
        for section in &self.sections {
            section.apply(appearance);
        }
    }

    fn rows(&self) -> impl Iterator<Item = &RowView> {
        self.sections.iter().flat_map(|section| &section.rows)
    }
}

/// The search field of a page, on the page below the header.
#[derive(Debug)]
struct SearchView {
    field: Control,
    id: ControlId,
}

impl SearchView {
    fn create(host: HWND, shown: &view::SearchField, appearance: &Appearance) -> Result<Self> {
        let mut search = Self {
            field: Control::create(host, ControlKind::TextField, TextRole::Body, PAGE_TEXT)?,
            id: shown.id.clone(),
        };
        search.field.apply(appearance);
        search.render(shown);
        Ok(search)
    }

    fn render(&mut self, shown: &view::SearchField) {
        self.field.set_placeholder(shown.placeholder);
        self.id.clone_from(&shown.id);
    }

    fn submitted(&self) -> WindowInput {
        WindowInput::Settings(SettingsInput::Submitted {
            id: self.id.clone(),
            text: self.field.typed_text(),
        })
    }
}

/// The back button and the heading, the cards and the footer of one section.
#[derive(Debug)]
struct SectionView {
    back: Control,
    heading: Control,
    footer: Control,
    shown_back: Option<view::Button>,
    has_heading: bool,
    has_footer: bool,
    rows: Vec<RowView>,
}

impl SectionView {
    fn create(host: HWND, appearance: &Appearance) -> Result<Self> {
        let section = Self {
            back: Control::create(host, ControlKind::Button, TextRole::Body, PAGE_TEXT)?,
            heading: Control::create(host, ControlKind::Label, TextRole::BodyStrong, PAGE_TEXT)?,
            footer: Control::create(host, ControlKind::Paragraph, TextRole::Caption, PAGE_DETAIL)?,
            shown_back: None,
            has_heading: false,
            has_footer: false,
            rows: Vec::new(),
        };
        section.apply(appearance);
        Ok(section)
    }

    /// Shows `shown` in the cards of its rows. A card that shows another kind of row or another
    /// control goes away, and a new card takes its place.
    fn render(&mut self, shown: &view::Section, host: HWND, appearance: &Appearance) -> Result<()> {
        self.has_heading = shown.heading.is_some();
        if let Some(heading) = &shown.heading {
            self.heading.set_text(heading);
        }
        if let Some(back) = &shown.back {
            self.back.set_text(back.label);
            enable_if(&self.back, back.is_enabled);
        }
        self.shown_back.clone_from(&shown.back);
        self.has_footer = shown.footer.is_some();
        self.footer.set_text(shown.footer.unwrap_or_default());
        let mut previous = mem::take(&mut self.rows).into_iter();
        for row in &shown.rows {
            let reused = previous
                .next()
                .and_then(|mut card| card.update(row).then_some(card));
            let card = match reused {
                Some(card) => card,
                None => RowView::create(row, host, appearance)?,
            };
            self.rows.push(card);
        }
        Ok(())
    }

    fn layout(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        self.layout_heading(column, placer, appearance);
        for (index, row) in self.rows.iter().enumerate() {
            if index > 0 {
                column.skip(appearance.px(CARD_GAP));
            }
            row.layout(column, placer, appearance, shapes);
        }
        if self.has_footer {
            column.skip(appearance.px(HEADING_GAP));
            let width = column.take(0).width;
            let height = appearance.wrapped_height(self.footer.current_text(), TextRole::Caption, width);
            placer.place(&self.footer, column.take(height));
        } else {
            self.footer.hide();
        }
    }

    /// Places the back button and the heading on one line above the cards.
    fn layout_heading(&self, column: &mut Column, placer: &mut Placer, appearance: &Appearance) {
        let heading_height = appearance.line_height(TextRole::BodyStrong);
        if self.shown_back.is_none() {
            self.back.hide();
            if !self.has_heading {
                self.heading.hide();
                return;
            }
            placer.place(&self.heading, column.take(heading_height));
            column.skip(appearance.px(HEADING_GAP));
            return;
        }
        let line = column.take(appearance.px(CONTROL_HEIGHT));
        let back_width = form::button_width(appearance, self.back.current_text());
        placer.place(
            &self.back,
            Bounds {
                width: back_width,
                ..line
            },
        );
        let gap = appearance.px(CONTROL_GAP);
        let heading = Bounds {
            x: line.x + back_width + gap,
            width: (line.width - back_width - gap).max(0),
            ..line
        };
        if self.has_heading {
            placer.place(&self.heading, heading.centered_band(heading_height));
        } else {
            self.heading.hide();
        }
        column.skip(appearance.px(HEADING_GAP));
    }

    fn input_for(&self, control: HWND, notification: u32) -> Option<WindowInput> {
        let is_back_clicked = control == self.back.handle() && notification == BN_CLICKED;
        if let Some(back) = self.shown_back.as_ref().filter(|_| is_back_clicked) {
            return Some(WindowInput::Settings(SettingsInput::Pressed(back.id.clone())));
        }
        self.rows
            .iter()
            .find_map(|row| row.input_for(control, notification))
    }

    fn apply(&self, appearance: &Appearance) {
        for control in [&self.back, &self.heading, &self.footer] {
            control.apply(appearance);
        }
        for row in &self.rows {
            row.apply(appearance);
        }
    }
}

/// The card of one row.
#[derive(Debug)]
enum RowView {
    Switch(SwitchCard),
    Picker(PickerCard),
    TextArea(TextAreaCard),
    TextField(TextFieldCard),
    Note(NoteCard),
    Output(OutputCard),
    Item(ItemCard),
    Message(MessageCard),
}

impl RowView {
    /// Creates the controls of a card that shows `row`.
    fn create(row: &Row, host: HWND, appearance: &Appearance) -> Result<Self> {
        let mut card = match row {
            Row::Switch(switch) => Self::Switch(SwitchCard::create(host, switch)?),
            Row::Picker(picker) => Self::Picker(PickerCard::create(host, picker)?),
            Row::TextArea(text_area) => Self::TextArea(TextAreaCard::create(host, text_area)?),
            Row::TextField(text_field) => Self::TextField(TextFieldCard::create(host, text_field)?),
            Row::Note(_) => Self::Note(NoteCard::create(host)?),
            Row::Output(_) => Self::Output(OutputCard::create(host)?),
            Row::Item(item) => Self::Item(ItemCard::create(host, item)?),
            Row::Loading(_) | Row::Message(_) => Self::Message(MessageCard::create(host)?),
        };
        card.update(row);
        card.apply(appearance);
        Ok(card)
    }

    /// Shows `row`, when it is of the card's kind and for the card's control. Returns whether it
    /// did.
    fn update(&mut self, row: &Row) -> bool {
        match (self, row) {
            (Self::Switch(card), Row::Switch(switch)) if card.shown.id == switch.id => card.render(switch),
            (Self::Picker(card), Row::Picker(picker)) if card.shown.id == picker.id => card.render(picker),
            (Self::TextArea(card), Row::TextArea(text_area)) if card.shown.id == text_area.id => {
                card.render(text_area);
            }
            (Self::TextField(card), Row::TextField(text_field)) if card.shown.id == text_field.id => {
                card.render(text_field);
            }
            (Self::Note(card), Row::Note(note)) => card.render(note),
            (Self::Output(card), Row::Output(output)) => card.render(output),
            (Self::Item(card), Row::Item(item)) => card.render(item),
            (Self::Message(card), Row::Loading(text)) => card.render(text, None),
            (Self::Message(card), Row::Message(message)) => {
                card.render(&message.text, message.retry.as_ref());
            }
            _ => return false,
        }
        true
    }

    fn layout(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        match self {
            Self::Switch(card) => card.layout(column, placer, appearance, shapes),
            Self::Picker(card) => card.layout(column, placer, appearance, shapes),
            Self::TextArea(card) => card.layout(column, placer, appearance, shapes),
            Self::TextField(card) => card.layout(column, placer, appearance, shapes),
            Self::Note(card) => TextCard {
                text: &card.text,
                detail: None,
            }
            .layout(column, placer, appearance, shapes),
            Self::Output(card) => TextCard {
                text: &card.text,
                detail: Some(&card.detail),
            }
            .layout(column, placer, appearance, shapes),
            Self::Item(card) => card.layout(column, placer, appearance, shapes),
            Self::Message(card) => card.layout(column, placer, appearance, shapes),
        }
    }

    fn input_for(&self, control: HWND, notification: u32) -> Option<WindowInput> {
        match (self, notification) {
            (Self::TextArea(card), EN_CHANGE) => card.edited(control),
            (Self::Switch(card), BN_CLICKED) => card.clicked(control),
            (Self::Picker(card), BN_CLICKED) => card.clicked(control),
            (Self::TextArea(card), BN_CLICKED) => card.clicked(control),
            (Self::TextField(card), BN_CLICKED) => card.clicked(control),
            (Self::Item(card), BN_CLICKED) => card.clicked(control),
            (Self::Message(card), BN_CLICKED) => card.clicked(control),
            _ => None,
        }
    }

    fn picker_menu(&self, id: &ControlId) -> Option<PickerMenu> {
        match self {
            Self::Picker(card) => (card.shown.id == *id).then(|| card.picker.picker_menu()),
            _ => None,
        }
    }

    fn submit_for(&self, focus: HWND) -> Option<WindowInput> {
        match self {
            Self::TextField(card) => card.submit_for(focus),
            _ => None,
        }
    }

    fn apply(&self, appearance: &Appearance) {
        let controls: &[&Control] = match self {
            Self::Switch(card) => &card.controls(),
            Self::Picker(card) => &card.controls(),
            Self::TextArea(card) => &card.controls(),
            Self::TextField(card) => &card.controls(),
            Self::Note(card) => &[&card.text],
            Self::Output(card) => &[&card.text, &card.detail],
            Self::Item(card) => &card.controls(),
            Self::Message(card) => &[&card.text, &card.button],
        };
        for control in controls {
            control.apply(appearance);
        }
    }
}

/// A card with a label, a description and a switch that turns the setting on and off.
#[derive(Debug)]
struct SwitchCard {
    label: Control,
    description: Control,
    switch: Control,
    shown: view::Switch,
}

impl SwitchCard {
    fn create(host: HWND, shown: &view::Switch) -> Result<Self> {
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            switch: control(ControlKind::Switch, TextRole::Body, CARD_TEXT)?,
            shown: shown.clone(),
        })
    }

    fn render(&mut self, shown: &view::Switch) {
        self.label.set_text(shown.label);
        self.description.set_text(shown.description);
        if shown.is_on {
            self.switch.set_text(SWITCH_ON_TEXT);
            self.switch.mark_selected();
        } else {
            self.switch.set_text(SWITCH_OFF_TEXT);
            self.switch.mark_unselected();
        }
        self.shown.clone_from(shown);
    }

    fn layout(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let card = SettingCard {
            label: &self.label,
            details: &[&self.description],
            control: &self.switch,
            control_width: SWITCH_WIDTH,
        };
        card.layout(column, placer, appearance, shapes);
    }

    fn clicked(&self, control: HWND) -> Option<WindowInput> {
        (control == self.switch.handle()).then(|| {
            WindowInput::Settings(SettingsInput::Toggled {
                id: self.shown.id.clone(),
                is_on: !self.shown.is_on,
            })
        })
    }

    fn controls(&self) -> [&Control; 3] {
        [&self.label, &self.description, &self.switch]
    }
}

/// A card with a label, a description, a note when there is one, and a picker that opens a menu
/// of the setting's values.
#[derive(Debug)]
struct PickerCard {
    label: Control,
    description: Control,
    note: Control,
    picker: Control,
    shown: view::Picker,
}

impl PickerCard {
    fn create(host: HWND, shown: &view::Picker) -> Result<Self> {
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            note: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            picker: control(ControlKind::Picker, TextRole::Body, CARD_TEXT)?,
            shown: shown.clone(),
        })
    }

    fn render(&mut self, shown: &view::Picker) {
        self.label.set_text(shown.label);
        self.description.set_text(shown.description);
        self.picker.set_choices(&shown.choices, shown.selected);
        enable_if(&self.picker, shown.is_enabled);
        if let Some(note) = &shown.note {
            show_note(&mut self.note, note);
        }
        self.shown.clone_from(shown);
    }

    fn layout(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let has_description = !self.shown.description.is_empty();
        let has_note = self.shown.note.is_some();
        if !has_description {
            self.description.hide();
        }
        if !has_note {
            self.note.hide();
        }
        let description = has_description.then_some(&self.description);
        let note = has_note.then_some(&self.note);
        let details: Vec<&Control> = description.into_iter().chain(note).collect();
        let card = SettingCard {
            label: &self.label,
            details: &details,
            control: &self.picker,
            control_width: PICKER_WIDTH,
        };
        card.layout(column, placer, appearance, shapes);
    }

    fn clicked(&self, control: HWND) -> Option<WindowInput> {
        (control == self.picker.handle()).then(|| WindowInput::OpenMenu(self.shown.id.clone()))
    }

    fn controls(&self) -> [&Control; 4] {
        [&self.label, &self.description, &self.note, &self.picker]
    }
}

/// A card with a label, a description, a field of several lines, and a button below the field.
#[derive(Debug)]
struct TextAreaCard {
    label: Control,
    description: Control,
    field: Control,
    button: Control,
    shown: view::TextArea,
}

impl TextAreaCard {
    fn create(host: HWND, shown: &view::TextArea) -> Result<Self> {
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            field: control(ControlKind::TextArea, TextRole::Body, CARD_TEXT)?,
            button: control(ControlKind::Button, TextRole::Body, CARD_TEXT)?,
            shown: shown.clone(),
        })
    }

    fn render(&mut self, shown: &view::TextArea) {
        self.label.set_text(shown.label);
        self.description.set_text(shown.description);
        self.field
            .replace_typed_text(&shown.text.replace(LINE_BREAK, EDIT_LINE_BREAK));
        if let Some(button) = &shown.button {
            self.button.set_text(button.label);
            enable_if(&self.button, button.is_enabled);
        }
        self.shown.clone_from(shown);
    }

    /// Places the label and description, the field, then the button.
    fn layout(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let padding_x = appearance.px(CARD_PADDING_X);
        let padding_y = appearance.px(CARD_PADDING_Y);
        let gap = appearance.px(CONTROL_GAP);
        let width = column.inner_width(padding_x);
        let label_height = appearance.line_height(TextRole::Body);
        let description_height =
            appearance.wrapped_height(self.description.current_text(), TextRole::Caption, width);
        let field_height = form::text_area_height(TEXT_AREA_LINES, appearance);
        let has_button = self.shown.button.is_some();
        let button_height = if has_button {
            gap + appearance.px(CONTROL_HEIGHT)
        } else {
            0
        };
        let card = column
            .take(label_height + description_height + gap + field_height + button_height + 2 * padding_y);
        shapes.push(Shape::Card(card));
        let mut inner = Column::new(card.x + padding_x, card.y + padding_y, width);
        let label = inner.take(label_height);
        let description = inner.take(description_height);
        inner.skip(gap);
        let field = inner.take(field_height);
        // The label goes right before the field in z-order, so it names the field for UI Automation.
        placer.place(&self.description, description);
        placer.place(&self.label, label);
        placer.place_text_area(&self.field, field, appearance, shapes);
        if has_button {
            inner.skip(gap);
            let button_width = form::button_width(appearance, self.button.current_text());
            let (_, button) = inner
                .take(appearance.px(CONTROL_HEIGHT))
                .split_trailing(button_width, 0);
            placer.place(&self.button, button);
        } else {
            self.button.hide();
        }
    }

    fn edited(&self, control: HWND) -> Option<WindowInput> {
        (control == self.field.handle()).then(|| {
            WindowInput::Settings(SettingsInput::Edited {
                id: self.shown.id.clone(),
                text: self.field.typed_text().replace(EDIT_LINE_BREAK, LINE_BREAK),
            })
        })
    }

    fn clicked(&self, control: HWND) -> Option<WindowInput> {
        let button = self.shown.button.as_ref()?;
        (control == self.button.handle())
            .then(|| WindowInput::Settings(SettingsInput::Pressed(button.id.clone())))
    }

    fn controls(&self) -> [&Control; 4] {
        [&self.label, &self.description, &self.field, &self.button]
    }
}

/// A label above a card that holds a field of one line and the button that submits its text.
#[derive(Debug)]
struct TextFieldCard {
    label: Control,
    field: Control,
    button: Control,
    shown: view::TextField,
}

impl TextFieldCard {
    fn create(host: HWND, shown: &view::TextField) -> Result<Self> {
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            label: control(ControlKind::Label, TextRole::BodyStrong, PAGE_TEXT)?,
            field: control(ControlKind::TextField, TextRole::Body, CARD_TEXT)?,
            button: control(ControlKind::PrimaryButton, TextRole::Body, CARD_TEXT)?,
            shown: shown.clone(),
        })
    }

    fn render(&mut self, shown: &view::TextField) {
        self.label.set_text(shown.label);
        self.field.set_placeholder(shown.placeholder);
        self.button.set_text(shown.submit_label);
        enable_if(&self.button, shown.is_submit_enabled);
        self.shown.clone_from(shown);
    }

    /// Places the label, then the card with the field and the button. The label goes right
    /// before the field in z-order, so it names the field for UI Automation.
    fn layout(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        placer.place(
            &self.label,
            column.take(appearance.line_height(TextRole::BodyStrong)),
        );
        column.skip(appearance.px(HEADING_GAP));
        let card = column.take(form::card_height(appearance.px(CONTROL_HEIGHT), appearance));
        shapes.push(Shape::Card(card));
        let inner = card.inset(appearance.px(CARD_PADDING_X), 0);
        let button_width = form::button_width(appearance, self.button.current_text());
        let (field, button) = inner.split_trailing(button_width, appearance.px(CONTROL_GAP));
        let band = appearance.px(CONTROL_HEIGHT);
        placer.place_field(&self.field, field.centered_band(band), appearance, shapes);
        placer.place(&self.button, button.centered_band(band));
    }

    fn clicked(&self, control: HWND) -> Option<WindowInput> {
        (control == self.button.handle()).then(|| self.submitted())
    }

    fn submit_for(&self, focus: HWND) -> Option<WindowInput> {
        (focus == self.field.handle()).then(|| self.submitted())
    }

    fn submitted(&self) -> WindowInput {
        WindowInput::Settings(SettingsInput::Submitted {
            id: self.shown.id.clone(),
            text: self.field.typed_text(),
        })
    }

    fn controls(&self) -> [&Control; 3] {
        [&self.label, &self.field, &self.button]
    }
}

/// A card with a line of status in the color of its tone.
#[derive(Debug)]
struct NoteCard {
    text: Control,
}

impl NoteCard {
    fn create(host: HWND) -> Result<Self> {
        Ok(Self {
            text: Control::create(host, ControlKind::Paragraph, TextRole::Body, CARD_TEXT)?,
        })
    }

    fn render(&mut self, shown: &view::Note) {
        show_note(&mut self.text, shown);
    }
}

/// A card with the text that a model wrote, and the facts about it below the text.
#[derive(Debug)]
struct OutputCard {
    text: Control,
    detail: Control,
}

impl OutputCard {
    fn create(host: HWND) -> Result<Self> {
        Ok(Self {
            text: Control::create(host, ControlKind::Paragraph, TextRole::Body, CARD_TEXT)?,
            detail: Control::create(host, ControlKind::Label, TextRole::Caption, CARD_DETAIL)?,
        })
    }

    fn render(&mut self, shown: &view::Output) {
        self.text.set_text(&shown.text);
        self.detail.set_text(&shown.detail);
    }
}

/// A card with a paragraph of text, and a line of detail below it when there is one.
#[derive(Debug, Clone, Copy)]
struct TextCard<'a> {
    text: &'a Control,
    detail: Option<&'a Control>,
}

impl TextCard<'_> {
    /// Places the card's controls in a card that it takes from `column`, and adds the card to
    /// `shapes`.
    fn layout(
        self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let padding_x = appearance.px(CARD_PADDING_X);
        let padding_y = appearance.px(CARD_PADDING_Y);
        let text_width = column.inner_width(padding_x);
        let text_height = appearance.wrapped_height(self.text.current_text(), TextRole::Body, text_width);
        let detail_height = self.detail.map_or(0, |_| {
            appearance.px(OUTPUT_DETAIL_GAP) + appearance.line_height(TextRole::Caption)
        });
        let card = column.take(text_height + detail_height + 2 * padding_y);
        shapes.push(Shape::Card(card));
        let mut inner = Column::new(card.x + padding_x, card.y + padding_y, text_width);
        placer.place(self.text, inner.take(text_height));
        if let Some(detail) = self.detail {
            inner.skip(appearance.px(OUTPUT_DETAIL_GAP));
            placer.place(detail, inner.take(appearance.line_height(TextRole::Caption)));
        }
    }
}

/// A card of one thing of a list: its title and subtitle, its facts, and the progress bar and the
/// button of its accessory.
#[derive(Debug)]
struct ItemCard {
    title: Control,
    subtitle: Control,
    detail: Control,
    progress: Control,
    button: Control,
    shown: view::Item,
}

impl ItemCard {
    fn create(host: HWND, shown: &view::Item) -> Result<Self> {
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            title: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            subtitle: control(ControlKind::Label, TextRole::Caption, CARD_DETAIL)?,
            detail: control(ControlKind::TrailingLabel, TextRole::Caption, CARD_DETAIL)?,
            progress: control(ControlKind::ProgressBar, TextRole::Caption, CARD_DETAIL)?,
            button: control(ControlKind::Button, TextRole::Body, CARD_TEXT)?,
            shown: shown.clone(),
        })
    }

    fn render(&mut self, shown: &view::Item) {
        self.title.set_text(&shown.title);
        self.subtitle.set_text(&shown.subtitle);
        self.detail.set_text(&shown.detail);
        self.progress.set_text(&shown.title);
        if let Accessory::Downloading { progress, .. } = shown.accessory {
            self.progress.set_progress(progress.0);
        }
        if let Some(button) = accessory_button(&shown.accessory) {
            self.button.set_text(button.label);
        }
        self.shown.clone_from(shown);
    }

    fn layout(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let body_line = appearance.line_height(TextRole::Body);
        let caption_line = appearance.line_height(TextRole::Caption);
        let has_subtitle = !self.shown.subtitle.is_empty();
        let text_height = if has_subtitle {
            body_line + caption_line
        } else {
            body_line
        };
        let card = column.take(form::card_height(text_height, appearance));
        shapes.push(Shape::Card(card));
        let gap = appearance.px(CONTROL_GAP);
        let inner = card.inset(appearance.px(CARD_PADDING_X), 0);
        let (rest, accessory) = inner.split_trailing(self.accessory_width(appearance), gap);
        let (text_area, detail_area) = if self.shown.detail.is_empty() {
            (rest, None)
        } else {
            let detail_width = appearance.text_width(&self.shown.detail, TextRole::Caption);
            let (text, detail) = rest.split_trailing(detail_width, gap);
            (text, Some(detail))
        };
        let text_block = text_area.centered_band(text_height);
        placer.place(
            &self.title,
            Bounds {
                height: body_line,
                ..text_block
            },
        );
        if has_subtitle {
            let subtitle = Bounds {
                y: text_block.y + body_line,
                height: caption_line,
                ..text_block
            };
            placer.place(&self.subtitle, subtitle);
        } else {
            self.subtitle.hide();
        }
        match detail_area {
            Some(detail) => placer.place(&self.detail, detail.centered_band(caption_line)),
            None => self.detail.hide(),
        }
        self.layout_accessory(accessory, placer, appearance);
    }

    fn layout_accessory(&self, accessory: Bounds, placer: &mut Placer, appearance: &Appearance) {
        let button_band = accessory.centered_band(appearance.px(CONTROL_HEIGHT));
        match &self.shown.accessory {
            Accessory::Downloading { .. } => {
                let progress_width = appearance.px(PROGRESS_WIDTH);
                let progress = Bounds {
                    width: progress_width,
                    ..accessory.centered_band(appearance.px(PROGRESS_HEIGHT))
                };
                placer.place(&self.progress, progress);
                let gap = appearance.px(CONTROL_GAP);
                let button = Bounds {
                    x: button_band.x + progress_width + gap,
                    width: (button_band.width - progress_width - gap).max(0),
                    ..button_band
                };
                placer.place(&self.button, button);
            }
            Accessory::Canceling | Accessory::Unavailable => {
                self.progress.hide();
                self.button.hide();
            }
            Accessory::Download(_)
            | Accessory::Open(_)
            | Accessory::Installed { .. }
            | Accessory::Failed { .. } => {
                self.progress.hide();
                placer.place(&self.button, button_band);
            }
        }
    }

    fn accessory_width(&self, appearance: &Appearance) -> i32 {
        let button = form::button_width(appearance, self.button.current_text());
        match &self.shown.accessory {
            Accessory::Downloading { .. } => {
                appearance.px(PROGRESS_WIDTH) + appearance.px(CONTROL_GAP) + button
            }
            Accessory::Canceling | Accessory::Unavailable => 0,
            Accessory::Download(_)
            | Accessory::Open(_)
            | Accessory::Installed { .. }
            | Accessory::Failed { .. } => button,
        }
    }

    fn clicked(&self, control: HWND) -> Option<WindowInput> {
        let button = accessory_button(&self.shown.accessory)?;
        (control == self.button.handle())
            .then(|| WindowInput::Settings(SettingsInput::Pressed(button.id.clone())))
    }

    fn controls(&self) -> [&Control; 5] {
        [
            &self.title,
            &self.subtitle,
            &self.detail,
            &self.progress,
            &self.button,
        ]
    }
}

/// A card that tells why a section has no rows, or that its rows are on their way, with a button
/// that tries again when trying again can help.
#[derive(Debug)]
struct MessageCard {
    text: Control,
    button: Control,
    retry: Option<view::Button>,
}

impl MessageCard {
    fn create(host: HWND) -> Result<Self> {
        Ok(Self {
            text: Control::create(host, ControlKind::Paragraph, TextRole::Body, CARD_TEXT)?,
            button: Control::create(host, ControlKind::Button, TextRole::Body, CARD_TEXT)?,
            retry: None,
        })
    }

    fn render(&mut self, text: &str, retry: Option<&view::Button>) {
        self.text.set_text(text);
        if let Some(retry) = retry {
            self.button.set_text(retry.label);
        }
        self.retry = retry.cloned();
    }

    fn layout(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let width = column.inner_width(appearance.px(CARD_PADDING_X));
        let button_width = self
            .retry
            .as_ref()
            .map_or(0, |_| form::button_width(appearance, self.button.current_text()));
        let gap = if button_width > 0 {
            appearance.px(CONTROL_GAP)
        } else {
            0
        };
        let text_width = width - button_width - gap;
        let text_height = appearance.wrapped_height(self.text.current_text(), TextRole::Body, text_width);
        let content_height = if button_width > 0 {
            text_height.max(appearance.px(CONTROL_HEIGHT))
        } else {
            text_height
        };
        let card = column.take(content_height + 2 * appearance.px(CARD_PADDING_Y));
        shapes.push(Shape::Card(card));
        let inner = card.inset(appearance.px(CARD_PADDING_X), appearance.px(CARD_PADDING_Y));
        let (text, button) = inner.split_trailing(button_width, gap);
        placer.place(&self.text, text.centered_band(text_height));
        if button_width > 0 {
            placer.place(&self.button, button.centered_band(appearance.px(CONTROL_HEIGHT)));
        } else {
            self.button.hide();
        }
    }

    fn clicked(&self, control: HWND) -> Option<WindowInput> {
        let retry = self.retry.as_ref()?;
        (control == self.button.handle())
            .then(|| WindowInput::Settings(SettingsInput::Pressed(retry.id.clone())))
    }
}

/// The button of `accessory`, if it shows one.
fn accessory_button(accessory: &Accessory) -> Option<&view::Button> {
    match accessory {
        Accessory::Download(button)
        | Accessory::Open(button)
        | Accessory::Downloading { cancel: button, .. }
        | Accessory::Installed { delete: button }
        | Accessory::Failed { retry: button } => Some(button),
        Accessory::Canceling | Accessory::Unavailable => None,
    }
}
