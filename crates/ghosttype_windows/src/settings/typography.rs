//! The Fluent type ramp as DirectWrite text formats at one DPI, and text measurement that matches
//! how the settings window draws text.

use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_METRICS, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD,
    DWRITE_LINE_SPACING_METHOD_UNIFORM, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_WORD_WRAPPING_NO_WRAP, DWRITE_WORD_WRAPPING_WRAP, DWriteCreateFactory, IDWriteFactory,
    IDWriteFontCollection, IDWriteTextFormat, IDWriteTextLayout,
};
use windows::core::{BOOL, Error, HSTRING, Result, w};

use crate::settings::layout::{Bounds, Dpi};

const DISPLAY_FACES: [&str; 2] = ["Segoe UI Variable Display", "Segoe UI"];
const TEXT_FACES: [&str; 2] = ["Segoe UI Variable Text", "Segoe UI"];
const SMALL_FACES: [&str; 2] = ["Segoe UI Variable Small", "Segoe UI"];
const ICON_FACES: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];
const DEFAULT_DPI: f32 = 96.0;
const DEFAULT_BASELINE_SHARE: f32 = 0.8;
const UNLIMITED: f32 = f32::MAX;

/// A step of the Fluent type ramp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextRole {
    Title,
    Body,
    BodyStrong,
    Caption,
    /// Segoe Fluent Icons glyphs at icon size.
    Icon,
    /// Segoe Fluent Icons glyphs inside controls, such as the chevron of a menu button.
    SmallIcon,
}

struct RampStep {
    faces: &'static [&'static str],
    size: i32,
    line_height: i32,
    weight: DWRITE_FONT_WEIGHT,
}

impl TextRole {
    fn ramp_step(self) -> RampStep {
        let (faces, size, line_height, weight): (&'static [&'static str], _, _, _) = match self {
            Self::Title => (&DISPLAY_FACES, 28, 36, DWRITE_FONT_WEIGHT_SEMI_BOLD),
            Self::Body => (&TEXT_FACES, 14, 20, DWRITE_FONT_WEIGHT_NORMAL),
            Self::BodyStrong => (&TEXT_FACES, 14, 20, DWRITE_FONT_WEIGHT_SEMI_BOLD),
            Self::Caption => (&SMALL_FACES, 12, 16, DWRITE_FONT_WEIGHT_NORMAL),
            Self::Icon => (&ICON_FACES, 16, 16, DWRITE_FONT_WEIGHT_NORMAL),
            Self::SmallIcon => (&ICON_FACES, 12, 12, DWRITE_FONT_WEIGHT_NORMAL),
        };
        RampStep {
            faces,
            size,
            line_height,
            weight,
        }
    }
}

/// How text fills its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lines {
    /// One line, cut with an ellipsis when too long.
    Single,
    /// As many lines as the width needs.
    Wrapped,
}

/// Where lines sit across their box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Alignment {
    Leading,
    Center,
    Trailing,
}

/// How a piece of text is set: its ramp step, its lines and its alignment. Lines always sit in
/// the vertical middle of their box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextStyle {
    pub(crate) role: TextRole,
    pub(crate) lines: Lines,
    pub(crate) alignment: Alignment,
}

impl TextStyle {
    /// One leading line in `role`.
    pub(crate) fn line(role: TextRole) -> Self {
        Self {
            role,
            lines: Lines::Single,
            alignment: Alignment::Leading,
        }
    }
}

#[derive(Debug)]
struct Formats {
    title: IDWriteTextFormat,
    body: IDWriteTextFormat,
    body_strong: IDWriteTextFormat,
    caption: IDWriteTextFormat,
    icon: IDWriteTextFormat,
    small_icon: IDWriteTextFormat,
}

/// The text formats of every ramp step at one DPI.
#[derive(Debug)]
pub(crate) struct Typography {
    factory: IDWriteFactory,
    formats: Formats,
    dpi: Dpi,
}

