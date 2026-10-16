//! The General pane: renders the core [`GeneralView`] and reports the clipboard access, the prompt
//! preparation and the backend the user picks, and the system prompt the user writes.

use ghosttype_core::general_pane::{
    BackendChoice, BackendPicker, GeneralAction, GeneralView, SettingSwitch, SystemPromptEditor,
};
use ghosttype_core::inference::SystemPrompt;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{BN_CLICKED, EN_CHANGE, WNDPROC};
use windows::core::{Result, w};

use crate::settings::appearance::Appearance;
use crate::settings::controls::{Control, ControlKind, PickerMenu};
use crate::settings::form::{
    self, CARD_DETAIL, CARD_PADDING_X, CARD_PADDING_Y, CARD_TEXT, CONTROL_GAP, CONTROL_HEIGHT, Header,
    Placer, SECTION_GAP, SettingCard, enable_if, show_note,
};
use crate::settings::layout::Column;
use crate::settings::page::{Page, Shape};
use crate::settings::typography::TextRole;

const PICKER_WIDTH: i32 = 280;
const SWITCH_WIDTH: i32 = 88;
const SWITCH_ON_TEXT: &str = "On";
const SWITCH_OFF_TEXT: &str = "Off";
const SYSTEM_PROMPT_LINES: i32 = 6;
const EDIT_LINE_BREAK: &str = "\r\n";
const LINE_BREAK: &str = "\n";

/// The General pane and its controls.
#[derive(Debug)]
pub(crate) struct GeneralPage {
    header: Header,
    clipboard: SwitchCard,
    preparation: SwitchCard,
    backend_label: Control,
    backend_description: Control,
    backend_note: Control,
    backend_picker: Control,
    system_prompt_label: Control,
    system_prompt_description: Control,
    system_prompt_field: Control,
    reset_system_prompt: Control,
    choices: Vec<BackendChoice>,
    has_note: bool,
    page: Page,
}

