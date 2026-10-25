//! Reads the text of the focused window from its pixels, for apps whose accessibility tree has no
//! text: `ScreenCaptureKit` captures a band of the window around the focused field, and Vision
//! recognizes the text in it. This needs the Screen & System Audio Recording permission.

use std::hash::{DefaultHasher, Hasher};
use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use block2::RcBlock;
use ghosttype_core::desktop::{Confidence, EnergySaver, OcrAvailability, ScreenLine, ScreenText};
use ghosttype_core::settings::OcrLevel;
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGDataProvider, CGImage, CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSString, NSURL};
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration, SCWindow,
};
use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel};

use crate::energy_saver;
use crate::focus::{self, FocusedField};
use crate::geometry::{contains, moved_by, screen_rect};

/// The height of the window above and below the field that the OCR reads, in points. Core keeps
/// only the lines nearest to the caret, so an OCR of the lines farther away is wasted work.
const BAND_POINTS: f64 = 600.0;
/// `ScreenCaptureKit` answers in about 100 ms. A longer wait means that the capture failed.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
const SCREEN_RECORDING_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

/// Tells whether GhostType can do an OCR now.
pub(crate) fn availability() -> OcrAvailability {
    if !CGPreflightScreenCaptureAccess() {
        return OcrAvailability::NotPermitted;
    }
    if energy_saver::read() == EnergySaver::On {
        return OcrAvailability::SavingPower;
    }
    OcrAvailability::Ready
}

/// Asks for the Screen & System Audio Recording permission. The first time, macOS shows its prompt
/// and adds GhostType to the list in System Settings. Then System Settings opens at that list.
pub(crate) fn ask_for_permission() {
    if CGRequestScreenCaptureAccess() {
        return;
    }
    let Some(url) = NSURL::URLWithString(&NSString::from_str(SCREEN_RECORDING_SETTINGS)) else {
        return;
    };
    if !NSWorkspace::sharedWorkspace().openURL(&url) {
        tracing::warn!("failed to open the screen recording settings");
    }
}

/// The OCR of the focused window. It keeps the lines of its last OCR, so that a capture with the
/// same pixels at the same level needs no other OCR.
#[derive(Default)]
pub(crate) struct Ocr {
    last: Option<Recognition>,
}

/// The lines of one OCR of a window.
struct Recognition {
    input: RecognitionInput,
    lines: Vec<RecognizedLine>,
}

/// What gives the lines of an OCR: the window, the hash of its pixels and the level.
#[derive(PartialEq, Eq)]
struct RecognitionInput {
    window: u32,
    pixels_hash: u64,
    level: OcrLevel,
}

/// A line that the OCR recognized, with its frame in points from the top left corner of the band.
struct RecognizedLine {
    text: String,
    frame: CGRect,
    confidence: Confidence,
}

/// A capture of a band of a window.
struct Capture {
    image: CFRetained<CGImage>,
    window: u32,
    band: CGRect,
    scale: f64,
}

impl Ocr {
    /// Reads the text near the focused field of the app of `process` from a capture of its window,
    /// at `level`. `None` for a password field, or when the capture or the OCR fails.
    pub(crate) fn read(&mut self, process: i32, level: OcrLevel) -> Option<ScreenText> {
        let field = focus::focused_field(process)?;
        let started = Instant::now();
        let capture = capture_band(process, field.frame)?;
        let captured = Instant::now();
        let input = RecognitionInput {
            window: capture.window,
            pixels_hash: pixels_hash(&capture, field.frame)?,
            level,
        };
        let hashed = Instant::now();
        let is_known = self.last.as_ref().is_some_and(|last| last.input == input);
        if !is_known {
            self.last = Some(Recognition {
                lines: recognize(&capture.image, capture.band.size, level)?,
                input,
            });
        }
        tracing::debug!(
            capture = ?captured - started,
            hash = ?hashed - captured,
            vision = ?hashed.elapsed(),
            is_cache_hit = is_known,
            width = CGImage::width(Some(&capture.image)),
            height = CGImage::height(Some(&capture.image)),
            ?level,
            "OCR read"
        );
        let lines = self.last.as_ref()?.lines.iter();
        Some(screen_text_of(lines, capture.band.origin, &field))
    }
}

