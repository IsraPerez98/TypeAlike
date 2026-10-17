//! A scrolling child window that holds the controls of one settings pane, and draws its cards,
//! the frames of its text fields and its scroll bar.
//!
//! A text field lets the pointer through where it shows its color key, so the page gets most of
//! the pointer input over a field. The page finds the field under the pointer for that input.

use std::rc::Rc;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    InvalidateRect, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE, RedrawWindow,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CS_DBLCLKS, GetClientRect, SW_HIDE, SW_SHOWNA, SetWindowTextW, ShowWindow, WNDPROC, WS_CHILD,
    WS_CLIPCHILDREN, WS_EX_CONTROLPARENT,
};
use windows::core::{HSTRING, Result, w};

use crate::settings::appearance::Appearance;
use crate::settings::controls;
use crate::settings::layout::{Bounds, Point, ScrollBar, ScrollBarSizes, ScrollMove, clamp_scroll, reveal};
use crate::settings::paint::{self, FieldPaint, FieldState, PaintSession};
use crate::window::{Window, WindowClass, WindowSpec};

const WHEEL_SCROLL: i32 = 120;
const WHEEL_DELTA: i32 = 120;
const SCROLL_GUTTER: i32 = 12;
const THUMB_WIDTH: i32 = 2;
const EXPANDED_THUMB_WIDTH: i32 = 6;
const THUMB_EDGE_GAP: i32 = 3;
const MIN_THUMB: i32 = 24;

/// Something the page draws behind its controls, in content coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// The surface of a group of controls.
    Card(Bounds),
    /// The frame of a text field.
    Field(FieldShape),
}

/// The frame around the edit control of a text field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldShape {
    pub(crate) frame: Bounds,
    /// Where the edit control shows its text.
    pub(crate) text_area: Bounds,
    pub(crate) edit: HWND,
    pub(crate) placeholder: &'static str,
}

impl Shape {
    fn scrolled(self, offset: i32) -> Self {
        match self {
            Self::Card(card) => Self::Card(card.scrolled(offset)),
            Self::Field(field) => Self::Field(FieldShape {
                frame: field.frame.scrolled(offset),
                text_area: field.text_area.scrolled(offset),
                ..field
            }),
        }
    }
}

/// A text field under the pointer, with the pointer in the coordinates of its edit control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldPoint {
    pub(crate) edit: HWND,
    pub(crate) point: Point,
}

/// The part of the page that shows the pointer over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hover {
    Gutter,
    /// The frame of the text field with this edit control.
    Field(HWND),
}

/// What the pointer did over the page itself, outside its controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pointer {
    Move(Point),
    Press(Point),
    Release,
    Leave,
    /// Another window took the mouse capture.
    CaptureLost,
}

/// A drag of the scroll bar thumb: where the pointer and the offset were when it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ThumbDrag {
    start_y: i32,
    start_offset: i32,
}

/// The window of one pane, with the scroll offset and shapes of its last layout.
#[derive(Debug)]
pub(crate) struct Page {
    window: Window,
    offset: i32,
    content_height: i32,
    shapes: Vec<Shape>,
    hover: Option<Hover>,
    drag: Option<ThumbDrag>,
}

impl Page {
    /// Registers the class of the pages in `parent`, whose messages go to `procedure`.
    pub(crate) fn register_class(procedure: WNDPROC, parent: HWND) -> Result<Rc<WindowClass>> {
        WindowClass::register(WindowSpec {
            class_name: w!("GhostType.SettingsPage"),
            procedure,
            class_style: CS_DBLCLKS,
            style: WS_CHILD | WS_CLIPCHILDREN,
            extended_style: WS_EX_CONTROLPARENT,
            parent: Some(parent),
        })
    }

    /// Creates a hidden page of `class`, see [`Self::register_class`].
    pub(crate) fn create(class: &Rc<WindowClass>) -> Result<Self> {
        let window = class.create_window()?;
        Ok(Self {
            window,
            offset: 0,
            content_height: 0,
            shapes: Vec::new(),
            hover: None,
            drag: None,
        })
    }

