//! The ghost text: a click-through, never-focused layered window drawn right after the caret, with
//! the lines that a long ghost text wraps onto below the line of the caret, or the mirror on a card
//! below the caret when text follows the caret on its line.

use typealike_core::controller::{Ghost, GhostPlacement, GhostView, WrappedLines};
use typealike_core::desktop::{GHOST_TEXT_ALPHA, TextColor};
use windows::Win32::Foundation::{POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{COLOR_GRAYTEXT, COLOR_HOTLIGHT, COLOR_WINDOWTEXT};
use windows::core::{Result, w};

use crate::caret::Caret;
use crate::display::{crossed_edges, monitor_dpi, rect_at, to_pixels, work_area};
use crate::focus;
use crate::gdi::{Font, FontWeight, TextBitmap, TextRun, line_width};
use crate::pixels::{Ink, Rgb, RoundedRectangle};
use crate::system_theme::{Contrast, SystemTheme, ThemeMode, TooltipColors, system_color};
use crate::window::{Window, WindowSpec};

const UI_FONT_FACES: [&str; 2] = ["Segoe UI Variable Text", "Segoe UI"];
const CARD_HORIZONTAL_PADDING_DIPS: i32 = 8;
const CARD_VERTICAL_PADDING_DIPS: i32 = 4;
const CARD_CORNER_RADIUS_DIPS: i32 = 4;
const CARD_BORDER_WIDTH_DIPS: i32 = 1;
const GAP_TO_CARET_LINE_DIPS: i32 = 4;
/// The critical and the success foreground colors of Fluent, for light mode.
const LIGHT_STRIKE: Rgb = Rgb {
    red: 0xC4,
    green: 0x2B,
    blue: 0x1C,
};
const LIGHT_WORD: Rgb = Rgb {
    red: 0x0F,
    green: 0x7B,
    blue: 0x0F,
};
/// The critical and the success foreground colors of Fluent, for dark mode.
const DARK_STRIKE: Rgb = Rgb {
    red: 0xFF,
    green: 0x99,
    blue: 0xA4,
};
const DARK_WORD: Rgb = Rgb {
    red: 0x6C,
    green: 0xCB,
    blue: 0x5F,
};
/// The gray of ghost text in a field that gives no text color, whose background is unknown. It is
/// the same in light and dark mode, and it shows on both: about 5:1 on #1E1E1E, 3.3:1 on white.
const NEUTRAL_GRAY: Rgb = Rgb::gray(0x8E);
/// The text on the card in light mode: the Fluent primary text color (16.5:1 on the card) for the
/// typed word, and the Fluent secondary one (6.1:1) for the ghost text.
const LIGHT_CARD_TEXT: CardText = CardText {
    typed: Rgb::gray(0x1A),
    ghost: Rgb::gray(0x5F),
};
/// The text on the card in dark mode: the Fluent primary text color (14:1 on the card) for the
/// typed word, and the Fluent tertiary one (5.1:1) for the ghost text. The secondary one is too
/// near white to tell the ghost text from the typed word.
const DARK_CARD_TEXT: CardText = CardText {
    typed: Rgb::WHITE,
    ghost: Rgb::gray(0x9C),
};

/// The colors of the fix of a typo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FixColors {
    /// The line through the typed letters.
    strike: Rgb,
    /// The fixed word.
    word: Rgb,
    /// The rest of the suggestion.
    rest: Ink,
}

/// The colors of a ghost text that the overlay only measures. The colors do not change its width.
const MEASURED_COLORS: FixColors = FixColors {
    strike: Rgb::BLACK,
    word: Rgb::BLACK,
    rest: Ink::CLEAR,
};

/// The colors of the text on the card below the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CardText {
    /// The end of the word before the caret, and the typo of a fix.
    typed: Rgb,
    /// The ghost text.
    ghost: Rgb,
}

/// How the overlay draws the typed letters before the caret: the end of the typed word, and the
/// typo of a fix.
#[derive(Debug, Clone, Copy)]
struct TypedLetters<'text> {
    word: &'text str,
    font: &'text Font,
    ink: Ink,
}

#[derive(Debug)]
pub(crate) struct Overlay {
    window: Window,
}

impl Overlay {
    pub(crate) fn create() -> Result<Self> {
        let window = Window::create(WindowSpec::floating(w!("TypeAlike.Overlay")))?;
        Ok(Self { window })
    }

