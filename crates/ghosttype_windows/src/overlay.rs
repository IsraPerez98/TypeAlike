//! The ghost text: a click-through, never-focused layered window drawn right after the caret.

use windows::Win32::Graphics::Gdi::{COLOR_GRAYTEXT, GetSysColor};
use windows::core::{Result, w};

use crate::focus::Caret;
use crate::gdi::{Font, FontHeight, TextBitmap, TextLayout};
use crate::pixels::Rgb;
use crate::window::{Window, WindowSpec};

const UI_FONT_FACES: [&str; 2] = ["Segoe UI Variable Text", "Segoe UI"];

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

    pub(crate) fn hide(&self) {
        self.window.hide();
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
