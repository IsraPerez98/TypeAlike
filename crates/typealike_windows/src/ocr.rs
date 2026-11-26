//! Reads the text of the foreground window from its pixels, for apps whose UI Automation tree has
//! no text: `Windows.Graphics.Capture` captures the window, and `Windows.Media.Ocr` recognizes the
//! text in a band of it around the focused field. The capture holds only that window, so the ghost
//! text and the badge of TypeAlike, which are windows of their own, are not in it. In a field that
//! shows its text only on the screen and gives no caret, the same capture also gives the block
//! cursor of the field.

mod capture;
mod geometry;

use std::num::TryFromIntError;
use std::time::Instant;

use typealike_core::desktop::{
    Confidence, CursorLook, EnergySaver, OcrAvailability, ScreenLine, ScreenRect, ScreenText,
};
use typealike_core::diagnostics::{RecordTurn, RecordedKinds};
use windows::Foundation::Rect;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrLine};
use windows::Storage::Streams::DataWriter;
use windows::Win32::Foundation::{HWND, POINT};

use self::capture::{Band, capture_band};
use self::geometry::field_miss;
use crate::display::round_to_pixel;
use crate::energy_saver;
use crate::focus::{foreground_hwnd, program_file_name_of};
use crate::screen_text::FieldPixels;
use crate::uia::{FocusedField, Uia};
use crate::window::WindowId;
use crate::window_capture::{CaptureError, Capturer, PixelRect, can_capture_without_border, window_bounds};

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
    /// The programs whose reads skipped the capture in this run, by the file name of the program.
    skipping_programs: RecordedKinds<String>,
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

/// A capture of the band of a window around its focused field.
struct FieldCapture {
    window: HWND,
    field: FocusedField,
    band: Band,
}

/// A line that the OCR recognized, with its frame in pixels from the top left corner of the band.
struct RecognizedLine {
    text: String,
    frame: PixelRect,
}

impl Ocr {
    /// Reads the text near the focused field from a capture of the foreground window, with or
    /// without the pixels of the field, as `field_pixels` tells. In a field without a caret whose
    /// pixels the read takes, it also looks for the block cursor in the same capture. Each part is
    /// `None` for a password field, for a field that the band of the window cannot hold, or when
    /// the capture fails. The text is also `None` when the OCR fails.
    pub(crate) fn read(
        &mut self,
        uia: &Uia,
        field_pixels: FieldPixels,
    ) -> (Option<ScreenText>, Option<CursorLook>) {
        let Some(capture) = self.capture(uia) else {
            return (None, None);
        };
        let cursor = cursor_look(&capture.band, &capture.field, field_pixels);
        (self.recognize_band(capture, field_pixels), cursor)
    }

    /// Looks for the block cursor of the focused field in a capture of the foreground window,
    /// without OCR. `None` unless the read takes the pixels of the field (`field_pixels`) and the
    /// field gives no caret, for a field that the band of the window cannot hold, and when the
    /// capture fails.
    pub(crate) fn look_at_cursor(&mut self, uia: &Uia, field_pixels: FieldPixels) -> Option<CursorLook> {
        if field_pixels != FieldPixels::Read {
            return None;
        }
        let capture = self.capture(uia)?;
        cursor_look(&capture.band, &capture.field, field_pixels)
    }

    /// Captures the band of the foreground window around its focused field. `None` for a password
    /// field, for a field that the band of the window cannot hold, or when the capture fails.
    fn capture(&mut self, uia: &Uia) -> Option<FieldCapture> {
        let field = uia.focused_field()?;
        let window = foreground_hwnd();
        if self.misses_window(window, field.frame) {
            return None;
        }
        if self.tools.is_none() {
            self.tools = OcrTools::create()
                .inspect_err(|error| tracing::warn!(%error, "failed to start the OCR"))
                .ok();
        }
        let tools = self.tools.as_ref()?;
        let started = Instant::now();
        let band = capture_band(&tools.capturer, window, field.frame, tools.max_side)
            .inspect_err(|error| tracing::warn!(%error, "failed to capture the window"))
            .ok()?;
        tracing::debug!(
            capture = ?started.elapsed(),
            width = band.width,
            height = band.height,
            "captured the window"
        );
        Some(FieldCapture { window, field, band })
    }

    /// Whether the band of `window` cannot hold `field`, the frame of its focused field, before a
    /// capture: the focus can be in a popup or in another window, and the frame can have no area.
    /// A capture then gives nothing to read, so the read skips it. The log tells each skip, at the
    /// info level for the first skip of each program in the run, and else at the debug level. When
    /// Windows gives no frame of the window, the capture goes on and tells what failed.
    fn misses_window(&mut self, window: HWND, field: ScreenRect) -> bool {
        let Ok(bounds) = window_bounds(window) else {
            return false;
        };
        let Some(miss) = field_miss(field, bounds) else {
            return false;
        };
        let program = program_file_name_of(window);
        match self.skipping_programs.note(program.clone()) {
            RecordTurn::First => {
                tracing::info!(program, ?miss, ?field, window = ?bounds, "skipped the capture of the OCR");
            }
            RecordTurn::Repeat => {
                tracing::debug!(program, ?miss, ?field, window = ?bounds, "skipped the capture of the OCR");
            }
        }
        true
    }

