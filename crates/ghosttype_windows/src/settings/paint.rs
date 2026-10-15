//! How each part of the settings window looks, after the WinUI 3 control templates: cards, text
//! fields, buttons, navigation items, menu buttons, progress bars and the scroll bar.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, HDC, PAINTSTRUCT};
use windows::Win32::UI::Controls::{ODS_DISABLED, ODS_FLAGS, ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED};
use windows::core::Result;

use crate::pixels::Color;
use crate::settings::appearance::Appearance;
use crate::settings::canvas::Painter;
use crate::settings::controls::ControlKind;
use crate::settings::layout::Bounds;
use crate::settings::look::ControlLook;
use crate::settings::palette::{Fills, Interaction, Surface};
use crate::settings::typography::{Alignment, Lines, TextRole, TextStyle, coordinate};

const CONTROL_RADIUS: i32 = 4;
const STROKE: i32 = 1;
const FOCUS_STROKE: i32 = 2;
const FOCUS_INNER_STROKE: i32 = 1;
const FIELD_UNDERLINE: i32 = 2;
const BUTTON_PADDING_X: i32 = 11;
const PICKER_PADDING_LEFT: i32 = 12;
const CHEVRON_SIZE: i32 = 12;
const CHEVRON_MARGIN: i32 = 14;
const CHEVRON_GAP: i32 = 8;
const CHEVRON_GLYPH: &str = "\u{E70D}";
const NAV_ICON_INSET: i32 = 12;
const NAV_ICON_SIZE: i32 = 16;
const NAV_LABEL_GAP: i32 = 16;
const INDICATOR_WIDTH: i32 = 3;
const INDICATOR_HEIGHT: i32 = 16;
const PROGRESS_TRACK: i32 = 1;
const PROGRESS_INDICATOR: i32 = 3;
const PROGRESS_FULL: i64 = 1000;

/// A `WM_PAINT` in progress on one window, ended on drop.
pub(crate) struct PaintSession {
    window: HWND,
    paint: PAINTSTRUCT,
}

impl PaintSession {
    pub(crate) fn begin(window: HWND) -> Self {
        let mut paint = PAINTSTRUCT::default();
        // SAFETY: called while handling WM_PAINT for `window`; `paint` is a local the call fills in.
        unsafe { BeginPaint(window, &raw mut paint) };
        Self { window, paint }
    }

    pub(crate) fn context(&self) -> HDC {
        self.paint.hdc
    }
}

impl Drop for PaintSession {
    fn drop(&mut self) {
        // Returns no error worth reporting.
        // SAFETY: ends the paint that `begin` started, with the same structure.
        let _ = unsafe { EndPaint(self.window, &raw const self.paint) };
    }
}

/// A control to draw, read from its window and its `WM_DRAWITEM`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ControlPaint<'a> {
    pub(crate) look: ControlLook,
    pub(crate) text: &'a str,
    pub(crate) interaction: Interaction,
    /// Whether keyboard focus is on the control and the user moves it with the keyboard.
    pub(crate) has_focus_visual: bool,
    /// How full a progress bar is, in thousandths.
    pub(crate) progress: u16,
}

/// How the pointer and the system state of a `WM_DRAWITEM` affect the control.
pub(crate) fn interaction(state: ODS_FLAGS, look: ControlLook) -> Interaction {
    if state.0 & ODS_DISABLED.0 != 0 {
        Interaction::Disabled
    } else if state.0 & ODS_SELECTED.0 != 0 {
        Interaction::Pressed
    } else if look.is_hovered {
        Interaction::Hover
    } else {
        Interaction::Rest
    }
}

/// Whether a `WM_DRAWITEM` asks for the keyboard focus visual: focus is on the control and the
/// window shows keyboard cues.
pub(crate) fn has_focus_visual(state: ODS_FLAGS) -> bool {
    state.0 & ODS_FOCUS.0 != 0 && state.0 & ODS_NOFOCUSRECT.0 == 0
}