impl Typography {
    pub(crate) fn new(dpi: Dpi) -> Result<Self> {
        // SAFETY: plain call.
        let factory: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        let mut collection = None;
        // SAFETY: `collection` is a local the call fills in.
        unsafe { factory.GetSystemFontCollection(&raw mut collection, false) }?;
        let collection = collection.ok_or_else(|| Error::new(E_FAIL, "no system font collection"))?;
        let format = |role| create_format(&factory, &collection, role, dpi);
        Ok(Self {
            formats: Formats {
                title: format(TextRole::Title)?,
                body: format(TextRole::Body)?,
                body_strong: format(TextRole::BodyStrong)?,
                caption: format(TextRole::Caption)?,
                icon: format(TextRole::Icon)?,
                small_icon: format(TextRole::SmallIcon)?,
            },
            factory,
            dpi,
        })
    }

    /// The line height of `role` in physical pixels.
    pub(crate) fn line_height(&self, role: TextRole) -> i32 {
        self.dpi.scale(role.ramp_step().line_height)
    }

    /// The width of `text` on one line in the `role` font, or 0 when it can't be measured.
    pub(crate) fn width(&self, text: &str, role: TextRole) -> i32 {
        self.metrics(text, TextStyle::line(role), UNLIMITED)
            .map_or(0, |metrics| {
                whole_pixels(metrics.widthIncludingTrailingWhitespace)
            })
    }

    /// The height of `text` wrapped at `width` in the `role` font, at least one line.
    pub(crate) fn wrapped_height(&self, text: &str, role: TextRole, width: i32) -> i32 {
        let line = self.line_height(role);
        let style = TextStyle {
            lines: Lines::Wrapped,
            ..TextStyle::line(role)
        };
        self.metrics(text, style, coordinate(width.max(1)))
            .map_or(line, |metrics| whole_pixels(metrics.height).max(line))
    }

    /// Lays out `text` in `style` to fill `bounds`.
    pub(crate) fn layout(&self, text: &str, style: TextStyle, bounds: Bounds) -> Result<IDWriteTextLayout> {
        self.create_layout(
            text,
            style,
            coordinate(bounds.width.max(0)),
            coordinate(bounds.height.max(0)),
        )
    }

    fn metrics(&self, text: &str, style: TextStyle, width: f32) -> Result<DWRITE_TEXT_METRICS> {
        let layout = self.create_layout(text, style, width, UNLIMITED)?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        // SAFETY: `metrics` is a local the call fills in.
        unsafe { layout.GetMetrics(&raw mut metrics) }?;
        Ok(metrics)
    }

    fn create_layout(
        &self,
        text: &str,
        style: TextStyle,
        width: f32,
        height: f32,
    ) -> Result<IDWriteTextLayout> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let format = self.format(style.role);
        // SAFETY: the text and format outlive the call; the layout copies them.
        let layout = unsafe { self.factory.CreateTextLayout(&units, format, width, height) }?;
        // SAFETY: plain calls on the layout just created.
        unsafe {
            layout.SetTextAlignment(text_alignment(style.alignment))?;
            layout.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        }
        match style.lines {
            Lines::Single => {
                // SAFETY: the format outlives the call.
                let ellipsis = unsafe { self.factory.CreateEllipsisTrimmingSign(format) }?;
                let trimming = DWRITE_TRIMMING {
                    granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                    delimiter: 0,
                    delimiterCount: 0,
                };
                // SAFETY: the trimming options and sign outlive the calls; the layout keeps the sign.
                unsafe {
                    layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
                    layout.SetTrimming(&raw const trimming, &ellipsis)?;
                }
            }
            // SAFETY: plain call on the layout just created.
            Lines::Wrapped => unsafe { layout.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP) }?,
        }
        Ok(layout)
    }

    fn format(&self, role: TextRole) -> &IDWriteTextFormat {
        match role {
            TextRole::Title => &self.formats.title,
            TextRole::Body => &self.formats.body,
            TextRole::BodyStrong => &self.formats.body_strong,
            TextRole::Caption => &self.formats.caption,
            TextRole::Icon => &self.formats.icon,
            TextRole::SmallIcon => &self.formats.small_icon,
        }
    }
}

