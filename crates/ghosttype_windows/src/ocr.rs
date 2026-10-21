//! Reads the text of the foreground window from its pixels, for apps whose UI Automation tree has
//! no text: `Windows.Graphics.Capture` captures the window, and `Windows.Media.Ocr` recognizes the
//! text in a band of it around the focused field. The capture holds only that window, so the ghost
//! text and the badge of GhostType, which are windows of their own, are not in it.

use std::hash::{DefaultHasher, Hasher};
use std::num::TryFromIntError;
use std::ops::Range;
use std::slice;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ghosttype_core::desktop::{Confidence, OcrAvailability, ScreenLine, ScreenRect, ScreenText};
use windows::Foundation::Metadata::ApiInformation;
use windows::Foundation::{Rect, TypedEventHandler};
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Graphics::SizeInt32;
use windows::Media::Ocr::{OcrEngine, OcrLine};
use windows::Storage::Streams::DataWriter;
use windows::System::Power::{EnergySaverStatus, PowerManager};
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
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
use windows::core::{Interface, factory, h};

use crate::focus::WindowId;
use crate::uia::{self, FocusedField, Uia};

/// The height of the window above and below the field that the OCR reads, in pixels at a scale
/// of 100 %. Core keeps only the lines nearest to the caret, so an OCR of the lines farther away
/// is wasted work.
const BAND_DIPS: i32 = 600;
/// The pixels in one inch at a scale of 100 %.
const DEFAULT_DPI: i32 = 96;
/// `Windows.Graphics.Capture` gives the first frame in less than 100 ms. A longer wait means
/// that the capture failed.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
/// Each read uses one frame.
const FRAME_BUFFERS: i32 = 1;
/// The bytes of one BGRA pixel.
const BYTES_PER_PIXEL: usize = 4;

/// Whether this computer can do an OCR at all, apart from what changes while GhostType runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OcrSupport {
    /// Windows can capture a window without a border, and recognize text in an installed language.
    Supported,
    /// Windows cannot capture a window without a border, or has no language for the OCR.
    Unsupported,
}

impl OcrSupport {
    /// Finds out whether this computer can do an OCR. When Windows cannot hide the border that it
    /// draws around a window during each capture, GhostType does no OCR.
    pub(crate) fn detect() -> Self {
        let can_capture = GraphicsCaptureSession::IsSupported().unwrap_or(false);
        let can_hide_border = ApiInformation::IsPropertyPresent(
            h!("Windows.Graphics.Capture.GraphicsCaptureSession"),
            h!("IsBorderRequired"),
        )
        .unwrap_or(false);
        let has_language = OcrEngine::AvailableRecognizerLanguages()
            .and_then(|languages| languages.Size())
            .is_ok_and(|count| count > 0);
        if can_capture && can_hide_border && has_language {
            Self::Supported
        } else {
            Self::Unsupported
        }
    }

    /// Tells whether GhostType can do an OCR now.
    pub(crate) fn availability(self) -> OcrAvailability {
        match self {
            Self::Unsupported => OcrAvailability::Unsupported,
            Self::Supported if is_saving_energy() => OcrAvailability::SavingPower,
            Self::Supported => OcrAvailability::Ready,
        }
    }
}

fn is_saving_energy() -> bool {
    PowerManager::EnergySaverStatus().is_ok_and(|status| status == EnergySaverStatus::On)
}

