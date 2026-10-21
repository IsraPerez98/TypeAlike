//! Renders text into premultiplied-alpha bitmaps and icons, for the overlay, the tray and window
//! icons, and holds the memory bitmaps that the settings window draws into.

use std::ffi::c_void;
use std::iter;
use std::marker::PhantomData;
use std::ops::Range;

use windows::Win32::Foundation::{COLORREF, E_FAIL, HWND, POINT, RECT, SIZE};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CLIP_DEFAULT_PRECIS, CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, DEFAULT_CHARSET,
    DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DrawTextW,
    FW_NORMAL, GdiFlush, GetTextExtentPoint32W, GetTextFaceW, HBITMAP, HDC, HFONT, HGDIOBJ, OUT_TT_PRECIS,
    SelectObject, SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, HICON, ICONINFO, ULW_ALPHA, UpdateLayeredWindow,
};
use windows::core::{Error, HSTRING, Owned, Result};

use crate::pixels::{Ink, Rgb, fill_band, lay_over, pad, strike_rows, tint_by_coverage, tint_spans};

const BITS_PER_PIXEL: u16 = 32;
const MAX_FACE_NAME_UNITS: usize = 32;
const ICON_FACES: [&str; 2] = ["Segoe Fluent Icons", "Segoe MDL2 Assets"];
const MONOCHROME_PLANES: u32 = 1;
const MONOCHROME_BITS: u32 = 1;

/// How a font's pixel height is measured.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FontHeight {
    /// Height of the whole character cell, like a line of text or a caret.
    Cell(i32),
    /// Height of the em square, which icon fonts fill.
    Em(i32),
}

#[derive(Debug)]
pub(crate) struct Font(Owned<HFONT>);

impl Font {
    /// Creates a regular antialiased font in the first of `faces` that is installed.
    pub(crate) fn first_installed(faces: &[&str], height: FontHeight) -> Result<Self> {
        for face in faces {
            let font = Self::create(face, height)?;
            if font.face_name()? == *face {
                return Ok(font);
            }
        }
        Err(Error::new(E_FAIL, "none of the font faces is installed"))
    }

    fn create(face: &str, height: FontHeight) -> Result<Self> {
        let height = match height {
            FontHeight::Cell(pixels) => pixels,
            FontHeight::Em(pixels) => -pixels,
        };
        // SAFETY: plain call; the face name outlives it.
        let font = unsafe {
            CreateFontW(
                height,
                0,
                0,
                0,
                FW_NORMAL.0.try_into().expect("font weight fits in i32"),
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
        let written = unsafe { GetTextFaceW(context.handle, Some(&mut units)) };
        let written = usize::try_from(written).ok().ok_or_else(Error::from_thread)?;
        let name = units.get(..written).unwrap_or_default();
        Ok(String::from_utf16_lossy(name).trim_end_matches('\0').to_owned())
    }
}

/// A part of a line of text in one font and one color, with an optional line through it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TextRun<'run> {
    pub(crate) text: &'run str,
    pub(crate) font: &'run Font,
    /// The color of the letters. Letters in a clear ink are left out, so that only the line
    /// through them shows, over the letters that the app draws.
    pub(crate) ink: Ink,
    /// The color of a line through the letters, if any.
    pub(crate) strike: Option<Rgb>,
}

impl<'run> TextRun<'run> {
    /// A run of `text` in `font` and `ink`, without a line through it.
    pub(crate) fn new(text: &'run str, font: &'run Font, ink: Ink) -> Self {
        Self {
            text,
            font,
            ink,
            strike: None,
        }
    }

    fn measure(&self, context: &MemoryContext) -> Result<SIZE> {
        let _font = context.select(HGDIOBJ(self.font.0.0));
        measure(context, &utf16(self.text))
    }

    fn draw(&self, context: &MemoryContext, left: i32) -> Result<()> {
        let _font = context.select(HGDIOBJ(self.font.0.0));
        draw_at(context, &mut utf16(self.text), left)
    }
}

/// A 32-bit bitmap holding text in a single color over transparency, premultiplied.
#[derive(Debug)]
pub(crate) struct TextBitmap {
    section: PixelSection,
    size: SIZE,
}

impl TextBitmap {
    /// Renders `text` in `ink` in the middle of a new square bitmap of `square_side` pixels.
    pub(crate) fn render_centered(text: &str, font: &Font, ink: Ink, square_side: i32) -> Result<Self> {
        let context = MemoryContext::new()?;
        let _font = context.select(HGDIOBJ(font.0.0));
        let mut units = utf16(text);
        let size = SIZE {
            cx: square_side,
            cy: square_side,
        };
        let mut section = PixelSection::new(size)?;
        let _bitmap = context.select(HGDIOBJ(section.bitmap.0));
        draw_centered(&context, &mut units, size)?;
        // SAFETY: plain call.
        unsafe { GdiFlush() }.ok()?;
        // SAFETY: the section is alive and GDI is done writing to it.
        tint_by_coverage(unsafe { section.pixels() }, ink);
        Ok(Self { section, size })
    }

