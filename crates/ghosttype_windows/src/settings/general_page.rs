//! The General pane: renders the core [`GeneralView`] and reports the backend the user picks.

use ghosttype_core::general_pane::{BackendChoice, BackendPicker, GeneralAction, GeneralView};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::WNDPROC;
use windows::core::{Result, w};

use crate::settings::appearance::Appearance;
use crate::settings::controls::{Control, ControlKind, PickerMenu};
use crate::settings::form::{
    self, CARD_DETAIL, CARD_TEXT, Header, Placer, SECTION_GAP, SettingCard, enable_if, show_note,
};
use crate::settings::page::Page;
use crate::settings::typography::TextRole;

const PICKER_WIDTH: i32 = 280;

/// The General pane and its controls.
#[derive(Debug)]
pub(crate) struct GeneralPage {
    header: Header,
    backend_label: Control,
    backend_description: Control,
    backend_note: Control,
    backend_picker: Control,
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
            backend_label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            backend_description: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            backend_note: control(ControlKind::Paragraph, TextRole::Caption, CARD_DETAIL)?,
            backend_picker: control(ControlKind::Picker, TextRole::Body, CARD_TEXT)?,
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
        if let Some(backend) = &view.backend {
            self.render_backend(backend);
        }
        self.has_note = view.backend_note.is_some();
        if let Some(note) = &view.backend_note {
            show_note(&mut self.backend_note, note);
        }
        self.layout(appearance);
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
            let content_height = column.top() + appearance.px(form::PAGE_PADDING);
            if !self.page.finish_layout(content_height, shapes) {
                return;
            }
        }
    }

    /// The action of the backend at `index` of the backend menu.
    pub(crate) fn choice_at(&self, index: usize) -> Option<GeneralAction> {
        self.choices.get(index).map(|choice| choice.action)
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
        self.header.controls().into_iter().chain([
            &self.backend_label,
            &self.backend_description,
            &self.backend_note,
            &self.backend_picker,
        ])
    }
}
