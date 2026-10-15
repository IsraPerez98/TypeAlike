//! The Playground pane: renders the core [`PlaygroundView`] and reports the model the user picks
//! and the text the user asks to complete.

use ghosttype_core::inference::Prompt;
use ghosttype_core::playground::{
    ModelChoice, Note, NoteTone, PlaygroundAction, PlaygroundOutput, PlaygroundView,
};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::MapWindowPoints;
use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, WNDPROC};
use windows::core::{Result, w};

use crate::settings::appearance::Appearance;
use crate::settings::controls::{Control, ControlKind};
use crate::settings::form::{
    self, CARD_DETAIL, CARD_PADDING_X, CARD_PADDING_Y, CARD_TEXT, CONTROL_GAP, CONTROL_HEIGHT, HEADING_GAP,
    Header, PAGE_TEXT, Placer, SECTION_GAP,
};
use crate::settings::layout::{Bounds, Column};
use crate::settings::page::{Page, Shape};
use crate::settings::palette::{ControlStyle, Surface, Tone};
use crate::settings::typography::TextRole;

const PICKER_WIDTH: i32 = 280;
const OUTPUT_DETAIL_GAP: i32 = 4;

/// The menu of downloaded models that the model picker opens.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ModelMenu {
    /// The picker on the screen, which the menu opens below.
    pub(crate) anchor: RECT,
    pub(crate) titles: Vec<String>,
    pub(crate) selected: usize,
}

/// What the output card shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputShape {
    Hidden,
    Note,
    Completion,
}

/// The Playground pane and its controls.
#[derive(Debug)]
pub(crate) struct PlaygroundPage {
    header: Header,
    model_label: Control,
    model_note: Control,
    model_picker: Control,
    prompt_heading: Control,
    prompt: Control,
    complete: Control,
    output_heading: Control,
    output_text: Control,
    output_detail: Control,
    choices: Vec<ModelChoice>,
    has_note: bool,
    output: OutputShape,
    is_complete_enabled: bool,
    page: Page,
}

