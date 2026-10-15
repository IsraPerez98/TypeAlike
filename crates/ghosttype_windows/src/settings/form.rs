//! Spacing and controls that every settings pane shares: the header and the tab-ordered placement.

use ghosttype_core::playground::{Note, NoteTone};
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
const TEXT_AREA_PADDING_Y: i32 = 6;

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
        let text_area =
            field_text_area(frame, appearance).centered_band(appearance.line_height(TextRole::Body));
        self.place_in_frame(control, frame, text_area, shapes);
    }

    /// Shows the text area `control` inside a frame at `frame`, given in content coordinates,
    /// and adds the frame to the shapes that the page draws. The text fills the frame.
    pub(crate) fn place_text_area(
        &mut self,
        control: &Control,
        frame: Bounds,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let text_area = field_text_area(frame, appearance).inset(0, appearance.px(TEXT_AREA_PADDING_Y));
        self.place_in_frame(control, frame, text_area, shapes);
    }

    fn place_in_frame(
        &mut self,
        control: &Control,
        frame: Bounds,
        text_area: Bounds,
        shapes: &mut Vec<Shape>,
    ) {
        self.place(control, text_area);
        shapes.push(Shape::Field(FieldShape {
            frame,
            text_area,
            edit: control.handle(),
            placeholder: control.placeholder(),
        }));
    }
}

/// The height of a text area frame that shows `lines` lines of text.
pub(crate) fn text_area_height(lines: i32, appearance: &Appearance) -> i32 {
    lines * appearance.line_height(TextRole::Body) + 2 * appearance.px(TEXT_AREA_PADDING_Y)
}

/// The part of a field `frame` where the text goes, between its left and right padding.
fn field_text_area(frame: Bounds, appearance: &Appearance) -> Bounds {
    let left = frame.x + appearance.px(FIELD_PADDING_LEFT);
    Bounds {
        x: left,
        width: (frame.right() - appearance.px(FIELD_PADDING_RIGHT) - left).max(0),
        ..frame
    }
}

/// The height of a card that holds content of `content_height`, never below the Fluent minimum.
pub(crate) fn card_height(content_height: i32, appearance: &Appearance) -> i32 {
    (content_height + 2 * appearance.px(CARD_PADDING_Y)).max(appearance.px(CARD_MIN_HEIGHT))
}

/// A settings row: a label with lines of detail below it at the leading edge, and a control at
/// the trailing edge.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SettingCard<'a> {
    pub(crate) label: &'a Control,
    /// Caption paragraphs below the label, which wrap onto as many lines as they need. The
    /// caller hides the ones that it leaves out.
    pub(crate) details: &'a [&'a Control],
    pub(crate) control: &'a Control,
    /// The width of `control`, in effective pixels.
    pub(crate) control_width: i32,
}

impl SettingCard<'_> {
    /// Places the card's controls in a card that it takes from `column`, and adds the card to
    /// `shapes`.
    pub(crate) fn layout(
        self,
        column: &mut Column,
        placer: &mut Placer,
        appearance: &Appearance,
        shapes: &mut Vec<Shape>,
    ) {
        let padding_x = appearance.px(CARD_PADDING_X);
        let control_width = appearance.px(self.control_width);
        let gap = appearance.px(CONTROL_GAP);
        let text_width = (column.take(0).width - 2 * padding_x - control_width - gap).max(0);
        let label_height = appearance.line_height(TextRole::Body);
        let detail_heights: Vec<i32> = self
            .details
            .iter()
            .map(|detail| appearance.wrapped_height(detail.current_text(), TextRole::Caption, text_width))
            .collect();
        let text_height = label_height + detail_heights.iter().sum::<i32>();
        let card = column.take(card_height(
            text_height.max(appearance.px(CONTROL_HEIGHT)),
            appearance,
        ));
        shapes.push(Shape::Card(card));
        let (text_area, control_area) = card.inset(padding_x, 0).split_trailing(control_width, gap);
        let text_block = text_area.centered_band(text_height);
        let mut text = Column::new(text_block.x, text_block.y, text_block.width);
        placer.place(self.label, text.take(label_height));
        for (detail, height) in self.details.iter().zip(detail_heights) {
            placer.place(detail, text.take(height));
        }
        placer.place(
            self.control,
            control_area.centered_band(appearance.px(CONTROL_HEIGHT)),
        );
    }
}

/// Lets the user use `control` only when `is_enabled`.
pub(crate) fn enable_if(control: &Control, is_enabled: bool) {
    if is_enabled {
        control.enable();
    } else {
        control.disable();
    }
}

/// Shows `note` in `control` on a card, in the color of its tone.
pub(crate) fn show_note(control: &mut Control, note: &Note) {
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
