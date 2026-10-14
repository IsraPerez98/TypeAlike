//! Turns grayscale text coverage into premultiplied color pixels for alpha-blended windows.

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
}
