//! The Models pane: renders the core [`ModelsView`] and reports the buttons the user clicks.

use ghosttype_core::models_pane::{ListContent, ModelList, ModelRow, ModelsAction, ModelsView, RowAccessory};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::WNDPROC;
use windows::core::{Result, w};

use crate::settings::appearance::Appearance;
use crate::settings::controls::{Control, ControlKind};
use crate::settings::form::{
    self, CARD_DETAIL, CARD_GAP, CARD_PADDING_X, CARD_PADDING_Y, CARD_TEXT, CONTROL_GAP, CONTROL_HEIGHT,
    HEADING_GAP, Header, PAGE_DETAIL, PAGE_TEXT, Placer, SECTION_GAP,
};
use crate::settings::layout::{Bounds, Column};
use crate::settings::page::{Page, Shape};
use crate::settings::typography::TextRole;

const SEARCH_GAP: i32 = 16;
const PROGRESS_WIDTH: i32 = 120;
const PROGRESS_HEIGHT: i32 = 3;

/// The Models pane and its controls.
#[derive(Debug)]
pub(crate) struct ModelsPage {
    header: Header,
    search: Control,
    browse: ListGroup,
    downloaded: ListGroup,
    page: Page,
}

impl ModelsPage {
    pub(crate) fn create(parent: HWND, procedure: WNDPROC) -> Result<Self> {
        let page = Page::create(w!("GhostType.ModelsPage"), procedure, parent)?;
        let host = page.handle();
        Ok(Self {
            header: Header::create(host)?,
            search: Control::create(host, ControlKind::TextField, TextRole::Body, PAGE_TEXT)?,
            browse: ListGroup::create(host)?,
            downloaded: ListGroup::create(host)?,
            page,
        })
    }

    pub(crate) fn page(&self) -> &Page {
        &self.page
    }

    pub(crate) fn page_mut(&mut self) -> &mut Page {
        &mut self.page
    }

    pub(crate) fn render(&mut self, view: &ModelsView, appearance: &Appearance) -> Result<()> {
        self.page.set_title(view.title);
        self.header.render(view.title, view.summary);
        self.search.set_placeholder(view.search_placeholder);
        self.browse.render(Some(&view.browse), appearance)?;
        self.downloaded.render(view.downloaded.as_ref(), appearance)?;
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
            column.skip(appearance.px(SEARCH_GAP));
            let search = column.take(appearance.px(CONTROL_HEIGHT));
            placer.place_field(&self.search, search, appearance, &mut shapes);
            self.browse
                .layout(&mut column, &mut placer, appearance, &mut shapes);
            self.downloaded
                .layout(&mut column, &mut placer, appearance, &mut shapes);
            let content_height = column.top() + appearance.px(form::PAGE_PADDING);
            if !self.page.finish_layout(content_height, shapes) {
                return;
            }
        }
    }

    /// The action of the button `control`, if it is one of this pane's buttons.
    pub(crate) fn action_for(&self, control: HWND) -> Option<ModelsAction> {
        self.browse
            .action_for(control)
            .or_else(|| self.downloaded.action_for(control))
    }

    /// The search the user asked for, when `control` is the search field.
    pub(crate) fn search_for(&self, control: HWND) -> Option<ModelsAction> {
        (control == self.search.handle()).then(|| ModelsAction::Search(self.search.typed_text()))
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
            .chain([&self.search])
            .chain(self.browse.controls())
            .chain(self.downloaded.controls())
    }
}

/// A titled list of rows: suggested models, search results, files or downloads.
#[derive(Debug)]
struct ListGroup {
    host: HWND,
    back: Control,
    heading: Control,
    rows: Vec<RowControls>,
    message: Control,
    message_button: Control,
    footer: Control,
    list: Option<ModelList>,
    back_action: Option<ModelsAction>,
    message_action: Option<ModelsAction>,
}

impl ListGroup {
    fn create(host: HWND) -> Result<Self> {
        Ok(Self {
            host,
            back: Control::create(host, ControlKind::Button, TextRole::Body, PAGE_TEXT)?,
            heading: Control::create(host, ControlKind::Label, TextRole::BodyStrong, PAGE_TEXT)?,
            rows: Vec::new(),
            message: Control::create(host, ControlKind::Paragraph, TextRole::Body, CARD_TEXT)?,
            message_button: Control::create(host, ControlKind::Button, TextRole::Body, CARD_TEXT)?,
            footer: Control::create(host, ControlKind::Paragraph, TextRole::Caption, PAGE_DETAIL)?,
            list: None,
            back_action: None,
            message_action: None,
        })
    }

