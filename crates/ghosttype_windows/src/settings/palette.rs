//! Fluent colors of the settings window: the WinUI 3 theme resources for the user's theme, or
//! only system colors while high contrast is on.

use crate::pixels::{Color, Rgb};
use crate::system_theme::{Contrast, SystemColors, SystemTheme, ThemeMode};

const ACCENT_HOVER_ALPHA: u8 = 0xE6;
const ACCENT_PRESSED_ALPHA: u8 = 0xCC;

const DARK: ThemeColors = ThemeColors {
    text: TextColors {
        primary: Color::argb(0xFFFF_FFFF),
        secondary: Color::argb(0xC5FF_FFFF),
        disabled: Color::argb(0x5DFF_FFFF),
        critical: Color::argb(0xFFFF_99A4),
        on_accent: Color::argb(0xFF00_0000),
        on_accent_secondary: Color::argb(0x8000_0000),
        on_accent_disabled: Color::argb(0x87FF_FFFF),
    },
    accent_disabled: Color::argb(0x28FF_FFFF),
    control: Fills {
        rest: Color::argb(0x0FFF_FFFF),
        hover: Color::argb(0x15FF_FFFF),
        pressed: Color::argb(0x08FF_FFFF),
        disabled: Color::argb(0x0BFF_FFFF),
    },
    subtle: Fills {
        rest: Color::TRANSPARENT,
        hover: Color::argb(0x0FFF_FFFF),
        pressed: Color::argb(0x0AFF_FFFF),
        disabled: Color::TRANSPARENT,
    },
    input_active: Color::argb(0xB31E_1E1E),
    strokes: Strokes {
        control: Color::argb(0x12FF_FFFF),
        control_bottom: Color::argb(0x18FF_FFFF),
        control_strong: Color::argb(0x8BFF_FFFF),
        accent: Color::argb(0x14FF_FFFF),
        accent_bottom: Color::argb(0x2300_0000),
        card: Color::argb(0x1900_0000),
        focus_outer: Color::argb(0xFFFF_FFFF),
        focus_inner: Color::argb(0xB300_0000),
    },
    card: Color::argb(0x0DFF_FFFF),
    scroll_thumb: Color::argb(0x8BFF_FFFF),
    backdrop: Rgb::gray(0x20),
};

const LIGHT: ThemeColors = ThemeColors {
    text: TextColors {
        primary: Color::argb(0xE400_0000),
        secondary: Color::argb(0x9E00_0000),
        disabled: Color::argb(0x5C00_0000),
        critical: Color::argb(0xFFC4_2B1C),
        on_accent: Color::argb(0xFFFF_FFFF),
        on_accent_secondary: Color::argb(0xB3FF_FFFF),
        on_accent_disabled: Color::argb(0xFFFF_FFFF),
    },
    accent_disabled: Color::argb(0x3700_0000),
    control: Fills {
        rest: Color::argb(0xB3FF_FFFF),
        hover: Color::argb(0x80F9_F9F9),
        pressed: Color::argb(0x4DF9_F9F9),
        disabled: Color::argb(0x4DF9_F9F9),
    },
    subtle: Fills {
        rest: Color::TRANSPARENT,
        hover: Color::argb(0x0900_0000),
        pressed: Color::argb(0x0600_0000),
        disabled: Color::TRANSPARENT,
    },
    input_active: Color::argb(0xFFFF_FFFF),
    strokes: Strokes {
        control: Color::argb(0x0F00_0000),
        control_bottom: Color::argb(0x2900_0000),
        control_strong: Color::argb(0x7200_0000),
        accent: Color::argb(0x14FF_FFFF),
        accent_bottom: Color::argb(0x6600_0000),
        card: Color::argb(0x0F00_0000),
        focus_outer: Color::argb(0xE400_0000),
        focus_inner: Color::argb(0xB3FF_FFFF),
    },
    card: Color::argb(0xB3FF_FFFF),
    scroll_thumb: Color::argb(0x7200_0000),
    backdrop: Rgb::gray(0xF3),
};

/// How a control reacts to the pointer, and whether the user can use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Interaction {
    Rest,
    Hover,
    Pressed,
    Disabled,
}