/// Why an OCR read failed.
#[derive(Debug, thiserror::Error)]
enum OcrError {
    #[error("a call to Windows failed")]
    Windows(#[from] windows::core::Error),
    #[error("a size is out of range")]
    Size(#[from] TryFromIntError),
    #[error("Direct3D gave no device")]
    NoDevice,
    #[error("the capture gave no frame in time")]
    NoFrame(#[from] mpsc::RecvTimeoutError),
    #[error("the focused field is outside of the window")]
    FieldOutsideWindow,
}

/// The OCR of the foreground window. It keeps the lines of its last OCR, so that a capture with
/// the same pixels needs no other OCR. It lives on the thread that made it, as its COM objects do.
#[derive(Default)]
pub(crate) struct Ocr {
    tools: Option<OcrTools>,
    last: Option<Recognition>,
}

/// What each OCR uses: the capture of the windows and the engine that recognizes the text.
struct OcrTools {
    capturer: Capturer,
    engine: OcrEngine,
    /// The longest side of an image that the engine reads, in pixels.
    max_side: i32,
}

/// The lines of one OCR of a window.
struct Recognition {
    input: RecognitionInput,
    lines: Vec<RecognizedLine>,
}

/// What gives the lines of an OCR: the window and the hash of its pixels.
#[derive(PartialEq, Eq)]
struct RecognitionInput {
    window: WindowId,
    pixels: u64,
}

/// A line that the OCR recognized, with its frame in pixels from the top left corner of the band.
struct RecognizedLine {
    text: String,
    frame: PixelRect,
}

impl Ocr {
    /// Reads the text near the focused field from a capture of the foreground window. `None` for
    /// a password field, or when the capture or the OCR fails.
    pub(crate) fn read(&mut self, uia: &Uia) -> Option<ScreenText> {
        let field = uia.focused_field()?;
        // SAFETY: plain call.
        let window = unsafe { GetForegroundWindow() };
        if self.tools.is_none() {
            self.tools = OcrTools::create()
                .inspect_err(|error| tracing::warn!(%error, "failed to start the OCR"))
                .ok();
        }
        let tools = self.tools.as_ref()?;
        let started = Instant::now();
        let mut band = tools
            .capturer
            .capture_band(window, field.frame, tools.max_side)
            .inspect_err(|error| tracing::warn!(%error, "failed to capture the window"))
            .ok()?;
        let captured = Instant::now();
        let input = RecognitionInput {
            window: WindowId::from(window),
            pixels: band.hide_and_hash(field.frame),
        };
        let hashed = Instant::now();
        let is_known = self.last.as_ref().is_some_and(|last| last.input == input);
        if !is_known {
            let lines = recognize(&tools.engine, &band)
                .inspect_err(|error| tracing::warn!(%error, "failed to recognize the text of the window"))
                .ok()?;
            self.last = Some(Recognition { input, lines });
        }
        tracing::debug!(
            capture = ?captured - started,
            hash = ?hashed - captured,
            ocr = ?hashed.elapsed(),
            is_cache_hit = is_known,
            width = band.width,
            height = band.height,
            "OCR read"
        );
        let lines = self.last.as_ref()?.lines.iter();
        Some(screen_text_of(lines, band.origin, field))
    }
}

impl OcrTools {
    fn create() -> Result<Self, OcrError> {
        Ok(Self {
            capturer: Capturer::create()?,
            engine: user_engine()?,
            max_side: i32::try_from(OcrEngine::MaxImageDimension()?)?,
        })
    }
}

/// The OCR engine for the languages of the user, else for the first installed language that has
/// an OCR.
fn user_engine() -> windows::core::Result<OcrEngine> {
    OcrEngine::TryCreateFromUserProfileLanguages().or_else(|_| {
        let language = OcrEngine::AvailableRecognizerLanguages()?.GetAt(0)?;
        OcrEngine::TryCreateFromLanguage(&language)
    })
}

fn screen_text_of<'a>(
    lines: impl Iterator<Item = &'a RecognizedLine>,
    band_origin: POINT,
    field: FocusedField,
) -> ScreenText {
    ScreenText {
        lines: lines
            .map(|line| ScreenLine {
                text: line.text.clone(),
                frame: line.frame.on_screen(band_origin),
                // `Windows.Media.Ocr` gives no confidence. Core still drops the noisy lines.
                confidence: Confidence::CERTAIN,
            })
            .collect(),
        caret: field.caret,
        field: field.frame,
    }
}

/// Captures windows with `Windows.Graphics.Capture`, on a Direct3D device of its own.
struct Capturer {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    capture_device: IDirect3DDevice,
}

impl Capturer {
    fn create() -> Result<Self, OcrError> {
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

    /// Captures the band of `window` from [`BAND_DIPS`] above `field` to [`BAND_DIPS`] below it,
    /// as wide as the window and at most `max_side` pixels in each direction, at full resolution.
    fn capture_band(&self, window: HWND, field: ScreenRect, max_side: i32) -> Result<Band, OcrError> {
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
struct Band {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
    /// Where the top left pixel of the band is on the screen.
    origin: POINT,
}

impl Band {
    /// Sets the pixels of `field`, a rectangle on the screen, to zero, and hashes the pixels. Thus
    /// the typed text and the caret in the field do not change the hash, and the OCR does not read
    /// them.
    fn hide_and_hash(&mut self, field: ScreenRect) -> u64 {
        clear(
            &mut self.pixels,
            self.width,
            PixelRect::from_screen(field, self.origin),
        );
        let mut hasher = DefaultHasher::new();
        hasher.write(&self.pixels);
        hasher.finish()
    }
}

/// Recognizes the lines of text in `band`.
fn recognize(engine: &OcrEngine, band: &Band) -> Result<Vec<RecognizedLine>, OcrError> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(&band.pixels)?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &writer.DetachBuffer()?,
        BitmapPixelFormat::Bgra8,
        i32::try_from(band.width)?,
        i32::try_from(band.height)?,
    )?;
    let result = engine.RecognizeAsync(&bitmap)?.join()?;
    Ok(result
        .Lines()?
        .into_iter()
        .filter_map(|line| recognized_line(&line))
        .collect())
}

fn recognized_line(line: &OcrLine) -> Option<RecognizedLine> {
    let words = line.Words().ok()?;
    let frame = enclosing_rect(words.into_iter().filter_map(|word| word.BoundingRect().ok()))?;
    Some(RecognizedLine {
        text: line.Text().ok()?.to_string(),
        frame,
    })
}

/// A rectangle of pixels from the top left corner of an image or a window: the columns
/// `left..right` of the rows `top..bottom`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PixelRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl PixelRect {
    /// The rectangle `rect` of the screen, counted from `origin`.
    fn from_screen(rect: ScreenRect, origin: POINT) -> Self {
        Self {
            left: rect.left.saturating_sub(origin.x),
            top: rect.top.saturating_sub(origin.y),
            right: rect.right.saturating_sub(origin.x),
            bottom: rect.bottom.saturating_sub(origin.y),
        }
    }

    /// The rectangle of the screen where this rectangle, counted from `origin`, is.
    fn on_screen(self, origin: POINT) -> ScreenRect {
        ScreenRect {
            left: self.left.saturating_add(origin.x),
            top: self.top.saturating_add(origin.y),
            right: self.right.saturating_add(origin.x),
            bottom: self.bottom.saturating_add(origin.y),
        }
    }

    fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }

    fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }
}