    fn render(&mut self, list: Option<&ModelList>, appearance: &Appearance) -> Result<()> {
        self.list = list.cloned();
        let Some(list) = list else {
            return Ok(());
        };
        self.heading.set_text(&list.heading);
        self.back_action = list.back.as_ref().map(|back| back.action.clone());
        if let Some(back) = &list.back {
            self.back.set_text(back.label);
        }
        self.footer.set_text(list.footer.unwrap_or_default());
        match &list.content {
            ListContent::Rows(rows) => {
                while self.rows.len() < rows.len() {
                    let row = RowControls::create(self.host)?;
                    row.apply(appearance);
                    self.rows.push(row);
                }
                for (controls, row) in self.rows.iter_mut().zip(rows) {
                    controls.render(row);
                }
                self.message_action = None;
            }
            ListContent::Loading(text) => {
                self.message.set_text(text);
                self.message_action = None;
            }
            ListContent::Message { text, retry } => {
                self.message.set_text(text);
                self.message_action = retry.as_ref().map(|retry| retry.action.clone());
                if let Some(retry) = retry {
                    self.message_button.set_text(retry.label);
                }
            }
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
        let Some(list) = &self.list else {
            self.hide_all();
            return;
        };
        column.skip(appearance.px(SECTION_GAP));
        self.layout_heading(list, column, placer, appearance);
        column.skip(appearance.px(HEADING_GAP));
        let row_count = match &list.content {
            ListContent::Rows(rows) => {
                self.message.hide();
                self.message_button.hide();
                for (index, (controls, row)) in self.rows.iter().zip(rows).enumerate() {
                    if index > 0 {
                        column.skip(appearance.px(CARD_GAP));
                    }
                    controls.layout(row, column, placer, appearance, shapes);
                }
                rows.len()
            }
            ListContent::Loading(_) | ListContent::Message { .. } => {
                self.layout_message(column, placer, appearance, shapes);
                0
            }
        };
        for row in self.rows.iter().skip(row_count) {
            row.hide();
        }
        if list.footer.is_some() {
            column.skip(appearance.px(HEADING_GAP));
            let width = column.take(0).width;
            let height = appearance.wrapped_height(self.footer.current_text(), TextRole::Caption, width);
            placer.place(&self.footer, column.take(height));
        } else {
            self.footer.hide();
        }
    }

    fn layout_heading(
        &self,
        list: &ModelList,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
    ) {
        let heading_height = appearance.line_height(TextRole::BodyStrong);
        let Some(back) = &list.back else {
            self.back.hide();
            placer.place(&self.heading, column.take(heading_height));
            return;
        };
        let line = column.take(appearance.px(CONTROL_HEIGHT));
        let back_width = form::button_width(appearance, back.label);
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
        placer.place(&self.heading, heading.centered_band(heading_height));
    }

    fn layout_message(
        &self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let width = column.inner_width(appearance.px(CARD_PADDING_X));
        let button_width = self.message_action.as_ref().map_or(0, |_| {
            form::button_width(appearance, self.message_button.current_text())
        });
        let gap = if button_width > 0 {
            appearance.px(CONTROL_GAP)
        } else {
            0
        };
        let text_width = width - button_width - gap;
        let text_height = appearance.wrapped_height(self.message.current_text(), TextRole::Body, text_width);
        let content_height = if button_width > 0 {
            text_height.max(appearance.px(CONTROL_HEIGHT))
        } else {
            text_height
        };
        let card = column.take(content_height + 2 * appearance.px(CARD_PADDING_Y));
        shapes.push(Shape::Card(card));
        let inner = card.inset(appearance.px(CARD_PADDING_X), appearance.px(CARD_PADDING_Y));
        let (text, button) = inner.split_trailing(button_width, gap);
        placer.place(&self.message, text.centered_band(text_height));
        if button_width > 0 {
            placer.place(
                &self.message_button,
                button.centered_band(appearance.px(CONTROL_HEIGHT)),
            );
        } else {
            self.message_button.hide();
        }
    }

    fn hide_all(&self) {
        for control in self.controls() {
            control.hide();
        }
    }

    fn action_for(&self, control: HWND) -> Option<ModelsAction> {
        if control == self.back.handle() {
            return self.back_action.clone();
        }
        if control == self.message_button.handle() {
            return self.message_action.clone();
        }
        self.rows
            .iter()
            .find(|row| row.button.handle() == control)
            .and_then(|row| row.action.clone())
    }

    fn controls(&self) -> impl Iterator<Item = &Control> {
        [
            &self.back,
            &self.heading,
            &self.message,
            &self.message_button,
            &self.footer,
        ]
        .into_iter()
        .chain(self.rows.iter().flat_map(RowControls::controls))
    }
}

/// The controls of one model, file or repository row.
#[derive(Debug)]
struct RowControls {
    title: Control,
    subtitle: Control,
    detail: Control,
    progress: Control,
    button: Control,
    action: Option<ModelsAction>,
}

impl RowControls {
    fn create(host: HWND) -> Result<Self> {
        Ok(Self {
            title: Control::create(host, ControlKind::Label, TextRole::Body, CARD_TEXT)?,
            subtitle: Control::create(host, ControlKind::Label, TextRole::Caption, CARD_DETAIL)?,
            detail: Control::create(host, ControlKind::TrailingLabel, TextRole::Caption, CARD_DETAIL)?,
            progress: Control::create(host, ControlKind::ProgressBar, TextRole::Caption, CARD_DETAIL)?,
            button: Control::create(host, ControlKind::Button, TextRole::Body, CARD_TEXT)?,
            action: None,
        })
    }