/// The fills of one kind of control, one for each interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fills {
    pub(crate) rest: Color,
    pub(crate) hover: Color,
    pub(crate) pressed: Color,
    pub(crate) disabled: Color,
}

impl Fills {
    const fn uniform(color: Color) -> Self {
        Self {
            rest: color,
            hover: color,
            pressed: color,
            disabled: color,
        }
    }

    /// The fill for `interaction`.
    pub(crate) fn of(self, interaction: Interaction) -> Color {
        match interaction {
            Interaction::Rest => self.rest,
            Interaction::Hover => self.hover,
            Interaction::Pressed => self.pressed,
            Interaction::Disabled => self.disabled,
        }
    }
}

/// Text colors (`TextFillColor*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextColors {
    pub(crate) primary: Color,
    pub(crate) secondary: Color,
    pub(crate) disabled: Color,
    pub(crate) critical: Color,
    /// Text on an accent fill.
    pub(crate) on_accent: Color,
    /// Text on a pressed accent fill.
    pub(crate) on_accent_secondary: Color,
    pub(crate) on_accent_disabled: Color,
}

impl TextColors {
    /// The color of text in `tone`.
    pub(crate) fn of(self, tone: Tone) -> Color {
        match tone {
            Tone::Primary => self.primary,
            Tone::Secondary => self.secondary,
            Tone::Critical => self.critical,
        }
    }
}

/// One-pixel borders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Strokes {
    /// The border of controls.
    pub(crate) control: Color,
    /// The bottom edge of controls, a little stronger so they look raised.
    pub(crate) control_bottom: Color,
    /// The bottom edge of text fields and the track of progress bars.
    pub(crate) control_strong: Color,
    /// The border of accent controls.
    pub(crate) accent: Color,
    /// The bottom edge of accent controls.
    pub(crate) accent_bottom: Color,
    pub(crate) card: Color,
    /// The outer ring of the keyboard focus visual.
    pub(crate) focus_outer: Color,
    /// The inner ring of the keyboard focus visual.
    pub(crate) focus_inner: Color,
}

/// The colors the settings window draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Palette {
    pub(crate) text: TextColors,
    /// Primary buttons and the marks of selected and running things.
    pub(crate) accent: Fills,
    /// Buttons, menus and text fields.
    pub(crate) control: Fills,
    /// Things that show a fill only on hover, such as navigation items.
    pub(crate) subtle: Fills,
    /// A text field with keyboard focus.
    pub(crate) input_active: Color,
    pub(crate) strokes: Strokes,
    /// Cards on the window's backdrop.
    pub(crate) card: Color,
    pub(crate) scroll_thumb: Color,
    /// The solid color that replaces Mica, and the window color in high contrast.
    pub(crate) backdrop: Rgb,
}

struct ThemeColors {
    text: TextColors,
    accent_disabled: Color,
    control: Fills,
    subtle: Fills,
    input_active: Color,
    strokes: Strokes,
    card: Color,
    scroll_thumb: Color,
    backdrop: Rgb,
}

impl Palette {
    /// Picks the Fluent colors for `theme`, or only system colors when high contrast is on.
    pub(crate) fn new(theme: SystemTheme) -> Self {
        match (theme.contrast, theme.mode) {
            (Contrast::High(colors), _) => Self::high_contrast(colors),
            (Contrast::Standard, ThemeMode::Dark) => Self::themed(&DARK, theme.accent.on_dark),
            (Contrast::Standard, ThemeMode::Light) => Self::themed(&LIGHT, theme.accent.on_light),
        }
    }

    fn themed(colors: &ThemeColors, accent: Rgb) -> Self {
        let accent = Color::opaque(accent);
        Self {
            text: colors.text,
            accent: Fills {
                rest: accent,
                hover: accent.with_alpha(ACCENT_HOVER_ALPHA),
                pressed: accent.with_alpha(ACCENT_PRESSED_ALPHA),
                disabled: colors.accent_disabled,
            },
            control: colors.control,
            subtle: colors.subtle,
            input_active: colors.input_active,
            strokes: colors.strokes,
            card: colors.card,
            scroll_thumb: colors.scroll_thumb,
            backdrop: colors.backdrop,
        }
    }