    /// Renders the runs before the caret and the runs after it one after the other on one line,
    /// each in its font and its color. Returns the bitmap and the distance from its left edge to
    /// the caret.
    pub(crate) fn render_line(
        before_caret: &[TextRun<'_>],
        after_caret: &[TextRun<'_>],
    ) -> Result<(Self, i32)> {
        let context = MemoryContext::new()?;
        let runs = [before_caret, after_caret].concat();
        let sizes = runs
            .iter()
            .map(|run| run.measure(&context))
            .collect::<Result<Vec<_>>>()?;
        let edges = run_edges(&sizes);
        let size = SIZE {
            cx: edges.last().copied().unwrap_or_default(),
            cy: sizes.iter().map(|size| size.cy).max().unwrap_or_default(),
        };
        let mut section = PixelSection::new(size)?;
        let _bitmap = context.select(HGDIOBJ(section.bitmap.0));
        for (run, &left) in runs.iter().zip(&edges).filter(|(run, _)| !run.ink.is_clear()) {
            run.draw(&context, left)?;
        }
        // SAFETY: plain call.
        unsafe { GdiFlush() }.ok()?;
        let columns = edges
            .windows(2)
            .map(|edge| to_index(edge[0])..to_index(edge[1]))
            .collect::<Vec<_>>();
        // SAFETY: the section is alive and GDI is done writing to it.
        let pixels = unsafe { section.pixels() };
        paint_runs(pixels, size, &runs, &columns);
        Ok((Self { section, size }, edges[before_caret.len()]))
    }

    /// Returns the text inside a transparent frame, `padding.cx` pixels wide at the left and the
    /// right and `padding.cy` pixels high at the top and the bottom.
    pub(crate) fn padded(mut self, padding: SIZE) -> Result<Self> {
        let size = SIZE {
            cx: self.size.cx + 2 * padding.cx,
            cy: self.size.cy + 2 * padding.cy,
        };
        let [width, horizontal, vertical] =
            [self.size.cx, padding.cx, padding.cy].map(|value| usize::try_from(value).unwrap_or(0));
        // SAFETY: no device context has the bitmap selected, so GDI does not draw into it.
        let pixels = pad(unsafe { self.section.pixels() }, width, horizontal, vertical);
        let mut section = PixelSection::new(size)?;
        // SAFETY: no device context has the new bitmap selected, so GDI does not draw into it.
        unsafe { section.pixels() }.copy_from_slice(&pixels);
        Ok(Self { section, size })
    }

    /// Puts `backdrop` behind the text: premultiplied BGRA pixels of the same size, top row first.
    pub(crate) fn lay_over(&mut self, backdrop: &[u32]) {
        // SAFETY: no device context has the bitmap selected, so GDI does not draw into it.
        lay_over(unsafe { self.section.pixels() }, backdrop);
    }

    /// Size of the bitmap in physical pixels.
    pub(crate) fn size(&self) -> SIZE {
        self.size
    }

    /// Raw bitmap handle, valid while `self` lives.
    pub(crate) fn handle(&self) -> HBITMAP {
        *self.section.bitmap
    }

    /// Makes this bitmap the whole content of the layered `window`, placing it at `origin`.
    pub(crate) fn present(&self, window: HWND, origin: POINT) -> Result<()> {
        let context = MemoryContext::new()?;
        let _bitmap = context.select(HGDIOBJ(self.section.bitmap.0));
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER.try_into().expect("blend op fits in a byte"),
            BlendFlags: 0,
            SourceConstantAlpha: u8::MAX,
            AlphaFormat: AC_SRC_ALPHA.try_into().expect("alpha format fits in a byte"),
        };
        let source_origin = POINT::default();
        // SAFETY: every pointer refers to a local that outlives the call.
        unsafe {
            UpdateLayeredWindow(
                window,
                None,
                Some(&raw const origin),
                Some(&raw const self.size),
                Some(context.handle),
                Some(&raw const source_origin),
                COLORREF::default(),
                Some(&raw const blend),
                ULW_ALPHA,
            )
        }
    }
}

/// Renders `glyph` of the system icon font in `color`, `glyph_side` pixels high, in the middle of
/// a square of `side` pixels.
pub(crate) fn glyph_bitmap(glyph: &str, glyph_side: i32, side: i32, color: Rgb) -> Result<TextBitmap> {
    let font = Font::first_installed(&ICON_FACES, FontHeight::Em(glyph_side))?;
    TextBitmap::render_centered(glyph, &font, color.into(), side)
}

/// Renders `glyph` of the system icon font as a square icon of `side` pixels in `color`.
pub(crate) fn glyph_icon(glyph: &str, side: i32, color: Rgb) -> Result<Owned<HICON>> {
    let bitmap = glyph_bitmap(glyph, side, side, color)?;
    let bitmap_size = bitmap.size();
    // SAFETY: plain call; the bitmap is owned right after.
    let ignored_mask = unsafe {
        CreateBitmap(
            bitmap_size.cx,
            bitmap_size.cy,
            MONOCHROME_PLANES,
            MONOCHROME_BITS,
            None,
        )
    };
    if ignored_mask.is_invalid() {
        return Err(Error::from_thread());
    }
    // SAFETY: CreateBitmap returned a valid bitmap that nothing else owns.
    let ignored_mask = unsafe { Owned::new(ignored_mask) };
    let info = ICONINFO {
        fIcon: true.into(),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: *ignored_mask,
        hbmColor: bitmap.handle(),
    };
    // SAFETY: both bitmaps are alive and unselected; the system copies them into the icon.
    let icon = unsafe { CreateIconIndirect(&raw const info) }?;
    // SAFETY: CreateIconIndirect returned a valid icon that nothing else owns.
    Ok(unsafe { Owned::new(icon) })
}

fn measure(context: &MemoryContext, units: &[u16]) -> Result<SIZE> {
    let mut size = SIZE::default();
    // SAFETY: the text and the output size are valid for the call.
    unsafe { GetTextExtentPoint32W(context.handle, units, &raw mut size) }.ok()?;
    Ok(size)
}

fn utf16(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Returns the left edge of each run of `sizes` on one line, then the right edge of the last.
fn run_edges(sizes: &[SIZE]) -> Vec<i32> {
    iter::once(0)
        .chain(sizes.iter().scan(0, |right, size| {
            *right += size.cx;
            Some(*right)
        }))
        .collect()
}

/// Turns a distance in pixels into an index of a pixel, 0 for a negative distance.
fn to_index(pixels: i32) -> usize {
    usize::try_from(pixels).unwrap_or(0)
}

/// Recolors the letters of each of `runs` at its `columns` of `pixels`, a bitmap of `size`, to
/// the ink of the run, and draws the line through each run that has one.
fn paint_runs(pixels: &mut [u32], size: SIZE, runs: &[TextRun<'_>], columns: &[Range<usize>]) {
    let [width, height] = [size.cx, size.cy].map(to_index);
    let spans = runs
        .iter()
        .zip(columns)
        .map(|(run, run_columns)| (run_columns.clone(), run.ink))
        .collect::<Vec<_>>();
    tint_spans(pixels, width, &spans);
    for (run, run_columns) in runs.iter().zip(columns) {
        if let Some(strike) = run.strike {
            fill_band(pixels, width, run_columns.clone(), strike_rows(height), strike);
        }
    }
}

/// Draws `units` in white on the transparent bitmap of `context`, `left` pixels from its left edge.
/// Draws nothing for no units.
fn draw_at(context: &MemoryContext, units: &mut [u16], left: i32) -> Result<()> {
    if units.is_empty() {
        return Ok(());
    }
    // SAFETY: plain calls on a context this module owns; the text outlives the call.
    let is_drawn = unsafe {
        SetTextColor(context.handle, COLORREF(Rgb::WHITE.to_colorref()));
        SetBkMode(context.handle, TRANSPARENT);
        TextOutW(context.handle, left, 0, units)
    };
    if is_drawn.as_bool() {
        Ok(())
    } else {
        Err(Error::from_thread())
    }
}

/// Draws `units` in white in the middle of the transparent bitmap of `context`, which is `size`.
fn draw_centered(context: &MemoryContext, units: &mut [u16], size: SIZE) -> Result<()> {
    let mut bounds = RECT {
        left: 0,
        top: 0,
        right: size.cx,
        bottom: size.cy,
    };
    let format = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
    // SAFETY: plain calls on a context this module owns; the text and rectangle outlive the call.
    let text_height = unsafe {
        SetTextColor(context.handle, COLORREF(Rgb::WHITE.to_colorref()));
        SetBkMode(context.handle, TRANSPARENT);
        DrawTextW(context.handle, units, &raw mut bounds, format)
    };
    if text_height == 0 {
        Err(Error::from_thread())
    } else {
        Ok(())
    }
}

/// A DIB section bitmap whose pixel memory this process can write.
#[derive(Debug)]
pub(crate) struct PixelSection {
    bitmap: Owned<HBITMAP>,
    bits: *mut u32,
    pixel_count: usize,
}

impl PixelSection {
    /// Creates a top-down 32-bit bitmap of `size`, every pixel transparent black.
    pub(crate) fn new(size: SIZE) -> Result<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>()
                    .try_into()
                    .expect("header size fits in u32"),
                biWidth: size.cx,
                biHeight: -size.cy,
                biPlanes: 1,
                biBitCount: BITS_PER_PIXEL,
                biCompression: BI_RGB.0,
                ..BITMAPINFOHEADER::default()
            },
            ..BITMAPINFO::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: the header describes a top-down 32-bit bitmap; `bits` receives its pixel memory.
        let bitmap =
            unsafe { CreateDIBSection(None, &raw const info, DIB_RGB_COLORS, &raw mut bits, None, 0) }?;
        // SAFETY: CreateDIBSection returned a valid bitmap that nothing else owns.
        let bitmap = unsafe { Owned::new(bitmap) };
        let pixel_count = usize::try_from(size.cx).unwrap_or(0) * usize::try_from(size.cy).unwrap_or(0);
        Ok(Self {
            bitmap,
            bits: bits.cast(),
            pixel_count,
        })
    }

    /// Pixels of the section, top row first.
    ///
    /// # Safety
    ///
    /// GDI must not be drawing into the bitmap while the slice is used.
    unsafe fn pixels(&mut self) -> &mut [u32] {
        // SAFETY: the section holds `pixel_count` 32-bit pixels, alive as long as `self.bitmap`.
        unsafe { std::slice::from_raw_parts_mut(self.bits, self.pixel_count) }
    }
}

/// A memory device context, deleted on drop.
pub(crate) struct MemoryContext {
    handle: HDC,
}

impl MemoryContext {
    pub(crate) fn new() -> Result<Self> {
        // SAFETY: plain call; the context is owned right after.
        let handle = unsafe { CreateCompatibleDC(None) };
        if handle.is_invalid() {
            return Err(Error::from_thread());
        }
        Ok(Self { handle })
    }

