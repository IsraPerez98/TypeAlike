//! The anchor of the mirror in a field that shows its text only on the screen and gives no caret,
//! as a terminal: the block cursor that a look at the pixels of the field finds, else the bottom
//! edge of the field. The looks never change a prompt.

use crate::desktop::{CursorLook, ScreenRead, ScreenReadId, ScreenRect};

use super::Engine;
use super::session::FieldTextAccess;

/// The characters that the user can type after a look at the cursor before the next look, because
/// the typed line can wrap onto the next row.
// ponytail: a fixed character budget, a wrap check from the width of the row if the looks cost too much
const MAX_CHARS_AFTER_LOOK: usize = 20;

/// The frame that the mirror shows below, else above, when the platform finds no caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MirrorAnchor {
    /// The frame on the screen.
    pub frame: ScreenRect,
    /// Where the frame comes from.
    pub kind: MirrorAnchorKind,
}

/// Where the anchor of the mirror comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirrorAnchorKind {
    /// The block cursor that the newest look at the pixels of the field found.
    Cursor,
    /// The block cursor that an earlier look of the session found, because the newest look found
    /// no block or more than one.
    Kept,
    /// The bottom edge of the field, because no look of the session found the cursor. The input
    /// line of a terminal is most often near that edge.
    FieldBottom,
}

/// What the looks at the block cursor in a session found.
#[derive(Debug, Default)]
pub(super) struct CursorLooks {
    /// The block of the newest look that found exactly one block.
    found: Option<FoundCursor>,
    /// Whether the newest look found exactly one block.
    is_newest_found: bool,
    /// How far the user typed at the newest look, or `None` before the first look.
    typed_at_look: Option<TypedSpot>,
    /// The look that runs, if any.
    running: Option<ScreenReadId>,
}

/// The block cursor that a look found, and the frame of the field of that look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FoundCursor {
    block: ScreenRect,
    field: ScreenRect,
}

impl FoundCursor {
    /// Whether `other` is on the same row of the same field.
    fn is_on_the_row_of(self, other: Self) -> bool {
        self.field == other.field
            && self.block.top == other.block.top
            && self.block.bottom == other.block.bottom
    }
}

/// How far the user typed: the characters of the text before the caret. Enter ends the session in
/// a field that shows its text only on the screen, so the next session looks again from its own
/// read of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TypedSpot {
    chars: usize,
}

impl TypedSpot {
    fn of(typed: &str) -> Self {
        Self {
            chars: typed.chars().count(),
        }
    }

    /// Whether the row of the caret can be another row at `other` than at this spot.
    fn is_far_from(self, other: Self) -> bool {
        self.chars.abs_diff(other.chars) >= MAX_CHARS_AFTER_LOOK
    }
}

impl CursorLooks {
    /// Keeps `look`, a look after the user typed `typed`. A look with exactly one block on another
    /// row moves the anchor to that block. On the row of the anchor, the anchor stays, so that the
    /// mirror does not move sideways while the user types. A look without pixels counts as a look
    /// that found no block.
    fn keep(&mut self, look: Option<&CursorLook>, typed: &str) {
        self.typed_at_look = Some(TypedSpot::of(typed));
        let single_block = look.and_then(|look| match look.blocks[..] {
            [block] => Some(FoundCursor {
                block,
                field: look.field,
            }),
            _ => None,
        });
        self.is_newest_found = single_block.is_some();
        if single_block.is_some_and(|found| self.found.is_none_or(|kept| !kept.is_on_the_row_of(found))) {
            self.found = single_block;
        }
    }

    /// The anchor of the mirror in `field`, the frame of the field now, when a look of the session
    /// found the cursor in a field with that frame.
    fn anchor(&self, field: ScreenRect) -> Option<MirrorAnchor> {
        let found = self.found.filter(|found| found.field == field)?;
        let kind = if self.is_newest_found {
            MirrorAnchorKind::Cursor
        } else {
            MirrorAnchorKind::Kept
        };
        Some(MirrorAnchor {
            frame: found.block,
            kind,
        })
    }

    /// Whether the session needs a new look after the user typed `typed`: no look runs, and the
    /// typed line can be on another row since the newest look.
    fn is_due(&self, typed: &str) -> bool {
        self.running.is_none()
            && self
                .typed_at_look
                .is_some_and(|spot| spot.is_far_from(TypedSpot::of(typed)))
    }
}

impl Engine {
    /// The anchor of the mirror in `field`, the frame of the field now, when the field of the
    /// session shows its text only on the screen: the block cursor that a look of the session
    /// found, else the bottom edge of the field. `None` in other fields.
    pub(crate) fn mirror_anchor(&self, field: ScreenRect) -> Option<MirrorAnchor> {
        let session = self
            .session
            .as_ref()
            .filter(|session| session.field.text == FieldTextAccess::OnlyOnScreen)?;
        let bottom_edge = MirrorAnchor {
            frame: ScreenRect {
                top: field.bottom,
                ..field
            },
            kind: MirrorAnchorKind::FieldBottom,
        };
        Some(session.cursor_looks.anchor(field).unwrap_or(bottom_edge))
    }

    /// Whether `read` is the look at the cursor that runs.
    pub(crate) fn is_cursor_look(&self, read: ScreenReadId) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.cursor_looks.running == Some(read))
    }

    /// Keeps the look at the cursor of `read` for the session, when `read` answers the look that
    /// runs or the newest read of the screen. Returns whether the found block moved. The newest
    /// read of the screen also ends a look that runs, because the platform answers only the
    /// newest of the reads that wait together.
    pub(crate) fn on_cursor_look(&mut self, read: &ScreenRead) -> bool {
        let is_newest_read = self.screen_reads.is_newest_reading(read.id);
        let Some(session) = &mut self.session else {
            return false;
        };
        let looks = &mut session.cursor_looks;
        let is_look = looks.running == Some(read.id);
        if is_look || is_newest_read {
            looks.running = None;
        }
        let has_look = is_look || is_newest_read && read.cursor.is_some();
        if !has_look {
            return false;
        }
        let found_before = looks.found;
        looks.keep(read.cursor.as_ref(), &session.text_before);
        looks.found != found_before
    }

    /// Whether the session needs a look at the cursor: its field shows its text only on the
    /// screen, no read of the screen runs, and the user typed far enough since the newest look for
    /// the typed line to be on another row.
    pub(crate) fn is_cursor_look_due(&self) -> bool {
        !self.screen_reads.is_reading()
            && self.session.as_ref().is_some_and(|session| {
                session.field.text == FieldTextAccess::OnlyOnScreen
                    && session.cursor_looks.is_due(&session.text_before)
            })
    }

    /// Starts a look at the cursor when [`Engine::is_cursor_look_due`], and returns its id.
    pub(crate) fn start_cursor_look(&mut self) -> Option<ScreenReadId> {
        if !self.is_cursor_look_due() {
            return None;
        }
        let read = self.screen_reads.next_id();
        self.session.as_mut()?.cursor_looks.running = Some(read);
        Some(read)
    }
}