impl GeneralPage {
    pub(crate) fn create(parent: HWND, procedure: WNDPROC) -> Result<Self> {
        let page = Page::create(w!("GhostType.GeneralPage"), procedure, parent)?;
        let host = page.handle();
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            header: Header::create(host)?,
            clipboard: SwitchCard::create(host)?,
            preparation: SwitchCard::create(host)?,
            backend_label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            backend_description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            backend_note: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            backend_picker: control(ControlKind::Picker, TextRole::Body, CARD_TEXT)?,
            system_prompt_label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            system_prompt_description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            system_prompt_field: control(ControlKind::TextArea, TextRole::Body, CARD_TEXT)?,
            reset_system_prompt: control(ControlKind::Button, TextRole::Body, CARD_TEXT)?,
            choices: Vec::new(),
            has_note: false,
            page,
        })
    }

    pub(crate) fn page(&self) -> &Page {
        &self.page
    }

    pub(crate) fn page_mut(&mut self) -> &mut Page {
        &mut self.page
    }

    pub(crate) fn render(&mut self, view: &GeneralView, appearance: &Appearance) {
        self.page.set_title(view.title);
        self.header.render(view.title, view.summary);
        self.clipboard.render(&view.clipboard);
        self.preparation.render(&view.prompt_preparation);
        if let Some(backend) = &view.backend {
            self.render_backend(backend);
        }
        self.has_note = view.backend_note.is_some();
        if let Some(note) = &view.backend_note {
            show_note(&mut self.backend_note, note);
        }
        self.render_system_prompt(&view.system_prompt);
        self.layout(appearance);
    }

    fn render_system_prompt(&mut self, editor: &SystemPromptEditor) {
        self.system_prompt_label.set_text(editor.label);
        self.system_prompt_description.set_text(editor.description);
        self.system_prompt_field
            .replace_typed_text(&editor.text.replace(LINE_BREAK, EDIT_LINE_BREAK));
        self.reset_system_prompt.set_text(editor.reset_label);
        enable_if(&self.reset_system_prompt, editor.is_reset_enabled);
    }

    fn render_backend(&mut self, backend: &BackendPicker) {
        self.backend_label.set_text(backend.label);
        self.backend_description.set_text(backend.description);
        let titles: Vec<String> = backend
            .choices
            .iter()
            .map(|choice| choice.title.to_owned())
            .collect();
        self.backend_picker.set_choices(&titles, backend.selected);
        enable_if(&self.backend_picker, backend.is_enabled);
        self.choices.clone_from(&backend.choices);
    }

    /// Places every control for the page's size and scroll offset.
    pub(crate) fn layout(&mut self, appearance: &Appearance) {
        loop {
            let mut column = form::content_column(self.page.viewport(), appearance);
            let mut placer = Placer::new(self.page.offset());
            let mut shapes = Vec::new();
            self.header.layout(&mut column, &mut placer, appearance);
            column.skip(appearance.px(SECTION_GAP));
            for switch_card in [&self.clipboard, &self.preparation] {
                switch_card.layout(&mut column, &mut placer, appearance, &mut shapes);
                column.skip(appearance.px(SECTION_GAP));
            }
            let with_note = [&self.backend_description, &self.backend_note];
            let details: &[&Control] = if self.has_note {
                &with_note
            } else {
                self.backend_note.hide();
                &with_note[..1]
            };
            let card = SettingCard {
                label: &self.backend_label,
                details,
                control: &self.backend_picker,
                control_width: PICKER_WIDTH,
            };
            card.layout(&mut column, &mut placer, appearance, &mut shapes);
            column.skip(appearance.px(SECTION_GAP));
            self.layout_system_prompt(&mut column, &mut placer, appearance, &mut shapes);
            let content_height = column.top() + appearance.px(form::PAGE_PADDING);
            if !self.page.finish_layout(content_height, shapes) {
                return;
            }
        }
    }

    /// Places the system prompt card: the label and description, the field, then the reset button.
    fn layout_system_prompt(
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
        let description_height = appearance.wrapped_height(
            self.system_prompt_description.current_text(),
            TextRole::Caption,
            width,
        );
        let field_height = form::text_area_height(SYSTEM_PROMPT_LINES, appearance);
        let button_height = appearance.px(CONTROL_HEIGHT);
        let card = column
            .take(label_height + description_height + field_height + button_height + 2 * gap + 2 * padding_y);
        shapes.push(Shape::Card(card));
        let mut inner = Column::new(card.x + padding_x, card.y + padding_y, width);
        let label = inner.take(label_height);
        let description = inner.take(description_height);
        inner.skip(gap);
        let field = inner.take(field_height);
        inner.skip(gap);
        let button_width = form::button_width(appearance, self.reset_system_prompt.current_text());
        let (_, button) = inner.take(button_height).split_trailing(button_width, 0);
        // The label goes right before the field in z-order, so it names the field for UI Automation.
        placer.place(&self.system_prompt_description, description);
        placer.place(&self.system_prompt_label, label);
        placer.place_text_area(&self.system_prompt_field, field, appearance, shapes);
        placer.place(&self.reset_system_prompt, button);
    }

    /// The action of the backend at `index` of the backend menu.
    pub(crate) fn choice_at(&self, index: usize) -> Option<GeneralAction> {
        self.choices.get(index).map(|choice| choice.action.clone())
    }

    /// The action of a `WM_COMMAND` with `notification` from `control`: a click of the clipboard
    /// switch or of the prompt preparation switch, an edit of the system prompt field, or a click
    /// of its reset button.
    pub(crate) fn action_for(&self, control: HWND, notification: u32) -> Option<GeneralAction> {
        let switches = [
            (
                &self.clipboard,
                GeneralAction::DenyClipboard,
                GeneralAction::AllowClipboard,
            ),
            (
                &self.preparation,
                GeneralAction::PrepareOnFirstKey,
                GeneralAction::PrepareOnFocus,
            ),
        ];
        let clicked = switches
            .into_iter()
            .find(|(switch_card, ..)| switch_card.is_clicked(control, notification));
        if let Some((switch_card, turn_off, turn_on)) = clicked {
            return Some(if switch_card.is_on { turn_off } else { turn_on });
        }
        if control == self.system_prompt_field.handle() && notification == EN_CHANGE {
            let text = self
                .system_prompt_field
                .typed_text()
                .replace(EDIT_LINE_BREAK, LINE_BREAK);
            return Some(GeneralAction::EditSystemPrompt(SystemPrompt(text)));
        }
        (control == self.reset_system_prompt.handle() && notification == BN_CLICKED)
            .then_some(GeneralAction::ResetSystemPrompt)
    }

    /// Whether `control` is the backend picker.
    pub(crate) fn is_backend_picker(&self, control: HWND) -> bool {
        control == self.backend_picker.handle()
    }

    /// The menu that the backend picker opens, below the picker on the screen.
    pub(crate) fn backend_menu(&self) -> PickerMenu {
        self.backend_picker.picker_menu()
    }

    pub(crate) fn apply(&self, appearance: &Appearance) {
        self.page.apply();
        for control in self.controls() {
            control.apply(appearance);
        }
    }

    fn controls(&self) -> impl Iterator<Item = &Control> {
        self.header
            .controls()
            .into_iter()
            .chain(self.clipboard.controls())
            .chain(self.preparation.controls())
            .chain([
                &self.backend_label,
                &self.backend_description,
                &self.backend_note,
                &self.backend_picker,
                &self.system_prompt_label,
                &self.system_prompt_description,
                &self.system_prompt_field,
                &self.reset_system_prompt,
            ])
    }
}

/// A card with a label, a description and a switch that turns the setting on and off.
#[derive(Debug)]
struct SwitchCard {
    label: Control,
    description: Control,
    switch: Control,
    is_on: bool,
}

impl SwitchCard {
    fn create(host: HWND) -> Result<Self> {
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            switch: control(ControlKind::Switch, TextRole::Body, CARD_TEXT)?,
            is_on: false,
        })
    }

    fn render(&mut self, setting: &SettingSwitch) {
        self.label.set_text(setting.label);
        self.description.set_text(setting.description);
        if setting.is_on {
            self.switch.set_text(SWITCH_ON_TEXT);
            self.switch.mark_selected();
        } else {
            self.switch.set_text(SWITCH_OFF_TEXT);
            self.switch.mark_unselected();
        }
        self.is_on = setting.is_on;
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

    fn is_clicked(&self, control: HWND, notification: u32) -> bool {
        control == self.switch.handle() && notification == BN_CLICKED
    }

    fn controls(&self) -> [&Control; 3] {
        [&self.label, &self.description, &self.switch]
    }
}
