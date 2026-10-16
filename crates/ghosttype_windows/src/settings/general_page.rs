//! The General pane: renders the core [`GeneralView`] and reports the autostart, the clipboard
//! access, the prompt preparation, the suggestion display, the suggestion length, the suggestion
//! model and the backend the user picks, and the system prompt the user writes.

use ghosttype_core::general_pane::{
    GeneralAction, GeneralView, SettingChoice, SettingPicker, SettingSwitch, SuggestionModelPicker,
    SystemPromptEditor,
};
use ghosttype_core::inference::SystemPrompt;
use ghosttype_core::playground::{ModelChoice, PlaygroundAction};
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
    autostart: SwitchCard,
    has_autostart: bool,
    clipboard: SwitchCard,
    preparation: SwitchCard,
    display: SwitchCard,
    length: PickerCard,
    suggestion_model: ModelCard,
    backend: PickerCard,
    system_prompt_label: Control,
    system_prompt_description: Control,
    system_prompt_field: Control,
    reset_system_prompt: Control,
    page: Page,
}

impl GeneralPage {
    pub(crate) fn create(parent: HWND, procedure: WNDPROC) -> Result<Self> {
        let page = Page::create(w!("GhostType.GeneralPage"), procedure, parent)?;
        let host = page.handle();
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            header: Header::create(host)?,
            autostart: SwitchCard::create(host)?,
            has_autostart: false,
            clipboard: SwitchCard::create(host)?,
            preparation: SwitchCard::create(host)?,
            display: SwitchCard::create(host)?,
            length: PickerCard::create(host)?,
            suggestion_model: ModelCard::create(host)?,
            backend: PickerCard::create(host)?,
            system_prompt_label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            system_prompt_description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            system_prompt_field: control(ControlKind::TextArea, TextRole::Body, CARD_TEXT)?,
            reset_system_prompt: control(ControlKind::Button, TextRole::Body, CARD_TEXT)?,
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
        self.has_autostart = view.autostart.is_some();
        if let Some(autostart) = &view.autostart {
            self.autostart.render(autostart);
        }
        self.clipboard.render(&view.clipboard);
        self.preparation.render(&view.prompt_preparation);
        self.display.render(&view.suggestion_display);
        self.length.render(&view.suggestion_length);
        self.suggestion_model.render(&view.suggestion_model);
        if let Some(backend) = &view.backend {
            self.backend.render(backend);
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

    /// Places every control for the page's size and scroll offset.
    pub(crate) fn layout(&mut self, appearance: &Appearance) {
        loop {
            let mut column = form::content_column(self.page.viewport(), appearance);
            let mut placer = Placer::new(self.page.offset());
            let mut shapes = Vec::new();
            self.header.layout(&mut column, &mut placer, appearance);
            column.skip(appearance.px(SECTION_GAP));
            if !self.has_autostart {
                self.autostart.hide();
            }
            let autostart = self.has_autostart.then_some(&self.autostart);
            let switch_cards =
                autostart
                    .into_iter()
                    .chain([&self.clipboard, &self.preparation, &self.display]);
            for switch_card in switch_cards {
                switch_card.layout(&mut column, &mut placer, appearance, &mut shapes);
                column.skip(appearance.px(SECTION_GAP));
            }
            self.length
                .layout(&mut column, &mut placer, appearance, &mut shapes);
            column.skip(appearance.px(SECTION_GAP));
            self.suggestion_model
                .layout(&mut column, &mut placer, appearance, &mut shapes);
            column.skip(appearance.px(SECTION_GAP));
            self.backend
                .layout(&mut column, &mut placer, appearance, &mut shapes);
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
    pub(crate) fn backend_choice_at(&self, index: usize) -> Option<GeneralAction> {
        self.backend.choice_at(index)
    }

    /// The action of the suggestion length at `index` of the suggestion length menu.
    pub(crate) fn length_choice_at(&self, index: usize) -> Option<GeneralAction> {
        self.length.choice_at(index)
    }

    /// The action of the model at `index` of the suggestion model menu, when picking it does
    /// something.
    pub(crate) fn suggestion_model_choice_at(&self, index: usize) -> Option<PlaygroundAction> {
        self.suggestion_model.choice_at(index)
    }

    /// The action of a `WM_COMMAND` with `notification` from `control`: a click of the autostart,
    /// clipboard, prompt preparation or suggestion display switch, an edit of the system prompt
    /// field, or a click of its reset button.
    pub(crate) fn action_for(&self, control: HWND, notification: u32) -> Option<GeneralAction> {
        let switches = [
            (
                &self.autostart,
                GeneralAction::DisableAutostart,
                GeneralAction::EnableAutostart,
            ),
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
            (
                &self.display,
                GeneralAction::ShowWhenComplete,
                GeneralAction::ShowWordByWord,
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
        control == self.backend.picker.handle()
    }

    /// The menu that the backend picker opens, below the picker on the screen.
    pub(crate) fn backend_menu(&self) -> PickerMenu {
        self.backend.picker.picker_menu()
    }

    /// Whether `control` is the suggestion length picker.
    pub(crate) fn is_length_picker(&self, control: HWND) -> bool {
        control == self.length.picker.handle()
    }

    /// The menu that the suggestion length picker opens, below the picker on the screen.
    pub(crate) fn length_menu(&self) -> PickerMenu {
        self.length.picker.picker_menu()
    }

    /// Whether `control` is the suggestion model picker.
    pub(crate) fn is_suggestion_model_picker(&self, control: HWND) -> bool {
        control == self.suggestion_model.picker.handle()
    }

    /// The menu that the suggestion model picker opens, below the picker on the screen.
    pub(crate) fn suggestion_model_menu(&self) -> PickerMenu {
        self.suggestion_model.picker.picker_menu()
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
            .chain(self.autostart.controls())
            .chain(self.clipboard.controls())
            .chain(self.preparation.controls())
            .chain(self.display.controls())
            .chain(self.length.controls())
            .chain(self.suggestion_model.controls())
            .chain(self.backend.controls())
            .chain([
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

    fn hide(&self) {
        for control in self.controls() {
            control.hide();
        }
    }

    fn controls(&self) -> [&Control; 3] {
        [&self.label, &self.description, &self.switch]
    }
}

/// A card with a label, a description and a picker that opens a menu of the setting's values.
#[derive(Debug)]
struct PickerCard {
    label: Control,
    description: Control,
    picker: Control,
    choices: Vec<SettingChoice>,
}

impl PickerCard {
    fn create(host: HWND) -> Result<Self> {
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            picker: control(ControlKind::Picker, TextRole::Body, CARD_TEXT)?,
            choices: Vec::new(),
        })
    }

    fn render(&mut self, setting: &SettingPicker) {
        self.label.set_text(setting.label);
        self.description.set_text(setting.description);
        let titles: Vec<String> = setting
            .choices
            .iter()
            .map(|choice| choice.title.to_owned())
            .collect();
        self.picker.set_choices(&titles, setting.selected);
        enable_if(&self.picker, setting.is_enabled);
        self.choices.clone_from(&setting.choices);
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
            control: &self.picker,
            control_width: PICKER_WIDTH,
        };
        card.layout(column, placer, appearance, shapes);
    }

    fn choice_at(&self, index: usize) -> Option<GeneralAction> {
        self.choices.get(index).map(|choice| choice.action.clone())
    }

    fn controls(&self) -> [&Control; 3] {
        [&self.label, &self.description, &self.picker]
    }
}

/// A card with a label, a description, a note when there is one, and a picker that opens a menu of
/// the downloaded models.
#[derive(Debug)]
struct ModelCard {
    label: Control,
    description: Control,
    note: Control,
    picker: Control,
    choices: Vec<ModelChoice>,
    has_note: bool,
}

impl ModelCard {
    fn create(host: HWND) -> Result<Self> {
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            note: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            picker: control(ControlKind::Picker, TextRole::Body, CARD_TEXT)?,
            choices: Vec::new(),
            has_note: false,
        })
    }

    fn render(&mut self, setting: &SuggestionModelPicker) {
        self.label.set_text(setting.menu.label);
        self.description.set_text(setting.description);
        let titles: Vec<String> = setting
            .menu
            .choices
            .iter()
            .map(|choice| choice.title.clone())
            .collect();
        self.picker.set_choices(&titles, setting.menu.selected);
        enable_if(&self.picker, setting.menu.is_enabled);
        self.choices.clone_from(&setting.menu.choices);
        self.has_note = setting.note.is_some();
        if let Some(note) = &setting.note {
            show_note(&mut self.note, note);
        }
    }

    fn layout(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        if !self.has_note {
            self.note.hide();
        }
        let note = self.has_note.then_some(&self.note);
        let details: Vec<&Control> = [&self.description].into_iter().chain(note).collect();
        let card = SettingCard {
            label: &self.label,
            details: &details,
            control: &self.picker,
            control_width: PICKER_WIDTH,
        };
        card.layout(column, placer, appearance, shapes);
    }

    fn choice_at(&self, index: usize) -> Option<PlaygroundAction> {
        self.choices.get(index)?.action.clone()
    }

    fn controls(&self) -> [&Control; 4] {
        [&self.label, &self.description, &self.note, &self.picker]
    }
}
