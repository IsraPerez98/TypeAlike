//! The capture of a window with `Windows.Graphics.Capture`, on a WARP device of its own.
//!
//! The capture holds only that window, so the windows of TypeAlike, such as the ghost text and the
//! badge, are never in it. A copy of a part of the window holds rows of BGRA pixels without
//! padding.

use std::num::TryFromIntError;
use std::slice;
use std::sync::mpsc;
use std::time::Duration;

use typealike_core::desktop::ScreenRect;
use windows::Foundation::Metadata::ApiInformation;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HMODULE, HWND, POINT, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_WARP;
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
use windows::core::{Interface, factory, h};

use crate::display::screen_rect;
use crate::struct_size;

/// `Windows.Graphics.Capture` gives the first frame in less than 100 ms. A longer wait means
/// that the capture failed.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
/// Each read uses one frame.
const FRAME_BUFFERS: i32 = 1;
/// The bytes of one BGRA pixel.
pub(crate) const BYTES_PER_PIXEL: usize = 4;

/// Why a capture failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum CaptureError {
    #[error("a call to Windows failed")]
    Windows(#[from] windows::core::Error),
    #[error("a size is out of range")]
    Size(#[from] TryFromIntError),
    #[error("a byte count of the pixels does not fit in memory")]
    PixelCount,
    #[error("Direct3D gave no device")]
    NoDevice,
    #[error("the capture gave no frame in time")]
    NoFrame(#[from] mpsc::RecvTimeoutError),
}

/// Whether Windows can capture a window without the border that it draws around a window during
/// each capture. Windows 10 always draws the border.
pub(crate) fn can_capture_without_border() -> bool {
    let can_capture = GraphicsCaptureSession::IsSupported().unwrap_or(false);
    let can_hide_border = ApiInformation::IsPropertyPresent(
        h!("Windows.Graphics.Capture.GraphicsCaptureSession"),
        h!("IsBorderRequired"),
    )
    .unwrap_or(false);
    can_capture && can_hide_border
}

/// A rectangle of pixels from the top left corner of an image or a window: the columns
/// `left..right` of the rows `top..bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PixelRect {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) right: i32,
    pub(crate) bottom: i32,
}

impl PixelRect {
    /// The rectangle `rect` of the screen, counted from `origin`.
    pub(crate) fn from_screen(rect: ScreenRect, origin: POINT) -> Self {
        Self {
            left: rect.left.saturating_sub(origin.x),
            top: rect.top.saturating_sub(origin.y),
            right: rect.right.saturating_sub(origin.x),
            bottom: rect.bottom.saturating_sub(origin.y),
        }
    }

    /// The rectangle of the screen where this rectangle, counted from `origin`, is.
    pub(crate) fn on_screen(self, origin: POINT) -> ScreenRect {
        ScreenRect {
            left: self.left.saturating_add(origin.x),
            top: self.top.saturating_add(origin.y),
            right: self.right.saturating_add(origin.x),
            bottom: self.bottom.saturating_add(origin.y),
        }
    }

    pub(crate) fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    pub(crate) fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }
}

/// Captures windows with `Windows.Graphics.Capture`, on a Direct3D device of its own. The device is
/// WARP, the software rasterizer of Direct3D, and not the graphics card: on a device of the graphics
/// card, Windows keeps a surface of the window's size in the GPU memory of the process for each
/// capture, also after the capture and the device drop.
pub(crate) struct Capturer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    capture_device: IDirect3DDevice,
}

impl Capturer {
    pub(crate) fn create() -> Result<Self, CaptureError> {
        let (mut device, mut context) = (None, None);
        // SAFETY: plain call. The outputs are locals that the call fills in.
        unsafe {
            D3D11CreateDevice(
                None::<&IDXGIAdapter>,
                D3D_DRIVER_TYPE_WARP,
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
            return Err(CaptureError::NoDevice);
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

    /// The device in the form that a pool of capture frames takes.
    pub(crate) fn capture_device(&self) -> &IDirect3DDevice {
        &self.capture_device
    }

    /// Captures one frame of `window`.
    pub(crate) fn capture_window(&self, window: HWND) -> Result<WindowFrame, CaptureError> {
        let bounds = window_bounds(window)?;
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        // SAFETY: plain call with the handle of a window.
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(window) }?;
        let session = CaptureSession::start(&self.capture_device, &item)?;
        let frame = session.first_frame()?;
        Ok(WindowFrame {
            texture: frame.texture,
            size: frame.size,
            origin: POINT {
                x: bounds.left,
                y: bounds.top,
            },
            _frame: frame.frame,
            _session: session,
        })
    }

    /// Copies the pixels of `rect` of `texture`, a texture of this device, into rows without
    /// padding.
    pub(crate) fn copy_pixels(
        &self,
        texture: &ID3D11Texture2D,
        rect: PixelRect,
    ) -> Result<Vec<u8>, CaptureError> {
        let width = u32::try_from(rect.width())?;
        let height = u32::try_from(rect.height())?;
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
        let staging = staging.ok_or(CaptureError::NoDevice)?;
        let region = D3D11_BOX {
            left: u32::try_from(rect.left)?,
            top: u32::try_from(rect.top)?,
            front: 0,
            right: u32::try_from(rect.right)?,
            bottom: u32::try_from(rect.bottom)?,
            back: 1,
        };
        // SAFETY: plain call on textures of this device. The region lies inside the source.
        unsafe {
            self.context
                .CopySubresourceRegion(&staging, 0, 0, 0, 0, texture, 0, Some(&raw const region));
        }
        let row_bytes = usize::try_from(width)?
            .checked_mul(BYTES_PER_PIXEL)
            .ok_or(CaptureError::PixelCount)?;
        let rows = usize::try_from(height)?;
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: plain call. `mapped` is a local that the call fills in.
        unsafe {
            self.context
                .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&raw mut mapped))
        }?;
        let pitch = usize::try_from(mapped.RowPitch)?;
        let pixels = mapped_length(pitch, rows, row_bytes).map(|length| {
            // SAFETY: the map gives `rows` rows `pitch` bytes apart, each with at least `row_bytes`
            // bytes. They stay valid until the unmap below.
            let mapped_bytes = unsafe { slice::from_raw_parts(mapped.pData.cast::<u8>(), length) };
            without_padding(mapped_bytes, pitch, row_bytes)
        });
        // SAFETY: ends the map above.
        unsafe { self.context.Unmap(&staging, 0) };
        pixels.ok_or(CaptureError::PixelCount)
    }
}

