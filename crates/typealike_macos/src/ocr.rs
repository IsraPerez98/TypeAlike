//! Reads the text of the focused window from its pixels, for apps whose accessibility tree has no
//! text: `ScreenCaptureKit` captures a band of the window around the focused field, and Vision
//! recognizes the text in it. This needs the Screen & System Audio Recording permission.

use std::cmp::Reverse;
use std::hash::{DefaultHasher, Hasher};
use std::ptr::{self, NonNull};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use block2::RcBlock;
use dispatch2::{DispatchQoS, DispatchQueue, GlobalQueueIdentifier};
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::{CFData, CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapInfo, CGColorRenderingIntent, CGColorSpace, CGDataProvider, CGImage,
    CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSString, NSURL};
use objc2_screen_capture_kit::{
    SCContentFilter, SCScreenshotManager, SCShareableContent, SCStreamConfiguration, SCWindow,
};
use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel};
use typealike_core::desktop::{
    Confidence, EnergySaver, OcrArea, OcrAvailability, OcrRead, ScreenLine, ScreenText,
};
use typealike_core::settings::OcrLevel;

use crate::accessibility::{self, ROLE_ATTRIBUTE, WEB_AREA_ROLE, WINDOW_ATTRIBUTE, WINDOW_ROLE};
use crate::energy_saver;
use crate::focus::{self, FocusedField};
use crate::geometry::{contains, intersection, moved_by, screen_rect};

/// The height of the window above and below the field that the OCR reads, in points. Core keeps
/// only the lines nearest to the caret, so an OCR of the lines farther away is wasted work.
const BAND_POINTS: f64 = 600.0;
/// The most elements that the OCR reads on the way up from the focused field to its window.
const MAX_CONTENT_DEPTH: usize = 64;
const SPLIT_GROUP_ROLE: &str = "AXSplitGroup";
/// `ScreenCaptureKit` answers in about 100 ms. A longer wait means that the capture failed.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(3);
const SCREEN_RECORDING_SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
const WARM_UP_WIDTH: usize = 120;
const WARM_UP_HEIGHT: usize = 40;
/// The bars of the letters "HI" in the warm-up image, which Vision reads as "HI".
const WARM_UP_BARS: [PixelRect; 4] = [
    PixelRect {
        left: 10,
        top: 10,
        right: 13,
        bottom: 30,
    },
    PixelRect {
        left: 22,
        top: 10,
        right: 25,
        bottom: 30,
    },
    PixelRect {
        left: 10,
        top: 19,
        right: 25,
        bottom: 22,
    },
    PixelRect {
        left: 32,
        top: 10,
        right: 35,
        bottom: 30,
    },
];
const GRAY_PIXEL_BITS: usize = 8;
const BLACK: u8 = 0;
const WHITE: u8 = u8::MAX;

/// Tells whether the user gave TypeAlike the Screen & System Audio Recording permission.
pub(crate) fn is_permitted() -> bool {
    CGPreflightScreenCaptureAccess()
}

/// Tells whether TypeAlike can do an OCR now.
pub(crate) fn availability() -> OcrAvailability {
    if !is_permitted() {
        return OcrAvailability::NotPermitted;
    }
    if energy_saver::read() == EnergySaver::On {
        return OcrAvailability::SavingPower;
    }
    OcrAvailability::Ready
}

/// Asks for the Screen & System Audio Recording permission. The first time, macOS shows its prompt
/// and adds TypeAlike to the list in System Settings. Then System Settings opens at that list.
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
    /// Reads the text near the focused field of the app of `process` from a capture of the area
    /// of `ocr` in its window, at the level of `ocr`. `None` for a password field, or when the
    /// capture or the OCR fails.
    pub(crate) fn read(&mut self, process: i32, ocr: OcrRead) -> Option<ScreenText> {
        let OcrRead { level, area } = ocr;
        let field = focus::focused_field(process)?;
        let started = Instant::now();
        let capture = capture_band(process, FieldWindow::of(&field, area))?;
        let captured = Instant::now();
        let input = RecognitionInput {
            window: capture.window,
            pixels_hash: pixels_hash(
                &capture,
                (!field.shows_text_only_on_screen).then_some(field.frame),
            )?,
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
            ?area,
            "OCR read"
        );
        let lines = self.last.as_ref()?.lines.iter();
        Some(screen_text_of(lines, capture.band.origin, &field))
    }
}

