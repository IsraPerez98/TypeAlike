//! The fonts that TypeAlike draws its text in.
//!
//! A font has the first installed face of a list, a height and a weight.

use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Graphics::Gdi::{
    ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, FW_NORMAL,
    FW_SEMIBOLD, GetTextFaceW, HFONT, HGDIOBJ, OUT_TT_PRECIS,
};
use windows::core::{Error, HSTRING, Owned, Result};

use super::memory::MemoryContext;
use crate::utf16::until_nul;

const MAX_FACE_NAME_UNITS: usize = 32;

/// How a font's pixel height is measured.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FontHeight {
    /// Height of the whole character cell, like a line of text or a caret.
    Cell(i32),
    /// Height of the em square, which icon fonts fill.
    Em(i32),
}

/// How thick the letters of a font are.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FontWeight {
    Regular,
    Semibold,
}

#[derive(Debug)]
pub(crate) struct Font(pub(super) Owned<HFONT>);

impl Font {
    /// Creates an antialiased font of `weight` in the first of `faces` that is installed.
    pub(crate) fn first_installed(faces: &[&str], height: FontHeight, weight: FontWeight) -> Result<Self> {
        for face in faces {
            let font = Self::create(face, height, weight)?;
            if font.face_name()? == *face {
                return Ok(font);
            }
        }
        Err(Error::new(E_FAIL, "none of the font faces is installed"))
    }

    fn create(face: &str, height: FontHeight, weight: FontWeight) -> Result<Self> {
        let height = match height {
            FontHeight::Cell(pixels) => pixels,
            FontHeight::Em(pixels) => -pixels,
        };
        let weight = match weight {
            FontWeight::Regular => FW_NORMAL,
            FontWeight::Semibold => FW_SEMIBOLD,
        };
        // SAFETY: plain call; the face name outlives it.
        let font = unsafe {
            CreateFontW(
                height,
                0,
                0,
                0,
                weight.0.try_into().expect("font weight fits in i32"),
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_TT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                DEFAULT_PITCH.0.into(),
                &HSTRING::from(face),
            )
        };
        if font.is_invalid() {
            return Err(Error::from_thread());
        }
        // SAFETY: CreateFontW returned a valid font that nothing else owns.
        Ok(Self(unsafe { Owned::new(font) }))
    }

    fn face_name(&self) -> Result<String> {
        let context = MemoryContext::new()?;
        let _font = context.select(HGDIOBJ(self.0.0));
        let mut units = [0_u16; MAX_FACE_NAME_UNITS];
        // SAFETY: the buffer is valid for the call and the font is selected into the context.
        let written = unsafe { GetTextFaceW(context.handle(), Some(&mut units)) };
        let written = usize::try_from(written).ok().ok_or_else(Error::from_thread)?;
        let name = until_nul(units.get(..written).unwrap_or_default());
        Ok(String::from_utf16_lossy(name))
    }
}
