//! The layout of a settings page in the style of the Settings app: the page that scrolls, the cards
//! on it, and the secondary text.

use ghosttype_core::settings_window::view::{Note, NoteTone};
use windows_reactor::{
    Border, Grid, GridLength, ScrollBarVisibility, ScrollViewer, StackPanel, TextBlock, TextWrapping,
    ThemeBrush, Thickness, VerticalAlignment, View,
};

const PAGE_MARGIN: Thickness = Thickness::new(56.0, 32.0, 56.0, 32.0);
const PAGE_MAX_WIDTH: f64 = 1000.0;
const CAPTION_SIZE: f64 = 12.0;
const SECONDARY_OPACITY: f64 = 0.75;
const CARD_PADDING: Thickness = Thickness::xy(16.0, 12.0);
const CARD_MIN_HEIGHT: f64 = 68.0;
const CARD_RADIUS: f64 = 4.0;
const CARD_STROKE: f64 = 1.0;
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

/// A card with the label and the description at the leading edge and `control` at the trailing
/// edge.
pub(super) fn control_card(label: &str, description: &str, note: Option<&Note>, control: View) -> View {
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
                note.map(|note| note_text(note).into()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<View>>(),
        );
    card(
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(CONTROL_GAP)
            .children((texts.grid_column(0), control_with_column(control)))
            .into(),
    )
}

pub(super) fn control_with_column(control: View) -> View {
    Border::new()
        .grid_column(1)
        .vertical_alignment(VerticalAlignment::Center)
        .content(control)
        .into()
}

pub(super) fn secondary_text(text: &str) -> TextBlock {
    TextBlock::new()
        .text(text)
        .font_size(CAPTION_SIZE)
        .opacity(SECONDARY_OPACITY)
        .text_wrapping(TextWrapping::Wrap)
}

pub(super) fn note_text(note: &Note) -> TextBlock {
    let text = secondary_text(note.text);
    match note.tone {
        NoteTone::Plain | NoteTone::Busy => text,
        NoteTone::Error => text.foreground(ThemeBrush::SystemCritical).opacity(1.0),
    }
}
