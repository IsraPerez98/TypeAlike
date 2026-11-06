//! The chat lines that the user scrolled past: the lines above the caret of the earlier reads of
//! one conversation, which stay in memory for a short time.

use std::collections::HashSet;
use std::time::Duration;

use crate::desktop::{ScreenLine, ScreenRect, ScreenText, Timestamp};

use super::{FieldContent, LineScope, caret_of, is_above, is_inside};

/// The time that a line stays after the newest read that showed it.
const MAX_LINE_AGE: Duration = Duration::from_mins(10);
/// The fewest characters of a line that tells that two reads show one conversation. A shorter line,
/// such as "ok" or "Today", shows in many conversations.
const MIN_SHARED_LINE_CHARS: usize = 20;
/// The fewest shared lines for a read to continue the conversation of the earlier reads. A chat app
/// can show one long line, such as "click here for contact info", above each conversation.
const MIN_SHARED_LINES: usize = 2;

/// A line above the caret, and the time of the newest read that showed it.
struct SeenLine {
    line: ScreenLine,
    seen_at: Timestamp,
}

/// The lines above the caret of the reads of one conversation, in reading order, as if the
/// conversation were on one tall screen.
#[derive(Default)]
pub(crate) struct ConversationLines {
    lines: Vec<SeenLine>,
}

impl ConversationLines {
    /// Adds the lines of `screen` that a read gave at `now`. When the read shares fewer than
    /// [`MIN_SHARED_LINES`] lines of [`MIN_SHARED_LINE_CHARS`] characters with the earlier reads,
    /// it shows another conversation, so the earlier lines go. Without a clock, no line stays.
    pub(crate) fn add_read(&mut self, screen: &ScreenText, now: Option<Timestamp>) {
        let Some(now) = now else {
            self.lines.clear();
            return;
        };
        let shown = lines_above_caret(screen);
        let shown_texts = shown
            .iter()
            .map(|line| line.text.as_str())
            .collect::<HashSet<_>>();
        self.lines.retain(|seen| is_recent(seen, now));
        if !self.shares_lines_with(&shown_texts) {
            self.lines.clear();
        }
        self.lines
            .retain(|seen| !shown_texts.contains(seen.line.text.as_str()));
        self.lines.extend(shown.into_iter().map(|line| SeenLine {
            line: line.clone(),
            seen_at: now,
        }));
    }

    /// Returns `screen` with the lines of the earlier reads that it does not show, above all its
    /// lines in their order. Returns `None` without a clock, or when `screen` shows each line that
    /// is younger than [`MAX_LINE_AGE`] at `now`.
    pub(crate) fn taller_screen(&self, screen: &ScreenText, now: Option<Timestamp>) -> Option<ScreenText> {
        let now = now?;
        let shown_texts = lines_above_caret(screen)
            .into_iter()
            .map(|line| line.text.as_str())
            .collect::<HashSet<_>>();
        let scrolled_past = self
            .lines
            .iter()
            .filter(|seen| is_recent(seen, now) && !shown_texts.contains(seen.line.text.as_str()))
            .map(|seen| &seen.line)
            .collect::<Vec<_>>();
        if scrolled_past.is_empty() {
            return None;
        }
        let top = screen
            .lines
            .iter()
            .map(|line| line.frame.top)
            .fold(screen.field.top, i32::min);
        let mut lines = stacked_above(&scrolled_past, top);
        lines.extend(screen.lines.iter().cloned());
        Some(ScreenText {
            lines,
            caret: screen.caret,
            field: screen.field,
        })
    }

    fn shares_lines_with(&self, texts: &HashSet<&str>) -> bool {
        let shared = self
            .lines
            .iter()
            .map(|seen| seen.line.text.as_str())
            .filter(|text| text.chars().count() >= MIN_SHARED_LINE_CHARS && texts.contains(text))
            .collect::<HashSet<_>>();
        shared.len() >= MIN_SHARED_LINES
    }
}

/// Returns the lines of `screen` in the column of its field above the caret and outside the field,
/// in reading order, without the second copy of a line with the same text and frame.
fn lines_above_caret(screen: &ScreenText) -> Vec<&ScreenLine> {
    let caret = caret_of(screen, FieldContent::Read(None));
    let mut copies = HashSet::new();
    let mut lines = LineScope::Column
        .lines(screen)
        .filter(|line| is_above(line.frame, caret) && !is_inside(line.frame, screen.field))
        .filter(|line| copies.insert((line.text.as_str(), line.frame)))
        .collect::<Vec<_>>();
    lines.sort_by_key(|line| (line.frame.top, line.frame.left));
    lines
}

fn is_recent(seen: &SeenLine, now: Timestamp) -> bool {
    now.0.saturating_sub(seen.seen_at.0) <= MAX_LINE_AGE.as_secs()
}

/// Returns `lines` one under the other in their order, with their heights, the last one just above
/// `top`.
fn stacked_above(lines: &[&ScreenLine], top: i32) -> Vec<ScreenLine> {
    let mut bottom = top;
    lines
        .iter()
        .rev()
        .map(|line| {
            let height = line.frame.bottom.saturating_sub(line.frame.top);
            let frame = ScreenRect {
                top: bottom.saturating_sub(height),
                bottom,
                ..line.frame
            };
            bottom = frame.top;
            ScreenLine {
                frame,
                ..(*line).clone()
            }
        })
        .collect()
}