/// The reach of the band above and below the field, in pixels, for a window at `dpi`. Windows
/// gives a `dpi` of zero when it cannot tell.
fn reach_for(dpi: u32) -> i32 {
    let dpi = i32::try_from(dpi)
        .ok()
        .filter(|dpi| *dpi > 0)
        .unwrap_or(DEFAULT_DPI);
    BAND_DIPS.saturating_mul(dpi) / DEFAULT_DPI
}

/// The band of a frame of `frame` pixels that the OCR reads: the rows from `reach` above `field`
/// to `reach` below it, as wide as the frame. A band larger than `max_side` in a direction
/// shrinks around the middle of the field. `None` when the field is outside of the frame.
fn band_in_frame(field: PixelRect, frame: SizeInt32, reach: i32, max_side: i32) -> Option<PixelRect> {
    let rows = field.top.saturating_sub(reach).max(0)..field.bottom.saturating_add(reach).min(frame.Height);
    let columns = 0..frame.Width;
    let middle = POINT {
        x: field.left.midpoint(field.right),
        y: field.top.midpoint(field.bottom),
    };
    let has_field = rows.start < rows.end && (columns.start..columns.end).contains(&middle.x);
    if !has_field {
        return None;
    }
    let rows = shrink_around(rows, middle.y, max_side);
    let columns = shrink_around(columns, middle.x, max_side);
    Some(PixelRect {
        left: columns.start,
        top: rows.start,
        right: columns.end,
        bottom: rows.end,
    })
}

