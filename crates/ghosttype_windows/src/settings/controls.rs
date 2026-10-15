//! Standard Windows controls of the settings window, destroyed on drop.
//!
//! Standard controls give each element its UI Automation role, name and keyboard behavior. The
//! settings window draws them itself (owner draw) with Direct2D, so they look like WinUI
//! controls over Mica. Text fields stay edit controls: layered windows whose background color is
//! a color key, so only their text and caret cover the frame that the page draws behind them.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{InvalidateRect, MapWindowPoints};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, EM_SETMARGINS, ODA_DRAWENTIRE, ODS_FLAGS, ODT_STATIC, PBM_GETPOS, PBM_SETPOS,
    PBM_SETRANGE32, SetWindowTheme, WM_MOUSELEAVE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, IsWindowEnabled, SetFocus, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, EC_LEFTMARGIN, EC_RIGHTMARGIN, GWLP_USERDATA, GetClientRect, GetParent,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, HMENU, HWND_TOP, SW_HIDE, SW_SHOWNA,
    SWP_NOACTIVATE, SendMessageW, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_DRAWITEM, WM_ERASEBKGND, WM_MOUSEMOVE, WM_NCDESTROY, WM_PAINT,
    WM_SETFONT, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_TABSTOP,
};
use windows::core::{HSTRING, PCWSTR, Result, w};

use crate::settings::appearance::Appearance;
use crate::settings::layout::Bounds;
use crate::settings::look::ControlLook;
use crate::settings::paint::{self, ControlPaint, FieldState, PaintSession};
use crate::settings::palette::ControlStyle;
use crate::settings::typography::TextRole;

/// The id of every control. Above `IDOK` and `IDCANCEL`, so dialog keys never click a control.
const CONTROL_ID: u16 = 0x1000;
const SUBCLASS_ID: usize = 1;
const PROGRESS_FULL: isize = 1000;
const STATIC_OWNER_DRAW: u32 = 0x000D;
const STATIC_NOPREFIX: u32 = 0x0080;
const BUTTON_OWNER_DRAW: u32 = 0x000B;
const EDIT_AUTO_SCROLL: u32 = 0x0080;
const EDIT_MULTILINE: u32 = 0x0004;
const EDIT_AUTO_VERTICAL_SCROLL: u32 = 0x0040;
const EDIT_WANT_RETURN: u32 = 0x1000;
const DRAWN: LRESULT = LRESULT(1);

/// What a control is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlKind {
    /// One line of text, cut with an ellipsis when too long.
    Label,
    /// One line of text aligned to the right edge.
    TrailingLabel,
    /// Text that wraps onto as many lines as it needs.
    Paragraph,
    /// A push button.
    Button,
    /// The push button of the primary action, filled with the accent color.
    PrimaryButton,
    /// An entry of the navigation list.
    NavigationButton,
    /// A button that shows a choice and opens a menu of the other choices, like a combo box.
    Picker,
    /// A toggle switch: a button that turns a setting on or off, with its state as text.
    Switch,
    /// A one-line text field.
    TextField,
    /// A text field of several lines, where Enter starts a new line.
    TextArea,
    /// A bar that fills as work completes.
    ProgressBar,
}

impl ControlKind {
    fn class_and_style(self) -> (PCWSTR, u32) {
        match self {
            Self::Label | Self::TrailingLabel | Self::Paragraph => {
                (w!("Static"), STATIC_OWNER_DRAW | STATIC_NOPREFIX)
            }
            Self::Button | Self::PrimaryButton | Self::NavigationButton | Self::Picker | Self::Switch => {
                (w!("Button"), BUTTON_OWNER_DRAW | WS_TABSTOP.0)
            }
            Self::TextField => (w!("Edit"), EDIT_AUTO_SCROLL | WS_TABSTOP.0),
            Self::TextArea => (
                w!("Edit"),
                EDIT_MULTILINE | EDIT_AUTO_VERTICAL_SCROLL | EDIT_WANT_RETURN | WS_TABSTOP.0,
            ),
            Self::ProgressBar => (w!("msctls_progress32"), 0),
        }
    }

    fn extended_style(self) -> WINDOW_EX_STYLE {
        match self {
            Self::TextField | Self::TextArea => WS_EX_LAYERED,
            Self::Label
            | Self::TrailingLabel
            | Self::Paragraph
            | Self::Button
            | Self::PrimaryButton
            | Self::NavigationButton
            | Self::Picker
            | Self::Switch
            | Self::ProgressBar => WINDOW_EX_STYLE::default(),
        }
    }

