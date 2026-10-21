//! The memory bitmaps and the memory device contexts that GDI draws into.

use std::ffi::c_void;
use std::marker::PhantomData;

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
    HBITMAP, HDC, HGDIOBJ, SelectObject,
};
use windows::core::{Error, Owned, Result};

use crate::struct_size;

const BITS_PER_PIXEL: u16 = 32;

/// A DIB section bitmap whose pixel memory this process can write.
#[derive(Debug)]
pub(super) struct PixelSection {
    pub(super) bitmap: Owned<HBITMAP>,
    bits: *mut u32,
    pixel_count: usize,
}

impl PixelSection {
    /// Creates a top-down 32-bit bitmap of `size`, every pixel transparent black.
    pub(super) fn new(size: SIZE) -> Result<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: struct_size::<BITMAPINFOHEADER>(),
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
    pub(super) unsafe fn pixels(&mut self) -> &mut [u32] {
        // SAFETY: the section holds `pixel_count` 32-bit pixels, alive as long as `self.bitmap`.
        unsafe { std::slice::from_raw_parts_mut(self.bits, self.pixel_count) }
    }
}

/// A memory device context, deleted on drop.
pub(crate) struct MemoryContext {
    pub(super) handle: HDC,
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
    pub(super) fn select(&self, object: HGDIOBJ) -> Selection<'_> {
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
pub(super) struct Selection<'context> {
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