fn screen_text_of<'a>(
    lines: impl Iterator<Item = &'a RecognizedLine>,
    band_origin: CGPoint,
    field: &FocusedField,
) -> ScreenText {
    ScreenText {
        lines: lines
            .map(|line| ScreenLine {
                text: line.text.clone(),
                frame: screen_rect(moved_by(line.frame, band_origin)),
                confidence: line.confidence,
            })
            .collect(),
        caret: field.caret.map(screen_rect),
        field: screen_rect(field.frame),
    }
}

/// Captures the band of the window of `process` around `field`. `ScreenCaptureKit` answers on
/// a queue of its own, so this waits for the answer.
fn capture_band(process: i32, field: CGRect) -> Option<Capture> {
    let (sender, receiver) = mpsc::channel();
    let on_content = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
        // SAFETY: `ScreenCaptureKit` gives a valid error or null, which lives during the call.
        log_capture_error(unsafe { error.as_ref() }, "list the windows");
        // SAFETY: `ScreenCaptureKit` gives a valid object or null.
        let content = unsafe { Retained::retain(content) };
        let is_started = content.is_some_and(|content| start_capture(&content, process, field, &sender));
        if !is_started {
            send_capture(&sender, None);
        }
    });
    // SAFETY: the block takes the argument types of the completion handler.
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            true,
            true,
            &on_content,
        );
    }
    receiver
        .recv_timeout(CAPTURE_TIMEOUT)
        .inspect_err(|error| tracing::debug!(%error, "the capture gave no answer"))
        .ok()
        .flatten()
}

/// Starts the capture of the band of the window of `process` around `field`, which goes to
/// `sender`. Returns whether it started: the window can be off the screen.
fn start_capture(
    content: &SCShareableContent,
    process: i32,
    field: CGRect,
    sender: &mpsc::Sender<Option<Capture>>,
) -> bool {
    // SAFETY: plain call on a valid object.
    let windows = unsafe { content.windows() };
    let Some(window) = windows
        .iter()
        .find(|window| is_window_of(window, process, field.mid()))
    else {
        return false;
    };
    // SAFETY: plain calls on a valid window.
    let (frame, window_id) = unsafe { (window.frame(), window.windowID()) };
    let band = band_around(field, frame);
    // SAFETY: plain calls on valid objects.
    let filter =
        unsafe { SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &window) };
    // SAFETY: plain call on a valid filter.
    let scale = f64::from(unsafe { SCShareableContent::infoForFilter(&filter).pointPixelScale() });
    let configuration = band_configuration(band, frame.origin, scale);
    let sender = sender.clone();
    let on_image = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        // SAFETY: `ScreenCaptureKit` gives a valid error or null, which lives during the call.
        log_capture_error(unsafe { error.as_ref() }, "capture the band");
        // SAFETY: `ScreenCaptureKit` gives a valid image or null, which the block does not own.
        let image = NonNull::new(image).map(|image| unsafe { CFRetained::retain(image) });
        let capture = image.map(|image| Capture {
            image,
            window: window_id,
            band,
            scale,
        });
        send_capture(&sender, capture);
    });
    // SAFETY: the block takes the argument types of the completion handler.
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(
            &filter,
            &configuration,
            Some(&on_image),
        );
    }
    true
}

/// Logs the code of `error`, which `ScreenCaptureKit` gives when `step` fails.
fn log_capture_error(error: Option<&NSError>, step: &str) {
    if let Some(error) = error {
        tracing::debug!(code = error.code(), step, "ScreenCaptureKit gave an error");
    }
}

fn send_capture(sender: &mpsc::Sender<Option<Capture>>, capture: Option<Capture>) {
    if sender.send(capture).is_err() {
        tracing::debug!("a capture came after the read stopped to wait for it");
    }
}

/// Whether `window` is a normal window of `process` that holds `point`.
fn is_window_of(window: &SCWindow, process: i32, point: CGPoint) -> bool {
    // SAFETY: plain calls on a valid window.
    let (owner, layer, frame) = unsafe {
        (
            window.owningApplication().map(|app| app.processID()),
            window.windowLayer(),
            window.frame(),
        )
    };
    owner == Some(process) && layer == 0 && contains(frame, point)
}