    /// Whether the control reacts to the pointer passing over it.
    fn tracks_hover(self) -> bool {
        match self {
            Self::Button
            | Self::PrimaryButton
            | Self::NavigationButton
            | Self::Picker
            | Self::Switch
            | Self::TextField
            | Self::TextArea => true,
            Self::Label | Self::TrailingLabel | Self::Paragraph | Self::ProgressBar => false,
        }
    }

    /// Whether the user types into the control, over a frame that its page draws.
    pub(crate) fn is_text_field(self) -> bool {
        matches!(self, Self::TextField | Self::TextArea)
    }

    /// Whether the user activates the control like a button, with a click, Space or Enter.
    pub(crate) fn is_button(self) -> bool {
        match self {
            Self::Button | Self::PrimaryButton | Self::NavigationButton | Self::Picker | Self::Switch => true,
            Self::Label
            | Self::TrailingLabel
            | Self::Paragraph
            | Self::TextField
            | Self::TextArea
            | Self::ProgressBar => false,
        }
    }
}

/// A standard control: a child window of the settings window.
#[derive(Debug)]
pub(crate) struct Control {
    handle: HWND,
    kind: ControlKind,
    text: String,
    placeholder: &'static str,
    choices: Vec<String>,
    selected_choice: usize,
}

impl Control {
    /// Creates a hidden control in `parent` that writes in `role` and draws in `style`.
    pub(crate) fn create(
        parent: HWND,
        kind: ControlKind,
        role: TextRole,
        style: ControlStyle,
    ) -> Result<Self> {
        let (class, kind_style) = kind.class_and_style();
        // SAFETY: the class names are static system classes; the parent is a live window of this thread.
        let handle = unsafe {
            CreateWindowExW(
                kind.extended_style(),
                class,
                PCWSTR::null(),
                WINDOW_STYLE(kind_style) | WS_CHILD | WS_CLIPSIBLINGS,
                0,
                0,
                0,
                0,
                Some(parent),
                Some(HMENU(std::ptr::with_exposed_provenance_mut(usize::from(
                    CONTROL_ID,
                )))),
                None,
                None,
            )
        }?;
        let control = Self {
            handle,
            kind,
            text: String::new(),
            placeholder: "",
            choices: Vec::new(),
            selected_choice: 0,
        };
        write_look(handle, ControlLook::new(kind, role, style));
        control.prepare();
        Ok(control)
    }

    fn prepare(&self) {
        match self.kind {
            ControlKind::ProgressBar => {
                // SAFETY: plain message to the control just created.
                unsafe {
                    SendMessageW(
                        self.handle,
                        PBM_SETRANGE32,
                        Some(WPARAM(0)),
                        Some(LPARAM(PROGRESS_FULL)),
                    )
                };
                // Visual styles animate the bar on their own timer; the settings window draws it instead.
                // SAFETY: the empty theme names are static.
                if let Err(error) = unsafe { SetWindowTheme(self.handle, w!(""), w!("")) } {
                    tracing::debug!(%error, "progress bar keeps its theme");
                }
            }
            ControlKind::Label
            | ControlKind::TextField
            | ControlKind::TextArea
            | ControlKind::TrailingLabel
            | ControlKind::Paragraph
            | ControlKind::Button
            | ControlKind::PrimaryButton
            | ControlKind::NavigationButton
            | ControlKind::Picker
            | ControlKind::Switch => {}
        }
        if self.kind.tracks_hover() || self.kind == ControlKind::ProgressBar {
            // SAFETY: the procedure lives for the whole program and removes itself on WM_NCDESTROY.
            if !unsafe { SetWindowSubclass(self.handle, Some(control_procedure), SUBCLASS_ID, 0) }.as_bool() {
                tracing::warn!("failed to watch control for the pointer");
            }
        }
    }

    /// Shows `glyph` from Segoe Fluent Icons before the label of a navigation item.
    #[must_use]
    pub(crate) fn with_glyph(self, glyph: char) -> Self {
        self.change_look(|look| look.glyph = Some(glyph));
        self
    }

    pub(crate) fn handle(&self) -> HWND {
        self.handle
    }

    /// The text this control shows, as last set.
    pub(crate) fn current_text(&self) -> &str {
        &self.text
    }

    /// Shows `text`. Does nothing when the text is already shown, so controls don't repaint.
    pub(crate) fn set_text(&mut self, text: &str) {
        if self.text == text {
            return;
        }
        self.write_text(text);
    }