fn create_format(
    factory: &IDWriteFactory,
    collection: &IDWriteFontCollection,
    role: TextRole,
    dpi: Dpi,
) -> Result<IDWriteTextFormat> {
    let step = role.ramp_step();
    let face = installed_face(collection, step.faces);
    let size = coordinate(step.size) * dpi_scale(dpi);
    let line = coordinate(dpi.scale(step.line_height));
    // SAFETY: the names outlive the call.
    let format = unsafe {
        factory.CreateTextFormat(
            &HSTRING::from(face),
            collection,
            step.weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("en-us"),
        )
    }?;
    let baseline =
        centered_baseline(collection, face, step.weight, size, line).unwrap_or(line * DEFAULT_BASELINE_SHARE);
    // SAFETY: plain call on the format just created.
    unsafe { format.SetLineSpacing(DWRITE_LINE_SPACING_METHOD_UNIFORM, line, baseline) }?;
    Ok(format)
}

fn installed_face(collection: &IDWriteFontCollection, faces: &'static [&'static str]) -> &'static str {
    faces
        .iter()
        .copied()
        .find(|face| family_index(collection, face).is_some())
        .or_else(|| faces.last().copied())
        .unwrap_or_default()
}

fn family_index(collection: &IDWriteFontCollection, face: &str) -> Option<u32> {
    let mut index = 0;
    let mut exists = BOOL::default();
    // SAFETY: the name and both outputs are locals that outlive the call.
    unsafe { collection.FindFamilyName(&HSTRING::from(face), &raw mut index, &raw mut exists) }.ok()?;
    exists.as_bool().then_some(index)
}

/// The baseline that centers the font's ascent and descent in a line of `line` pixels, so text
/// sits in the middle of its line like XAML text does.
fn centered_baseline(
    collection: &IDWriteFontCollection,
    face: &str,
    weight: DWRITE_FONT_WEIGHT,
    size: f32,
    line: f32,
) -> Option<f32> {
    let index = family_index(collection, face)?;
    // SAFETY: plain calls on the system collection.
    let font = unsafe {
        collection.GetFontFamily(index).and_then(|family| {
            family.GetFirstMatchingFont(weight, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL)
        })
    }
    .ok()?;
    let mut metrics = DWRITE_FONT_METRICS::default();
    // SAFETY: `metrics` is a local the call fills in.
    unsafe { font.GetMetrics(&raw mut metrics) };
    let em = f32::from(metrics.designUnitsPerEm);
    if em <= 0.0 {
        return None;
    }
    let ascent = f32::from(metrics.ascent) / em * size;
    let descent = f32::from(metrics.descent) / em * size;
    Some((line - ascent - descent) / 2.0 + ascent)
}

fn text_alignment(alignment: Alignment) -> DWRITE_TEXT_ALIGNMENT {
    match alignment {
        Alignment::Leading => DWRITE_TEXT_ALIGNMENT_LEADING,
        Alignment::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
        Alignment::Trailing => DWRITE_TEXT_ALIGNMENT_TRAILING,
    }
}

fn dpi_scale(dpi: Dpi) -> f32 {
    f32::from(u16::try_from(dpi.0).unwrap_or(u16::MAX)) / DEFAULT_DPI
}

/// Physical pixels as a Direct2D and DirectWrite coordinate, saturated to the range windows use.
pub(crate) fn coordinate(pixels: i32) -> f32 {
    let saturated = i16::try_from(pixels).unwrap_or(if pixels < 0 { i16::MIN } else { i16::MAX });
    f32::from(saturated)
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "text extents are small positive numbers; the cast saturates otherwise"
)]
fn whole_pixels(extent: f32) -> i32 {
    extent.ceil() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_keeps_pixels() {
        assert_eq!((coordinate(-12), coordinate(640)), (-12.0, 640.0));
    }

    #[test]
    fn coordinate_saturates_far_values() {
        assert_eq!(
            (coordinate(i32::MAX), coordinate(i32::MIN)),
            (f32::from(i16::MAX), f32::from(i16::MIN))
        );
    }

    #[test]
    fn whole_pixels_round_up() {
        assert_eq!(whole_pixels(19.2), 20);
    }

    #[test]
    fn dpi_scale_is_one_at_default_dpi() {
        assert_eq!((dpi_scale(Dpi(96)), dpi_scale(Dpi(144))), (1.0, 1.5));
    }
}
