//! The demo of a suggestion on the first step of the setup: a text box where the text types itself,
//! the suggestion fades in after it in gray, the key cap shows a press, and the suggestion becomes
//! typed text. It plays one time each time the step shows, and then stays on its last frame.

use std::time::{Duration, Instant};

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAccessibility as _, NSAccessibilityStaticTextRole, NSBox, NSColor, NSFont, NSFontWeightMedium,
    NSLayoutConstraint, NSLayoutConstraintOrientation, NSLayoutPriorityDefaultHigh, NSLayoutPriorityRequired,
    NSStackView, NSTextField, NSUserInterfaceLayoutOrientation,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSEdgeInsets, NSMutableAttributedString, NSString, NSTimer};
use typealike_core::setup::Demo;

use crate::overlay::attributed;
use crate::settings::controls::{label, wrapping_label};
use crate::settings::layout::{NO_INSETS, stack};
use crate::settings::setup::framed_box;
use crate::settings::setup::motion::Motion;

/// The time between two frames of the demo, about one frame of the display.
pub(super) const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const START_DELAY: Duration = Duration::from_millis(500);
const LETTER_INTERVAL: Duration = Duration::from_millis(35);
const SUGGESTION_DELAY: Duration = Duration::from_millis(250);
const SUGGESTION_FADE: Duration = Duration::from_millis(300);
const KEY_DELAY: Duration = Duration::from_millis(900);
const KEY_PRESS: Duration = Duration::from_millis(200);

const TEXT_FONT_SIZE: CGFloat = 15.0;
const KEY_FONT_SIZE: CGFloat = 11.0;
const BOX_CORNER_RADIUS: CGFloat = 8.0;
const KEY_CAP_CORNER_RADIUS: CGFloat = 5.0;
const BORDER_WIDTH: CGFloat = 1.0;
const COLUMN_SPACING: CGFloat = 12.0;
const BOX_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 12.0,
    left: 14.0,
    bottom: 12.0,
    right: 14.0,
};
const KEY_CAP_INSETS: NSEdgeInsets = NSEdgeInsets {
    top: 2.0,
    left: 7.0,
    bottom: 2.0,
    right: 7.0,
};

/// How the suggestion looks in a frame of the demo.
#[derive(Debug, Clone, Copy, PartialEq)]
enum SuggestionLook {
    Hidden,
    /// Gray text after the typed text, at an opacity from 0 to 1 while it fades in.
    Ghost {
        opacity: CGFloat,
    },
    /// Typed text, after the press of the key.
    Accepted,
}

/// Whether the key cap shows a press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyLook {
    Up,
    Down,
}

/// What the demo shows at one time of its animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DemoFrame {
    typed_letters: usize,
    suggestion: SuggestionLook,
    key: KeyLook,
}

impl DemoFrame {
    /// The frame `elapsed` after the start of the demo of a text with `letters` letters.
    fn at(elapsed: Duration, letters: usize) -> Self {
        let typing_end = START_DELAY.saturating_add(letters_duration(letters));
        if elapsed < typing_end {
            return Self::typing(letters_in(elapsed.saturating_sub(START_DELAY)).min(letters));
        }
        let fade_start = typing_end.saturating_add(SUGGESTION_DELAY);
        let fade_end = fade_start.saturating_add(SUGGESTION_FADE);
        let key_down = fade_end.saturating_add(KEY_DELAY);
        let (suggestion, key) = if elapsed < fade_start {
            (SuggestionLook::Hidden, KeyLook::Up)
        } else if elapsed < fade_end {
            let progress = elapsed.saturating_sub(fade_start).as_secs_f64() / SUGGESTION_FADE.as_secs_f64();
            (
                SuggestionLook::Ghost {
                    opacity: ease_out(progress),
                },
                KeyLook::Up,
            )
        } else if elapsed < key_down {
            (SuggestionLook::Ghost { opacity: 1.0 }, KeyLook::Up)
        } else if elapsed < key_down.saturating_add(KEY_PRESS) {
            (SuggestionLook::Accepted, KeyLook::Down)
        } else {
            (SuggestionLook::Accepted, KeyLook::Up)
        };
        Self {
            typed_letters: letters,
            suggestion,
            key,
        }
    }

    /// The frame where the demo stops: the whole text typed and the suggestion accepted.
    fn last(letters: usize) -> Self {
        Self {
            typed_letters: letters,
            suggestion: SuggestionLook::Accepted,
            key: KeyLook::Up,
        }
    }