    fn write_text(&mut self, text: &str) {
        text.clone_into(&mut self.text);
        // SAFETY: plain call on a control this type owns; the text outlives the call.
        if let Err(error) = unsafe { SetWindowTextW(self.handle, &HSTRING::from(text)) } {
            tracing::warn!(%error, "failed to set control text");
        }
        self.repaint();
    }

    /// Asks for a repaint from the message loop, which draws with the current appearance even
    /// when an immediate repaint ran while the runtime was busy.
    fn repaint(&self) {
        // Returns whether it invalidated; a failure only delays the repaint.
        // SAFETY: plain call on a control this type owns.
        let _ = unsafe { InvalidateRect(Some(self.handle), None, false) };
    }

    /// Reads the text the user typed into a text field.
    pub(crate) fn typed_text(&self) -> String {
        window_text(self.handle)
    }

    /// Shows `text` in a text field, unless the field already holds it, so that the caret and
    /// the undo of the user stay while the user types.
    pub(crate) fn replace_typed_text(&mut self, text: &str) {
        if self.typed_text() == text {
            return;
        }
        self.write_text(text);
    }

    /// The hint that a text field shows while it is empty.
    pub(crate) fn placeholder(&self) -> &'static str {
        self.placeholder
    }

    pub(crate) fn set_placeholder(&mut self, placeholder: &'static str) {
        self.placeholder = placeholder;
    }

    pub(crate) fn show(&self) {
        // Returns the previous visibility, not an error.
        // SAFETY: plain call on a control this type owns.
        let _ = unsafe { ShowWindow(self.handle, SW_SHOWNA) };
    }

    pub(crate) fn hide(&self) {
        // Returns the previous visibility, not an error.
        // SAFETY: plain call on a control this type owns.
        let _ = unsafe { ShowWindow(self.handle, SW_HIDE) };
    }

    pub(crate) fn enable(&self) {
        // Returns the previous state, not an error.
        // SAFETY: plain call on a control this type owns.
        let _ = unsafe { EnableWindow(self.handle, true) };
    }

    pub(crate) fn disable(&self) {
        // Returns the previous state, not an error.
        // SAFETY: plain call on a control this type owns.
        let _ = unsafe { EnableWindow(self.handle, false) };
    }

    pub(crate) fn focus(&self) {
        // SAFETY: plain call on a control this type owns.
        if let Err(error) = unsafe { SetFocus(Some(self.handle)) } {
            tracing::warn!(%error, "failed to focus control");
        }
    }

    /// Moves the control to `bounds` and right after `previous` in tab order, then shows it.
    pub(crate) fn place(&self, bounds: Bounds, previous: Option<HWND>) {
        // SAFETY: plain call on a control this type owns; `previous` is a sibling.
        let placed = unsafe {
            SetWindowPos(
                self.handle,
                Some(previous.unwrap_or(HWND_TOP)),
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                SWP_NOACTIVATE,
            )
        };
        if let Err(error) = placed {
            tracing::warn!(%error, "failed to place control");
        }
        self.show();
    }

    /// Changes the surface and tone the control draws with.
    pub(crate) fn set_style(&self, style: ControlStyle) {
        self.change_look(|look| look.style = style);
    }

    /// Marks a navigation item as the one for the shown page.
    pub(crate) fn mark_selected(&self) {
        self.change_look(|look| look.is_selected = true);
    }

    pub(crate) fn mark_unselected(&self) {
        self.change_look(|look| look.is_selected = false);
    }

    fn change_look(&self, change: impl FnOnce(&mut ControlLook)) {
        let Some(mut look) = look_of(self.handle) else {
            return;
        };
        let before = look;
        change(&mut look);
        if look != before {
            write_look(self.handle, look);
            self.repaint();
        }
    }

    /// Applies the font and color key of a text field, and repaints any control with `appearance`.
    pub(crate) fn apply(&self, appearance: &Appearance) {
        if self.kind.is_text_field() {
            let font = appearance.field_font().handle();
            let margins = usize::try_from(EC_LEFTMARGIN | EC_RIGHTMARGIN).unwrap_or_default();
            // SAFETY: the font stays alive while this appearance is in use. A new font resets the
            // margins, so zero margins, which put the text where the page draws the placeholder, follow it.
            unsafe {
                SendMessageW(
                    self.handle,
                    WM_SETFONT,
                    Some(WPARAM(font.0.expose_provenance())),
                    Some(LPARAM(1)),
                );
                SendMessageW(self.handle, EM_SETMARGINS, Some(WPARAM(margins)), Some(LPARAM(0)));
            }
            appearance.apply_field_key(self.handle);
        }
        self.repaint();
    }

    /// Fills a progress bar to `permille` thousandths.
    pub(crate) fn set_progress(&self, permille: u16) {
        // SAFETY: plain message to a control this type owns.
        unsafe {
            SendMessageW(
                self.handle,
                PBM_SETPOS,
                Some(WPARAM(permille.into())),
                Some(LPARAM(0)),
            )
        };
    }

    /// Gives a picker the choices `titles` and shows the one at `selected`.
    pub(crate) fn set_choices(&mut self, titles: &[String], selected: usize) {
        titles.clone_into(&mut self.choices);
        self.selected_choice = selected;
        let shown = titles.get(selected).cloned().unwrap_or_default();
        self.set_text(&shown);
    }

    /// The menu of choices that a picker opens, below the picker on the screen.
    pub(crate) fn picker_menu(&self) -> PickerMenu {
        let mut area = RECT::default();
        // SAFETY: `area` is a local the call fills in.
        if let Err(error) = unsafe { GetClientRect(self.handle, &raw mut area) } {
            tracing::warn!(%error, "failed to read picker size");
        }
        let mut corners = [
            POINT {
                x: area.left,
                y: area.top,
            },
            POINT {
                x: area.right,
                y: area.bottom,
            },
        ];
        // SAFETY: the points are a local array the call converts in place.
        unsafe { MapWindowPoints(Some(self.handle), None, &mut corners) };
        PickerMenu {
            anchor: RECT {
                left: corners[0].x,
                top: corners[0].y,
                right: corners[1].x,
                bottom: corners[1].y,
            },
            titles: self.choices.clone(),
            selected: self.selected_choice,
        }
    }
}