    /// Shows the ghost text of `view` at its placement at `caret`, sized to the caret, in the font
    /// face of the control at the caret when it is installed, else in the system UI font.
    pub(crate) fn show(&self, view: &GhostView, caret: &Caret) -> Result<()> {
        let font = ghost_text_font(caret, FontWeight::Regular)?;
        let theme = SystemTheme::read();
        let (bitmap, origin) = match view.placement {
            GhostPlacement::AfterCaret => inline_bitmap(view, &font, theme, caret)?,
            GhostPlacement::BelowCaret => card_bitmap(view, &font, theme, caret)?,
        };
        log_shown_ghost(view, rect_at(origin, bitmap.size()));
        bitmap.present(self.window.handle(), origin)?;
        self.window.show_on_top()
    }

    pub(crate) fn hide(&self) {
        self.window.hide();
    }

    pub(crate) fn is_shown(&self) -> bool {
        self.window.is_visible()
    }
}

/// Renders the ghost text of `view` in `font` right after `caret`, and its wrapped lines below.
/// Returns the bitmap and its top left corner on the screen.
fn inline_bitmap(
    view: &GhostView,
    font: &Font,
    theme: SystemTheme,
    caret: &Caret,
) -> Result<(TextBitmap, POINT)> {
    let colors = inline_colors(theme, caret.text_color);
    let (first_line, caret_left) = render(no_typed_letters(font), &view.ghost, font, colors)?;
    let first_line_left = caret.origin.x - caret_left;
    let (bitmap, left) = match &view.wrapped {
        Some(wrapped) => {
            let style = TextRun::new("", font, colors.rest);
            with_wrapped_lines((first_line, first_line_left), wrapped, style, caret.height)?
        }
        None => (first_line, first_line_left),
    };
    let origin = POINT {
        x: left,
        y: caret.origin.y,
    };
    Ok((bitmap, origin))
}

/// Renders the typed word of `view` in a semibold font of the ghost text faces and the ghost text
/// in `font` on a card below the line of `caret`, with the ghost text under the caret, or above
/// the line when the card does not fit below. Returns the bitmap and its top left corner on the
/// screen.
fn card_bitmap(
    view: &GhostView,
    font: &Font,
    theme: SystemTheme,
    caret: &Caret,
) -> Result<(TextBitmap, POINT)> {
    let typed_font = ghost_text_font(caret, FontWeight::Semibold)?;
    let colors = TooltipColors::for_theme(theme);
    let text_colors = card_text(theme);
    let typed = TypedLetters {
        word: &view.typed_word,
        font: &typed_font,
        ink: text_colors.typed.into(),
    };
    let (text, caret_left) = render(
        typed,
        &view.ghost,
        font,
        fix_colors(theme, text_colors.ghost.into()),
    )?;
    let text_left = caret.origin.x - caret_left;
    let line = RECT {
        left: text_left,
        top: caret.origin.y,
        right: text_left,
        bottom: caret.origin.y.saturating_add(caret.height),
    };
    let layout = CardLayout::new(
        line,
        text.size(),
        work_area(caret.origin)?,
        monitor_dpi(caret.origin),
    );
    let mut bitmap = text.padded(layout.padding)?;
    bitmap.lay_over(&layout.plate(colors).pixels());
    let top_left = POINT {
        x: layout.frame.left,
        y: layout.frame.top,
    };
    Ok((bitmap, top_left))
}

/// Logs where the ghost text of `view` shows: `covered`, the rectangle of the screen that its
/// bitmap covers, and the edges of the foreground window that it goes past. It logs no text.
fn log_shown_ghost(view: &GhostView, covered: RECT) {
    if !tracing::enabled!(tracing::Level::DEBUG) {
        return;
    }
    let window = focus::foreground_window_bounds();
    let crossed_edges = window.map(|window| crossed_edges(covered, window));
    tracing::debug!(
        placement = ?view.placement,
        wrapped_lines = view.wrapped.as_ref().map_or(0, |wrapped| wrapped.lines.len()),
        ?covered,
        ?window,
        leaves_window = crossed_edges.as_ref().is_some_and(|edges| !edges.is_empty()),
        ?crossed_edges,
        "showed the ghost text"
    );
}