impl PlaygroundPage {
    pub(crate) fn create(parent: HWND, procedure: WNDPROC) -> Result<Self> {
        let page = Page::create(w!("GhostType.PlaygroundPage"), procedure, parent)?;
        let host = page.handle();
        let control = |kind, role, style| Control::create(host, kind, role, style);
        Ok(Self {
            header: Header::create(host)?,
            model_label: control(ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            model_note: control(ControlKind::Label, TextRole::Caption, CARD_DETAIL)?,
            model_picker: control(ControlKind::Picker, TextRole::Body, CARD_TEXT)?,
            prompt_heading: control(ControlKind::Label, TextRole::BodyStrong, PAGE_TEXT)?,
            prompt: control(ControlKind::TextField, TextRole::Body, CARD_TEXT)?,
            complete: control(ControlKind::PrimaryButton, TextRole::Body, CARD_TEXT)?,
            output_heading: control(ControlKind::Label, TextRole::BodyStrong, PAGE_TEXT)?,
            output_text: control(ControlKind::Paragraph, TextRole::Body, CARD_TEXT)?,
            output_detail: control(ControlKind::Label, TextRole::Caption, CARD_DETAIL)?,
            choices: Vec::new(),
            has_note: false,
            output: OutputShape::Hidden,
            is_complete_enabled: false,
            page,
        })
    }

    pub(crate) fn page(&self) -> &Page {
        &self.page
    }

    pub(crate) fn page_mut(&mut self) -> &mut Page {
        &mut self.page
    }

    pub(crate) fn render(&mut self, view: &PlaygroundView, appearance: &Appearance) {
        self.page.set_title(view.title);
        self.header.render(view.title, view.summary);
        self.model_label.set_text(view.model.label);
        let titles: Vec<String> = view
            .model
            .choices
            .iter()
            .map(|choice| choice.title.clone())
            .collect();
        self.model_picker.set_choices(&titles, view.model.selected);
        enable_if(&self.model_picker, view.model.is_enabled);
        self.choices.clone_from(&view.model.choices);
        self.has_note = view.model_note.is_some();
        if let Some(note) = &view.model_note {
            show_note(&mut self.model_note, note);
        }
        self.prompt_heading.set_text(view.prompt_label);
        self.prompt.set_placeholder(view.prompt_placeholder);
        self.complete.set_text(view.complete_label);
        enable_if(&self.complete, view.is_complete_enabled);
        self.is_complete_enabled = view.is_complete_enabled;
        self.output_heading.set_text(view.output_heading);
        self.render_output(view.output.as_ref());
        self.layout(appearance);
    }

    fn render_output(&mut self, output: Option<&PlaygroundOutput>) {
        self.output = match output {
            Some(PlaygroundOutput::Note(note)) => {
                show_note(&mut self.output_text, note);
                OutputShape::Note
            }
            Some(PlaygroundOutput::Completion { text, detail }) => {
                self.output_text.set_style(CARD_TEXT);
                self.output_text.set_text(text);
                self.output_detail.set_text(detail);
                OutputShape::Completion
            }
            None => OutputShape::Hidden,
        };
    }

    /// Places every control for the page's size and scroll offset.
    pub(crate) fn layout(&mut self, appearance: &Appearance) {
        loop {
            let mut column = form::content_column(self.page.viewport(), appearance);
            let mut placer = Placer::new(self.page.offset());
            let mut shapes = Vec::new();
            self.header.layout(&mut column, &mut placer, appearance);
            column.skip(appearance.px(SECTION_GAP));
            self.layout_model(&mut column, &mut placer, appearance, &mut shapes);
            column.skip(appearance.px(SECTION_GAP));
            self.layout_prompt(&mut column, &mut placer, appearance, &mut shapes);
            self.layout_output(&mut column, &mut placer, appearance, &mut shapes);
            let content_height = column.top() + appearance.px(form::PAGE_PADDING);
            if !self.page.finish_layout(content_height, shapes) {
                return;
            }
        }
    }

    fn layout_model(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let body_line = appearance.line_height(TextRole::Body);
        let caption_line = appearance.line_height(TextRole::Caption);
        let text_height = if self.has_note {
            body_line + caption_line
        } else {
            body_line
        };
        let content_height = text_height.max(appearance.px(CONTROL_HEIGHT));
        let card = column.take(form::card_height(content_height, appearance));
        shapes.push(Shape::Card(card));
        let inner = card.inset(appearance.px(CARD_PADDING_X), 0);
        let (text_area, picker_area) =
            inner.split_trailing(appearance.px(PICKER_WIDTH), appearance.px(CONTROL_GAP));
        let text_block = text_area.centered_band(text_height);
        placer.place(
            &self.model_label,
            Bounds {
                height: body_line,
                ..text_block
            },
        );
        if self.has_note {
            let note = Bounds {
                y: text_block.y + body_line,
                height: caption_line,
                ..text_block
            };
            placer.place(&self.model_note, note);
        } else {
            self.model_note.hide();
        }
        placer.place(
            &self.model_picker,
            picker_area.centered_band(appearance.px(CONTROL_HEIGHT)),
        );
    }

    fn layout_prompt(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        placer.place(
            &self.prompt_heading,
            column.take(appearance.line_height(TextRole::BodyStrong)),
        );
        column.skip(appearance.px(HEADING_GAP));
        let card = column.take(form::card_height(appearance.px(CONTROL_HEIGHT), appearance));
        shapes.push(Shape::Card(card));
        let inner = card.inset(appearance.px(CARD_PADDING_X), 0);
        let button_width = form::button_width(appearance, self.complete.current_text());
        let (field, button) = inner.split_trailing(button_width, appearance.px(CONTROL_GAP));
        let band = appearance.px(CONTROL_HEIGHT);
        placer.place_field(&self.prompt, field.centered_band(band), appearance, shapes);
        placer.place(&self.complete, button.centered_band(band));
    }

    fn layout_output(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        if self.output == OutputShape::Hidden {
            for control in [&self.output_heading, &self.output_text, &self.output_detail] {
                control.hide();
            }
            return;
        }
        column.skip(appearance.px(SECTION_GAP));
        placer.place(
            &self.output_heading,
            column.take(appearance.line_height(TextRole::BodyStrong)),
        );
        column.skip(appearance.px(HEADING_GAP));
        let padding_x = appearance.px(CARD_PADDING_X);
        let text_width = column.take(0).width - 2 * padding_x;
        let text_height =
            appearance.wrapped_height(self.output_text.current_text(), TextRole::Body, text_width);
        let detail_height = if self.output == OutputShape::Completion {
            appearance.px(OUTPUT_DETAIL_GAP) + appearance.line_height(TextRole::Caption)
        } else {
            0
        };
        let card = column.take(text_height + detail_height + 2 * appearance.px(CARD_PADDING_Y));
        shapes.push(Shape::Card(card));
        let mut inner = Column::new(
            card.x + padding_x,
            card.y + appearance.px(CARD_PADDING_Y),
            text_width,
        );
        placer.place(&self.output_text, inner.take(text_height));
        if self.output == OutputShape::Completion {
            inner.skip(appearance.px(OUTPUT_DETAIL_GAP));
            placer.place(
                &self.output_detail,
                inner.take(appearance.line_height(TextRole::Caption)),
            );
        } else {
            self.output_detail.hide();
        }
    }

    /// The action of the model at `index` of the model menu, when picking it does something.
    pub(crate) fn choice_at(&self, index: usize) -> Option<PlaygroundAction> {
        self.choices.get(index)?.action.clone()
    }

    /// Whether `control` is the model picker.
    pub(crate) fn is_model_picker(&self, control: HWND) -> bool {
        control == self.model_picker.handle()
    }

    /// The menu that the model picker opens, below the picker on the screen.
    pub(crate) fn model_menu(&self) -> ModelMenu {
        let picker = self.model_picker.handle();
        let mut area = RECT::default();
        // SAFETY: `area` is a local the call fills in.
        if let Err(error) = unsafe { GetClientRect(picker, &raw mut area) } {
            tracing::warn!(%error, "failed to read model picker size");
        }
        let mut corners = [
            POINT {
                x: area.left,
                y: area.top,
            },
            POINT {
                x: area.right,
                y: area.bottom,
            },
        ];
        // SAFETY: the points are a local array the call converts in place.
        unsafe { MapWindowPoints(Some(picker), None, &mut corners) };
        ModelMenu {
            anchor: RECT {
                left: corners[0].x,
                top: corners[0].y,
                right: corners[1].x,
                bottom: corners[1].y,
            },
            titles: self.model_picker.choices().to_vec(),
            selected: self.model_picker.selected_choice(),
        }
    }

    /// The completion the user asked for, when `control` is the prompt field or the complete button.
    pub(crate) fn completion_for(&self, control: HWND) -> Option<PlaygroundAction> {
        let asks_for_completion = control == self.prompt.handle() || control == self.complete.handle();
        (asks_for_completion && self.is_complete_enabled)
            .then(|| PlaygroundAction::Complete(Prompt(self.prompt.typed_text())))
    }

    pub(crate) fn apply(&self, appearance: &Appearance) {
        self.page.apply();
        for control in self.controls() {
            control.apply(appearance);
        }
    }

    fn controls(&self) -> impl Iterator<Item = &Control> {
        self.header.controls().into_iter().chain([
            &self.model_label,
            &self.model_note,
            &self.model_picker,
            &self.prompt_heading,
            &self.prompt,
            &self.complete,
            &self.output_heading,
            &self.output_text,
            &self.output_detail,
        ])
    }
}

fn enable_if(control: &Control, is_enabled: bool) {
    if is_enabled {
        control.enable();
    } else {
        control.disable();
    }
}

fn show_note(control: &mut Control, note: &Note) {
    let tone = match note.tone {
        NoteTone::Plain | NoteTone::Busy => Tone::Secondary,
        NoteTone::Error => Tone::Critical,
    };
    control.set_style(ControlStyle {
        surface: Surface::Card,
        tone,
    });
    control.set_text(note.text);
}
