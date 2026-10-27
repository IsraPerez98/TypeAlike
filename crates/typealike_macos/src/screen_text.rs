//! Reads the text near the focused field: the static text of the Accessibility (AX) tree of the
//! focused window, or the text that OCR recognizes in a capture of the window (`ocr`). A large
//! window takes long to read, so the reads run one at a time on a queue with a low priority, and
//! each answer comes back to the main thread as a delivery.

use std::sync::mpsc;
use std::time::Instant;

use dispatch2::{DispatchQoS, DispatchQueue, GlobalQueueIdentifier};
use objc2_application_services::AXUIElement;
use objc2_core_foundation::{CFRetained, CGRect, Type};
use typealike_core::controller::{Delivery, DeliverySink};
use typealike_core::desktop::{Confidence, ScreenLine, ScreenRead, ScreenReadId, ScreenSource, ScreenText};

use crate::accessibility::{
    self, ROLE_ATTRIBUTE, VALUE_ATTRIBUTE, WINDOW_ATTRIBUTE, WINDOW_ROLE, children, is_same, parent,
};
use crate::focus::focused_field;
use crate::geometry::screen_rect;
use crate::ocr::Ocr;

const STATIC_TEXT_ROLE: &str = "AXStaticText";
/// The most elements that one read looks at, so that the read of a large window ends soon.
const MAX_ELEMENTS: usize = 2000;

/// Reads the screen text off the main thread, and gives each answer to the controller. Drop stops
/// the reads after the current one and does not wait for it.
#[derive(Debug)]
pub(crate) struct ScreenTextReader {
    requests: mpsc::Sender<ScreenRequest>,
}

/// A read that core asked for.
struct ScreenRequest {
    read: ScreenReadId,
    source: ScreenSource,
    process: Option<i32>,
    requested: Instant,
}

impl ScreenTextReader {
    /// Starts the reads on a queue with a low priority. Each answer goes to `deliver`.
    pub(crate) fn new(deliver: DeliverySink) -> Self {
        let (requests, received) = mpsc::channel();
        let queue =
            DispatchQueue::global_queue(GlobalQueueIdentifier::QualityOfService(DispatchQoS::Utility));
        queue.exec_async(move || answer_reads(&received, &deliver));
        Self { requests }
    }

    /// Reads the text near the focused field of the app of `process` from `source`, as it is now.
    /// The answer has the id `read`, and has no text when no app has the focus.
    pub(crate) fn read(&self, read: ScreenReadId, source: ScreenSource, process: Option<i32>) {
        tracing::debug!(?read, ?source, "screen read requested");
        let request = ScreenRequest {
            read,
            source,
            process,
            requested: Instant::now(),
        };
        if self.requests.send(request).is_err() {
            tracing::warn!("the screen text reader stopped");
        }
    }
}

/// Answers the reads until the reader drops. Of the reads that wait together, only the newest
/// gets an answer, because core uses only the newest.
fn answer_reads(requests: &mpsc::Receiver<ScreenRequest>, deliver: &DeliverySink) {
    let mut ocr = Ocr::default();
    while let Ok(oldest) = requests.recv() {
        let newest = requests.try_iter().last().unwrap_or(oldest);
        let started = Instant::now();
        let text = newest.process.and_then(|process| match newest.source {
            ScreenSource::Accessibility => read_screen_text(process),
            ScreenSource::Ocr(level) => ocr.read(process, level),
        });
        tracing::debug!(
            read = ?newest.read,
            source = ?newest.source,
            waited = ?started - newest.requested,
            duration = ?started.elapsed(),
            lines = text.as_ref().map(|text| text.lines.len()),
            "screen read answered"
        );
        deliver(Delivery::Screen(ScreenRead {
            id: newest.read,
            text,
        }));
    }
}

/// Reads the static text of the focused window of the app of `process`, without a password
/// field.
fn read_screen_text(process: i32) -> Option<ScreenText> {
    let field = focused_field(process)?;
    let window = accessibility::copy_element(&field.element, WINDOW_ATTRIBUTE)?;
    let visible = accessibility::frame(&window)?;
    Some(ScreenText {
        lines: nearby_static_texts(&field.element, visible),
        caret: field.caret.map(screen_rect),
        field: screen_rect(field.frame),
    })
}

/// Collects the visible static texts of the window of `focused`, from the elements nearest to it
/// in the tree outward, up to [`MAX_ELEMENTS`] elements.
fn nearby_static_texts(focused: &AXUIElement, visible: CGRect) -> Vec<ScreenLine> {
    let mut walk = Walk {
        visible,
        elements_left: MAX_ELEMENTS,
        lines: Vec::new(),
    };
    let mut child = focused.retain();
    while let Some(parent) = parent(&child) {
        for sibling in children(&parent)
            .into_iter()
            .filter(|sibling| !is_same(sibling, &child))
        {
            walk.visit(sibling);
        }
        let is_window = accessibility::copy_string(&parent, ROLE_ATTRIBUTE).as_deref() == Some(WINDOW_ROLE);
        if is_window || walk.elements_left == 0 {
            break;
        }
        child = parent;
    }
    walk.lines
}

/// A walk through the elements of a window.
struct Walk {
    visible: CGRect,
    elements_left: usize,
    lines: Vec<ScreenLine>,
}

impl Walk {
    /// Collects the visible static texts of `root` and the elements below it.
    fn visit(&mut self, root: CFRetained<AXUIElement>) {
        let mut pending = vec![root];
        while let Some(element) = pending.pop() {
            let Some(left) = self.elements_left.checked_sub(1) else {
                return;
            };
            self.elements_left = left;
            let frame = accessibility::frame(&element);
            if frame.is_some_and(|frame| !is_visible(frame, self.visible)) {
                continue;
            }
            if accessibility::copy_string(&element, ROLE_ATTRIBUTE).as_deref() == Some(STATIC_TEXT_ROLE) {
                self.lines.extend(static_text_line(&element, frame));
                continue;
            }
            pending.extend(children(&element).into_iter().rev());
        }
    }
}

/// The line of the static text `element` at `frame`, when it has a value and a frame.
fn static_text_line(element: &AXUIElement, frame: Option<CGRect>) -> Option<ScreenLine> {
    Some(ScreenLine {
        text: accessibility::copy_string(element, VALUE_ATTRIBUTE)?,
        confidence: Confidence::CERTAIN,
        frame: screen_rect(frame?),
    })
}

/// Whether an element at `frame` shows in `visible`. An element without area can hold visible
/// elements, so it counts as visible.
fn is_visible(frame: CGRect, visible: CGRect) -> bool {
    let has_area = frame.size.width > 0.0 && frame.size.height > 0.0;
    let overlaps = frame.origin.x < visible.origin.x + visible.size.width
        && visible.origin.x < frame.origin.x + frame.size.width
        && frame.origin.y < visible.origin.y + visible.size.height
        && visible.origin.y < frame.origin.y + frame.size.height;
    !has_area || overlaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::frame;

    #[test]
    fn element_that_overlaps_the_window_is_visible() {
        let window = frame(100.0, 100.0, 800.0, 600.0);
        let elements = [
            frame(120.0, 150.0, 300.0, 20.0),
            frame(80.0, 680.0, 100.0, 40.0),
            frame(100.0, -500.0, 300.0, 400.0),
            frame(100.0, 2000.0, 0.0, 0.0),
        ];

        let visible = elements.map(|element| is_visible(element, window));

        assert_eq!(visible, [true, true, false, true]);
    }
}