/// The warm-up of the Accurate level, once in each process. Vision loads the model of that level
/// at the first recognition of a process, which took close to a minute in a new build.
#[derive(Debug, Default)]
pub(crate) struct AccurateWarmUp {
    is_started: bool,
}

impl AccurateWarmUp {
    /// Starts the warm-up the first time that `level` is Accurate, on a queue with a low priority
    /// and off the thread of the screen reads.
    pub(crate) fn start_for(&mut self, level: OcrLevel) {
        if self.is_due_for(level) {
            DispatchQueue::global_queue(GlobalQueueIdentifier::QualityOfService(DispatchQoS::Utility))
                .exec_async(warm_up_accurate);
        }
    }

    /// Tells whether the warm-up starts for `level`, and then counts it as started.
    fn is_due_for(&mut self, level: OcrLevel) -> bool {
        let is_due = level == OcrLevel::Accurate && !self.is_started;
        self.is_started |= is_due;
        is_due
    }
}

/// Recognizes the letters of a small image at the Accurate level, so that Vision loads its model.
fn warm_up_accurate() {
    let started = Instant::now();
    let lines = warm_up_image().and_then(|image| recognize(&image, CGSize::ZERO, OcrLevel::Accurate));
    tracing::debug!(
        duration = ?started.elapsed(),
        lines = lines.map(|lines| lines.len()),
        "OCR warm-up of the accurate level"
    );
}

/// Makes the image of the warm-up from [`warm_up_pixels`].
fn warm_up_image() -> Option<CFRetained<CGImage>> {
    let pixels = CFData::from_bytes(&warm_up_pixels());
    let provider = CGDataProvider::with_cf_data(Some(&pixels))?;
    let gray = CGColorSpace::new_device_gray()?;
    // SAFETY: the provider holds one gray byte for each pixel, and the decode array can be null.
    unsafe {
        CGImage::new(
            WARM_UP_WIDTH,
            WARM_UP_HEIGHT,
            GRAY_PIXEL_BITS,
            GRAY_PIXEL_BITS,
            WARM_UP_WIDTH,
            Some(&gray),
            CGBitmapInfo::empty(),
            Some(&provider),
            ptr::null(),
            false,
            CGColorRenderingIntent::RenderingIntentDefault,
        )
    }
}

/// The gray pixels of the warm-up image, row by row: the [`WARM_UP_BARS`] in black on white.
/// A blank image does not load the model that reads the letters.
fn warm_up_pixels() -> Vec<u8> {
    (0..WARM_UP_HEIGHT)
        .flat_map(|row| {
            (0..WARM_UP_WIDTH).map(move |column| {
                let is_bar = WARM_UP_BARS.iter().any(|bar| {
                    (bar.left..bar.right).contains(&column) && (bar.top..bar.bottom).contains(&row)
                });
                if is_bar { BLACK } else { WHITE }
            })
        })
        .collect()
}

/// What the AX window of the focused field tells about the window to capture.
struct FieldWindow {
    /// The part of the field in the frame of its window, or all of the field when that frame is
    /// unknown or holds no part of it. A long text view that scrolled has a frame taller than its
    /// window.
    visible_part: CGRect,
    /// The title of the window. Two windows of one app can have the same frame.
    title: Option<String>,
    /// The frame of the content that holds the field, for an OCR of [`OcrArea::Content`].
    content: Option<CGRect>,
}