/// Draws the control `item` to fill `bounds`.
pub(crate) fn control(
    painter: &Painter<'_>,
    bounds: Bounds,
    item: &ControlPaint<'_>,
    appearance: &Appearance,
) -> Result<()> {
    if item.look.style.surface == Surface::Card {
        painter.fill(bounds, appearance.palette.card)?;
    }
    match item.look.kind {
        ControlKind::Label => label(
            painter,
            bounds,
            item,
            Lines::Single,
            Alignment::Leading,
            appearance,
        ),
        ControlKind::TrailingLabel => label(
            painter,
            bounds,
            item,
            Lines::Single,
            Alignment::Trailing,
            appearance,
        ),
        ControlKind::Paragraph => label(
            painter,
            bounds,
            item,
            Lines::Wrapped,
            Alignment::Leading,
            appearance,
        ),
        ControlKind::Button | ControlKind::PrimaryButton => button(painter, bounds, item, appearance),
        ControlKind::NavigationButton => navigation_item(painter, bounds, item, appearance),
        ControlKind::Picker => picker(painter, bounds, item, appearance),
        ControlKind::ProgressBar => progress_bar(painter, bounds, item, appearance),
        ControlKind::TextField => Ok(()),
    }
}

fn label(
    painter: &Painter<'_>,
    bounds: Bounds,
    item: &ControlPaint<'_>,
    lines: Lines,
    alignment: Alignment,
    appearance: &Appearance,
) -> Result<()> {
    // Owner-drawn statics report themselves disabled in their draw state, so labels go by tone only.
    let color = appearance.palette.text.of(item.look.style.tone);
    let style = TextStyle {
        role: item.look.role,
        lines,
        alignment,
    };
    painter.text(item.text, bounds, style, color)
}

fn button(
    painter: &Painter<'_>,
    bounds: Bounds,
    item: &ControlPaint<'_>,
    appearance: &Appearance,
) -> Result<()> {
    let palette = &appearance.palette;
    let is_primary = item.look.kind == ControlKind::PrimaryButton;
    let (fills, stroke, bottom, text) = if is_primary {
        (
            palette.accent,
            palette.strokes.accent,
            palette.strokes.accent_bottom,
            accent_text(item.interaction, appearance),
        )
    } else {
        (
            palette.control,
            palette.strokes.control,
            palette.strokes.control_bottom,
            control_text(item.interaction, appearance),
        )
    };
    raised_shape(
        painter,
        bounds,
        fills,
        item.interaction,
        (stroke, bottom),
        appearance,
    )?;
    let style = TextStyle {
        role: item.look.role,
        lines: Lines::Single,
        alignment: Alignment::Center,
    };
    painter.text(
        item.text,
        bounds.inset(appearance.px(BUTTON_PADDING_X), 0),
        style,
        text,
    )?;
    focus_visual(painter, bounds, item, appearance)
}

fn picker(
    painter: &Painter<'_>,
    bounds: Bounds,
    item: &ControlPaint<'_>,
    appearance: &Appearance,
) -> Result<()> {
    let palette = &appearance.palette;
    let strokes = (palette.strokes.control, palette.strokes.control_bottom);
    raised_shape(
        painter,
        bounds,
        palette.control,
        item.interaction,
        strokes,
        appearance,
    )?;
    let chevron = Bounds {
        x: bounds.right() - appearance.px(CHEVRON_MARGIN) - appearance.px(CHEVRON_SIZE),
        width: appearance.px(CHEVRON_SIZE),
        ..bounds
    };
    let chevron_color = match item.interaction {
        Interaction::Disabled => palette.text.disabled,
        Interaction::Rest | Interaction::Hover | Interaction::Pressed => palette.text.secondary,
    };
    let chevron_style = TextStyle {
        alignment: Alignment::Center,
        ..TextStyle::line(TextRole::SmallIcon)
    };
    painter.text(CHEVRON_GLYPH, chevron, chevron_style, chevron_color)?;
    let left = bounds.x + appearance.px(PICKER_PADDING_LEFT);
    let label = Bounds {
        x: left,
        width: (chevron.x - appearance.px(CHEVRON_GAP) - left).max(0),
        ..bounds
    };
    let text = control_text(item.interaction, appearance);
    painter.text(item.text, label, TextStyle::line(item.look.role), text)?;
    focus_visual(painter, bounds, item, appearance)
}

