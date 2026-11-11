//! The pixels of the screen after the caret, for a field whose text after the caret UI Automation
//! does not give: the editor of VS Code and the input box of a terminal of xterm.js.
//!
//! `Windows.Graphics.Capture` captures only the foreground window, so the windows of TypeAlike,
//! such as the ghost text, the card and the badge, are never in the pixels. The start of a capture
//! takes 40 to 100 ms, so the capture of a window stays open while the looks at the caret in it
//! need its pixels, and each look copies only the area after the caret from the newest frame. The
//! pixels go to core, and nothing keeps or logs them.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use typealike_core::desktop::{PixelColor, ScreenPixels, ScreenRect};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::core::{Interface, factory};

use crate::window::WindowId;
use crate::window_capture::{
    BYTES_PER_PIXEL, CaptureError, Capturer, PixelRect, can_capture_without_border, window_bounds,
};

/// The longest wait for the first frame of a new capture.
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_millis(500);
/// The buffers of the frames: one for the newest frame, and one for the frame after it.
const FRAME_BUFFERS: i32 = 2;
/// A capture that no look used for this long stops, so that TypeAlike does not capture a window
/// that the user only reads.
pub(crate) const IDLE_CAPTURE_TIMEOUT: Duration = Duration::from_secs(60);

/// Captures the pixels after the caret, on the thread of the caret reader.
#[derive(Default)]
pub(crate) struct CaretPixels {
    capturer: CapturerState,
    capture: Option<LiveCapture>,
}

impl CaretPixels {
    /// Reads the pixels of `area` of the screen in `window`, the foreground window. Starts a
    /// capture of `window` when none is open for it, and starts it again when the window changed
    /// its size. `None` when the capture fails, or when the window does not show all of `area`.
    pub(crate) fn read(&mut self, window: HWND, area: ScreenRect) -> Option<ScreenPixels> {
        match self.read_frame(window, area)? {
            FrameRead::Pixels(pixels) => Some(pixels),
            FrameRead::Outside => None,
            FrameRead::Resized => {
                self.capture = None;
                match self.read_frame(window, area)? {
                    FrameRead::Pixels(pixels) => Some(pixels),
                    FrameRead::Outside | FrameRead::Resized => None,
                }
            }
        }
    }

    /// Starts a capture of `window` when none is open for it, so that the first look at the caret
    /// in it does not wait for the start.
    pub(crate) fn prepare(&mut self, window: HWND) {
        self.open(window);
    }

    /// Stops the open capture, because the looks at the caret do not need its pixels.
    pub(crate) fn stop(&mut self) {
        self.capture = None;
    }

    fn read_frame(&mut self, window: HWND, area: ScreenRect) -> Option<FrameRead> {
        self.open(window);
        let (CapturerState::Ready(capturer), Some(capture)) = (&self.capturer, &self.capture) else {
            return None;
        };
        capture
            .read(capturer, window, area)
            .inspect_err(|error| tracing::debug!(%error, "failed to read the pixels after the caret"))
            .ok()
    }

    /// Opens a capture of `window` when no capture of it is open.
    fn open(&mut self, window: HWND) {
        let id = WindowId::from(window);
        if self.capture.as_ref().is_some_and(|capture| capture.window != id) {
            self.capture = None;
        }
        if self.capture.is_some() {
            return;
        }
        let Some(capturer) = self.capturer.ready() else {
            return;
        };
        let started = Instant::now();
        self.capture = LiveCapture::start(capturer, window)
            .inspect_err(|error| tracing::debug!(%error, "failed to start the capture of the window"))
            .ok();
        tracing::debug!(
            duration = ?started.elapsed(),
            is_started = self.capture.is_some(),
            "started the capture of the window"
        );
    }
}

/// The Direct3D device of the captures, which starts at the first capture.
#[derive(Default)]
enum CapturerState {
    #[default]
    NotStarted,
    Ready(Capturer),
    /// Windows cannot capture a window without a border, or the device did not start.
    Unavailable,
}

impl CapturerState {
    /// The capturer, which starts at the first call.
    fn ready(&mut self) -> Option<&Capturer> {
        if matches!(self, Self::NotStarted) {
            *self = if can_capture_without_border() {
                Capturer::create()
                    .inspect_err(|error| tracing::warn!(%error, "failed to start the capture of windows"))
                    .map_or(Self::Unavailable, Self::Ready)
            } else {
                Self::Unavailable
            };
        }
        match self {
            Self::Ready(capturer) => Some(capturer),
            Self::NotStarted | Self::Unavailable => None,
        }
    }
}

/// What a read of the newest frame gives.
enum FrameRead {
    Pixels(ScreenPixels),
    /// The window does not show all of the area.
    Outside,
    /// The window changed its size since the capture started, so the frame can miss a part of it.
    Resized,
}

/// The newest frame of a capture. The handler of the frames replaces it on a thread of
/// `Windows.Graphics.Capture`. A shared slot and not a channel, because a frame keeps its buffer
/// until it drops, and a capture without a free buffer gives no new frames.
type NewestFrame = Arc<Mutex<Option<Direct3D11CaptureFrame>>>;