    fn typing(typed_letters: usize) -> Self {
        Self {
            typed_letters,
            suggestion: SuggestionLook::Hidden,
            key: KeyLook::Up,
        }
    }

    fn is_last(&self) -> bool {
        self.suggestion == SuggestionLook::Accepted && self.key == KeyLook::Up
    }

    /// The opacity of the gray suggestion over the typed text.
    fn ghost_opacity(&self) -> CGFloat {
        match self.suggestion {
            SuggestionLook::Ghost { opacity } => opacity,
            SuggestionLook::Hidden | SuggestionLook::Accepted => 0.0,
        }
    }

    /// The opacity of the key cap, which shows with the suggestion.
    fn key_opacity(&self) -> CGFloat {
        match self.suggestion {
            SuggestionLook::Hidden => 0.0,
            SuggestionLook::Ghost { opacity } => opacity,
            SuggestionLook::Accepted => 1.0,
        }
    }
}

fn letters_duration(letters: usize) -> Duration {
    LETTER_INTERVAL.saturating_mul(u32::try_from(letters).unwrap_or(u32::MAX))
}

fn letters_in(typing: Duration) -> usize {
    usize::try_from(typing.as_millis() / LETTER_INTERVAL.as_millis()).unwrap_or(usize::MAX)
}

/// A curve that starts fast and ends slow, from 0 to 1 for `progress` from 0 to 1.
fn ease_out(progress: f64) -> CGFloat {
    let remaining = 1.0 - progress.clamp(0.0, 1.0);
    1.0 - remaining * remaining
}

/// The first `letters` characters of `text`, and the characters after them.
fn split_at_letters(text: &str, letters: usize) -> (&str, &str) {
    let end = text
        .char_indices()
        .nth(letters)
        .map_or(text.len(), |(index, _)| index);
    text.split_at(end)
}

/// The box of the demo, and the animation while it plays.
#[derive(Debug)]
pub(super) struct DemoView {
    pub(super) root: Retained<NSBox>,
    parts: DemoParts,
    playback: Option<Playback>,
}

/// The views in the box that show a frame of the demo.
#[derive(Debug)]
struct DemoParts {
    text: Retained<NSTextField>,
    suggestion: Retained<NSTextField>,
    key_cap: Retained<NSBox>,
    key_label: Retained<NSTextField>,
}

/// The demo while it plays: the demo, when it started, the last frame that shows, and the timer
/// that shows the next frames.
#[derive(Debug)]
struct Playback {
    demo: Demo,
    start: Instant,
    shown: DemoFrame,
    timer: Retained<NSTimer>,
}

impl DemoView {
    pub(super) fn new(main_thread: MainThreadMarker) -> Self {
        let text = demo_label(main_thread);
        let suggestion = demo_label(main_thread);
        let key_label = label(&key_font(), main_thread);
        key_label.setAccessibilityElement(false);
        let key_cap = framed_box(&key_label, KEY_CAP_INSETS, main_thread);
        key_cap.setCornerRadius(KEY_CAP_CORNER_RADIUS);
        key_cap.setBorderWidth(BORDER_WIDTH);
        key_cap.setBorderColor(&NSColor::separatorColor());
        for orientation in [
            NSLayoutConstraintOrientation::Horizontal,
            NSLayoutConstraintOrientation::Vertical,
        ] {
            key_cap.setContentHuggingPriority_forOrientation(NSLayoutPriorityDefaultHigh, orientation);
            key_cap.setContentCompressionResistancePriority_forOrientation(
                NSLayoutPriorityRequired,
                orientation,
            );
        }
        let row = stack(
            NSUserInterfaceLayoutOrientation::Horizontal,
            COLUMN_SPACING,
            NO_INSETS,
            main_thread,
        );
        row.addArrangedSubview(&text);
        row.addArrangedSubview(&key_cap);
        overlay_on(&suggestion, &text, &row);
        let root = framed_box(&row, BOX_INSETS, main_thread);
        root.setCornerRadius(BOX_CORNER_RADIUS);
        root.setBorderWidth(BORDER_WIDTH);
        root.setBorderColor(&NSColor::separatorColor());
        root.setFillColor(&NSColor::textBackgroundColor());
        root.setAccessibilityElement(true);
        // SAFETY: AppKit defines the role before any code runs.
        root.setAccessibilityRole(Some(unsafe { NSAccessibilityStaticTextRole }));
        Self {
            root,
            parts: DemoParts {
                text,
                suggestion,
                key_cap,
                key_label,
            },
            playback: None,
        }
    }