impl FieldWindow {
    fn of(field: &FocusedField, area: OcrArea) -> Self {
        let window = accessibility::copy_element(&field.element, WINDOW_ATTRIBUTE);
        Self {
            visible_part: window
                .as_deref()
                .and_then(accessibility::frame)
                .and_then(|window| intersection(field.frame, window))
                .unwrap_or(field.frame),
            title: field.window_title.clone(),
            content: match area {
                OcrArea::Window => None,
                OcrArea::Content => content_frame(field),
            },
        }
    }
}

/// Reads the frame of the content that holds `field`: the element at [`content_position`] on the
/// way up from the field to its window.
fn content_frame(field: &FocusedField) -> Option<CGRect> {
    let mut elements = Vec::new();
    let mut roles = Vec::new();
    let mut next = Some(field.element.clone());
    while let Some(element) = next
        && roles.len() < MAX_CONTENT_DEPTH
    {
        let role = accessibility::copy_string(&element, ROLE_ATTRIBUTE);
        if role.as_deref() == Some(WINDOW_ROLE) {
            break;
        }
        next = accessibility::parent(&element);
        elements.push(element);
        roles.push(role);
    }
    accessibility::frame(&elements[content_position(&roles)?])
}

/// Returns the position in `path`, the roles from the focused field up to its window, of the
/// element that holds the content of the field: the outermost web area above the field, which is
/// the page in a browser, else the nearest pane of a split group, such as the editor of Notes
/// beside its sidebar and its list of notes. A web area that the field is, such as the body of an
/// email in Mail, holds no other content.
fn content_position(path: &[Option<String>]) -> Option<usize> {
    let has_role = |role: &Option<String>, wanted: &str| role.as_deref() == Some(wanted);
    let outermost_web_area = path
        .iter()
        .rposition(|role| has_role(role, WEB_AREA_ROLE))
        .filter(|&position| position > 0);
    outermost_web_area.or_else(|| {
        path.windows(2)
            .position(|pair| has_role(&pair[1], SPLIT_GROUP_ROLE))
    })
}

/// Returns the part of `window` that the OCR reads: the part of `content` in it, else the whole
/// window.
fn capture_area(window: CGRect, content: Option<CGRect>) -> CGRect {
    content
        .and_then(|content| intersection(content, window))
        .unwrap_or(window)
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

/// Captures the band of the window of `process` around the field of `field_window`.
/// `ScreenCaptureKit` answers on a queue of its own, so this waits for the answer.
fn capture_band(process: i32, field_window: FieldWindow) -> Option<Capture> {
    let (sender, receiver) = mpsc::channel();
    let on_content = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
        // SAFETY: `ScreenCaptureKit` gives a valid error or null, which lives during the call.
        log_capture_error(unsafe { error.as_ref() }, "list the windows");
        // SAFETY: `ScreenCaptureKit` gives a valid object or null.
        let content = unsafe { Retained::retain(content) };
        let is_started =
            content.is_some_and(|content| start_capture(&content, process, &field_window, &sender));
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

/// Starts the capture of the band of the window of `process` around the field of `field_window`,
/// which goes to `sender`. Returns whether it started: the window can be off the screen.
fn start_capture(
    content: &SCShareableContent,
    process: i32,
    field_window: &FieldWindow,
    sender: &mpsc::Sender<Option<Capture>>,
) -> bool {
    let field = field_window.visible_part;
    // SAFETY: plain call on a valid object.
    let windows = unsafe { content.windows() };
    let holding = windows
        .iter()
        .filter(|window| is_window_of(window, process, field.mid()))
        .collect::<Vec<_>>();
    // SAFETY: plain calls on valid windows.
    let titles = holding
        .iter()
        .map(|window| unsafe { window.title() }.map(|title| title.to_string()))
        .collect::<Vec<_>>();
    let Some(window) = picked_window(&titles, field_window.title.as_deref()).map(|index| &holding[index])
    else {
        return false;
    };
    // SAFETY: plain calls on a valid window.
    let (frame, window_id) = unsafe { (window.frame(), window.windowID()) };
    let band = band_around(field, capture_area(frame, field_window.content));
    // SAFETY: plain calls on valid objects.
    let filter =
        unsafe { SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), window) };
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

/// Returns the index of the window to capture among windows with `titles`: the window with the
/// longest title that starts the title of the AX window, `title`, else the first window. Chrome
/// gives its name after the title of the page only in the AX title.
fn picked_window(titles: &[Option<String>], title: Option<&str>) -> Option<usize> {
    let longest_start = titles
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            let candidate = candidate.as_deref().filter(|candidate| !candidate.is_empty())?;
            title?
                .starts_with(candidate)
                .then_some((candidate.len(), Reverse(index)))
        })
        .max()
        .map(|(_, Reverse(index))| index);
    longest_start.or_else(|| (!titles.is_empty()).then_some(0))
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