/// Renders the `typed` letters, then `ghost` in `font` and `colors`. A fix has a line through its
/// typo, as wide as the letters of the typo in the font of `typed`, so it can differ a little from
/// the letters of a control in another font. Returns the bitmap and the distance from its left
/// edge to the caret.
fn render(
    typed: TypedLetters<'_>,
    ghost: &Ghost,
    font: &Font,
    colors: FixColors,
) -> Result<(TextBitmap, i32)> {
    with_line_runs(typed, ghost, font, colors, TextBitmap::render_line)
}

/// Puts the `wrapped` lines in the font and the ink of `style` below the bitmap of the first line,
/// which starts at its left edge, each `line_height` pixels below the line above it. Returns the
/// bitmap of all the lines and its left edge.
fn with_wrapped_lines(
    (first_line, first_line_left): (TextBitmap, i32),
    wrapped: &WrappedLines,
    style: TextRun<'_>,
    line_height: i32,
) -> Result<(TextBitmap, i32)> {
    let left = first_line_left.min(wrapped.left);
    let mut lines = vec![(
        first_line,
        POINT {
            x: first_line_left - left,
            y: 0,
        },
    )];
    for (row, line) in (1..).zip(&wrapped.lines) {
        let (bitmap, _) = TextBitmap::render_line(&[], &[TextRun { text: line, ..style }])?;
        let at = POINT {
            x: wrapped.left - left,
            y: row * line_height,
        };
        lines.push((bitmap, at));
    }
    Ok((TextBitmap::stacked(lines)?, left))
}

/// Returns the width of `ghost` after `caret`, as [`Overlay::show`] draws it right after the caret.
pub(crate) fn inline_width(ghost: &Ghost, caret: &Caret) -> Result<i32> {
    let font = ghost_text_font(caret, FontWeight::Regular)?;
    with_line_runs(
        no_typed_letters(&font),
        ghost,
        &font,
        MEASURED_COLORS,
        |_, after_caret| line_width(after_caret),
    )
}

/// Gives `use_runs` the runs before the caret: the `typed` letters, and the typo of a fix. Then
/// the runs after it: `ghost` in `font` and `colors`, with the word of a fix after a space.
fn with_line_runs<Output>(
    typed: TypedLetters<'_>,
    ghost: &Ghost,
    font: &Font,
    colors: FixColors,
    use_runs: impl FnOnce(&[TextRun<'_>], &[TextRun<'_>]) -> Result<Output>,
) -> Result<Output> {
    let typed_word = TextRun::new(typed.word, typed.font, typed.ink);
    match ghost {
        Ghost::Text(text) => use_runs(&[typed_word], &[TextRun::new(text, font, colors.rest)]),
        Ghost::Fix(fix) => {
            let typo = TextRun {
                strike: Some(colors.strike),
                ..TextRun::new(&fix.typo, typed.font, typed.ink)
            };
            let word = format!(" {}", fix.word);
            use_runs(
                &[typed_word, typo],
                &[
                    TextRun::new(&word, font, colors.word.into()),
                    TextRun::new(&fix.rest, font, colors.rest),
                ],
            )
        }
    }
}

/// The typed letters before a ghost text right after the caret: none, because the control draws
/// them.
fn no_typed_letters(font: &Font) -> TypedLetters<'_> {
    TypedLetters {
        word: "",
        font,
        ink: Ink::CLEAR,
    }
}

/// Returns the font of `weight` for ghost text at `caret`: the face of the control at the caret
/// when it is installed, else the system UI font, sized to the caret.
fn ghost_text_font(caret: &Caret, weight: FontWeight) -> Result<Font> {
    let faces = ghost_text_faces(caret.font_face.as_deref());
    Font::first_installed(&faces, caret.height, weight)
}

/// Where the card below the caret shows and how large its parts are, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CardLayout {
    frame: RECT,
    padding: SIZE,
    corner_radius: i32,
    border_width: i32,
}