    fn render(&mut self, row: &ModelRow) {
        self.title.set_text(&row.title);
        self.subtitle.set_text(&row.subtitle);
        self.detail.set_text(&row.detail);
        self.progress.set_text(&row.title);
        let button = match &row.accessory {
            RowAccessory::Download(button)
            | RowAccessory::Open(button)
            | RowAccessory::Installed { delete: button }
            | RowAccessory::Failed { retry: button } => Some(button),
            RowAccessory::Downloading { progress, cancel } => {
                self.progress.set_progress(progress.0);
                Some(cancel)
            }
            RowAccessory::Canceling | RowAccessory::Unavailable => None,
        };
        self.action = button.map(|button| button.action.clone());
        if let Some(button) = button {
            self.button.set_text(button.label);
        }
    }

    fn layout(
        &self,
        row: &ModelRow,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let body_line = appearance.line_height(TextRole::Body);
        let caption_line = appearance.line_height(TextRole::Caption);
        let has_subtitle = !row.subtitle.is_empty();
        let text_height = if has_subtitle {
            body_line + caption_line
        } else {
            body_line
        };
        let card = column.take(form::card_height(text_height, appearance));
        shapes.push(Shape::Card(card));
        let gap = appearance.px(CONTROL_GAP);
        let inner = card.inset(appearance.px(CARD_PADDING_X), 0);
        let (rest, accessory) = inner.split_trailing(self.accessory_width(row, appearance), gap);
        let (text_area, detail_area) = if row.detail.is_empty() {
            (rest, None)
        } else {
            let (text, detail) =
                rest.split_trailing(appearance.text_width(&row.detail, TextRole::Caption), gap);
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
        self.layout_accessory(row, accessory, placer, appearance);
    }

    fn layout_accessory(
        &self,
        row: &ModelRow,
        accessory: Bounds,
        placer: &mut Placer,
        appearance: &Appearance,
    ) {
        let button_band = accessory.centered_band(appearance.px(CONTROL_HEIGHT));
        match &row.accessory {
            RowAccessory::Downloading { .. } => {
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
            RowAccessory::Canceling | RowAccessory::Unavailable => {
                self.progress.hide();
                self.button.hide();
            }
            RowAccessory::Download(_)
            | RowAccessory::Open(_)
            | RowAccessory::Installed { .. }
            | RowAccessory::Failed { .. } => {
                self.progress.hide();
                placer.place(&self.button, button_band);
            }
        }
    }

    fn accessory_width(&self, row: &ModelRow, appearance: &Appearance) -> i32 {
        let button = form::button_width(appearance, self.button.current_text());
        match &row.accessory {
            RowAccessory::Downloading { .. } => {
                appearance.px(PROGRESS_WIDTH) + appearance.px(CONTROL_GAP) + button
            }
            RowAccessory::Canceling | RowAccessory::Unavailable => 0,
            RowAccessory::Download(_)
            | RowAccessory::Open(_)
            | RowAccessory::Installed { .. }
            | RowAccessory::Failed { .. } => button,
        }
    }

    fn hide(&self) {
        for control in self.controls() {
            control.hide();
        }
    }

    fn apply(&self, appearance: &Appearance) {
        for control in self.controls() {
            control.apply(appearance);
        }
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
