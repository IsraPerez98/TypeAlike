//! Spacing and controls that every settings pane shares: the header and the tab-ordered placement.

use windows::Win32::Foundation::HWND;
use windows::core::Result;

use crate::settings::appearance::Appearance;
use crate::settings::controls::{Control, ControlKind};
use crate::settings::layout::{Bounds, Column, content_width};
use crate::settings::page::{FieldShape, Shape};
use crate::settings::palette::{ControlStyle, Surface, Tone};
use crate::settings::typography::TextRole;

pub(crate) const PAGE_PADDING: i32 = 32;
pub(crate) const MAX_CONTENT_WIDTH: i32 = 1000;
pub(crate) const SECTION_GAP: i32 = 28;
pub(crate) const HEADING_GAP: i32 = 8;
pub(crate) const CARD_GAP: i32 = 4;
pub(crate) const CARD_PADDING_X: i32 = 16;
pub(crate) const CARD_PADDING_Y: i32 = 16;
pub(crate) const CARD_MIN_HEIGHT: i32 = 68;
pub(crate) const CONTROL_GAP: i32 = 12;
pub(crate) const CONTROL_HEIGHT: i32 = 32;
const SUMMARY_GAP: i32 = 8;
const BUTTON_PADDING: i32 = 12;
const MIN_BUTTON_WIDTH: i32 = 96;
const FIELD_PADDING_LEFT: i32 = 11;
const FIELD_PADDING_RIGHT: i32 = 8;

pub(crate) const PAGE_TEXT: ControlStyle = ControlStyle {
    surface: Surface::Page,
    tone: Tone::Primary,
};
pub(crate) const PAGE_DETAIL: ControlStyle = ControlStyle {
    surface: Surface::Page,
    tone: Tone::Secondary,
};
pub(crate) const CARD_TEXT: ControlStyle = ControlStyle {
    surface: Surface::Card,
    tone: Tone::Primary,
};
pub(crate) const CARD_DETAIL: ControlStyle = ControlStyle {
    surface: Surface::Card,
    tone: Tone::Secondary,
};

/// Places controls at a scroll offset, each right after the one before in tab order, so tab
/// order matches visual order.
#[derive(Debug)]
pub(crate) struct Placer {
    offset: i32,
    previous: Option<HWND>,
}

impl Placer {
    pub(crate) fn new(offset: i32) -> Self {
        Self {
            offset,
            previous: None,
        }
    }

    /// Shows `control` at `bounds`, given in content coordinates.
    pub(crate) fn place(&mut self, control: &Control, bounds: Bounds) {
        control.place(bounds.scrolled(self.offset), self.previous);
        self.previous = Some(control.handle());
    }

    /// Shows the text field `control` inside a frame at `frame`, given in content coordinates,
    /// and adds the frame to the shapes that the page draws.
    pub(crate) fn place_field(
        &mut self,
        control: &Control,
        frame: Bounds,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let left = frame.x + appearance.px(FIELD_PADDING_LEFT);
        let text_area = Bounds {
            x: left,
            width: (frame.right() - appearance.px(FIELD_PADDING_RIGHT) - left).max(0),
            ..frame.centered_band(appearance.line_height(TextRole::Body))
        };
        self.place(control, text_area);
        shapes.push(Shape::Field(FieldShape {
            frame,
            text_area,
            edit: control.handle(),
            placeholder: control.placeholder(),
        }));
    }
}

/// The height of a card that holds content of `content_height`, never below the Fluent minimum.
pub(crate) fn card_height(content_height: i32, appearance: &Appearance) -> i32 {
    (content_height + 2 * appearance.px(CARD_PADDING_Y)).max(appearance.px(CARD_MIN_HEIGHT))
}

/// The column of a pane's content in a page of `viewport` size.
pub(crate) fn content_column(viewport: Bounds, appearance: &Appearance) -> Column {
    let padding = appearance.px(PAGE_PADDING);
    let width = content_width(viewport.width, padding, appearance.px(MAX_CONTENT_WIDTH));
    Column::new(padding, padding, width)
}

/// The width of a push button that shows `label`.
pub(crate) fn button_width(appearance: &Appearance, label: &str) -> i32 {
    let text = appearance.text_width(label, TextRole::Body);
    (text + 2 * appearance.px(BUTTON_PADDING)).max(appearance.px(MIN_BUTTON_WIDTH))
}

/// The title and summary at the top of a pane.
#[derive(Debug)]
pub(crate) struct Header {
    title: Control,
    summary: Control,
}

impl Header {
    pub(crate) fn create(parent: HWND) -> Result<Self> {
        Ok(Self {
            title: Control::create(parent, ControlKind::Label, TextRole::Title, PAGE_TEXT)?,
            summary: Control::create(parent, ControlKind::Paragraph, TextRole::Body, PAGE_TEXT)?,
        })
    }

    pub(crate) fn render(&mut self, title: &str, summary: &str) {
        self.title.set_text(title);
        self.summary.set_text(summary);
    }

    pub(crate) fn layout(&self, column: &mut Column, placer: &mut Placer, appearance: &Appearance) {
        placer.place(&self.title, column.take(appearance.line_height(TextRole::Title)));
        column.skip(appearance.px(SUMMARY_GAP));
        let width = column.take(0).width;
        let height = appearance.wrapped_height(self.summary.current_text(), TextRole::Body, width);
        placer.place(&self.summary, column.take(height));
    }

    pub(crate) fn controls(&self) -> [&Control; 2] {
        [&self.title, &self.summary]
    }
}
