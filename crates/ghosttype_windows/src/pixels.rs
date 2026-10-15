//! Colors, and grayscale text coverage turned into premultiplied color pixels for alpha-blended
//! windows.

/// An opaque color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
}

impl Rgb {
    pub(crate) const BLACK: Self = Self {
        red: 0,
        green: 0,
        blue: 0,
    };
    pub(crate) const WHITE: Self = Self {
        red: 255,
        green: 255,
        blue: 255,
    };

    /// A gray with all three channels at `level`.
    pub(crate) const fn gray(level: u8) -> Self {
        Self {
            red: level,
            green: level,
            blue: level,
        }
    }

    /// A color one blue step away, which looks the same but never compares equal.
    #[must_use]
    pub(crate) const fn neighbor(self) -> Self {
        let blue = if self.blue == u8::MAX {
            self.blue - 1
        } else {
            self.blue + 1
        };
        Self { blue, ..self }
    }

    /// Reads a Win32 `COLORREF` value laid out as `0x00BBGGRR`.
    pub(crate) fn from_colorref(value: u32) -> Self {
        let [red, green, blue, _] = value.to_le_bytes();
        Self { red, green, blue }
    }

    /// Returns the `COLORREF` value laid out as `0x00BBGGRR`.
    pub(crate) fn to_colorref(self) -> u32 {
        u32::from_le_bytes([self.red, self.green, self.blue, 0])
    }
}

/// A color with straight (not premultiplied) alpha, as Fluent design tokens give it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Color {
    pub(crate) rgb: Rgb,
    pub(crate) alpha: u8,
}

impl Color {
    pub(crate) const TRANSPARENT: Self = Self::argb(0);

    /// Reads `0xAARRGGBB`, the order of Fluent and XAML color literals.
    pub(crate) const fn argb(value: u32) -> Self {
        let [blue, green, red, alpha] = value.to_le_bytes();
        Self {
            rgb: Rgb { red, green, blue },
            alpha,
        }
    }

    pub(crate) const fn opaque(rgb: Rgb) -> Self {
        Self { rgb, alpha: u8::MAX }
    }

    /// The same color at `alpha`.
    #[must_use]
    pub(crate) const fn with_alpha(self, alpha: u8) -> Self {
        Self { alpha, ..self }
    }

    /// The opaque color seen where this color covers `base`.
    pub(crate) fn over(self, base: Rgb) -> Rgb {
        let alpha = u16::from(self.alpha);
        let full = u16::from(u8::MAX);
        let blend = |top: u8, bottom: u8| {
            let mixed = (u16::from(top) * alpha + u16::from(bottom) * (full - alpha)) / full;
            u8::try_from(mixed).expect("a weighted mean of bytes fits in a byte")
        };
        Rgb {
            red: blend(self.rgb.red, base.red),
            green: blend(self.rgb.green, base.green),
            blue: blend(self.rgb.blue, base.blue),
        }
    }
}

/// Recolors `pixels` (BGRA, white text drawn on black) to `color`, with text coverage as alpha.
pub(crate) fn tint_by_coverage(pixels: &mut [u32], color: Rgb) {
    for pixel in pixels {
        *pixel = premultiplied(color, coverage(*pixel));
    }
}

fn coverage(pixel: u32) -> u8 {
    let [blue, green, red, _] = pixel.to_le_bytes();
    blue.max(green).max(red)
}

fn premultiplied(color: Rgb, alpha: u8) -> u32 {
    let scale = |channel: u8| {
        u8::try_from(u16::from(channel) * u16::from(alpha) / u16::from(u8::MAX))
            .expect("a channel scaled by at most one fits in a byte")
    };
    u32::from_le_bytes([scale(color.blue), scale(color.green), scale(color.red), alpha])
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAY: Rgb = Rgb {
        red: 0x80,
        green: 0x60,
        blue: 0x40,
    };

    #[test]
    fn full_coverage_is_opaque_color() {
        let mut pixels = [0x00FF_FFFF];

        tint_by_coverage(&mut pixels, GRAY);

        assert_eq!(pixels, [0xFF80_6040]);
    }

    #[test]
    fn no_coverage_is_transparent() {
        let mut pixels = [0x0000_0000];

        tint_by_coverage(&mut pixels, GRAY);

        assert_eq!(pixels, [0x0000_0000]);
    }

    #[test]
    fn partial_coverage_is_premultiplied() {
        let mut pixels = [0x0080_8080];

        tint_by_coverage(&mut pixels, Rgb::WHITE);

        assert_eq!(pixels, [0x8080_8080]);
    }

    #[test]
    fn colorref_round_trips() {
        let color = Rgb::from_colorref(0x0040_6080);

        assert_eq!((color, color.to_colorref()), (GRAY, 0x0040_6080));
    }

    #[test]
    fn argb_reads_fluent_color_literal() {
        let color = Color::argb(0xC580_6040);

        assert_eq!(
            color,
            Color {
                rgb: GRAY,
                alpha: 0xC5
            }
        );
    }

    #[test]
    fn translucent_color_blends_over_base() {
        let card = Color::argb(0x0DFF_FFFF);

        assert_eq!(card.over(Rgb::gray(0x20)), Rgb::gray(0x2B));
    }

    #[test]
    fn opaque_color_hides_base() {
        assert_eq!(Color::opaque(GRAY).over(Rgb::WHITE), GRAY);
    }

    #[test]
    fn transparent_color_shows_base() {
        assert_eq!(Color::TRANSPARENT.over(GRAY), GRAY);
    }

    #[test]
    fn neighbor_differs_by_one_blue_step() {
        assert_eq!(
            (Rgb::gray(0x20).neighbor(), Rgb::WHITE.neighbor()),
            (
                Rgb {
                    blue: 0x21,
                    ..Rgb::gray(0x20)
                },
                Rgb {
                    blue: 0xFE,
                    ..Rgb::WHITE
                }
            )
        );
    }
}