    /// Recognizes the text of the band of `capture`, without the pixels of its field unless
    /// `field_pixels` takes them. The OCR does not run again when the last OCR of the window had
    /// the same pixels. `None` when the OCR fails.
    fn recognize_band(&mut self, capture: FieldCapture, field_pixels: FieldPixels) -> Option<ScreenText> {
        let FieldCapture {
            window,
            field,
            mut band,
        } = capture;
        if field_pixels == FieldPixels::LeftOut {
            band.hide_field(field.frame);
        }
        let started = Instant::now();
        let input = RecognitionInput {
            window: WindowId::from(window),
            pixels: band.hash(),
        };
        let hashed = Instant::now();
        let is_known = self.last.as_ref().is_some_and(|last| last.input == input);
        if !is_known {
            let engine = &self.tools.as_ref()?.engine;
            let lines = recognize(engine, &band)
                .inspect_err(|error| tracing::warn!(%error, "failed to recognize the text of the window"))
                .ok()?;
            self.last = Some(Recognition { input, lines });
        }
        tracing::debug!(
            hash = ?hashed - started,
            ocr = ?hashed.elapsed(),
            is_cache_hit = is_known,
            "OCR read"
        );
        let lines = self.last.as_ref()?.lines.iter();
        Some(screen_text_of(lines, band.origin, field))
    }
}

/// The block cursors in the pixels of `field` in `band`, when the read takes the pixels of the
/// field (`field_pixels`) and the field gives no caret: a field that shows its text only on the
/// screen. The pixels are those of the visible frame of the field, which the caret reader also
/// gives to core, so that core keeps the cursor for that frame. `None` for other fields, and when
/// the band holds no part of the field.
fn cursor_look(band: &Band, field: &FocusedField, field_pixels: FieldPixels) -> Option<CursorLook> {
    if field_pixels != FieldPixels::Read || field.caret.is_some() {
        return None;
    }
    let frame = field.visible_frame?;
    let started = Instant::now();
    let blocks = band.pixels_of(frame)?.cursor_blocks();
    tracing::debug!(
        field = ?frame,
        ?blocks,
        duration = ?started.elapsed(),
        "looked for the block cursor"
    );
    Some(CursorLook { field: frame, blocks })
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
    use std::time::Duration;

    use super::*;
    use crate::display::{screen_rect, visible_field_frame};
    use crate::focus::focused_client_area;
    use crate::uia::UiaConnection;
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

    const BAND_ORIGIN: POINT = POINT { x: 100, y: 200 };
    const BAND_WIDTH: usize = 40;
    const BAND_HEIGHT: usize = 30;
    const FIELD: ScreenRect = ScreenRect {
        left: 100,
        top: 200,
        right: 140,
        bottom: 230,
    };
    /// A block cursor of 9 x 20 pixels in the band of [`band_with_a_cursor`].
    const CURSOR: ScreenRect = ScreenRect {
        left: 110,
        top: 205,
        right: 119,
        bottom: 225,
    };

    /// A dark band at [`BAND_ORIGIN`] with a light block cursor at [`CURSOR`].
    fn band_with_a_cursor() -> Band {
        let mut pixels = Vec::new();
        for row in 0..BAND_HEIGHT {
            for column in 0..BAND_WIDTH {
                let x = BAND_ORIGIN.x + i32::try_from(column).unwrap();
                let y = BAND_ORIGIN.y + i32::try_from(row).unwrap();
                let is_cursor =
                    (CURSOR.left..CURSOR.right).contains(&x) && (CURSOR.top..CURSOR.bottom).contains(&y);
                let level = if is_cursor { 220 } else { 30 };
                pixels.extend([level, level, level, 255]);
            }
        }
        Band {
            pixels,
            width: BAND_WIDTH,
            height: BAND_HEIGHT,
            origin: BAND_ORIGIN,
        }
    }

    fn field_without_a_caret() -> FocusedField {
        FocusedField {
            frame: FIELD,
            visible_frame: Some(FIELD),
            caret: None,
        }
    }

    #[test]
    fn look_finds_the_block_cursor_of_a_field_that_shows_its_text_only_on_the_screen() {
        let look = cursor_look(&band_with_a_cursor(), &field_without_a_caret(), FieldPixels::Read);

        assert_eq!(
            look,
            Some(CursorLook {
                field: FIELD,
                blocks: vec![CURSOR],
            })
        );
    }

    #[test]
    fn field_that_gives_its_text_or_its_caret_gets_no_look() {
        let field_with_a_caret = FocusedField {
            caret: Some(CURSOR),
            ..field_without_a_caret()
        };
        let cases = [
            (field_without_a_caret(), FieldPixels::LeftOut),
            (field_with_a_caret, FieldPixels::Read),
        ];

        let looks = cases.map(|(field, pixels)| cursor_look(&band_with_a_cursor(), &field, pixels));

        assert_eq!(looks, [None, None]);
    }

    /// Looks at the window in front, which must be a window of Herdr that shows a block cursor,
    /// such as the window of a terminal of Herdr that runs this test. The look must find one block,
    /// in the field frame that the caret reader gives to core.
    #[test]
    #[ignore = "captures the window in front, which must be a window of Herdr with a block cursor"]
    fn window_of_herdr_in_front_gives_one_cursor_block() {
        let mut connection = UiaConnection::connect(Duration::from_millis(500), Duration::from_secs(3));
        let uia = connection.connected().expect("a connection to UIA");
        let caret_reader_field =
            visible_field_frame(uia.caret().frame, focused_client_area()).map(screen_rect);

        let look = Ocr::default().look_at_cursor(uia, FieldPixels::Read);

        let found = look.as_ref().map(|look| (look.field, look.blocks.len()));
        assert_eq!(found, caret_reader_field.map(|field| (field, 1)), "{look:?}");
    }
}
