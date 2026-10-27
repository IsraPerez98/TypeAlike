//! The color of the field text, and the contrast math of the Web Content Accessibility Guidelines
//! (WCAG) 2 that the ghost text uses.

/// The opacity, out of 255, of ghost text in the color of the field text. As a placeholder, the
/// ghost text then keeps the contrast of the field text at less emphasis on any background.
pub const GHOST_TEXT_ALPHA: u8 = 128;

/// The weights of red, green and blue in the relative luminance of WCAG 2.
const LUMINANCE_WEIGHTS: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// The opaque sRGB color in which a field draws its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextColor {
    /// The red channel.
    pub red: u8,
    /// The green channel.
    pub green: u8,
    /// The blue channel.
    pub blue: u8,
}

impl TextColor {
    /// Whether the text is light, as on a dark background: it has more contrast with black than
    /// with white.
    #[must_use]
    pub fn is_light(self) -> bool {
        let luminance = self.relative_luminance();
        contrast_ratio(luminance, 0.0) > contrast_ratio(1.0, luminance)
    }

    /// The contrast ratio of WCAG 2 between this color and `other`, from 1 to 21. Text needs at
    /// least 4.5 on its background.
    #[must_use]
    pub fn contrast_with(self, other: Self) -> f64 {
        let [first, second] = [self, other].map(Self::relative_luminance);
        contrast_ratio(first.max(second), first.min(second))
    }

    fn relative_luminance(self) -> f64 {
        [self.red, self.green, self.blue]
            .map(linear_channel)
            .iter()
            .zip(LUMINANCE_WEIGHTS)
            .map(|(channel, weight)| channel * weight)
            .sum()
    }
}

/// Undoes the sRGB gamma of `channel`.
fn linear_channel(channel: u8) -> f64 {
    let value = f64::from(channel) / f64::from(u8::MAX);
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The contrast ratio of WCAG 2 between a lighter and a darker relative luminance.
fn contrast_ratio(lighter: f64, darker: f64) -> f64 {
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn gray(level: u8) -> TextColor {
        TextColor {
            red: level,
            green: level,
            blue: level,
        }
    }

    const fn rgb(value: u32) -> TextColor {
        let [_, red, green, blue] = value.to_be_bytes();
        TextColor { red, green, blue }
    }

    #[test]
    fn dark_text_is_not_light() {
        let colors = [gray(0), rgb(0x1F_2328), rgb(0x33_3333), rgb(0x09_69DA)];

        let shades = colors.map(TextColor::is_light);

        assert_eq!(shades, [false; 4]);
    }

    #[test]
    fn light_text_is_light() {
        let colors = [gray(255), rgb(0xCC_CCCC), rgb(0xD4_D4D4), rgb(0x58_A6FF)];

        let shades = colors.map(TextColor::is_light);

        assert_eq!(shades, [true; 4]);
    }

    #[test]
    fn contrast_of_black_and_white_is_21_to_1_in_both_orders() {
        let pairs = [(gray(0), gray(255)), (gray(255), gray(0))];

        let ratios = pairs.map(|(first, second)| (first.contrast_with(second) * 100.0).round());

        assert_eq!(ratios, [2100.0; 2]);
    }

    #[test]
    fn contrast_of_mid_gray_on_white_is_below_the_text_minimum() {
        let ratio = gray(0x80).contrast_with(gray(0xFF));

        assert_eq!((ratio * 100.0).round(), 395.0);
    }

    #[test]
    fn gray_text_is_light_from_where_it_contrasts_more_with_black_than_with_white() {
        let colors = [gray(117), gray(118)];

        let shades = colors.map(TextColor::is_light);

        assert_eq!(shades, [false, true]);
    }
}