impl CardLayout {
    /// Puts a card around text of `text_size` below `line`, the line of the caret at the left edge
    /// of the text, on a monitor of `dpi`. The card goes above the line when it does not fit below
    /// in `work_area`, and stays inside `work_area`.
    fn new(line: RECT, text_size: SIZE, work_area: RECT, dpi: i32) -> Self {
        let padding = SIZE {
            cx: to_pixels(CARD_HORIZONTAL_PADDING_DIPS, dpi),
            cy: to_pixels(CARD_VERTICAL_PADDING_DIPS, dpi),
        };
        let gap = to_pixels(GAP_TO_CARET_LINE_DIPS, dpi);
        let width = text_size.cx + 2 * padding.cx;
        let height = text_size.cy + 2 * padding.cy;
        let below = line.bottom.saturating_add(gap);
        let top = if below.saturating_add(height) <= work_area.bottom {
            below
        } else {
            line.top
                .saturating_sub(gap)
                .saturating_sub(height)
                .max(work_area.top)
        };
        let left = line
            .left
            .saturating_sub(padding.cx)
            .min(work_area.right.saturating_sub(width))
            .max(work_area.left);
        Self {
            frame: RECT {
                left,
                top,
                right: left.saturating_add(width),
                bottom: top.saturating_add(height),
            },
            padding,
            corner_radius: to_pixels(CARD_CORNER_RADIUS_DIPS, dpi),
            border_width: to_pixels(CARD_BORDER_WIDTH_DIPS, dpi),
        }
    }

    fn plate(&self, colors: TooltipColors) -> RoundedRectangle {
        RoundedRectangle {
            width: self.frame.right - self.frame.left,
            height: self.frame.bottom - self.frame.top,
            corner_radius: self.corner_radius,
            border_width: self.border_width,
            fill: colors.fill,
            border: colors.border,
        }
    }
}

/// The colors of ghost text after the caret of a field with text in `text_color`: that color at
/// less opacity, with the fix colors for the background of the field. Without that color, the
/// neutral gray with the fix colors of `theme`. In high contrast, the gray text color of the system.
fn inline_colors(theme: SystemTheme, text_color: Option<TextColor>) -> FixColors {
    match (theme.contrast, text_color) {
        (Contrast::High, _) => fix_colors(theme, ghost_text_color().into()),
        (Contrast::Standard, None) => fix_colors(theme, NEUTRAL_GRAY.into()),
        (Contrast::Standard, Some(text_color)) => {
            let mode = if text_color.is_light() {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            };
            let ink = Ink {
                color: text_color.into(),
                alpha: GHOST_TEXT_ALPHA,
            };
            fix_colors(SystemTheme { mode, ..theme }, ink)
        }
    }
}

/// The colors of the text on the card below the caret in `theme`: the typed word in the primary
/// text color, and the ghost text in a lighter color that keeps the contrast of text on the card.
/// In high contrast, the text color and the gray text color of the system.
fn card_text(theme: SystemTheme) -> CardText {
    match (theme.contrast, theme.mode) {
        (Contrast::High, _) => CardText {
            typed: system_color(COLOR_WINDOWTEXT),
            ghost: ghost_text_color(),
        },
        (Contrast::Standard, ThemeMode::Light) => LIGHT_CARD_TEXT,
        (Contrast::Standard, ThemeMode::Dark) => DARK_CARD_TEXT,
    }
}

/// The colors of a fix in `theme`, with the rest of the suggestion in `rest`: Fluent red and
/// green, or the colors of high contrast.
fn fix_colors(theme: SystemTheme, rest: Ink) -> FixColors {
    match (theme.contrast, theme.mode) {
        (Contrast::High, _) => FixColors {
            strike: system_color(COLOR_WINDOWTEXT),
            word: system_color(COLOR_HOTLIGHT),
            rest,
        },
        (Contrast::Standard, ThemeMode::Light) => FixColors {
            strike: LIGHT_STRIKE,
            word: LIGHT_WORD,
            rest,
        },
        (Contrast::Standard, ThemeMode::Dark) => FixColors {
            strike: DARK_STRIKE,
            word: DARK_WORD,
            rest,
        },
    }
}

/// Lists the faces to draw ghost text in, best first: the face of the host control, then the UI faces.
fn ghost_text_faces(host_face: Option<&str>) -> Vec<&str> {
    host_face.into_iter().chain(UI_FONT_FACES).collect()
}