    fn high_contrast(colors: SystemColors) -> Self {
        let window = Color::opaque(colors.window);
        let window_text = Color::opaque(colors.window_text);
        let button_face = Color::opaque(colors.button_face);
        let button_text = Color::opaque(colors.button_text);
        let highlight = Color::opaque(colors.highlight);
        let gray_text = Color::opaque(colors.gray_text);
        Self {
            text: TextColors {
                primary: window_text,
                secondary: window_text,
                disabled: gray_text,
                critical: window_text,
                on_accent: Color::opaque(colors.highlight_text),
                on_accent_secondary: Color::opaque(colors.highlight_text),
                on_accent_disabled: gray_text,
            },
            accent: Fills {
                disabled: window,
                ..Fills::uniform(highlight)
            },
            control: Fills::uniform(button_face),
            subtle: Fills {
                rest: Color::TRANSPARENT,
                ..Fills::uniform(button_face)
            },
            input_active: button_face,
            strokes: Strokes {
                control: button_text,
                control_bottom: button_text,
                control_strong: button_text,
                accent: highlight,
                accent_bottom: highlight,
                card: window_text,
                focus_outer: window_text,
                focus_inner: window,
            },
            card: window,
            scroll_thumb: window_text,
            backdrop: colors.window,
        }
    }

    /// The opaque color of a text field at rest on a card, over the solid backdrop.
    fn field_rest(self) -> Rgb {
        self.control.rest.over(self.card.over(self.backdrop))
    }

    /// The color key of text fields. It is one step from the field color, so text edges blend
    /// toward the field while no drawn color matches the key.
    pub(crate) fn field_key(self) -> Rgb {
        self.field_rest().neighbor()
    }

    /// The text color of text fields, opaque because GDI draws them without alpha.
    pub(crate) fn field_text(self) -> Rgb {
        self.text.primary.over(self.field_rest())
    }
}

/// What fills the window behind its content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backdrop {
    /// The system draws Mica, and every transparent pixel shows it.
    Mica,
    /// The window paints the backdrop color, because high contrast drops materials.
    Solid,
}

impl Backdrop {
    pub(crate) fn new(theme: SystemTheme) -> Self {
        match theme.contrast {
            Contrast::Standard => Self::Mica,
            Contrast::High(_) => Self::Solid,
        }
    }
}

/// What a control sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Surface {
    /// The window backdrop.
    Page,
    /// A card.
    Card,
}

/// How strongly a control's text reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    /// Main text.
    Primary,
    /// Descriptions and details.
    Secondary,
    /// Errors.
    Critical,
}

