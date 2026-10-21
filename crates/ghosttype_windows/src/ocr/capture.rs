//! The capture of a band of a window.
//!
//! `Windows.Graphics.Capture` captures the window on a Direct3D device of its own. The band holds
//! rows of BGRA pixels without padding.

use std::hash::{DefaultHasher, Hasher};
use std::ops::Range;
use std::slice;
use std::sync::mpsc;
use std::time::Duration;

use ghosttype_core::desktop::ScreenRect;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND, POINT, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter, IDXGIDevice};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::core::{Interface, factory};

use super::OcrError;
use super::geometry::{PixelRect, band_in_frame, reach_for};
use crate::uia;

/// `Windows.Graphics.Capture` gives the first frame in less than 100 ms. A longer wait means
/// that the capture failed.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
/// Each read uses one frame.
const FRAME_BUFFERS: i32 = 1;
/// The bytes of one BGRA pixel.
const BYTES_PER_PIXEL: usize = 4;

/// Captures windows with `Windows.Graphics.Capture`, on a Direct3D device of its own.
pub(super) struct Capturer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    capture_device: IDirect3DDevice,
}

impl Capturer {
    pub(super) fn create() -> Result<Self, OcrError> {
        let (mut device, mut context) = (None, None);
        // SAFETY: plain call. The outputs are locals that the call fills in.
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&raw mut device),
                None,
                Some(&raw mut context),
            )
        }?;
        let (Some(device), Some(context)) = (device, context) else {
            return Err(OcrError::NoDevice);
        };
        let dxgi_device: IDXGIDevice = device.cast()?;
        // SAFETY: plain call on a valid device.
        let capture_device = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }?.cast()?;
        Ok(Self {
            device,
            context,
            capture_device,
        })
    }

    /// Captures the band of `window` from `BAND_DIPS` above `field` to `BAND_DIPS` below it,
    /// as wide as the window and at most `max_side` pixels in each direction, at full resolution.
    pub(super) fn capture_band(
        &self,
        window: HWND,
        field: ScreenRect,
        max_side: i32,
    ) -> Result<Band, OcrError> {
        let bounds = window_bounds(window)?;
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        // SAFETY: plain call with the handle of a window.
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(window) }?;
        let capture = CaptureSession::start(&self.capture_device, &item)?;
        let frame = capture.first_frame()?;
        // SAFETY: plain call with the handle of a window.
        let reach = reach_for(unsafe { GetDpiForWindow(window) });
        let field_in_window = PixelRect::from_screen(
            field,
            POINT {
                x: bounds.left,
                y: bounds.top,
            },
        );
        let band = band_in_frame(field_in_window, frame.size, reach, max_side)
            .ok_or(OcrError::FieldOutsideWindow)?;
        Ok(Band {
            pixels: self.copy_pixels(&frame.texture, band)?,
            width: usize::try_from(band.width())?,
            height: usize::try_from(band.height())?,
            origin: POINT {
                x: bounds.left.saturating_add(band.left),
                y: bounds.top.saturating_add(band.top),
            },
        })
    }

    /// Copies the pixels of `band` of `texture`, a texture of this device, into rows without
    /// padding.
    fn copy_pixels(&self, texture: &ID3D11Texture2D, band: PixelRect) -> Result<Vec<u8>, OcrError> {
        let width = u32::try_from(band.width())?;
        let height = u32::try_from(band.height())?;
        let description = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0.cast_unsigned(),
            MiscFlags: 0,
        };
        let mut staging = None;
        // SAFETY: plain call. `staging` is a local that the call fills in.
        unsafe {
            self.device
                .CreateTexture2D(&raw const description, None, Some(&raw mut staging))
        }?;
        let staging = staging.ok_or(OcrError::NoDevice)?;
        let region = D3D11_BOX {
            left: u32::try_from(band.left)?,
            top: u32::try_from(band.top)?,
            front: 0,
            right: u32::try_from(band.right)?,
            bottom: u32::try_from(band.bottom)?,
            back: 1,
        };
        // SAFETY: plain call on textures of this device. The region lies inside the source.
        unsafe {
            self.context
                .CopySubresourceRegion(&staging, 0, 0, 0, 0, texture, 0, Some(&raw const region));
        }
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: plain call. `mapped` is a local that the call fills in.
        unsafe {
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&raw mut mapped))
        }?;
        let row_bytes = usize::try_from(width)? * BYTES_PER_PIXEL;
        let pitch = usize::try_from(mapped.RowPitch)?;
        let mapped_length = pitch * (usize::try_from(height)? - 1) + row_bytes;
        // SAFETY: the map gives `height` rows `pitch` bytes apart, each with at least `row_bytes`
        // bytes. They stay valid until the unmap below.
        let mapped_bytes = unsafe { slice::from_raw_parts(mapped.pData.cast::<u8>(), mapped_length) };
        let pixels = without_padding(mapped_bytes, pitch, row_bytes);
        // SAFETY: ends the map above.
        unsafe { self.context.Unmap(&staging, 0) };
        Ok(pixels)
    }
}

