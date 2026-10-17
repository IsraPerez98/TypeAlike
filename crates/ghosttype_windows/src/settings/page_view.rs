//! Renders a core settings page in a page window: the header, then each section of rows as cards.
//! Each kind of row has one card. A card stays from one render to the next while its row keeps
//! its kind and control, so that the focus and the typed text stay.

use std::mem;

use ghosttype_core::settings_view::{self as view, ControlId, Row, SettingsInput};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{BN_CLICKED, EN_CHANGE, WNDPROC};
use windows::core::{PCWSTR, Result};

use crate::settings::appearance::Appearance;
use crate::settings::controls::{Control, ControlKind, PickerMenu};
use crate::settings::form::{
    self, CARD_DETAIL, CARD_GAP, CARD_PADDING_X, CARD_PADDING_Y, CARD_TEXT, CONTROL_GAP, CONTROL_HEIGHT,
    Header, Placer, SECTION_GAP, SettingCard, enable_if, show_note,
};
use crate::settings::layout::Column;
use crate::settings::page::{Page, Shape};
use crate::settings::typography::TextRole;
use crate::settings::{MenuPicker, WindowInput};

const PICKER_WIDTH: i32 = 280;
const SWITCH_WIDTH: i32 = 88;
const SWITCH_ON_TEXT: &str = "On";
const SWITCH_OFF_TEXT: &str = "Off";
const TEXT_AREA_LINES: i32 = 6;
const EDIT_LINE_BREAK: &str = "\r\n";
const LINE_BREAK: &str = "\n";

/// A page window that shows a core settings page.
#[derive(Debug)]
pub(crate) struct PageView {
    header: Header,
    sections: Vec<SectionView>,
    page: Page,
}

impl PageView {
    pub(crate) fn create(class_name: PCWSTR, procedure: WNDPROC, parent: HWND) -> Result<Self> {
        let page = Page::create(class_name, procedure, parent)?;
        Ok(Self {
            header: Header::create(page.handle())?,
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
        self.sections
            .resize_with(shown.sections.len(), SectionView::default);
        let host = self.page.handle();
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
        self.rows().find_map(|row| row.input_for(control, notification))
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
        for row in self.rows() {
            row.apply(appearance);
        }
    }

    fn rows(&self) -> impl Iterator<Item = &RowView> {
        self.sections.iter().flat_map(|section| &section.rows)
    }
}

/// The cards of one section.
#[derive(Debug, Default)]
struct SectionView {
    rows: Vec<RowView>,
}

impl SectionView {
    /// Shows `shown` in the cards of its rows. A card that shows another kind of row or another
    /// control goes away, and a new card takes its place.
    fn render(&mut self, shown: &view::Section, host: HWND, appearance: &Appearance) -> Result<()> {
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
        for (index, row) in self.rows.iter().enumerate() {
            if index > 0 {
                column.skip(appearance.px(CARD_GAP));
            }
            row.layout(column, placer, appearance, shapes);
        }
    }
}

/// The card of one row.
#[derive(Debug)]
enum RowView {
    Switch(SwitchCard),
    Picker(PickerCard),
    TextArea(TextAreaCard),
}

impl RowView {
    /// Creates the controls of a card that shows `row`.
    fn create(row: &Row, host: HWND, appearance: &Appearance) -> Result<Self> {
        let card = match row {
            Row::Switch(switch) => Self::Switch(SwitchCard::create(host, switch)?),
            Row::Picker(picker) => Self::Picker(PickerCard::create(host, picker)?),
            Row::TextArea(text_area) => Self::TextArea(TextAreaCard::create(host, text_area)?),
        };
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
        }
    }

    fn input_for(&self, control: HWND, notification: u32) -> Option<WindowInput> {
        match self {
            Self::Switch(card) => card.input_for(control, notification),
            Self::Picker(card) => card.input_for(control, notification),
            Self::TextArea(card) => card.input_for(control, notification),
        }
    }

    fn picker_menu(&self, id: &ControlId) -> Option<PickerMenu> {
        match self {
            Self::Picker(card) => (card.shown.id == *id).then(|| card.picker.picker_menu()),
            Self::Switch(_) | Self::TextArea(_) => None,
        }
    }

    fn apply(&self, appearance: &Appearance) {
        let controls: &[&Control] = match self {
            Self::Switch(card) => &card.controls(),
            Self::Picker(card) => &card.controls(),
            Self::TextArea(card) => &card.controls(),
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
        let mut card = Self {
            label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            switch: control(ControlKind::Switch, TextRole::Body, CARD_TEXT)?,
            shown: shown.clone(),
        };
        card.render(shown);
        Ok(card)
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

    fn input_for(&self, control: HWND, notification: u32) -> Option<WindowInput> {
        let is_clicked = control == self.switch.handle() && notification == BN_CLICKED;
        is_clicked.then(|| {
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
        let mut card = Self {
            label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            note: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            picker: control(ControlKind::Picker, TextRole::Body, CARD_TEXT)?,
            shown: shown.clone(),
        };
        card.render(shown);
        Ok(card)
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

    fn input_for(&self, control: HWND, notification: u32) -> Option<WindowInput> {
        let is_clicked = control == self.picker.handle() && notification == BN_CLICKED;
        is_clicked.then(|| WindowInput::OpenMenu(MenuPicker::Control(self.shown.id.clone())))
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
        let mut card = Self {
            label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            field: control(ControlKind::TextArea, TextRole::Body, CARD_TEXT)?,
            button: control(ControlKind::Button, TextRole::Body, CARD_TEXT)?,
            shown: shown.clone(),
        };
        card.render(shown);
        Ok(card)
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

    fn input_for(&self, control: HWND, notification: u32) -> Option<WindowInput> {
        if control == self.field.handle() && notification == EN_CHANGE {
            let text = self.field.typed_text().replace(EDIT_LINE_BREAK, LINE_BREAK);
            return Some(WindowInput::Settings(SettingsInput::Edited {
                id: self.shown.id.clone(),
                text,
            }));
        }
        let button = self.shown.button.as_ref()?;
        let is_clicked = control == self.button.handle() && notification == BN_CLICKED;
        is_clicked.then(|| WindowInput::Settings(SettingsInput::Pressed(button.id.clone())))
    }

    fn controls(&self) -> [&Control; 4] {
        [&self.label, &self.description, &self.field, &self.button]
    }
}