/// The surface and tone of a control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ControlStyle {
    pub(crate) surface: Surface,
    pub(crate) tone: Tone,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system_theme::AccentShades;

    const ACCENT: AccentShades = AccentShades {
        on_dark: Rgb {
            red: 0x4C,
            green: 0xC2,
            blue: 0xFF,
        },
        on_light: Rgb {
            red: 0x00,
            green: 0x67,
            blue: 0xC0,
        },
    };

    const SYSTEM_COLORS: SystemColors = SystemColors {
        window: Rgb::gray(0x00),
        window_text: Rgb::gray(0xFF),
        highlight: Rgb {
            red: 0x1A,
            green: 0xEB,
            blue: 0xFF,
        },
        highlight_text: Rgb::gray(0x00),
        gray_text: Rgb::gray(0x3F),
        button_face: Rgb::gray(0x10),
        button_text: Rgb::gray(0xF0),
    };

    fn theme(mode: ThemeMode, contrast: Contrast) -> SystemTheme {
        SystemTheme {
            mode,
            contrast,
            accent: ACCENT,
        }
    }

    #[test]
    fn light_theme_uses_light_fluent_colors() {
        let palette = Palette::new(theme(ThemeMode::Light, Contrast::Standard));

        assert_eq!(
            (palette.text.primary, palette.card, palette.backdrop),
            (
                Color::argb(0xE400_0000),
                Color::argb(0xB3FF_FFFF),
                Rgb::gray(0xF3)
            )
        );
    }

    #[test]
    fn dark_theme_uses_dark_fluent_colors() {
        let palette = Palette::new(theme(ThemeMode::Dark, Contrast::Standard));

        assert_eq!(
            (palette.text.primary, palette.card, palette.backdrop),
            (
                Color::argb(0xFFFF_FFFF),
                Color::argb(0x0DFF_FFFF),
                Rgb::gray(0x20)
            )
        );
    }

    #[test]
    fn dark_theme_fills_with_light_accent_shade() {
        let palette = Palette::new(theme(ThemeMode::Dark, Contrast::Standard));

        assert_eq!(palette.accent.rest, Color::opaque(ACCENT.on_dark));
    }

    #[test]
    fn light_theme_fills_with_dark_accent_shade() {
        let palette = Palette::new(theme(ThemeMode::Light, Contrast::Standard));

        assert_eq!(palette.accent.rest, Color::opaque(ACCENT.on_light));
    }

    #[test]
    fn accent_fades_on_hover_and_press() {
        let palette = Palette::new(theme(ThemeMode::Light, Contrast::Standard));

        assert_eq!(
            (palette.accent.hover.alpha, palette.accent.pressed.alpha),
            (0xE6, 0xCC)
        );
    }

    #[test]
    fn high_contrast_uses_only_system_colors() {
        let palette = Palette::new(theme(ThemeMode::Dark, Contrast::High(SYSTEM_COLORS)));

        assert_eq!(
            (
                palette.text.primary,
                palette.text.disabled,
                palette.control.hover,
                palette.accent.rest,
                palette.strokes.control,
                palette.card,
                palette.backdrop,
            ),
            (
                Color::opaque(SYSTEM_COLORS.window_text),
                Color::opaque(SYSTEM_COLORS.gray_text),
                Color::opaque(SYSTEM_COLORS.button_face),
                Color::opaque(SYSTEM_COLORS.highlight),
                Color::opaque(SYSTEM_COLORS.button_text),
                Color::opaque(SYSTEM_COLORS.window),
                SYSTEM_COLORS.window,
            )
        );
    }

    #[test]
    fn fills_pick_the_color_of_each_interaction() {
        let fills = Fills {
            rest: Color::argb(1),
            hover: Color::argb(2),
            pressed: Color::argb(3),
            disabled: Color::argb(4),
        };
        let cases = [
            (Interaction::Rest, fills.rest),
            (Interaction::Hover, fills.hover),
            (Interaction::Pressed, fills.pressed),
            (Interaction::Disabled, fills.disabled),
        ];

        let picked = cases.map(|(interaction, _)| fills.of(interaction));

        assert_eq!(picked, cases.map(|(_, color)| color));
    }

    #[test]
    fn tone_picks_text_color() {
        let palette = Palette::new(theme(ThemeMode::Light, Contrast::Standard));

        assert_eq!(
            [Tone::Primary, Tone::Secondary, Tone::Critical].map(|tone| palette.text.of(tone)),
            [
                Color::argb(0xE400_0000),
                Color::argb(0x9E00_0000),
                Color::argb(0xFFC4_2B1C)
            ]
        );
    }

    #[test]
    fn field_key_is_next_to_the_field_seen_on_a_card() {
        let palette = Palette::new(theme(ThemeMode::Dark, Contrast::Standard));

        assert_eq!(
            palette.field_key(),
            Rgb {
                blue: 0x38,
                ..Rgb::gray(0x37)
            }
        );
    }

    #[test]
    fn field_text_is_opaque_primary_text() {
        let palette = Palette::new(theme(ThemeMode::Dark, Contrast::Standard));

        assert_eq!(palette.field_text(), Rgb::WHITE);
    }

    #[test]
    fn standard_contrast_shows_mica() {
        let backdrop = Backdrop::new(theme(ThemeMode::Light, Contrast::Standard));

        assert_eq!(backdrop, Backdrop::Mica);
    }

    #[test]
    fn high_contrast_shows_a_solid_background() {
        let backdrop = Backdrop::new(theme(ThemeMode::Dark, Contrast::High(SYSTEM_COLORS)));

        assert_eq!(backdrop, Backdrop::Solid);
    }
}
