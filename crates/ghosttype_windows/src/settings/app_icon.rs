//! The icon of an app, from the file of its program, as the bytes of an icon file that the
//! settings window can show.

use std::ffi::c_void;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, DIB_RGB_COLORS, GetDIBits, GetObjectW, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::Shell::ExtractIconExW;
use windows::Win32::UI::WindowsAndMessaging::{GetIconInfo, HICON, ICONINFO};
use windows::core::{HSTRING, Owned};

use crate::gdi::{BITS_PER_PIXEL, MemoryContext, top_down_bitmap_format};

const BYTES_PER_PIXEL: usize = 4;
const ALPHA: usize = 3;
const OPAQUE: u8 = u8::MAX;
const ICON_DIRECTORY_BYTES: u32 = 6;
const ICON_ENTRY_BYTES: u32 = 16;
const BITMAP_HEADER_BYTES: u32 = 40;
const ICON_TYPE: u16 = 1;
/// An icon image has one color plane.
const COLOR_PLANES: u16 = 1;
/// Each row of the AND mask, one bit for each pixel, fills whole groups of 32 bits.
const MASK_ROW_ALIGNMENT_BITS: u32 = 32;
/// The resolution and the palette fields of the bitmap header, which an icon leaves at zero.
const UNUSED_RESOLUTION_AND_PALETTE_BYTES: usize = 16;
/// The width or the height of 256 pixels, which the one byte of an icon entry gives as zero.
const LARGEST_SIDE: u32 = 256;

/// Returns the large icon in the program file at `program_path`, as an icon file. `None` when the
/// file has no icon that GhostType can read.
pub(super) fn app_icon_file(program_path: &str) -> Option<Vec<u8>> {
    let mut large = HICON::default();
    // SAFETY: the path outlives the call, and the output pointer refers to a local.
    let count = unsafe { ExtractIconExW(&HSTRING::from(program_path), 0, Some(&raw mut large), None, 1) };
    if count == 0 || large.is_invalid() {
        return None;
    }
    // SAFETY: ExtractIconExW returned an icon that the caller owns.
    let icon = unsafe { Owned::new(large) };
    let mut info = ICONINFO::default();
    // SAFETY: the icon is valid, and the output pointer refers to a local.
    unsafe { GetIconInfo(*icon, &raw mut info) }.ok()?;
    // SAFETY: GetIconInfo made both bitmaps for the caller to delete.
    let (color, mask) = unsafe { (Owned::new(info.hbmColor), Owned::new(info.hbmMask)) };
    if color.is_invalid() {
        return None;
    }
    let (width, height) = bitmap_size(*color)?;
    let mut pixels = top_down_pixels(*color, width, height)?;
    let has_alpha = pixels
        .as_chunks::<BYTES_PER_PIXEL>()
        .0
        .iter()
        .any(|pixel| pixel[ALPHA] != 0);
    if !has_alpha {
        let mask_pixels = top_down_pixels(*mask, width, height)?;
        take_alpha_from_mask(&mut pixels, &mask_pixels);
    }
    Some(icon_file(width, height, &pixels))
}

fn bitmap_size(bitmap: HBITMAP) -> Option<(u32, u32)> {
    let mut description = BITMAP::default();
    let size = i32::try_from(size_of::<BITMAP>()).ok()?;
    // SAFETY: the output pointer refers to a local of the size passed.
    let written = unsafe {
        GetObjectW(
            HGDIOBJ::from(bitmap),
            size,
            Some((&raw mut description).cast::<c_void>()),
        )
    };
    if written == 0 {
        return None;
    }
    Some((
        u32::try_from(description.bmWidth).ok()?,
        u32::try_from(description.bmHeight).ok()?,
    ))
}

/// Reads the pixels of `bitmap` as 32-bit BGRA, top row first.
fn top_down_pixels(bitmap: HBITMAP, width: u32, height: u32) -> Option<Vec<u8>> {
    let context = MemoryContext::new().ok()?;
    let mut bitmap_format = top_down_bitmap_format(i32::try_from(width).ok()?, i32::try_from(height).ok()?);
    let byte_count = usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)?
        .checked_mul(BYTES_PER_PIXEL)?;
    let mut pixels = vec![0_u8; byte_count];
    // SAFETY: the buffer holds `height` rows of `width` 32-bit pixels, as the header describes.
    let rows = unsafe {
        GetDIBits(
            context.handle(),
            bitmap,
            0,
            height,
            Some(pixels.as_mut_ptr().cast::<c_void>()),
            &raw mut bitmap_format,
            DIB_RGB_COLORS,
        )
    };
    (u32::try_from(rows).ok() == Some(height)).then_some(pixels)
}

