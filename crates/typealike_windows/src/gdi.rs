//! Renders text and glyphs into premultiplied-alpha bitmaps and icons, for the overlay, the badge
//! and the tray, and gives the memory device context and the bitmap format that the app icons read
//! their pixels with.

mod font;
mod memory;
mod text_bitmap;

pub(crate) use self::font::{Font, FontWeight};
pub(crate) use self::memory::{BITS_PER_PIXEL, MemoryContext, top_down_bitmap_format};
pub(crate) use self::text_bitmap::{TextBitmap, TextRun, line_width};
