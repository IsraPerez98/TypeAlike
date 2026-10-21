//! Renders text and glyphs into premultiplied-alpha bitmaps and icons, for the overlay, the badge
//! and the tray, and gives the memory device context that the app icons read their pixels with.

mod font;
mod memory;
mod text_bitmap;

pub(crate) use self::font::{Font, FontHeight, FontWeight};
pub(crate) use self::memory::MemoryContext;
pub(crate) use self::text_bitmap::{TextBitmap, TextRun, glyph_bitmap, glyph_icon};