/// Makes each pixel of `pixels` opaque where the AND mask `mask` is black, and clear elsewhere,
/// for an icon without alpha.
fn take_alpha_from_mask(pixels: &mut [u8], mask: &[u8]) {
    let mask_pixels = mask.as_chunks::<BYTES_PER_PIXEL>().0;
    for (pixel, mask_pixel) in pixels
        .as_chunks_mut::<BYTES_PER_PIXEL>()
        .0
        .iter_mut()
        .zip(mask_pixels)
    {
        let is_shown = mask_pixel[..ALPHA].iter().all(|&channel| channel == 0);
        pixel[ALPHA] = if is_shown { OPAQUE } else { 0 };
    }
}

/// Encodes `top_down_bgra`, `width` × `height` pixels of 32-bit BGRA with the top row first, as an
/// icon file with one image. The alpha channel gives the shape, so the AND mask is empty.
fn icon_file(width: u32, height: u32, top_down_bgra: &[u8]) -> Vec<u8> {
    let row_bytes = usize::try_from(width).unwrap_or(0) * BYTES_PER_PIXEL;
    let mask_row_bytes =
        usize::try_from(width.div_ceil(MASK_ROW_ALIGNMENT_BITS) * (MASK_ROW_ALIGNMENT_BITS / u8::BITS))
            .unwrap_or(0);
    let mask_bytes = mask_row_bytes * usize::try_from(height).unwrap_or(0);
    let header_bytes = usize::try_from(BITMAP_HEADER_BYTES).expect("40 fits in usize");
    let directory_bytes = usize::try_from(ICON_DIRECTORY_BYTES + ICON_ENTRY_BYTES).expect("22 fits in usize");
    let image_bytes = header_bytes + top_down_bgra.len() + mask_bytes;
    let image_size = u32::try_from(image_bytes).unwrap_or(u32::MAX);
    let mut file = Vec::with_capacity(image_bytes + directory_bytes);
    for value in [0, ICON_TYPE, 1] {
        file.extend_from_slice(&value.to_le_bytes());
    }
    file.extend_from_slice(&[entry_side(width), entry_side(height), 0, 0]);
    file.extend_from_slice(&COLOR_PLANES.to_le_bytes());
    file.extend_from_slice(&BITS_PER_PIXEL.to_le_bytes());
    file.extend_from_slice(&image_size.to_le_bytes());
    file.extend_from_slice(&(ICON_DIRECTORY_BYTES + ICON_ENTRY_BYTES).to_le_bytes());
    file.extend_from_slice(&BITMAP_HEADER_BYTES.to_le_bytes());
    file.extend_from_slice(&i32::try_from(width).unwrap_or(0).to_le_bytes());
    file.extend_from_slice(&i32::try_from(height.saturating_mul(2)).unwrap_or(0).to_le_bytes());
    file.extend_from_slice(&COLOR_PLANES.to_le_bytes());
    file.extend_from_slice(&BITS_PER_PIXEL.to_le_bytes());
    file.extend_from_slice(&BI_RGB.0.to_le_bytes());
    file.extend_from_slice(
        &u32::try_from(top_down_bgra.len() + mask_bytes)
            .unwrap_or(0)
            .to_le_bytes(),
    );
    file.extend_from_slice(&[0; UNUSED_RESOLUTION_AND_PALETTE_BYTES]);
    for row in top_down_bgra.chunks_exact(row_bytes.max(1)).rev() {
        file.extend_from_slice(row);
    }
    file.resize(file.len() + mask_bytes, 0);
    file
}

/// The width or the height of an icon entry, which has one byte for it.
fn entry_side(side: u32) -> u8 {
    if side >= LARGEST_SIDE {
        0
    } else {
        u8::try_from(side).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [u8; 4] = [0, 0, 255, 255];
    const BLUE: [u8; 4] = [255, 0, 0, 255];

    #[test]
    fn icon_file_directory_has_one_image_entry() {
        let pixels = [RED, BLUE].concat();

        let file = icon_file(1, 2, &pixels);

        assert_eq!(&file[..6], &[0, 0, 1, 0, 1, 0]);
        assert_eq!(&file[6..10], &[1, 2, 0, 0]);
        assert_eq!(&file[18..22], &22_u32.to_le_bytes());
    }

    #[test]
    fn icon_file_rows_go_bottom_up() {
        let pixels = [RED, BLUE].concat();

        let file = icon_file(1, 2, &pixels);

        assert_eq!(&file[30..34], &4_i32.to_le_bytes());
        assert_eq!(&file[62..70], &[BLUE, RED].concat()[..]);
        assert_eq!(file.len(), 62 + 8 + 8);
    }

    #[test]
    fn largest_side_is_zero_in_the_entry() {
        let sides = [32, 255, 256].map(entry_side);

        assert_eq!(sides, [32, 255, 0]);
    }

    #[test]
    fn black_mask_makes_a_pixel_opaque() {
        let mut pixels = [10, 20, 30, 0, 40, 50, 60, 0];
        let mask = [0, 0, 0, 0, 255, 255, 255, 0];

        take_alpha_from_mask(&mut pixels, &mask);

        assert_eq!(pixels, [10, 20, 30, 255, 40, 50, 60, 0]);
    }
}
