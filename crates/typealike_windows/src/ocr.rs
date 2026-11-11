//! Reads the text of the foreground window from its pixels, for apps whose UI Automation tree has
//! no text: `Windows.Graphics.Capture` captures the window, and `Windows.Media.Ocr` recognizes the
//! text in a band of it around the focused field. The capture holds only that window, so the ghost
//! text and the badge of TypeAlike, which are windows of their own, are not in it.

mod capture;
mod geometry;

use std::num::TryFromIntError;
use std::time::Instant;

use typealike_core::desktop::{Confidence, EnergySaver, OcrAvailability, ScreenLine, ScreenText};
use windows::Foundation::Rect;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrLine};
use windows::Storage::Streams::DataWriter;
use windows::Win32::Foundation::POINT;

use self::capture::{Band, capture_band};
use crate::display::round_to_pixel;
use crate::energy_saver;
use crate::focus::foreground_hwnd;
use crate::uia::{FocusedField, Uia};
use crate::window::WindowId;
use crate::window_capture::{CaptureError, Capturer, PixelRect, can_capture_without_border};

/// Whether this computer can do an OCR at all, apart from what changes while TypeAlike runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OcrSupport {
    /// Windows can capture a window without a border, and recognize text in an installed language.
    Supported,
    /// Windows cannot capture a window without a border, or has no language for the OCR.
    Unsupported,
}

impl OcrSupport {
    /// Finds out whether this computer can do an OCR. When Windows cannot hide the border that it
    /// draws around a window during each capture, TypeAlike does no OCR.
    pub(crate) fn detect() -> Self {
        let has_language = OcrEngine::AvailableRecognizerLanguages()
            .and_then(|languages| languages.Size())
            .is_ok_and(|count| count > 0);
        if can_capture_without_border() && has_language {
            Self::Supported
        } else {
            Self::Unsupported
        }
    }

    /// Tells whether TypeAlike can do an OCR now.
    pub(crate) fn availability(self) -> OcrAvailability {
        match self {
            Self::Unsupported => OcrAvailability::Unsupported,
            Self::Supported if energy_saver::read() == EnergySaver::On => OcrAvailability::SavingPower,
            Self::Supported => OcrAvailability::Ready,
        }
    }
}

/// Why an OCR read failed.
#[derive(Debug, thiserror::Error)]
enum OcrError {
    #[error("a call to Windows failed")]
    Windows(#[from] windows::core::Error),
    #[error("a size is out of range")]
    Size(#[from] TryFromIntError),
    #[error("the capture of the window failed")]
    Capture(#[from] CaptureError),
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
        let window = foreground_hwnd();
        if self.tools.is_none() {
            self.tools = OcrTools::create()
                .inspect_err(|error| tracing::warn!(%error, "failed to start the OCR"))
                .ok();
        }
        let tools = self.tools.as_ref()?;
        let started = Instant::now();
        let mut band = capture_band(&tools.capturer, window, field.frame, tools.max_side)
            .inspect_err(|error| tracing::warn!(%error, "failed to capture the window"))
            .ok()?;
        let captured = Instant::now();
        band.hide_field(field.frame);
        let input = RecognitionInput {
            window: WindowId::from(window),
            pixels: band.hash(),
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
    let recognized = engine.RecognizeAsync(&bitmap)?.join()?;
    Ok(recognized
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

/// The smallest rectangle of whole pixels that holds each of `rects`. `None` without a rectangle.
fn enclosing_rect(rects: impl Iterator<Item = Rect>) -> Option<PixelRect> {
    rects
        .map(|rect| PixelRect {
            left: round_to_pixel(f64::from(rect.X)),
            top: round_to_pixel(f64::from(rect.Y)),
            right: round_to_pixel(f64::from(rect.X + rect.Width)),
            bottom: round_to_pixel(f64::from(rect.Y + rect.Height)),
        })
        .reduce(|first, second| PixelRect {
            left: first.left.min(second.left),
            top: first.top.min(second.top),
            right: first.right.max(second.right),
            bottom: first.bottom.max(second.bottom),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_capture::tests::rect;

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
}