/// Returns the band of `area` from [`BAND_POINTS`] above `field` to [`BAND_POINTS`] below it,
/// as wide as the area.
fn band_around(field: CGRect, area: CGRect) -> CGRect {
    let top = (field.min().y - BAND_POINTS).max(area.min().y);
    let bottom = (field.max().y + BAND_POINTS).min(area.max().y);
    CGRect::new(
        CGPoint::new(area.origin.x, top),
        CGSize::new(area.size.width, (bottom - top).max(0.0)),
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

/// Hashes the pixels of `capture` outside of `typing_area`, so that the typed text and the caret in
/// a field do not change the hash. A field that shows its text only on the screen has no typing
/// area, because the OCR reads its text.
fn pixels_hash(capture: &Capture, typing_area: Option<CGRect>) -> Option<u64> {
    let image = &*capture.image;
    let data = CGDataProvider::data(CGImage::data_provider(Some(image)).as_deref())?;
    let layout = PixelLayout {
        width: CGImage::width(Some(image)),
        height: CGImage::height(Some(image)),
        bytes_per_row: CGImage::bytes_per_row(Some(image)),
        bytes_per_pixel: CGImage::bits_per_pixel(Some(image)) / 8,
    };
    let mask = PixelRect::of_typing_area(typing_area, capture.band.origin, capture.scale);
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
#[derive(Default)]
struct PixelRect {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

impl PixelRect {
    /// The pixels of `typing_area` in a band at `band_origin` with `scale` pixels for each point,
    /// or no pixel without a typing area.
    fn of_typing_area(typing_area: Option<CGRect>, band_origin: CGPoint, scale: f64) -> Self {
        typing_area.map_or_else(Self::default, |area| Self {
            left: to_pixels(area.min().x - band_origin.x, scale),
            top: to_pixels(area.min().y - band_origin.y, scale),
            right: to_pixels(area.max().x - band_origin.x, scale),
            bottom: to_pixels(area.max().y - band_origin.y, scale),
        })
    }
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

    #[test]
    fn window_with_the_title_of_the_field_window_is_picked() {
        let titles = [
            Some("Inbox".to_owned()),
            Some("Spring Meetup".to_owned()),
            Some("Spring".to_owned()),
        ];
        let cases = [
            (&titles[..], Some("Spring Meetup - Google Chrome"), Some(1)),
            (&titles[..], Some("Inbox"), Some(0)),
            (&titles[..], Some("Other - Google Chrome"), Some(0)),
            (&titles[..], None, Some(0)),
            (&[][..], Some("Spring Meetup - Google Chrome"), None),
        ];

        let picks = cases.map(|(titles, title, _)| picked_window(titles, title));

        assert_eq!(picks, cases.map(|(_, _, pick)| pick));
    }

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
    fn change_in_a_field_that_shows_its_text_only_on_the_screen_changes_the_hash() {
        let mask = PixelRect::of_typing_area(None, CGPoint::new(0.0, 0.0), 1.0);
        let hash = masked_hash(&PIXELS, &LAYOUT, &mask);
        let mut pixels = PIXELS;
        pixels[7] = 99;

        let changed = masked_hash(&pixels, &LAYOUT, &mask);

        assert_ne!(changed, hash);
    }

    #[test]
    fn typing_area_masks_the_pixels_of_the_field_in_the_band() {
        let field = CGRect::new(CGPoint::new(110.0, 61.0), CGSize::new(20.0, 10.0));

        let mask = PixelRect::of_typing_area(Some(field), CGPoint::new(100.0, 60.0), 2.0);

        assert_eq!([mask.left, mask.top, mask.right, mask.bottom], [20, 2, 60, 22]);
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

    fn path(roles: &[&str]) -> Vec<Option<String>> {
        roles.iter().map(|role| Some((*role).to_owned())).collect()
    }

    #[test]
    fn content_of_a_field_in_a_web_page_is_the_outermost_web_area_above_it() {
        let paths = [
            path(&["AXTextArea", "AXGroup", "AXWebArea", "AXScrollArea", "AXGroup"]),
            path(&["AXTextArea", "AXWebArea", "AXGroup", "AXWebArea", "AXSplitGroup"]),
        ];

        let positions = paths.each_ref().map(|path| content_position(path));

        assert_eq!(positions, [Some(2), Some(3)]);
    }

    #[test]
    fn content_of_a_field_outside_of_a_web_page_is_the_pane_of_the_nearest_split_group() {
        let paths = [
            path(&["AXTextArea", "AXScrollArea", "AXSplitGroup"]),
            path(&[
                "AXTextArea",
                "AXScrollArea",
                "AXSplitGroup",
                "AXGroup",
                "AXSplitGroup",
            ]),
            path(&["AXTextArea", "AXSplitGroup"]),
        ];

        let positions = paths.each_ref().map(|path| content_position(path));

        assert_eq!(positions, [Some(1), Some(1), Some(0)]);
    }

    #[test]
    fn field_that_is_a_web_area_or_outside_of_a_pane_has_no_content() {
        let paths = [
            path(&["AXWebArea", "AXGroup"]),
            path(&["AXTextArea", "AXScrollArea", "AXGroup"]),
            vec![None, None],
            Vec::new(),
        ];

        let positions = paths.each_ref().map(|path| content_position(path));

        assert_eq!(positions, [None; 4]);
    }

    #[test]
    fn capture_area_is_the_part_of_the_content_in_the_window() {
        let window = frame(0.0, 33.0, 1512.0, 882.0);

        let areas = [
            capture_area(window, Some(frame(0.0, 154.0, 1512.0, 761.0))),
            capture_area(window, Some(frame(685.0, 210.0, 571.0, 2000.0))),
            capture_area(window, Some(frame(2000.0, 0.0, 100.0, 100.0))),
            capture_area(window, None),
        ];

        assert_eq!(
            areas,
            [
                frame(0.0, 154.0, 1512.0, 761.0),
                frame(685.0, 210.0, 571.0, 705.0),
                window,
                window,
            ]
        );
    }

    #[test]
    fn accurate_warm_up_is_due_only_at_the_first_accurate_level() {
        let mut warm_up = AccurateWarmUp::default();
        let levels = [
            OcrLevel::Fast,
            OcrLevel::Accurate,
            OcrLevel::Fast,
            OcrLevel::Accurate,
        ];

        let due = levels.map(|level| warm_up.is_due_for(level));

        assert_eq!(due, [false, true, false, false]);
    }

    #[test]
    fn vision_bounds_become_points_from_the_top_left_corner() {
        let in_band = frame_in_band(frame(0.25, 0.5, 0.5, 0.25), CGSize::new(800.0, 400.0));

        assert_eq!(in_band, frame(200.0, 100.0, 400.0, 100.0));
    }
}