    pub(crate) fn handle(&self) -> HWND {
        self.window.handle()
    }

    /// Names the page for screen readers.
    pub(crate) fn set_title(&self, title: &str) {
        // SAFETY: plain call on a window this type owns.
        if let Err(error) = unsafe { SetWindowTextW(self.handle(), &HSTRING::from(title)) } {
            tracing::warn!(%error, "failed to name settings page");
        }
    }

    pub(crate) fn show(&self) {
        // Returns the previous visibility, not an error.
        // SAFETY: plain call on a window this type owns.
        let _ = unsafe { ShowWindow(self.handle(), SW_SHOWNA) };
    }

    pub(crate) fn hide(&self) {
        // Returns the previous visibility, not an error.
        // SAFETY: plain call on a window this type owns.
        let _ = unsafe { ShowWindow(self.handle(), SW_HIDE) };
    }

    /// The client area, where the controls go.
    pub(crate) fn viewport(&self) -> Bounds {
        let mut client = RECT::default();
        // SAFETY: plain call on a window this type owns; `client` is a local the call fills in.
        if let Err(error) = unsafe { GetClientRect(self.handle(), &raw mut client) } {
            tracing::warn!(%error, "failed to read settings page size");
        }
        Bounds {
            x: 0,
            y: 0,
            width: client.right,
            height: client.bottom,
        }
    }

    /// The offset that the next layout must move the controls up by.
    pub(crate) fn offset(&self) -> i32 {
        self.offset
    }

    /// Stores the result of a layout made at [`Self::offset`]. Returns whether the offset had to
    /// change to fit the content, in which case the layout must run again.
    pub(crate) fn finish_layout(&mut self, content_height: i32, shapes: Vec<Shape>) -> bool {
        self.content_height = content_height;
        self.shapes = shapes;
        let fitted = clamp_scroll(self.offset, content_height, self.viewport().height);
        let has_moved = fitted != self.offset;
        self.offset = fitted;
        self.repaint();
        has_moved
    }

    /// Applies a scroll request. Returns whether the offset changed, in which case the layout must run again.
    pub(crate) fn scroll(&mut self, scroll: ScrollMove) -> bool {
        let viewport_height = self.viewport().height;
        let target = clamp_scroll(
            scroll.target(self.offset, viewport_height),
            self.content_height,
            viewport_height,
        );
        let has_moved = target != self.offset;
        self.offset = target;
        has_moved
    }

    /// Scrolls so that `bounds`, placed at the current offset, is fully visible.
    pub(crate) fn reveal(&mut self, bounds: Bounds) -> bool {
        let target = reveal(self.offset, bounds, self.viewport().height);
        self.scroll(ScrollMove::To(target))
    }

    /// The text field whose frame contains `point`, a point of the page.
    pub(crate) fn field_at(&self, point: Point) -> Option<FieldPoint> {
        field_at(&self.shapes, self.offset, point)
    }

    /// Converts a mouse wheel rotation, in `WHEEL_DELTA` units up, to a scroll.
    pub(crate) fn wheel_move(rotation: i32, appearance: &Appearance) -> ScrollMove {
        ScrollMove::By(-rotation * appearance.px(WHEEL_SCROLL) / WHEEL_DELTA)
    }