/// Fills a control and draws its border with the stronger bottom edge that makes it look raised,
/// which pressed and disabled controls drop.
fn raised_shape(
    painter: &Painter<'_>,
    bounds: Bounds,
    fills: Fills,
    interaction: Interaction,
    (stroke, bottom): (Color, Color),
    appearance: &Appearance,
) -> Result<()> {
    let radius = radius(appearance);
    let stroke_width = appearance.px(STROKE).max(1);
    painter.fill_rounded(bounds, radius, fills.of(interaction))?;
    match interaction {
        Interaction::Rest | Interaction::Hover => {
            painter.stroke_rounded(bounds, radius, stroke_width, stroke)?;
            painter.bottom_edge(bounds, radius, stroke_width, bottom)
        }
        Interaction::Pressed => painter.stroke_rounded(bounds, radius, stroke_width, stroke),
        Interaction::Disabled => {
            painter.stroke_rounded(bounds, radius, stroke_width, appearance.palette.strokes.control)
        }
    }
}

fn navigation_item(
    painter: &Painter<'_>,
    bounds: Bounds,
    item: &ControlPaint<'_>,
    appearance: &Appearance,
) -> Result<()> {
    let palette = &appearance.palette;
    let subtle = palette.subtle;
    let fill = match (item.look.is_selected, item.interaction) {
        (true, Interaction::Hover) => subtle.pressed,
        (true, Interaction::Rest | Interaction::Pressed | Interaction::Disabled) => subtle.hover,
        (false, interaction) => subtle.of(interaction),
    };
    painter.fill_rounded(bounds, radius(appearance), fill)?;
    if item.look.is_selected {
        let indicator = Bounds {
            width: appearance.px(INDICATOR_WIDTH),
            ..bounds.centered_band(appearance.px(INDICATOR_HEIGHT))
        };
        painter.fill_pill(indicator, palette.accent.rest)?;
    }
    let text = control_text(item.interaction, appearance);
    let icon = Bounds {
        x: bounds.x + appearance.px(NAV_ICON_INSET),
        width: appearance.px(NAV_ICON_SIZE),
        ..bounds
    };
    if let Some(glyph) = item.look.glyph {
        let style = TextStyle {
            alignment: Alignment::Center,
            ..TextStyle::line(TextRole::Icon)
        };
        painter.text(glyph.encode_utf8(&mut [0; 4]), icon, style, text)?;
    }
    let left = icon.right() + appearance.px(NAV_LABEL_GAP);
    let label = Bounds {
        x: left,
        width: (bounds.right() - appearance.px(NAV_ICON_INSET) - left).max(0),
        ..bounds
    };
    painter.text(item.text, label, TextStyle::line(item.look.role), text)?;
    focus_visual(painter, bounds, item, appearance)
}

fn progress_bar(
    painter: &Painter<'_>,
    bounds: Bounds,
    item: &ControlPaint<'_>,
    appearance: &Appearance,
) -> Result<()> {
    let palette = &appearance.palette;
    let track = bounds.centered_band(appearance.px(PROGRESS_TRACK).max(1));
    painter.fill(track, palette.strokes.control_strong)?;
    let filled = i64::from(bounds.width) * i64::from(item.progress).min(PROGRESS_FULL) / PROGRESS_FULL;
    let width = i32::try_from(filled).unwrap_or(bounds.width);
    if width <= 0 {
        return Ok(());
    }
    let indicator = Bounds {
        width,
        ..bounds.centered_band(appearance.px(PROGRESS_INDICATOR))
    };
    painter.fill_pill(indicator, palette.accent.rest)
}

fn focus_visual(
    painter: &Painter<'_>,
    bounds: Bounds,
    item: &ControlPaint<'_>,
    appearance: &Appearance,
) -> Result<()> {
    if !item.has_focus_visual {
        return Ok(());
    }
    let strokes = appearance.palette.strokes;
    let outer = appearance.px(FOCUS_STROKE).max(1);
    let radius = radius(appearance);
    painter.stroke_rounded(bounds, radius, outer, strokes.focus_outer)?;
    let inner_radius = (radius - coordinate(outer)).max(0.0);
    painter.stroke_rounded(
        bounds.inset(outer, outer),
        inner_radius,
        appearance.px(FOCUS_INNER_STROKE).max(1),
        strokes.focus_inner,
    )
}

fn control_text(interaction: Interaction, appearance: &Appearance) -> Color {
    let text = appearance.palette.text;
    match interaction {
        Interaction::Rest | Interaction::Hover => text.primary,
        Interaction::Pressed => text.secondary,
        Interaction::Disabled => text.disabled,
    }
}

fn accent_text(interaction: Interaction, appearance: &Appearance) -> Color {
    let text = appearance.palette.text;
    match interaction {
        Interaction::Rest | Interaction::Hover => text.on_accent,
        Interaction::Pressed => text.on_accent_secondary,
        Interaction::Disabled => text.on_accent_disabled,
    }
}