/// Returns the band of `window` from [`BAND_POINTS`] above `field` to [`BAND_POINTS`] below it,
/// as wide as the window.
fn band_around(field: CGRect, window: CGRect) -> CGRect {
    let top = (field.min().y - BAND_POINTS).max(window.min().y);
    let bottom = (field.max().y + BAND_POINTS).min(window.max().y);
    CGRect::new(
        CGPoint::new(window.origin.x, top),
        CGSize::new(window.size.width, (bottom - top).max(0.0)),
    )
}

/// The capture of `band` of the window at `window_origin`, at full resolution.
fn band_configuration(band: CGRect, window_origin: CGPoint, scale: f64) -> Retained<SCStreamConfiguration> {
    // SAFETY: plain calls on a new configuration.
    unsafe {
        let configuration = SCStreamConfiguration::new();
        configuration.setSourceRect(CGRect::new(
            CGPoint::new(band.origin.x - window_origin.x, band.origin.y - window_origin.y),
            band.size,
        ));
        configuration.setWidth(to_pixels(band.size.width, scale));
        configuration.setHeight(to_pixels(band.size.height, scale));
        configuration.setShowsCursor(false);
        configuration.setIgnoreShadowsSingleWindow(true);
        configuration
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "no conversion from f64 to usize exists without `as`, and `as` saturates"
)]
fn to_pixels(points: f64, scale: f64) -> usize {
    (points * scale).round() as usize
}

/// Hashes the pixels of `capture` outside of `field`, so that the typed text and the caret in the
/// field do not change the hash.
fn pixels_hash(capture: &Capture, field: CGRect) -> Option<u64> {
    let image = &*capture.image;
    let data = CGDataProvider::data(CGImage::data_provider(Some(image)).as_deref())?;
    let layout = PixelLayout {
        width: CGImage::width(Some(image)),
        height: CGImage::height(Some(image)),
        bytes_per_row: CGImage::bytes_per_row(Some(image)),
        bytes_per_pixel: CGImage::bits_per_pixel(Some(image)) / 8,
    };
    let origin = capture.band.origin;
    let mask = PixelRect {
        left: to_pixels(field.min().x - origin.x, capture.scale),
        top: to_pixels(field.min().y - origin.y, capture.scale),
        right: to_pixels(field.max().x - origin.x, capture.scale),
        bottom: to_pixels(field.max().y - origin.y, capture.scale),
    };
    // SAFETY: `data` is a copy of the pixels that nothing changes.
    let pixels = unsafe { data.as_bytes_unchecked() };
    Some(masked_hash(pixels, &layout, &mask))
}

/// How the pixels of an image lie in memory.
struct PixelLayout {
    width: usize,
    height: usize,
    bytes_per_row: usize,
    bytes_per_pixel: usize,
}