    /// Reacts to the pointer over the page: it hovers and drags the scroll bar. Returns the
    /// scroll it asks for.
    pub(crate) fn on_pointer(&mut self, pointer: Pointer, appearance: &Appearance) -> Option<ScrollMove> {
        match pointer {
            Pointer::Move(point) => {
                if let Some(drag) = self.drag {
                    let bar = self.scroll_bar(appearance)?;
                    return Some(ScrollMove::To(
                        bar.offset_after_drag(drag.start_offset, point.y - drag.start_y),
                    ));
                }
                let hover = self.hover_at(point, appearance);
                if hover.is_some() && hover != self.hover {
                    self.watch_pointer_leave();
                }
                self.show_hover(hover);
                None
            }
            Pointer::Press(point) => {
                let bar = self.scroll_bar(appearance)?;
                if bar.thumb_band().contains(point) {
                    self.drag = Some(ThumbDrag {
                        start_y: point.y,
                        start_offset: self.offset,
                    });
                    // SAFETY: plain call on a window this type owns; it returns the previous capture.
                    unsafe { SetCapture(self.handle()) };
                    None
                } else if bar.gutter.contains(point) {
                    Some(if point.y < bar.thumb.y {
                        ScrollMove::PageUp
                    } else {
                        ScrollMove::PageDown
                    })
                } else {
                    None
                }
            }
            Pointer::Release => {
                if self.drag.take().is_some() {
                    // SAFETY: plain call; it releases the capture this page took for the drag.
                    if let Err(error) = unsafe { ReleaseCapture() } {
                        tracing::debug!(%error, "failed to release the mouse capture");
                    }
                    self.repaint();
                }
                None
            }
            Pointer::Leave => {
                if self.drag.is_none() {
                    self.show_hover(None);
                }
                None
            }
            Pointer::CaptureLost => {
                self.drag = None;
                self.repaint();
                None
            }
        }
    }

    fn hover_at(&self, point: Point, appearance: &Appearance) -> Option<Hover> {
        if self
            .scroll_bar(appearance)
            .is_some_and(|bar| bar.gutter.contains(point))
        {
            return Some(Hover::Gutter);
        }
        self.field_at(point).map(|field| Hover::Field(field.edit))
    }

    fn show_hover(&mut self, hover: Option<Hover>) {
        if self.hover != hover {
            self.hover = hover;
            self.repaint();
        }
    }

    fn watch_pointer_leave(&self) {
        let mut tracking = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>()
                .try_into()
                .expect("tracking size fits in u32"),
            dwFlags: TME_LEAVE,
            hwndTrack: self.handle(),
            dwHoverTime: 0,
        };
        // SAFETY: `tracking` is a local whose size is set as the call requires.
        if let Err(error) = unsafe { TrackMouseEvent(&raw mut tracking) } {
            tracing::debug!(%error, "failed to track the pointer leaving");
        }
    }

    fn scroll_bar(&self, appearance: &Appearance) -> Option<ScrollBar> {
        let is_expanded = self.hover == Some(Hover::Gutter) || self.drag.is_some();
        let thumb_width = if is_expanded {
            EXPANDED_THUMB_WIDTH
        } else {
            THUMB_WIDTH
        };
        let sizes = ScrollBarSizes {
            gutter: appearance.px(SCROLL_GUTTER),
            thumb_width: appearance.px(thumb_width),
            edge_gap: appearance.px(THUMB_EDGE_GAP),
            min_thumb: appearance.px(MIN_THUMB),
        };
        ScrollBar::place(self.viewport(), self.content_height, self.offset, sizes)
    }

    fn repaint(&self) {
        // Returns whether it invalidated; a failure only delays the repaint.
        // SAFETY: plain call on a window this type owns; the repaint happens later in WM_PAINT.
        let _ = unsafe { InvalidateRect(Some(self.handle()), None, false) };
    }

    /// Repaints the page and its controls with a new appearance.
    pub(crate) fn apply(&self) {
        // Returns whether it redrew; a failure only delays the repaint.
        // SAFETY: plain call on a window this type owns.
        let _ = unsafe {
            RedrawWindow(
                Some(self.handle()),
                None,
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
            )
        };
    }

    /// Paints the cards, the text field frames and the scroll bar. Call only while handling `WM_PAINT`.
    pub(crate) fn paint(&self, appearance: &Appearance) {
        let session = PaintSession::begin(self.handle());
        let scroll_bar = self.scroll_bar(appearance);
        appearance.paint(session.context(), self.viewport(), |painter| {
            for shape in &self.shapes {
                match shape.scrolled(self.offset) {
                    Shape::Card(card) => paint::card(painter, card, appearance)?,
                    Shape::Field(field) => {
                        let is_empty = controls::is_field_empty(field.edit);
                        let frame = FieldPaint {
                            frame: field.frame,
                            text_area: field.text_area,
                            placeholder: (is_empty && !field.placeholder.is_empty())
                                .then_some(field.placeholder),
                            state: self.field_state(field.edit),
                        };
                        paint::text_field(painter, &frame, appearance)?;
                    }
                }
            }
            match scroll_bar {
                Some(bar) => paint::scroll_thumb(painter, bar.thumb, appearance),
                None => Ok(()),
            }
        });
    }

    /// How the text field `edit` draws: hovered also while the pointer is over its frame on the page.
    fn field_state(&self, edit: HWND) -> FieldState {
        match controls::field_state(edit) {
            FieldState::Rest if self.hover == Some(Hover::Field(edit)) => FieldState::Hover,
            state => state,
        }
    }
}