    pub(crate) fn handle(&self) -> HDC {
        self.handle
    }

    /// Selects `object` until the returned selection drops.
    pub(crate) fn select(&self, object: HGDIOBJ) -> Selection<'_> {
        // SAFETY: `object` is a live GDI object owned by the caller for the selection's lifetime.
        let previous = unsafe { SelectObject(self.handle, object) };
        Selection {
            context: self.handle,
            previous,
            lifetime: PhantomData,
        }
    }
}

impl Drop for MemoryContext {
    fn drop(&mut self) {
        // A failed delete during drop leaves nothing to recover.
        // SAFETY: the context was created by this type and all selections were restored.
        let _ = unsafe { DeleteDC(self.handle) };
    }
}

/// Restores the previously selected object when dropped.
pub(crate) struct Selection<'context> {
    context: HDC,
    previous: HGDIOBJ,
    lifetime: PhantomData<&'context MemoryContext>,
}

impl Drop for Selection<'_> {
    fn drop(&mut self) {
        // SAFETY: restores the object that was selected before; the context is still alive.
        unsafe { SelectObject(self.context, self.previous) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_edges_give_the_left_of_each_run_and_the_end_of_the_line() {
        let sizes = [
            SIZE { cx: 30, cy: 20 },
            SIZE { cx: 0, cy: 20 },
            SIZE { cx: 45, cy: 20 },
        ];

        let edges = run_edges(&sizes);

        assert_eq!(edges, [0, 30, 30, 75]);
    }
}