    /// Shows the box when there is a `demo`, and gives assistive technology its typed text and
    /// suggestion. The frame of the demo stays.
    pub(super) fn render(&self, demo: Option<&Demo>) {
        self.root.setHidden(demo.is_none());
        let Some(demo) = demo else {
            return;
        };
        let spoken = NSString::from_str(&format!("{}{}", demo.typed, demo.suggestion));
        // SAFETY: the value of a static text is a string.
        unsafe { self.root.setAccessibilityValue(Some(&spoken)) };
    }

    /// Plays `demo` from its start, with the timer that `schedule` starts, which must call
    /// [`Self::advance`] at each frame. With reduced motion, it shows the last frame at once.
    pub(super) fn play(
        &mut self,
        demo: Option<&Demo>,
        motion: Motion,
        schedule: impl FnOnce() -> Retained<NSTimer>,
    ) {
        self.stop();
        let Some(demo) = demo else {
            return;
        };
        let letters = demo.typed.chars().count();
        if motion == Motion::Reduced {
            self.parts.show(demo, DemoFrame::last(letters));
            return;
        }
        let first = DemoFrame::at(Duration::ZERO, letters);
        self.parts.show(demo, first);
        self.playback = Some(Playback {
            demo: demo.clone(),
            start: Instant::now(),
            shown: first,
            timer: schedule(),
        });
    }

    /// Shows the frame of the demo for the time now, and stops the timer at the last frame.
    pub(super) fn advance(&mut self) {
        let Some(playback) = &mut self.playback else {
            return;
        };
        let frame = DemoFrame::at(playback.start.elapsed(), playback.demo.typed.chars().count());
        if frame != playback.shown {
            playback.shown = frame;
            self.parts.show(&playback.demo, frame);
        }
        if frame.is_last() {
            self.stop();
        }
    }

    /// Stops the animation on the frame that shows.
    pub(super) fn stop(&mut self) {
        if let Some(playback) = self.playback.take() {
            playback.timer.invalidate();
        }
    }
}

impl DemoParts {
    fn show(&self, demo: &Demo, frame: DemoFrame) {
        let font = text_font();
        let typed_color = NSColor::labelColor();
        let clear = NSColor::clearColor();
        let (typed, untyped) = split_at_letters(&demo.typed, frame.typed_letters);
        let accepted_color = if frame.suggestion == SuggestionLook::Accepted {
            &typed_color
        } else {
            &clear
        };
        self.text.setAttributedStringValue(&joined(
            &[
                (typed, &typed_color),
                (untyped, &clear),
                (&demo.suggestion, accepted_color),
            ],
            &font,
        ));
        self.suggestion.setAttributedStringValue(&joined(
            &[
                (&demo.typed, &clear),
                (&demo.suggestion, &NSColor::secondaryLabelColor()),
            ],
            &font,
        ));
        self.suggestion.setAlphaValue(frame.ghost_opacity());
        self.key_label.setStringValue(&NSString::from_str(&demo.key));
        self.key_cap.setAlphaValue(frame.key_opacity());
        let (fill, key_text) = match frame.key {
            KeyLook::Up => (
                NSColor::quaternarySystemFillColor(),
                NSColor::secondaryLabelColor(),
            ),
            KeyLook::Down => (
                NSColor::selectedContentBackgroundColor(),
                NSColor::alternateSelectedControlTextColor(),
            ),
        };
        self.key_cap.setFillColor(&fill);
        self.key_label.setTextColor(Some(&key_text));
    }
}

/// A wrapping label for the text of the demo, which assistive technology skips, because the box
/// reads the whole text.
fn demo_label(main_thread: MainThreadMarker) -> Retained<NSTextField> {
    let label = wrapping_label(main_thread);
    label.setSelectable(false);
    label.setFont(Some(&text_font()));
    label.setAccessibilityElement(false);
    label
}

/// Puts `top` over `base`, which `stack` arranges, at the same place and width, so that their
/// texts wrap at the same words.
fn overlay_on(top: &NSTextField, base: &NSTextField, stack: &NSStackView) {
    stack.addSubview(top);
    top.setTranslatesAutoresizingMaskIntoConstraints(false);
    NSLayoutConstraint::activateConstraints(&NSArray::from_retained_slice(&[
        top.leadingAnchor().constraintEqualToAnchor(&base.leadingAnchor()),
        top.trailingAnchor()
            .constraintEqualToAnchor(&base.trailingAnchor()),
        top.topAnchor().constraintEqualToAnchor(&base.topAnchor()),
    ]));
}