/// The menu of choices that a picker opens.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PickerMenu {
    /// The picker on the screen, which the menu opens below.
    pub(crate) anchor: RECT,
    pub(crate) titles: Vec<String>,
    pub(crate) selected: usize,
}

impl Drop for Control {
    fn drop(&mut self) {
        // SAFETY: the control was created by this type and is destroyed once, before its parent.
        if let Err(error) = unsafe { DestroyWindow(self.handle) } {
            tracing::warn!(%error, "failed to destroy control");
        }
    }
}

/// Draws the control of a `WM_DRAWITEM` with `appearance`. Returns whether it is one of the
/// settings window's controls.
pub(crate) fn draw_item(item: &DRAWITEMSTRUCT, appearance: &Appearance) -> bool {
    let Some(look) = look_of(item.hwndItem) else {
        return false;
    };
    let text = window_text(item.hwndItem);
    let progress = if look.kind == ControlKind::ProgressBar {
        progress_of(item.hwndItem)
    } else {
        0
    };
    let control = ControlPaint {
        look,
        text: &text,
        interaction: paint::interaction(item.itemState, look),
        has_focus_visual: paint::has_focus_visual(item.itemState),
        progress,
    };
    let area = Bounds {
        x: item.rcItem.left,
        y: item.rcItem.top,
        width: item.rcItem.right - item.rcItem.left,
        height: item.rcItem.bottom - item.rcItem.top,
    };
    let local = Bounds { x: 0, y: 0, ..area };
    appearance.paint(item.hDC, area, |painter| {
        paint::control(painter, local, &control, appearance)
    });
    true
}

/// The look a control of this module stores in its window, if `window` is one.
pub(crate) fn look_of(window: HWND) -> Option<ControlLook> {
    // SAFETY: plain call on a window handle; foreign windows return their own data, which fails to parse.
    let data = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    ControlLook::from_data(data.cast_unsigned())
}

/// How the text field `edit` draws now: focus, pointer and enabled state.
pub(crate) fn field_state(edit: HWND) -> FieldState {
    // SAFETY: plain calls on a window handle.
    let (is_enabled, focus) = unsafe { (IsWindowEnabled(edit).as_bool(), GetFocus()) };
    if !is_enabled {
        FieldState::Disabled
    } else if focus == edit {
        FieldState::Focused
    } else if look_of(edit).is_some_and(|look| look.is_hovered) {
        FieldState::Hover
    } else {
        FieldState::Rest
    }
}

/// Whether the text field `edit` holds no text.
pub(crate) fn is_field_empty(edit: HWND) -> bool {
    // SAFETY: plain call on a window handle.
    let length = unsafe { GetWindowTextLengthW(edit) };
    length == 0
}

fn write_look(window: HWND, look: ControlLook) {
    // SAFETY: plain call on a control of this module; its user data is free for the application.
    unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, look.to_data().cast_signed()) };
}

