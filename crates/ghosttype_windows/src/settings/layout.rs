//! The layout of a settings page in the style of the Settings app.
//!
//! It has the page that scrolls, the cards on it, the icon and the texts at the leading edge of a
//! card, and the secondary text.

use std::iter;

use ghosttype_core::settings_window::view::{Note, NoteTone, RowIcon};
use windows_reactor::{
    Border, FontIcon, Grid, GridLength, HorizontalAlignment, ScrollBarVisibility, ScrollViewer, StackPanel,
    TextBlock, TextWrapping, ThemeBrush, Thickness, VerticalAlignment, View,
};

use super::icons::row_glyph;

const PAGE_MARGIN: Thickness = Thickness::new(56.0, 32.0, 56.0, 32.0);
const PAGE_MAX_WIDTH: f64 = 1000.0;
const CAPTION_SIZE: f64 = 12.0;
const SECONDARY_OPACITY: f64 = 0.75;
const CARD_PADDING: Thickness = Thickness::xy(16.0, 12.0);
const CARD_MIN_HEIGHT: f64 = 68.0;
const CARD_RADIUS: f64 = 4.0;
const CARD_STROKE: f64 = 1.0;
const ICON_COLUMN_WIDTH: f64 = 24.0;
const GLYPH_SIZE: f64 = 20.0;
const STACK_GAP: f64 = 8.0;
const STACKED_CONTENT_MARGIN: Thickness = Thickness::new(ICON_COLUMN_WIDTH + CONTROL_GAP, 0.0, 0.0, 0.0);
pub(super) const CONTROL_GAP: f64 = 16.0;

/// Puts `column` at the leading edge of a page that scrolls up and down, as wide as the page up to
/// [`PAGE_MAX_WIDTH`]. The width limit is on a grid column, because WinUI places the direct child
/// of a `ScrollViewer` past its right edge when that child has a `MaxWidth` smaller than the page.
pub(super) fn scrolling_page(column: View) -> View {
    let limited = Grid::new()
        .columns([GridLength::STAR.max(PAGE_MAX_WIDTH)])
        .children((column,));
    ScrollViewer::new()
        .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
        .content(Border::new().padding(PAGE_MARGIN).content(limited))
        .into()
}

pub(super) fn card(content: View) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(CARD_STROKE)
        .corner_radius(CARD_RADIUS)
        .padding(CARD_PADDING)
        .min_height(CARD_MIN_HEIGHT)
        .content(content)
        .into()
}

/// `glyph` of the icon font, as large as the icon of a card of the Settings app.
pub(super) fn glyph_icon(glyph: &str) -> View {
    FontIcon::new().glyph(glyph).font_size(GLYPH_SIZE).into()
}

/// The header of a card: `icon` in a column of its own, then the label, the description and the
/// note of a setting. The texts of all the headers start at the same distance from the edge.
pub(super) fn card_header(icon: View, label: &str, description: &str, note: Option<&Note>) -> Grid {
    let icon = Border::new()
        .horizontal_alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Center)
        .content(icon);
    let texts = StackPanel::new()
        .vertical_alignment(VerticalAlignment::Center)
        .children(
            [
                Some(
                    TextBlock::new()
                        .text(label)
                        .text_wrapping(TextWrapping::Wrap)
                        .into(),
                ),
                (!description.is_empty()).then(|| secondary_text(description).into()),
                note.map(note_view),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<View>>(),
        );
    Grid::new()
        .columns([GridLength::Pixel(ICON_COLUMN_WIDTH), GridLength::STAR])
        .column_spacing(CONTROL_GAP)
        .children((icon, texts.grid_column(1)))
}

/// A card with `header` at the leading edge and `control` at the trailing edge.
pub(super) fn control_card(header: Grid, control: View) -> View {
    two_column_card(header.grid_column(0).into(), Some(control))
}

/// A card with `header` at the top and `content` below it, in line with the texts of the header.
pub(super) fn stacked_card(header: Grid, content: Vec<View>) -> View {
    let content = StackPanel::new()
        .spacing(STACK_GAP)
        .margin(STACKED_CONTENT_MARGIN)
        .children(content);
    card(
        StackPanel::new()
            .spacing(STACK_GAP)
            .children((header, content))
            .into(),
    )
}

/// A card with `leading`, which the caller puts in column 0, at the leading edge, and `trailing`,
/// when there is one, at the trailing edge.
pub(super) fn two_column_card(leading: View, trailing: Option<View>) -> View {
    let trailing = trailing.map(|control| {
        Border::new()
            .grid_column(1)
            .vertical_alignment(VerticalAlignment::Center)
            .content(control)
            .into()
    });
    card(
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(CONTROL_GAP)
            .children(iter::once(leading).chain(trailing).collect::<Vec<View>>())
            .into(),
    )
}

/// A row with `leading` at its own width, when there is one, then `text`, which wraps in the rest
/// of the row.
pub(super) fn leading_and_text(leading: Option<View>, text: TextBlock, spacing: f64) -> Grid {
    match leading {
        Some(leading) => Grid::new()
            .columns([GridLength::Auto, GridLength::STAR])
            .column_spacing(spacing)
            .children((leading, text.grid_column(1))),
        None => Grid::new().columns([GridLength::STAR]).children((text,)),
    }
}

pub(super) fn secondary_text(text: &str) -> TextBlock {
    TextBlock::new()
        .text(text)
        .font_size(CAPTION_SIZE)
        .opacity(SECONDARY_OPACITY)
        .text_wrapping(TextWrapping::Wrap)
}

/// The text of `note`, after a warning sign when it warns. Reactor has no caution brush, so a
/// warning stands out by its sign and its text at full opacity.
pub(super) fn note_view(note: &Note) -> View {
    let text = secondary_text(note.text);
    match note.tone {
        NoteTone::Plain | NoteTone::Busy => text.into(),
        NoteTone::Warning => {
            let sign = FontIcon::new()
                .glyph(row_glyph(RowIcon::WarningSign))
                .font_size(CAPTION_SIZE)
                .into();
            leading_and_text(Some(sign), text.opacity(1.0), STACK_GAP).into()
        }
        NoteTone::Error => text.foreground(ThemeBrush::SystemCritical).opacity(1.0).into(),
    }
}