/// A capture of one window that stays open, and keeps its newest frame.
struct LiveCapture {
    window: WindowId,
    /// The size of the buffers of the pool: the size of the window when the capture started.
    size: SizeInt32,
    newest: NewestFrame,
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
}

impl LiveCapture {
    /// Starts the capture of `window`, without the mouse pointer and without a border, and waits
    /// for its first frame.
    fn start(capturer: &Capturer, window: HWND) -> Result<Self, CaptureError> {
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        // SAFETY: plain call with the handle of a window.
        let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(window) }?;
        let size = item.Size()?;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            capturer.capture_device(),
            DirectXPixelFormat::B8G8R8A8UIntNormalized,
            FRAME_BUFFERS,
            size,
        )?;
        let newest = NewestFrame::default();
        let (arrived, arrivals) = mpsc::sync_channel(1);
        let newest_of_handler = Arc::clone(&newest);
        pool.FrameArrived(&TypedEventHandler::new(
            move |pool: windows::core::Ref<Direct3D11CaptureFramePool>, _| {
                let Some(pool) = pool.as_ref() else {
                    return Ok(());
                };
                let frame = pool.TryGetNextFrame()?;
                *newest_of_handler.lock().unwrap_or_else(PoisonError::into_inner) = Some(frame);
                // A full channel already tells that a frame came.
                let _ = arrived.try_send(());
                Ok(())
            },
        ))?;
        let session = pool.CreateCaptureSession(&item)?;
        session.SetIsCursorCaptureEnabled(false)?;
        session.SetIsBorderRequired(false)?;
        session.StartCapture()?;
        let capture = Self {
            window: WindowId::from(window),
            size,
            newest,
            session,
            pool,
        };
        arrivals.recv_timeout(FIRST_FRAME_TIMEOUT)?;
        Ok(capture)
    }

    /// Copies the pixels of `area` of the screen from the newest frame of `window`.
    fn read(&self, capturer: &Capturer, window: HWND, area: ScreenRect) -> Result<FrameRead, CaptureError> {
        let frame = self
            .newest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or(CaptureError::NoFrame(RecvTimeoutError::Timeout))?;
        if frame.ContentSize()? != self.size {
            return Ok(FrameRead::Resized);
        }
        let bounds = window_bounds(window)?;
        let origin = POINT {
            x: bounds.left,
            y: bounds.top,
        };
        let rect = PixelRect::from_screen(area, origin);
        if !is_inside(rect, self.size) {
            return Ok(FrameRead::Outside);
        }
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        // SAFETY: plain call on the surface of a frame of this capture.
        let texture = unsafe { access.GetInterface::<ID3D11Texture2D>() }?;
        let bgra = capturer.copy_pixels(&texture, rect)?;
        Ok(ScreenPixels::new(rect.on_screen(origin), colors_of_bgra(&bgra))
            .map_or(FrameRead::Outside, FrameRead::Pixels))
    }
}

impl Drop for LiveCapture {
    fn drop(&mut self) {
        // A failed close leaves nothing to recover, and the release also stops the capture.
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// Whether `rect` has an area and lies inside a frame of `size`.
fn is_inside(rect: PixelRect, size: SizeInt32) -> bool {
    rect.left >= 0
        && rect.top >= 0
        && rect.right <= size.Width
        && rect.bottom <= size.Height
        && rect.width() > 0
        && rect.height() > 0
}

/// The colors of `bgra`, pixels of 4 bytes in the order blue, green, red and alpha.
fn colors_of_bgra(bgra: &[u8]) -> Vec<PixelColor> {
    let (pixels, _) = bgra.as_chunks::<BYTES_PER_PIXEL>();
    pixels
        .iter()
        .map(|&[blue, green, red, _]| PixelColor { red, green, blue })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_capture::tests::rect;

    const SIZE: SizeInt32 = SizeInt32 {
        Width: 800,
        Height: 600,
    };

    #[test]
    fn bgra_bytes_give_the_colors_of_the_pixels() {
        let colors = colors_of_bgra(&[10, 20, 30, 255, 1, 2, 3, 0]);

        assert_eq!(
            colors,
            [
                PixelColor {
                    red: 30,
                    green: 20,
                    blue: 10
                },
                PixelColor {
                    red: 3,
                    green: 2,
                    blue: 1
                },
            ]
        );
    }

    #[test]
    fn rectangle_inside_the_frame_with_an_area_is_inside() {
        let rects = [rect(0, 0, 800, 600), rect(10, 20, 30, 40)];

        assert_eq!(rects.map(|rect| is_inside(rect, SIZE)), [true, true]);
    }

    #[test]
    fn rectangle_past_an_edge_of_the_frame_or_without_an_area_is_not_inside() {
        let rects = [
            rect(-1, 0, 100, 100),
            rect(0, -1, 100, 100),
            rect(700, 0, 801, 100),
            rect(0, 500, 100, 601),
            rect(10, 10, 10, 20),
        ];

        assert_eq!(rects.map(|rect| is_inside(rect, SIZE)), [false; 5]);
    }
}