fn window_text(window: HWND) -> String {
    // SAFETY: plain call on a window handle.
    let length = unsafe { GetWindowTextLengthW(window) };
    let mut units = vec![0_u16; usize::try_from(length).unwrap_or(0) + 1];
    // SAFETY: the buffer is valid for the call and holds the text and its terminator.
    let copied = unsafe { GetWindowTextW(window, &mut units) };
    String::from_utf16_lossy(
        units
            .get(..usize::try_from(copied).unwrap_or(0))
            .unwrap_or_default(),
    )
}

fn progress_of(window: HWND) -> u16 {
    // SAFETY: plain message to a progress bar.
    let position = unsafe { SendMessageW(window, PBM_GETPOS, None, None) };
    u16::try_from(position.0).unwrap_or(0)
}

/// Watches the pointer over controls and draws progress bars like owner-drawn controls.
extern "system" fn control_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass: usize,
    _data: usize,
) -> LRESULT {
    let is_progress_bar = || look_of(window).is_some_and(|look| look.kind == ControlKind::ProgressBar);
    match message {
        WM_MOUSEMOVE => begin_hover(window),
        WM_MOUSELEAVE => end_hover(window),
        WM_PAINT if is_progress_bar() => {
            paint_progress(window);
            return LRESULT(0);
        }
        WM_ERASEBKGND if is_progress_bar() => return DRAWN,
        WM_NCDESTROY => remove_subclass(window),
        _ => {}
    }
    // SAFETY: forwards the system's own arguments unchanged.
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

fn remove_subclass(window: HWND) {
    // SAFETY: removes the subclass this module installed, as the window goes away.
    if !unsafe { RemoveWindowSubclass(window, Some(control_procedure), SUBCLASS_ID) }.as_bool() {
        tracing::debug!("control subclass already gone");
    }
}

fn begin_hover(window: HWND) {
    let Some(look) = look_of(window).filter(|look| !look.is_hovered) else {
        return;
    };
    let mut tracking = TRACKMOUSEEVENT {
        cbSize: size_of::<TRACKMOUSEEVENT>()
            .try_into()
            .expect("tracking size fits in u32"),
        dwFlags: TME_LEAVE,
        hwndTrack: window,
        dwHoverTime: 0,
    };
    // SAFETY: `tracking` is a local whose size is set as the call requires.
    if let Err(error) = unsafe { TrackMouseEvent(&raw mut tracking) } {
        tracing::debug!(%error, "failed to track the pointer leaving");
        return;
    }
    show_hover(
        window,
        ControlLook {
            is_hovered: true,
            ..look
        },
    );
}

fn end_hover(window: HWND) {
    if let Some(look) = look_of(window).filter(|look| look.is_hovered) {
        show_hover(
            window,
            ControlLook {
                is_hovered: false,
                ..look
            },
        );
    }
}

/// Stores the hover state and repaints what shows it: the page frame for text fields.
fn show_hover(window: HWND, look: ControlLook) {
    write_look(window, look);
    let shown_by = if look.kind.is_text_field() {
        // SAFETY: plain call on a child window.
        unsafe { GetParent(window) }.unwrap_or(window)
    } else {
        window
    };
    // Returns whether it invalidated; a failure only delays the repaint.
    // SAFETY: plain call on a window of this thread.
    let _ = unsafe { InvalidateRect(Some(shown_by), None, false) };
}

/// Paints a progress bar through its parent's `WM_DRAWITEM`, like the owner-drawn controls.
fn paint_progress(window: HWND) {
    let session = PaintSession::begin(window);
    let mut area = RECT::default();
    // SAFETY: `area` is a local the call fills in.
    if unsafe { GetClientRect(window, &raw mut area) }.is_err() {
        return;
    }
    let item = DRAWITEMSTRUCT {
        CtlType: ODT_STATIC,
        CtlID: CONTROL_ID.into(),
        itemID: 0,
        itemAction: ODA_DRAWENTIRE,
        itemState: ODS_FLAGS(0),
        hwndItem: window,
        hDC: session.context(),
        rcItem: area,
        itemData: 0,
    };
    // SAFETY: plain call on a child window.
    let Ok(parent) = (unsafe { GetParent(window) }) else {
        return;
    };
    // SAFETY: `item` and its device context stay valid while the parent handles the message.
    unsafe {
        SendMessageW(
            parent,
            WM_DRAWITEM,
            Some(WPARAM(usize::from(CONTROL_ID))),
            Some(LPARAM(
                std::ptr::from_ref(&item).expose_provenance().cast_signed(),
            )),
        )
    };
}