/// `parts` one after the other, each text in its color, all in `font`.
fn joined(parts: &[(&str, &NSColor)], font: &NSFont) -> Retained<NSMutableAttributedString> {
    let text = NSMutableAttributedString::new();
    for &(part, color) in parts {
        text.appendAttributedString(&attributed(part, font, color, None));
    }
    text
}

fn text_font() -> Retained<NSFont> {
    NSFont::systemFontOfSize(TEXT_FONT_SIZE)
}

fn key_font() -> Retained<NSFont> {
    // SAFETY: the weight is a constant of AppKit.
    NSFont::systemFontOfSize_weight(KEY_FONT_SIZE, unsafe { NSFontWeightMedium })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LETTERS: usize = 10;

    fn typing_end() -> Duration {
        START_DELAY + letters_duration(LETTERS)
    }

    fn key_down() -> Duration {
        typing_end() + SUGGESTION_DELAY + SUGGESTION_FADE + KEY_DELAY
    }

    #[test]
    fn demo_starts_with_an_empty_box() {
        let frame = DemoFrame::at(Duration::ZERO, LETTERS);

        assert_eq!(frame, DemoFrame::typing(0));
    }

    #[test]
    fn typing_adds_one_letter_at_each_interval() {
        let frame = DemoFrame::at(START_DELAY + LETTER_INTERVAL * 3, LETTERS);

        assert_eq!(frame, DemoFrame::typing(3));
    }

    #[test]
    fn suggestion_waits_after_the_last_letter() {
        let frame = DemoFrame::at(typing_end(), LETTERS);

        assert_eq!(frame.suggestion, SuggestionLook::Hidden);
        assert_eq!(frame.typed_letters, LETTERS);
    }

    #[test]
    fn suggestion_fades_in_after_the_typed_text() {
        let halfway = typing_end() + SUGGESTION_DELAY + SUGGESTION_FADE / 2;

        let frame = DemoFrame::at(halfway, LETTERS);

        assert!(frame.ghost_opacity() > 0.0 && frame.ghost_opacity() < 1.0);
    }

    #[test]
    fn key_cap_shows_with_the_suggestion() {
        let halfway = typing_end() + SUGGESTION_DELAY + SUGGESTION_FADE / 2;

        let frame = DemoFrame::at(halfway, LETTERS);

        assert!((frame.key_opacity() - frame.ghost_opacity()).abs() < f64::EPSILON);
    }

    #[test]
    fn suggestion_shows_in_full_before_the_key_press() {
        let frame = DemoFrame::at(key_down().saturating_sub(KEY_PRESS), LETTERS);

        assert_eq!(frame.suggestion, SuggestionLook::Ghost { opacity: 1.0 });
        assert_eq!(frame.key, KeyLook::Up);
    }

    #[test]
    fn key_press_accepts_the_suggestion() {
        let frame = DemoFrame::at(key_down(), LETTERS);

        assert_eq!(frame.suggestion, SuggestionLook::Accepted);
        assert_eq!(frame.key, KeyLook::Down);
        assert!(!frame.is_last());
    }

    #[test]
    fn demo_ends_on_the_accepted_suggestion_after_the_key_press() {
        let frame = DemoFrame::at(key_down() + KEY_PRESS, LETTERS);

        assert_eq!(frame, DemoFrame::last(LETTERS));
        assert!(frame.is_last());
    }

    #[test]
    fn accepted_suggestion_hides_the_gray_text() {
        let frame = DemoFrame::last(LETTERS);

        assert!(frame.ghost_opacity().abs() < f64::EPSILON);
        assert!((frame.key_opacity() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ease_out_goes_from_zero_to_one() {
        assert!(ease_out(0.0).abs() < f64::EPSILON);
        assert!((ease_out(1.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn ease_out_moves_faster_at_the_start() {
        assert!(ease_out(0.5) > 0.5);
    }

    #[test]
    fn split_counts_characters_and_not_bytes() {
        let parts = split_at_letters("envío el", 4);

        assert_eq!(parts, ("enví", "o el"));
    }

    #[test]
    fn split_past_the_end_keeps_the_whole_text() {
        let parts = split_at_letters("Tab", 10);

        assert_eq!(parts, ("Tab", ""));
    }
}
