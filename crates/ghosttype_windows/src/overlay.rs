//! The ghost text: a click-through, never-focused layered window drawn right after the caret.

use ghosttype_core::engine::WordFix;
use windows::Win32::Foundation::POINT;
use windows::Win32::Graphics::Gdi::{COLOR_GRAYTEXT, COLOR_HOTLIGHT, COLOR_WINDOWTEXT, GetSysColor};
use windows::core::{Result, w};

use crate::focus::Caret;
use crate::gdi::{FixColors, Font, FontHeight, TextBitmap, TextLayout};
use crate::pixels::Rgb;
use crate::system_theme::{Contrast, SystemTheme, ThemeMode};
use crate::window::{Window, WindowSpec};

const UI_FONT_FACES: [&str; 2] = ["Segoe UI Variable Text", "Segoe UI"];
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

#[derive(Debug)]
pub(crate) struct Overlay {
    window: Window,
}

impl Overlay {
    pub(crate) fn create() -> Result<Self> {
        let window = Window::create(WindowSpec::floating(w!("GhostType.Overlay")))?;
        Ok(Self { window })
    }

    /// Shows `text` starting at `caret`, sized to the caret, in the font face of the control at the
    /// caret when it is installed, else in the system UI font.
    pub(crate) fn show(&self, text: &str, caret: &Caret) -> Result<()> {
        let faces = ghost_text_faces(caret.font_face.as_deref());
        let font = Font::first_installed(&faces, FontHeight::Cell(caret.height))?;
        let bitmap = TextBitmap::render(text, &font, ghost_text_color(), TextLayout::Fit)?;
        bitmap.present(self.window.handle(), caret.origin)?;
        self.window.show_on_top()
    }

    /// Shows `fix` at `caret`: a line through the typed letters before the caret, then the fixed
    /// word and the rest of the suggestion. The line spans the width of the letters in the font of
    /// the ghost text, so it can differ a little from the letters of a control in another font.
    pub(crate) fn show_fix(&self, fix: &WordFix, caret: &Caret) -> Result<()> {
        let faces = ghost_text_faces(caret.font_face.as_deref());
        let font = Font::first_installed(&faces, FontHeight::Cell(caret.height))?;
        let colors = fix_colors(SystemTheme::read());
        let word = format!(" {}", fix.word);
        let (bitmap, typo_width) = TextBitmap::render_fix(&fix.typo, &word, &fix.rest, &font, colors)?;
        let origin = POINT {
            x: caret.origin.x - typo_width,
            y: caret.origin.y,
        };
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

/// The colors of a fix in `theme`: Fluent red and green, or the colors of high contrast.
fn fix_colors(theme: SystemTheme) -> FixColors {
    let rest = ghost_text_color();
    match (theme.contrast, theme.mode) {
        (Contrast::High, _) => {
            // SAFETY: plain calls.
            let (text, link) = unsafe { (GetSysColor(COLOR_WINDOWTEXT), GetSysColor(COLOR_HOTLIGHT)) };
            FixColors {
                strike: Rgb::from_colorref(text),
                word: Rgb::from_colorref(link),
                rest,
            }
        }
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
    // SAFETY: plain call.
    Rgb::from_colorref(unsafe { GetSysColor(COLOR_GRAYTEXT) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_font_face_comes_before_ui_faces() {
        let faces = ghost_text_faces(Some("Consolas"));

        assert_eq!(faces, ["Consolas", "Segoe UI Variable Text", "Segoe UI"]);
    }

    #[test]
    fn ui_faces_draw_ghost_text_without_host_font_face() {
        let faces = ghost_text_faces(None);

        assert_eq!(faces, UI_FONT_FACES);
    }
}