/// The text field whose frame, at the scroll `offset`, contains `point`.
fn field_at(shapes: &[Shape], offset: i32, point: Point) -> Option<FieldPoint> {
    shapes.iter().find_map(|shape| match shape.scrolled(offset) {
        Shape::Field(field) if field.frame.contains(point) => Some(FieldPoint {
            edit: field.edit,
            point: Point {
                x: point.x - field.text_area.x,
                y: point.y - field.text_area.y,
            },
        }),
        Shape::Card(_) | Shape::Field(_) => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEARCH: HWND = HWND(std::ptr::without_provenance_mut(1));
    const PROMPT: HWND = HWND(std::ptr::without_provenance_mut(2));

    fn field(edit: HWND, frame: Bounds, text_area: Bounds) -> Shape {
        Shape::Field(FieldShape {
            frame,
            text_area,
            edit,
            placeholder: "",
        })
    }

    fn shapes() -> Vec<Shape> {
        vec![
            Shape::Card(Bounds {
                x: 0,
                y: 0,
                width: 400,
                height: 300,
            }),
            field(
                SEARCH,
                Bounds {
                    x: 10,
                    y: 20,
                    width: 200,
                    height: 32,
                },
                Bounds {
                    x: 21,
                    y: 26,
                    width: 181,
                    height: 20,
                },
            ),
            field(
                PROMPT,
                Bounds {
                    x: 10,
                    y: 100,
                    width: 200,
                    height: 80,
                },
                Bounds {
                    x: 21,
                    y: 106,
                    width: 181,
                    height: 68,
                },
            ),
        ]
    }

    #[test]
    fn field_at_finds_the_field_under_the_point_in_its_own_coordinates() {
        let shapes = shapes();

        let found = field_at(&shapes, 0, Point { x: 50, y: 120 });

        assert_eq!(
            found,
            Some(FieldPoint {
                edit: PROMPT,
                point: Point { x: 29, y: 14 },
            })
        );
    }

    #[test]
    fn field_at_includes_the_frame_around_the_text() {
        let shapes = shapes();

        let found = field_at(&shapes, 0, Point { x: 12, y: 22 });

        assert_eq!(
            found,
            Some(FieldPoint {
                edit: SEARCH,
                point: Point { x: -9, y: -4 },
            })
        );
    }

    #[test]
    fn field_at_follows_the_scroll_offset() {
        let shapes = shapes();

        let found = field_at(&shapes, 90, Point { x: 50, y: 30 });

        assert_eq!(
            found,
            Some(FieldPoint {
                edit: PROMPT,
                point: Point { x: 29, y: 14 },
            })
        );
    }

    #[test]
    fn field_at_ignores_points_outside_every_frame() {
        let shapes = shapes();

        let found = field_at(&shapes, 0, Point { x: 300, y: 60 });

        assert_eq!(found, None);
    }
}