/// The columns `left..right` of the rows `top..bottom` of an image.
struct PixelRect {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

/// Hashes `pixels` outside of `mask`. The padding at the end of each row does not count.
fn masked_hash(pixels: &[u8], layout: &PixelLayout, mask: &PixelRect) -> u64 {
    let row_bytes = layout.width * layout.bytes_per_pixel;
    let mask_start = mask.left.min(layout.width) * layout.bytes_per_pixel;
    let mask_end = mask.right.min(layout.width) * layout.bytes_per_pixel;
    let mut hasher = DefaultHasher::new();
    for (row, bytes) in pixels
        .chunks(layout.bytes_per_row)
        .take(layout.height)
        .enumerate()
    {
        let shown = &bytes[..row_bytes.min(bytes.len())];
        if (mask.top..mask.bottom).contains(&row) {
            hasher.write(shown.get(..mask_start).unwrap_or(shown));
            hasher.write(shown.get(mask_end..).unwrap_or_default());
        } else {
            hasher.write(shown);
        }
    }
    hasher.finish()
}

/// Recognizes the lines of text in `image`, which shows `size` points of the screen, at `level`.
fn recognize(image: &CGImage, size: CGSize, level: OcrLevel) -> Option<Vec<RecognizedLine>> {
    // SAFETY: plain calls on new objects.
    let request = unsafe { VNRecognizeTextRequest::init(VNRecognizeTextRequest::alloc()) };
    request.setRecognitionLevel(match level {
        OcrLevel::Fast => VNRequestTextRecognitionLevel::Fast,
        OcrLevel::Accurate => VNRequestTextRecognitionLevel::Accurate,
    });
    request.setUsesLanguageCorrection(true);
    request.setAutomaticallyDetectsLanguage(true);
    // SAFETY: an empty dictionary is a valid set of options.
    let handler = unsafe {
        VNImageRequestHandler::initWithCGImage_options(
            VNImageRequestHandler::alloc(),
            image,
            &NSDictionary::new(),
        )
    };
    let requests: Retained<NSArray<VNRequest>> =
        NSArray::from_retained_slice(&[Retained::into_super(Retained::into_super(request.clone()))]);
    if let Err(error) = handler.performRequests_error(&requests) {
        tracing::warn!(code = error.code(), "failed to recognize the text of the window");
        return None;
    }
    let observations = request.results().unwrap_or_default();
    Some(
        observations
            .iter()
            .filter_map(|observation| {
                let candidate = observation.topCandidates(1).firstObject()?;
                Some(RecognizedLine {
                    text: candidate.string().to_string(),
                    // SAFETY: plain call on a valid observation.
                    frame: frame_in_band(unsafe { observation.boundingBox() }, size),
                    confidence: Confidence::from_fraction(candidate.confidence()),
                })
            })
            .collect(),
    )
}

/// Turns `bounds`, which Vision gives as fractions of the image from its bottom left corner, into
/// points from the top left corner of a band of `size` points.
fn frame_in_band(bounds: CGRect, size: CGSize) -> CGRect {
    CGRect::new(
        CGPoint::new(
            bounds.origin.x * size.width,
            (1.0 - bounds.origin.y - bounds.size.height) * size.height,
        ),
        CGSize::new(bounds.size.width * size.width, bounds.size.height * size.height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::frame;

    /// An image of 4 × 3 pixels with 1 byte for each pixel and 2 bytes of padding in each row.
    const LAYOUT: PixelLayout = PixelLayout {
        width: 4,
        height: 3,
        bytes_per_row: 6,
        bytes_per_pixel: 1,
    };
    const MASK: PixelRect = PixelRect {
        left: 1,
        top: 1,
        right: 3,
        bottom: 2,
    };
    const PIXELS: [u8; 18] = [1, 2, 3, 4, 0, 0, 5, 6, 7, 8, 0, 0, 9, 10, 11, 12, 0, 0];

    fn hash_with(index: usize, value: u8) -> u64 {
        let mut pixels = PIXELS;
        pixels[index] = value;
        masked_hash(&pixels, &LAYOUT, &MASK)
    }

    #[test]
    fn change_in_the_field_or_the_padding_keeps_the_hash() {
        let hash = masked_hash(&PIXELS, &LAYOUT, &MASK);

        let changed = [hash_with(7, 99), hash_with(8, 99), hash_with(4, 99)];

        assert_eq!(changed, [hash; 3]);
    }

    #[test]
    fn change_outside_the_field_changes_the_hash() {
        let hash = masked_hash(&PIXELS, &LAYOUT, &MASK);

        let changed = [
            hash_with(0, 99),
            hash_with(6, 99),
            hash_with(9, 99),
            hash_with(15, 99),
        ];

        assert!(changed.iter().all(|changed| *changed != hash), "{changed:?}");
    }

    #[test]
    fn band_reaches_above_and_below_the_field_within_the_window() {
        let window = frame(100.0, 50.0, 1000.0, 2000.0);

        let bands = [
            band_around(frame(200.0, 1000.0, 400.0, 30.0), window),
            band_around(frame(200.0, 100.0, 400.0, 30.0), window),
        ];

        assert_eq!(
            bands,
            [
                frame(100.0, 400.0, 1000.0, 1230.0),
                frame(100.0, 50.0, 1000.0, 680.0)
            ]
        );
    }

    #[test]
    fn vision_bounds_become_points_from_the_top_left_corner() {
        let in_band = frame_in_band(frame(0.25, 0.5, 0.5, 0.25), CGSize::new(800.0, 400.0));

        assert_eq!(in_band, frame(200.0, 100.0, 400.0, 100.0));
    }
}