/// One frame of a window, which keeps its capture until it drops.
pub(crate) struct WindowFrame {
    /// The pixels of the window, on the device of the capturer.
    pub(crate) texture: ID3D11Texture2D,
    /// The size of the window in the texture.
    pub(crate) size: SizeInt32,
    /// Where the top left pixel of the texture is on the screen.
    pub(crate) origin: POINT,
    /// Keeps the texture for this read, so that the pool does not use it again.
    _frame: Direct3D11CaptureFrame,
    _session: CaptureSession,
}

/// The frame of `window` on the screen without its invisible borders, which is the area that its
/// capture shows.
pub(crate) fn window_bounds(window: HWND) -> Result<ScreenRect, CaptureError> {
    let mut bounds = RECT::default();
    // SAFETY: plain call. `bounds` is a local of the size that the call writes.
    unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&raw mut bounds).cast(),
            struct_size::<RECT>(),
        )
    }?;
    Ok(screen_rect(bounds))
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
    frame: Direct3D11CaptureFrame,
}

impl CaptureSession {
    /// Starts the capture of `item` on `device`, without the mouse pointer and without a border.
    fn start(device: &IDirect3DDevice, item: &GraphicsCaptureItem) -> Result<Self, CaptureError> {
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
    fn first_frame(&self) -> Result<CapturedFrame, CaptureError> {
        self.frames.recv_timeout(CAPTURE_TIMEOUT)?;
        let frame = self.pool.TryGetNextFrame()?;
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        // SAFETY: plain call on the surface of a frame of this capture.
        let texture = unsafe { access.GetInterface::<ID3D11Texture2D>() }?;
        Ok(CapturedFrame {
            texture,
            size: frame.ContentSize()?,
            frame,
        })
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        // A failed close leaves nothing to recover, and the release also stops the capture.
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// The length of `rows` rows that start `pitch` bytes apart, up to the end of the first
/// `row_bytes` of the last row. `None` without rows, or when the length does not fit in `usize`.
fn mapped_length(pitch: usize, rows: usize, row_bytes: usize) -> Option<usize> {
    pitch.checked_mul(rows.checked_sub(1)?)?.checked_add(row_bytes)
}

/// Copies the first `row_bytes` of each row of `bytes`, whose rows start `pitch` bytes apart.
fn without_padding(bytes: &[u8], pitch: usize, row_bytes: usize) -> Vec<u8> {
    let mut pixels = Vec::with_capacity(bytes.len());
    for row in bytes.chunks(pitch.max(1)) {
        pixels.extend_from_slice(&row[..row_bytes.min(row.len())]);
    }
    pixels
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn rect(left: i32, top: i32, right: i32, bottom: i32) -> PixelRect {
        PixelRect {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn mapped_length_spans_the_padded_rows_and_the_last_row() {
        assert_eq!(mapped_length(16, 3, 12), Some(44));
    }

    #[test]
    fn mapped_length_without_rows_or_past_the_address_space_is_none() {
        let cases = [(16, 0, 12), (usize::MAX, 3, 12), (16, 2, usize::MAX)];

        let lengths = cases.map(|(pitch, rows, row_bytes)| mapped_length(pitch, rows, row_bytes));

        assert_eq!(lengths, [None; 3]);
    }

    #[test]
    fn padding_at_the_end_of_each_row_is_dropped() {
        let bytes = [1, 2, 3, 0, 4, 5, 6, 0, 7, 8, 9];

        let pixels = without_padding(&bytes, 4, 3);

        assert_eq!(pixels, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn rectangle_moves_to_the_screen_and_back() {
        let origin = POINT { x: -1920, y: 300 };
        let field = ScreenRect {
            left: -1800,
            top: 900,
            right: -1200,
            bottom: 930,
        };

        let in_window = PixelRect::from_screen(field, origin);

        assert_eq!(
            (in_window, in_window.on_screen(origin)),
            (rect(120, 600, 720, 630), field)
        );
    }
}