/// How a text field draws, from its window's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldState {
    Rest,
    Hover,
    Focused,
    Disabled,
}

/// A text field frame to draw behind its edit control.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FieldPaint<'a> {
    pub(crate) frame: Bounds,
    /// Where the edit control shows its text.
    pub(crate) text_area: Bounds,
    /// The hint shown while the field is empty.
    pub(crate) placeholder: Option<&'a str>,
    pub(crate) state: FieldState,
}

/// Draws a text field: the edit control above shows only its text and caret over this frame.
pub(crate) fn text_field(
    painter: &Painter<'_>,
    field: &FieldPaint<'_>,
    appearance: &Appearance,
) -> Result<()> {
    let palette = &appearance.palette;
    let radius = radius(appearance);
    let fill = match field.state {
        FieldState::Rest => palette.control.rest,
        FieldState::Hover => palette.control.hover,
        FieldState::Focused => palette.input_active,
        FieldState::Disabled => palette.control.disabled,
    };
    let stroke = appearance.px(STROKE).max(1);
    painter.fill_rounded(field.frame, radius, fill)?;
    painter.stroke_rounded(field.frame, radius, stroke, palette.strokes.control)?;
    match field.state {
        FieldState::Rest | FieldState::Hover => {
            painter.bottom_edge(field.frame, radius, stroke, palette.strokes.control_strong)?;
        }
        FieldState::Focused => {
            let underline = appearance.px(FIELD_UNDERLINE).max(1);
            painter.bottom_edge(field.frame, radius, underline, palette.accent.rest)?;
        }
        FieldState::Disabled => {}
    }
    let Some(placeholder) = field.placeholder else {
        return Ok(());
    };
    let color = match field.state {
        FieldState::Disabled => palette.text.disabled,
        FieldState::Rest | FieldState::Hover | FieldState::Focused => palette.text.secondary,
    };
    painter.text(
        placeholder,
        field.text_area,
        TextStyle::line(TextRole::Body),
        color,
    )
}

/// Draws a card: the surface of a group of controls.
pub(crate) fn card(painter: &Painter<'_>, bounds: Bounds, appearance: &Appearance) -> Result<()> {
    let palette = &appearance.palette;
    let radius = radius(appearance);
    painter.fill_rounded(bounds, radius, palette.card)?;
    painter.stroke_rounded(bounds, radius, appearance.px(STROKE).max(1), palette.strokes.card)
}

/// Draws the thumb of a page's scroll bar.
pub(crate) fn scroll_thumb(painter: &Painter<'_>, thumb: Bounds, appearance: &Appearance) -> Result<()> {
    painter.fill_pill(thumb, appearance.palette.scroll_thumb)
}

fn radius(appearance: &Appearance) -> f32 {
    coordinate(appearance.px(CONTROL_RADIUS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::palette::{ControlStyle, Tone};

    fn look(is_hovered: bool) -> ControlLook {
        ControlLook {
            is_hovered,
            ..ControlLook::new(
                ControlKind::Button,
                TextRole::Body,
                ControlStyle {
                    surface: Surface::Card,
                    tone: Tone::Primary,
                },
            )
        }
    }

    #[test]
    fn draw_state_picks_interaction() {
        let cases = [
            (ODS_FLAGS(0), false, Interaction::Rest),
            (ODS_FLAGS(0), true, Interaction::Hover),
            (ODS_SELECTED, true, Interaction::Pressed),
            (
                ODS_FLAGS(ODS_DISABLED.0 | ODS_SELECTED.0),
                true,
                Interaction::Disabled,
            ),
        ];

        let picked = cases.map(|(state, is_hovered, _)| interaction(state, look(is_hovered)));

        assert_eq!(picked, cases.map(|(_, _, expected)| expected));
    }

    #[test]
    fn focus_visual_shows_only_with_keyboard_cues() {
        let cases = [
            (ODS_FOCUS, true),
            (ODS_FLAGS(ODS_FOCUS.0 | ODS_NOFOCUSRECT.0), false),
            (ODS_FLAGS(0), false),
        ];

        let shown = cases.map(|(state, _)| has_focus_visual(state));

        assert_eq!(shown, cases.map(|(_, expected)| expected));
    }
}