/// The frame of `window` on the screen without its invisible borders, which is the area that its
/// capture shows.
fn window_bounds(window: HWND) -> Result<ScreenRect, OcrError> {
    let mut bounds = RECT::default();
    // SAFETY: plain call. `bounds` is a local of the size that the call writes.
    unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut bounds).cast(),
            u32::try_from(size_of::<RECT>())?,
        )
    }?;
    Ok(uia::screen_rect(bounds))
}

/// A capture of a window, which stops when it drops.
struct CaptureSession {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    frames: mpsc::Receiver<()>,
}

/// A frame of a capture.
struct CapturedFrame {
    texture: ID3D11Texture2D,
    size: SizeInt32,
    /// Keeps the texture for this read, so that the pool does not use it again.
    _frame: Direct3D11CaptureFrame,
}

impl CaptureSession {
    /// Starts the capture of `item` on `device`, without the mouse pointer and without a border.
    fn start(device: &IDirect3DDevice, item: &GraphicsCaptureItem) -> Result<Self, OcrError> {
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            device,
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            FRAME_BUFFERS,
            item.Size()?,
        )?;
        let (sender, frames) = mpsc::channel();
        pool.FrameArrived(&TypedEventHandler::new(move |_, _| {
            // The receiver is gone when the read stopped to wait. Then the frame is not necessary.
            let _ = sender.send(());
            Ok(())
        }))?;
        let capture = Self {
            session: pool.CreateCaptureSession(item)?,
            pool,
            frames,
        };
        capture.session.SetIsCursorCaptureEnabled(false)?;
        capture.session.SetIsBorderRequired(false)?;
        capture.session.StartCapture()?;
        Ok(capture)
    }

    /// Waits for the first frame of the capture.
    fn first_frame(&self) -> Result<CapturedFrame, OcrError> {
        self.frames.recv_timeout(CAPTURE_TIMEOUT)?;
        let frame = self.pool.TryGetNextFrame()?;
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        // SAFETY: plain call on the surface of a frame of this capture.
        let texture = unsafe { access.GetInterface::<ID3D11Texture2D>() }?;
        Ok(CapturedFrame {
            texture,
            size: frame.ContentSize()?,
            _frame: frame,
        })
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        // A failed close leaves nothing to recover, and the release of the objects also stops the
        // capture.
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// The pixels of a band of a window: rows of BGRA pixels without padding.
pub(super) struct Band {
    pub(super) pixels: Vec<u8>,
    pub(super) width: usize,
    pub(super) height: usize,
    /// Where the top left pixel of the band is on the screen.
    pub(super) origin: POINT,
}

impl Band {
    /// Sets the pixels of `field`, a rectangle on the screen, to zero. Thus the typed text and the
    /// caret in the field do not change the hash, and the OCR does not read them.
    pub(super) fn hide_field(&mut self, field: ScreenRect) {
        clear(
            &mut self.pixels,
            self.width,
            PixelRect::from_screen(field, self.origin),
        );
    }

    /// The hash of the pixels, which is the same for two bands with the same pixels.
    pub(super) fn hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        hasher.write(&self.pixels);
        hasher.finish()
    }
}

/// Copies the first `row_bytes` of each row of `bytes`, whose rows start `pitch` bytes apart.
fn without_padding(bytes: &[u8], pitch: usize, row_bytes: usize) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(bytes.len());
    for row in bytes.chunks(pitch.max(1)) {
        pixels.extend_from_slice(&row[..row_bytes.min(row.len())]);
    }
    pixels
}

/// Sets the pixels of `rect` in `pixels`, rows of BGRA pixels `width` pixels wide, to zero.
fn clear(pixels: &mut [u8], width: usize, rect: PixelRect) {
    let row_bytes = width * BYTES_PER_PIXEL;
    if row_bytes == 0 {
        return;
    }
    let rows = clamped(rect.top..rect.bottom, pixels.len() / row_bytes);
    let columns = clamped(rect.left..rect.right, width);
    for row in pixels.chunks_exact_mut(row_bytes).take(rows.end).skip(rows.start) {
        row[columns.start * BYTES_PER_PIXEL..columns.end * BYTES_PER_PIXEL].fill(0);
    }
}

/// The indexes of `range` that lie in `0..limit`.
fn clamped(range: Range<i32>, limit: usize) -> Range<usize> {
    let to_index = |value: i32| usize::try_from(value).unwrap_or(0).min(limit);
    let start = to_index(range.start);
    start..to_index(range.end).max(start)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::tests::rect;

    #[test]
    fn padding_at_the_end_of_each_row_is_dropped() {
        let bytes = [1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9];

        let pixels = without_padding(&bytes, 4, 3);

        assert_eq!(pixels, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn clear_sets_only_the_pixels_of_the_rectangle_to_zero() {
        let mut pixels = vec![1; 3 * 3 * BYTES_PER_PIXEL];

        clear(&mut pixels, 3, rect(1, 1, 5, 2));

        let mut expected = vec![1; 3 * 3 * BYTES_PER_PIXEL];
        expected[16..24].fill(0);
        assert_eq!(pixels, expected);
    }

    #[test]
    fn rectangle_outside_of_the_image_clears_nothing() {
        let mut pixels = vec![1; 2 * 2 * BYTES_PER_PIXEL];

        clear(&mut pixels, 2, rect(-10, -10, -1, -1));

        assert_eq!(pixels, vec![1; 2 * 2 * BYTES_PER_PIXEL]);
    }
}