/// Shrinks `range` to at most `max_length`, around `middle` as far as the range allows.
fn shrink_around(range: Range<i32>, middle: i32, max_length: i32) -> Range<i32> {
    if range.end.saturating_sub(range.start) <= max_length {
        return range;
    }
    let start = middle
        .saturating_sub(max_length / 2)
        .clamp(range.start, range.end.saturating_sub(max_length));
    start..start.saturating_add(max_length)
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

/// The smallest rectangle of whole pixels that holds each of `rects`. `None` without a rectangle.
fn enclosing_rect(rects: impl Iterator<Item = Rect>) -> Option<PixelRect> {
    rects
        .map(|rect| PixelRect {
            left: to_pixel(rect.X),
            top: to_pixel(rect.Y),
            right: to_pixel(rect.X + rect.Width),
            bottom: to_pixel(rect.Y + rect.Height),
        })
        .reduce(|first, second| PixelRect {
            left: first.left.min(second.left),
            top: first.top.min(second.top),
            right: first.right.max(second.right),
            bottom: first.bottom.max(second.bottom),
        })
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "no conversion from f32 to i32 exists without `as`, and `as` saturates"
)]
fn to_pixel(coordinate: f32) -> i32 {
    coordinate.round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> PixelRect {
        PixelRect {
            left,
            top,
            right,
            bottom,
        }
    }

    const FRAME: SizeInt32 = SizeInt32 {
        Width: 1000,
        Height: 2000,
    };

    #[test]
    fn band_reaches_above_and_below_the_field_within_the_frame() {
        let bands = [
            band_in_frame(rect(100, 1000, 500, 1030), FRAME, 600, 5000),
            band_in_frame(rect(100, 50, 500, 80), FRAME, 600, 5000),
        ];

        assert_eq!(
            bands,
            [Some(rect(0, 400, 1000, 1630)), Some(rect(0, 0, 1000, 680))]
        );
    }

    #[test]
    fn band_larger_than_the_engine_reads_shrinks_around_the_field() {
        let band = band_in_frame(rect(800, 1000, 900, 1030), FRAME, 600, 400);

        assert_eq!(band, Some(rect(600, 815, 1000, 1215)));
    }

    #[test]
    fn field_outside_of_the_frame_gives_no_band() {
        let bands = [
            band_in_frame(rect(1200, 1000, 1300, 1030), FRAME, 600, 5000),
            band_in_frame(rect(100, 2700, 500, 2730), FRAME, 600, 5000),
        ];

        assert_eq!(bands, [None, None]);
    }

    #[test]
    fn reach_grows_with_the_scale_of_the_window() {
        let reaches = [reach_for(96), reach_for(144), reach_for(0)];

        assert_eq!(reaches, [600, 900, 600]);
    }

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

    #[test]
    fn line_frame_holds_each_word() {
        let words = [
            Rect {
                X: 10.4,
                Y: 20.0,
                Width: 30.0,
                Height: 12.0,
            },
            Rect {
                X: 50.0,
                Y: 18.6,
                Width: 40.0,
                Height: 14.0,
            },
        ];

        let frame = enclosing_rect(words.into_iter());

        assert_eq!(frame, Some(rect(10, 19, 90, 33)));
    }

    #[test]
    fn line_without_words_has_no_frame() {
        let frame = enclosing_rect(std::iter::empty());

        assert_eq!(frame, None);
    }

    #[test]
    fn frame_in_the_band_moves_to_the_screen_and_back() {
        let origin = POINT { x: -1920, y: 300 };
        let field = ScreenRect {
            left: -1800,
            top: 900,
            right: -1200,
            bottom: 930,
        };

        let in_band = PixelRect::from_screen(field, origin);

        assert_eq!(
            (in_band, in_band.on_screen(origin)),
            (rect(120, 600, 720, 630), field)
        );
    }
}