fn ghost_text_color() -> Rgb {
    system_color(COLOR_GRAYTEXT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_font_face_comes_before_ui_faces() {
        let faces = ghost_text_faces(Some("Consolas"));

        assert_eq!(faces, ["Consolas", "Segoe UI Variable Text", "Segoe UI"]);
    }

    const WORK_AREA: RECT = RECT {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };
    const TEXT_SIZE: SIZE = SIZE { cx: 120, cy: 20 };

    fn caret_line(left: i32, top: i32) -> RECT {
        RECT {
            left,
            top,
            right: left,
            bottom: top + 20,
        }
    }

    #[test]
    fn card_starts_its_text_under_the_caret_below_its_line() {
        let layout = CardLayout::new(caret_line(600, 200), TEXT_SIZE, WORK_AREA, 96);

        assert_eq!(
            layout.frame,
            RECT {
                left: 592,
                top: 224,
                right: 728,
                bottom: 252,
            }
        );
    }

    #[test]
    fn card_shows_above_a_caret_line_at_the_bottom_of_the_work_area() {
        let layout = CardLayout::new(caret_line(600, 1010), TEXT_SIZE, WORK_AREA, 96);

        assert_eq!(layout.frame.top, 978);
    }

    #[test]
    fn card_stays_inside_the_right_edge_of_the_work_area() {
        let layout = CardLayout::new(caret_line(1900, 200), TEXT_SIZE, WORK_AREA, 96);

        assert_eq!(layout.frame.right, 1920);
    }

    #[test]
    fn card_of_a_caret_line_at_the_end_of_the_coordinates_shows_above_the_line() {
        let layout = CardLayout::new(caret_line(600, i32::MAX - 20), TEXT_SIZE, WORK_AREA, 96);

        assert_eq!(
            layout.frame,
            RECT {
                left: 592,
                top: i32::MAX - 52,
                right: 728,
                bottom: i32::MAX - 24,
            }
        );
    }

    #[test]
    fn card_scales_with_the_monitor() {
        let layout = CardLayout::new(caret_line(600, 200), TEXT_SIZE, WORK_AREA, 144);

        assert_eq!(
            (layout.padding, layout.corner_radius, layout.border_width),
            (SIZE { cx: 12, cy: 6 }, 6, 2)
        );
    }

    #[test]
    fn card_text_keeps_the_contrast_of_text_on_the_card_in_both_modes() {
        let themes = [ThemeMode::Light, ThemeMode::Dark].map(|mode| SystemTheme {
            mode,
            contrast: Contrast::Standard,
        });

        let ratios = themes.map(|theme| {
            let fill = TextColor::from(TooltipColors::for_theme(theme).fill);
            let text = card_text(theme);
            [text.typed, text.ghost].map(|color| (TextColor::from(color).contrast_with(fill) * 10.0).round())
        });

        assert_eq!(ratios, [[165.0, 61.0], [140.0, 51.0]]);
    }

    #[test]
    fn ghost_text_in_a_field_with_light_text_takes_its_color_and_the_dark_fix_colors() {
        let theme = SystemTheme {
            mode: ThemeMode::Light,
            contrast: Contrast::Standard,
        };
        let light_text = TextColor {
            red: 212,
            green: 212,
            blue: 212,
        };

        let colors = inline_colors(theme, Some(light_text));

        assert_eq!(
            colors,
            FixColors {
                strike: DARK_STRIKE,
                word: DARK_WORD,
                rest: Ink {
                    color: Rgb::gray(212),
                    alpha: GHOST_TEXT_ALPHA,
                },
            }
        );
    }

    #[test]
    fn ghost_text_in_a_field_with_dark_text_takes_the_light_fix_colors() {
        let theme = SystemTheme {
            mode: ThemeMode::Dark,
            contrast: Contrast::Standard,
        };
        let dark_text = TextColor {
            red: 31,
            green: 35,
            blue: 40,
        };

        let colors = inline_colors(theme, Some(dark_text));

        assert_eq!((colors.strike, colors.word), (LIGHT_STRIKE, LIGHT_WORD));
    }

    #[test]
    fn ghost_text_in_a_field_without_a_text_color_takes_the_neutral_gray() {
        let themes = [ThemeMode::Light, ThemeMode::Dark].map(|mode| SystemTheme {
            mode,
            contrast: Contrast::Standard,
        });

        let colors = themes.map(|theme| inline_colors(theme, None).rest);

        assert_eq!(colors, [Ink::from(NEUTRAL_GRAY); 2]);
    }

    #[test]
    fn ui_faces_draw_ghost_text_without_host_font_face() {
        let faces = ghost_text_faces(None);

        assert_eq!(faces, UI_FONT_FACES);
    }
}
