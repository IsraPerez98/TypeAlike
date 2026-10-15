//! Everything the settings window draws with at one DPI and system theme: colors, fonts, the
//! Direct2D canvas, and the color key and font of text fields.

use windows::Win32::Foundation::{COLORREF, HWND, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, HBRUSH, HDC, SetBkColor, SetTextColor};
use windows::Win32::UI::WindowsAndMessaging::{LWA_COLORKEY, SetLayeredWindowAttributes};
use windows::core::{Error, Owned, Result};

use crate::gdi::{Font, FontHeight};
use crate::pixels::{Color, Rgb};
use crate::settings::canvas::{Canvas, Painter};
use crate::settings::layout::{Bounds, Dpi};
use crate::settings::palette::{Backdrop, Palette};
use crate::settings::typography::{TextRole, Typography};
use crate::system_theme::SystemTheme;

const FIELD_FACES: [&str; 2] = ["Segoe UI Variable Text", "Segoe UI"];
const FIELD_FONT_SIZE: i32 = 14;

/// Everything the settings window draws with. Replaced as a whole when the theme or DPI changes.
#[derive(Debug)]
pub(crate) struct Appearance {
    pub(crate) theme: SystemTheme,
    pub(crate) palette: Palette,
    pub(crate) backdrop: Backdrop,
    pub(crate) dpi: Dpi,
    typography: Typography,
    canvas: Canvas,
    field_font: Font,
    field_brush: Owned<HBRUSH>,
}

impl Appearance {
    pub(crate) fn new(theme: SystemTheme, dpi: Dpi) -> Result<Self> {
        let palette = Palette::new(theme);
        Ok(Self {
            theme,
            palette,
            backdrop: Backdrop::new(theme),
            dpi,
            typography: Typography::new(dpi)?,
            canvas: Canvas::new()?,
            field_font: Font::first_installed(&FIELD_FACES, FontHeight::Em(dpi.scale(FIELD_FONT_SIZE)))?,
            field_brush: solid_brush(palette.field_key())?,
        })
    }

    /// The line height of `role` in physical pixels.
    pub(crate) fn line_height(&self, role: TextRole) -> i32 {
        self.typography.line_height(role)
    }

    /// Converts effective pixels to physical pixels at this appearance's DPI.
    pub(crate) fn px(&self, effective: i32) -> i32 {
        self.dpi.scale(effective)
    }

    /// The width of `text` on one line in the `role` font, or 0 when it can't be measured.
    pub(crate) fn text_width(&self, text: &str, role: TextRole) -> i32 {
        self.typography.width(text, role)
    }

    /// The height of `text` wrapped at `width` in the `role` font, at least one line.
    pub(crate) fn wrapped_height(&self, text: &str, role: TextRole, width: i32) -> i32 {
        self.typography.wrapped_height(text, role, width)
    }

    /// Draws `area` of `context` over the backdrop: transparent over Mica, solid otherwise.
    /// `draw` works in coordinates relative to the area.
    pub(crate) fn paint(&self, context: HDC, area: Bounds, draw: impl FnOnce(&Painter<'_>) -> Result<()>) {
        let clear = match self.backdrop {
            Backdrop::Mica => Color::TRANSPARENT,
            Backdrop::Solid => Color::opaque(self.palette.backdrop),
        };
        if let Err(error) = self.canvas.paint(context, area, clear, &self.typography, draw) {
            tracing::warn!(%error, "failed to draw settings");
        }
    }

    /// The font that text fields type in.
    pub(crate) fn field_font(&self) -> &Font {
        &self.field_font
    }

    /// Answers `WM_CTLCOLOREDIT` from a text field: its text color, and the color key as its
    /// background, so only the text shows over the frame that the page draws.
    pub(crate) fn color_field(&self, wparam: WPARAM) -> LRESULT {
        let context = HDC(std::ptr::with_exposed_provenance_mut(wparam.0));
        // SAFETY: the system passes a device context that is valid while it handles the message.
        unsafe {
            SetTextColor(context, COLORREF(self.palette.field_text().to_colorref()));
            SetBkColor(context, COLORREF(self.palette.field_key().to_colorref()));
        }
        let brush: HBRUSH = *self.field_brush;
        LRESULT(brush.0.expose_provenance().cast_signed())
    }

    /// Makes the field key color of the layered text field `window` transparent.
    pub(crate) fn apply_field_key(&self, window: HWND) {
        let key = COLORREF(self.palette.field_key().to_colorref());
        // SAFETY: plain call on a live layered window of this thread.
        if let Err(error) = unsafe { SetLayeredWindowAttributes(window, key, u8::MAX, LWA_COLORKEY) } {
            tracing::warn!(%error, "failed to set text field color key");
        }
    }
}

fn solid_brush(color: Rgb) -> Result<Owned<HBRUSH>> {
    // SAFETY: plain call; the brush is owned right after.
    let brush = unsafe { CreateSolidBrush(COLORREF(color.to_colorref())) };
    if brush.is_invalid() {
        return Err(Error::from_thread());
    }
    // SAFETY: CreateSolidBrush returned a valid brush that nothing else owns.
    Ok(unsafe { Owned::new(brush) })
}
