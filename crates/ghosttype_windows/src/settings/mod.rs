//! The settings window: a navigation list of pages, like the Settings app, that renders the pages
//! of the core settings view and reports user actions back.
//!
//! Mica fills the whole window: the frame extends over the client area, and every window in it
//! draws with Direct2D into pixels with alpha. Transparent pixels show Mica, and cards and
//! controls blend over it with the WinUI colors.

mod appearance;
mod canvas;
mod controls;
mod form;
mod layout;
mod look;
mod page;
mod page_view;
mod paint;
mod palette;
mod typography;

use std::rc::Rc;

use ghosttype_core::settings_view::{ControlId, PageIcon, SettingsInput, SettingsView};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMSBT_MAINWINDOW, DWMSBT_NONE,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MapWindowPoints, MonitorFromPoint,
    RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE, RedrawWindow, ScreenToClient,
};
use windows::Win32::UI::Controls::{
    DRAWITEMSTRUCT, ICC_PROGRESS_CLASS, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx,
    MARGINS,
};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    BN_CLICKED, GetClientRect, GetCursorPos, GetWindowRect, HICON, ICON_BIG, ICON_SMALL, IDC_IBEAM, IsChild,
    IsIconic, LoadCursorW, MINMAXINFO, SM_CXICON, SM_CXSMICON, SW_HIDE, SW_RESTORE, SW_SHOW, SWP_NOACTIVATE,
    SWP_NOZORDER, SendMessageW, SetCursor, SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow,
    UIS_SET, UISF_HIDEACCEL, UISF_HIDEFOCUS, WM_CHANGEUISTATE, WM_CONTEXTMENU, WM_SETICON, WNDCLASS_STYLES,
    WNDPROC, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT, WS_OVERLAPPEDWINDOW,
};
use windows::core::{BOOL, HSTRING, Owned, Result, w};

use crate::gdi::glyph_icon;
use crate::settings::controls::{Control, ControlKind};
use crate::settings::form::PAGE_TEXT;
use crate::settings::layout::{Bounds, Dpi, Point, ScrollMove, centered_origin};
use crate::settings::page::Page;
use crate::settings::page_view::PageView;
use crate::settings::paint::PaintSession;
use crate::settings::palette::Backdrop;
use crate::settings::typography::TextRole;
use crate::system_theme::{Contrast, SystemTheme, ThemeMode};
use crate::window::{Window, WindowClass, WindowSpec};

pub(crate) use crate::settings::appearance::Appearance;
pub(crate) use crate::settings::controls::PickerMenu;
pub(crate) use crate::settings::page::Pointer;

const WINDOW_TITLE: &str = "GhostType";
const WINDOW_WIDTH: i32 = 1000;
const WINDOW_HEIGHT: i32 = 720;
const MIN_WINDOW_WIDTH: i32 = 640;
const MIN_WINDOW_HEIGHT: i32 = 480;
const NAV_WIDTH: i32 = 280;
const NAV_PADDING_X: i32 = 12;
const NAV_PADDING_TOP: i32 = 8;
const NAV_ITEM_HEIGHT: i32 = 36;
const NAV_ITEM_GAP: i32 = 4;
const WHOLE_WINDOW: i32 = -1;
const APP_GLYPH: &str = "\u{E70F}";
const GEAR_GLYPH: char = '\u{E713}';
const CUBE_GLYPH: char = '\u{E896}';
const TEXT_CURSOR_GLYPH: char = '\u{E70F}';

/// The window procedures that receive the settings window's messages.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Procedures {
    pub(crate) window: WNDPROC,
    pub(crate) page: WNDPROC,
}

/// Something the user did in the settings window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WindowInput {
    /// An input of a control of a core settings page.
    Settings(SettingsInput),
    /// The user opened the picker with this id. Its menu must show outside any borrow of the
    /// runtime.
    OpenMenu(ControlId),
}

/// Pointer input that the system gave to a page but that belongs to a text field on it. A field
/// lets the pointer through where it shows its color key, so its page gets most of that input.
#[derive(Debug, Clone, Copy)]
pub(crate) struct FieldInput {
    edit: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
}

impl FieldInput {
    /// Sends the input to its field. Call outside any borrow of the runtime: the field takes the
    /// focus and tells its page while it handles the input.
    pub(crate) fn send(self) -> LRESULT {
        // SAFETY: plain message to a text field of the settings window, with the arguments that the
        // system made for the page, changed to the field.
        unsafe { SendMessageW(self.edit, self.message, Some(self.wparam), Some(self.lparam)) }
    }
}

/// The title bar and taskbar icons of the window, kept alive while the window shows them.
#[derive(Debug)]
struct WindowIcons {
    _small: Owned<HICON>,
    _large: Owned<HICON>,
}

/// The settings window, hidden instead of destroyed when the user closes it. It shows one page
/// for each page of the core settings view, and a navigation item for each page.
#[derive(Debug)]
pub(crate) struct SettingsWindow {
    tabs: Vec<Control>,
    pages: Vec<PageView>,
    page_class: Rc<WindowClass>,
    selected: usize,
    appearance: Rc<Appearance>,
    icons: Option<WindowIcons>,
    window: Window,
}

impl SettingsWindow {
    /// Creates the hidden window for the current system theme and the DPI where it opens. The
    /// pages come with the first render.
    pub(crate) fn create(procedures: Procedures) -> Result<Self> {
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>()
                .try_into()
                .expect("size fits in u32"),
            dwICC: ICC_STANDARD_CLASSES | ICC_PROGRESS_CLASS,
        };
        // SAFETY: the structure is a local whose size is set as the call requires.
        if !unsafe { InitCommonControlsEx(&raw const controls) }.as_bool() {
            tracing::warn!("failed to load the common controls");
        }
        let window = Window::create(WindowSpec {
            class_name: w!("GhostType.Settings"),
            procedure: procedures.window,
            class_style: WNDCLASS_STYLES::default(),
            style: WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            extended_style: WS_EX_CONTROLPARENT,
            parent: None,
        })?;
        let handle = window.handle();
        // SAFETY: plain call on a window this function owns.
        unsafe { SetWindowTextW(handle, &HSTRING::from(WINDOW_TITLE)) }?;
        let appearance = Rc::new(Appearance::new(SystemTheme::read(), window_dpi(handle))?);
        let mut settings = Self {
            tabs: Vec::new(),
            pages: Vec::new(),
            page_class: Page::register_class(procedures.page, handle)?,
            selected: 0,
            appearance,
            icons: None,
            window,
        };
        settings.apply_to_controls();
        settings.place_on_cursor_monitor()?;
        Ok(settings)
    }

    pub(crate) fn handle(&self) -> HWND {
        self.window.handle()
    }

    /// The appearance the window draws with now; drawing messages must use it.
    pub(crate) fn appearance(&self) -> Rc<Appearance> {
        Rc::clone(&self.appearance)
    }

    /// Shows the window and brings it to the front, restored when it was minimized.
    pub(crate) fn show(&self) {
        // SAFETY: plain call on a window this type owns.
        let command = if unsafe { IsIconic(self.handle()) }.as_bool() {
            SW_RESTORE
        } else {
            SW_SHOW
        };
        // Both return the previous state, not an error.
        // SAFETY: plain calls on a window this type owns.
        unsafe {
            let _ = ShowWindow(self.handle(), command);
            let _ = SetForegroundWindow(self.handle());
        }
        if !self.contains_focus() {
            self.hide_keyboard_cues();
            if let Some(tab) = self.tabs.get(self.selected) {
                tab.focus();
            }
        }
    }

    /// Hides focus visuals until the user moves focus with the keyboard, as WinUI does.
    fn hide_keyboard_cues(&self) {
        let hidden = usize::try_from(UISF_HIDEFOCUS | UISF_HIDEACCEL).unwrap_or_default() << u16::BITS;
        let change = hidden | usize::try_from(UIS_SET).unwrap_or_default();
        // SAFETY: plain message to a window this type owns.
        unsafe {
            SendMessageW(
                self.handle(),
                WM_CHANGEUISTATE,
                Some(WPARAM(change)),
                Some(LPARAM(0)),
            )
        };
    }

    /// Hides the window. It keeps its state for the next time it opens.
    pub(crate) fn hide(&self) {
        // Returns the previous visibility, not an error.
        // SAFETY: plain call on a window this type owns.
        let _ = unsafe { ShowWindow(self.handle(), SW_HIDE) };
    }

    /// Shows `view`: a navigation item and a page for each of its pages.
    pub(crate) fn render(&mut self, view: &SettingsView) -> Result<()> {
        let appearance = Rc::clone(&self.appearance);
        let has_new_pages = self.pages.len() < view.pages.len();
        self.tabs.truncate(view.pages.len());
        self.pages.truncate(view.pages.len());
        for shown in view.pages.iter().skip(self.pages.len()) {
            let tab = Control::create(
                self.handle(),
                ControlKind::NavigationButton,
                TextRole::Body,
                PAGE_TEXT,
            )?
            .with_glyph(glyph(shown.icon));
            tab.apply(&appearance);
            self.tabs.push(tab);
            let page = PageView::create(&self.page_class)?;
            page.apply(&appearance);
            self.pages.push(page);
        }
        for ((tab, page), shown) in self.tabs.iter_mut().zip(&mut self.pages).zip(&view.pages) {
            tab.set_text(shown.title);
            page.render(shown, &appearance)?;
        }
        self.selected = self.selected.min(self.pages.len().saturating_sub(1));
        self.mark_selected_tab();
        self.show_selected_page();
        if has_new_pages {
            self.layout();
        }
        Ok(())
    }

    /// Places the navigation list and the pages for the window's size.
    pub(crate) fn layout(&mut self) {
        let client = client_bounds(self.handle());
        let appearance = Rc::clone(&self.appearance);
        let padding = appearance.px(NAV_PADDING_X);
        let nav_width = appearance.px(NAV_WIDTH);
        let item_height = appearance.px(NAV_ITEM_HEIGHT);
        let mut item = Bounds {
            x: padding,
            y: appearance.px(NAV_PADDING_TOP),
            width: nav_width - 2 * padding,
            height: item_height,
        };
        let mut previous = None;
        for tab in &self.tabs {
            tab.place(item, previous);
            previous = Some(tab.handle());
            item.y += item_height + appearance.px(NAV_ITEM_GAP);
        }
        let bounds = Bounds {
            x: nav_width,
            y: 0,
            width: (client.width - nav_width).max(0),
            height: client.height,
        };
        for page in &mut self.pages {
            move_window(page.page().handle(), bounds);
            page.layout(&appearance);
        }
    }

    /// Reads a `WM_COMMAND` from `control` with `notification`.
    pub(crate) fn input_for(&mut self, control: HWND, notification: u32) -> Option<WindowInput> {
        let page_input = self
            .pages
            .iter()
            .find_map(|page| page.input_for(control, notification));
        if page_input.is_some() || notification != BN_CLICKED {
            return page_input;
        }
        if let Some(index) = self.tabs.iter().position(|tab| tab.handle() == control) {
            self.select(index);
        }
        None
    }

    /// Reads the Enter key: it clicks a focused button, or submits a focused text field.
    pub(crate) fn input_for_enter(&mut self) -> Option<WindowInput> {
        // SAFETY: plain call.
        let focus = unsafe { GetFocus() };
        if controls::look_of(focus).is_some_and(|look| look.kind.is_button()) {
            return self.input_for(focus, BN_CLICKED);
        }
        self.pages.iter().find_map(|page| page.submit_for(focus))
    }

    /// The menu that the picker `id` opens, while the picker shows.
    pub(crate) fn picker_menu(&self, id: &ControlId) -> Option<PickerMenu> {
        self.pages.iter().find_map(|page| page.picker_menu(id))
    }

    /// Reacts to the pointer over the page `page_window`, which scrolls with its scroll bar.
    pub(crate) fn on_page_pointer(&mut self, page_window: HWND, pointer: Pointer) {
        let Some(index) = self.page_index(page_window) else {
            return;
        };
        let appearance = Rc::clone(&self.appearance);
        let scroll = self
            .pages
            .get_mut(index)
            .and_then(|page| page.page_mut().on_pointer(pointer, &appearance));
        if let Some(scroll) = scroll {
            self.scroll_page(index, scroll);
        }
    }

    /// Changes a press of the left button or a context menu request that the page `page_window`
    /// got over one of its text fields to input for that field.
    pub(crate) fn field_input(
        &self,
        page_window: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<FieldInput> {
        let page = self.page_of(page_window)?.page();
        if message == WM_CONTEXTMENU {
            let field = page.field_at(client_point(page_window, pointer_position(lparam))?)?;
            return Some(FieldInput {
                edit: field.edit,
                message,
                wparam: WPARAM(field.edit.0.expose_provenance()),
                lparam,
            });
        }
        let field = page.field_at(pointer_position(lparam))?;
        Some(FieldInput {
            edit: field.edit,
            message,
            wparam,
            lparam: pointer_lparam(field.point),
        })
    }

    /// Shows the text cursor while the pointer is over a text field of the page `page_window`.
    /// Returns whether it did.
    pub(crate) fn show_field_cursor(&self, page_window: HWND) -> bool {
        let Some(page) = self.page_of(page_window) else {
            return false;
        };
        let is_over_field = cursor_in(page_window)
            .and_then(|point| page.page().field_at(point))
            .is_some();
        if !is_over_field {
            return false;
        }
        // SAFETY: plain call that loads a shared system cursor, which is never freed.
        match unsafe { LoadCursorW(None, IDC_IBEAM) } {
            Ok(text_cursor) => {
                // SAFETY: plain call; it returns the previous cursor, which stays shared.
                unsafe { SetCursor(Some(text_cursor)) };
                true
            }
            Err(error) => {
                tracing::debug!(%error, "failed to load the text cursor");
                false
            }
        }
    }

    /// Scrolls the selected page by a mouse wheel rotation, in `WHEEL_DELTA` units up.
    pub(crate) fn scroll_by_wheel(&mut self, rotation: i32) {
        let scroll = Page::wheel_move(rotation, &self.appearance);
        self.scroll_page(self.selected, scroll);
    }

    /// Scrolls the selected page so that the control with keyboard focus is visible.
    pub(crate) fn reveal_focus(&mut self) {
        // SAFETY: plain call.
        let focus = unsafe { GetFocus() };
        let Some(page) = self.pages.get_mut(self.selected) else {
            return;
        };
        let page_window = page.page().handle();
        // SAFETY: plain call on two window handles.
        if focus.is_invalid() || !unsafe { IsChild(page_window, focus) }.as_bool() {
            return;
        }
        if page.page_mut().reveal(bounds_in(focus, page_window)) {
            page.layout(&self.appearance);
        }
    }

    /// Redraws the window for a new system theme.
    pub(crate) fn change_theme(&mut self) -> Result<()> {
        let appearance = Appearance::new(SystemTheme::read(), self.appearance.dpi)?;
        self.change_appearance(appearance);
        Ok(())
    }

    /// Rescales the window for a new DPI and moves it to the size the system `suggested`.
    pub(crate) fn change_dpi(&mut self, dpi: u32, suggested: RECT) -> Result<()> {
        let appearance = Appearance::new(self.appearance.theme, Dpi(dpi))?;
        self.change_appearance(appearance);
        let bounds = Bounds {
            x: suggested.left,
            y: suggested.top,
            width: suggested.right - suggested.left,
            height: suggested.bottom - suggested.top,
        };
        move_window(self.handle(), bounds);
        self.layout();
        Ok(())
    }

    fn change_appearance(&mut self, appearance: Appearance) {
        self.appearance = Rc::new(appearance);
        self.apply_to_controls();
    }

    /// Paints the background of `window`, the settings window or a busy page: Mica, or the solid
    /// backdrop. Call only while handling `WM_PAINT`.
    pub(crate) fn paint_background(window: HWND, appearance: &Appearance) {
        let session = PaintSession::begin(window);
        appearance.paint(session.context(), client_bounds(window), |_| Ok(()));
    }

    /// Paints the page `page_window`. Call only while handling `WM_PAINT`.
    pub(crate) fn paint_page(&self, page_window: HWND) {
        if let Some(page) = self.page_of(page_window) {
            page.page().paint(&self.appearance);
        }
    }

    /// Draws a control for the `WM_DRAWITEM` whose structure `lparam` points to. Returns whether
    /// the control is one of the settings window's.
    pub(crate) fn draw_item(lparam: LPARAM, appearance: &Appearance) -> bool {
        // SAFETY: for WM_DRAWITEM the lparam points to a DRAWITEMSTRUCT valid during the message.
        let item = unsafe { &*std::ptr::with_exposed_provenance::<DRAWITEMSTRUCT>(lparam.0.cast_unsigned()) };
        controls::draw_item(item, appearance)
    }

    /// Sets the smallest size the user can drag the window to, for `WM_GETMINMAXINFO`.
    pub(crate) fn limit_size(handle: HWND, lparam: LPARAM) {
        let dpi = window_dpi(handle);
        // SAFETY: for WM_GETMINMAXINFO the lparam points to a MINMAXINFO valid during the message.
        let limits =
            unsafe { &mut *std::ptr::with_exposed_provenance_mut::<MINMAXINFO>(lparam.0.cast_unsigned()) };
        limits.ptMinTrackSize = POINT {
            x: dpi.scale(MIN_WINDOW_WIDTH),
            y: dpi.scale(MIN_WINDOW_HEIGHT),
        };
    }

    fn select(&mut self, index: usize) {
        self.selected = index;
        self.mark_selected_tab();
        self.show_selected_page();
    }

    fn mark_selected_tab(&self) {
        for (index, tab) in self.tabs.iter().enumerate() {
            if index == self.selected {
                tab.mark_selected();
            } else {
                tab.mark_unselected();
            }
        }
    }

    fn show_selected_page(&self) {
        for (index, page) in self.pages.iter().enumerate() {
            if index == self.selected {
                page.page().show();
            } else {
                page.page().hide();
            }
        }
    }

    fn scroll_page(&mut self, index: usize, scroll: ScrollMove) {
        let appearance = Rc::clone(&self.appearance);
        if let Some(page) = self.pages.get_mut(index)
            && page.page_mut().scroll(scroll)
        {
            page.layout(&appearance);
        }
    }

    fn page_index(&self, page_window: HWND) -> Option<usize> {
        self.pages
            .iter()
            .position(|page| page.page().handle() == page_window)
    }

    fn page_of(&self, page_window: HWND) -> Option<&PageView> {
        self.pages.iter().find(|page| page.page().handle() == page_window)
    }

    fn contains_focus(&self) -> bool {
        // SAFETY: plain calls.
        let focus = unsafe { GetFocus() };
        // SAFETY: plain call on two window handles.
        !focus.is_invalid() && unsafe { IsChild(self.handle(), focus) }.as_bool()
    }

    fn apply_to_controls(&mut self) {
        let appearance = Rc::clone(&self.appearance);
        apply_frame(self.handle(), &appearance);
        match set_icons(self.handle(), &appearance) {
            Ok(icons) => self.icons = Some(icons),
            Err(error) => tracing::warn!(%error, "failed to set window icons"),
        }
        for tab in &self.tabs {
            tab.apply(&appearance);
        }
        for page in &self.pages {
            page.apply(&appearance);
        }
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

    fn place_on_cursor_monitor(&self) -> Result<()> {
        let mut cursor = POINT::default();
        // SAFETY: `cursor` is a local the call fills in.
        unsafe { GetCursorPos(&raw mut cursor) }?;
        // SAFETY: plain call.
        let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>()
                .try_into()
                .expect("info size fits in u32"),
            ..MONITORINFO::default()
        };
        // SAFETY: `info` is a local whose size is set as the call requires.
        if !unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() {
            return Ok(());
        }
        let work_area = Bounds {
            x: info.rcWork.left,
            y: info.rcWork.top,
            width: info.rcWork.right - info.rcWork.left,
            height: info.rcWork.bottom - info.rcWork.top,
        };
        let dpi = self.appearance.dpi;
        let (width, height) = (
            dpi.scale(WINDOW_WIDTH).min(work_area.width),
            dpi.scale(WINDOW_HEIGHT).min(work_area.height),
        );
        let (x, y) = centered_origin(work_area, width, height);
        move_window(self.handle(), Bounds { x, y, width, height });
        Ok(())
    }
}

/// Reads the pointer position of a mouse message: two signed words in `lparam`.
pub(crate) fn pointer_position(lparam: LPARAM) -> Point {
    let value = lparam.0.cast_unsigned();
    let word = |shift: u32| {
        let masked = (value >> shift) & usize::from(u16::MAX);
        i32::from(u16::try_from(masked).expect("masked to 16 bits").cast_signed())
    };
    Point {
        x: word(0),
        y: word(u16::BITS),
    }
}

/// Writes `point` as the pointer position of a mouse message, as [`pointer_position`] reads it.
fn pointer_lparam(point: Point) -> LPARAM {
    let word = |value: i32| {
        let clamped = value.clamp(i16::MIN.into(), i16::MAX.into());
        usize::from(
            i16::try_from(clamped)
                .expect("clamped to 16 bits")
                .cast_unsigned(),
        )
    };
    LPARAM((word(point.x) | (word(point.y) << u16::BITS)).cast_signed())
}

/// The pointer position in the client coordinates of `window`.
fn cursor_in(window: HWND) -> Option<Point> {
    let mut cursor = POINT::default();
    // SAFETY: `cursor` is a local the call fills in.
    unsafe { GetCursorPos(&raw mut cursor) }.ok()?;
    client_point(
        window,
        Point {
            x: cursor.x,
            y: cursor.y,
        },
    )
}

/// Converts `screen`, a point on the screen, to the client coordinates of `window`.
fn client_point(window: HWND, screen: Point) -> Option<Point> {
    let mut point = POINT {
        x: screen.x,
        y: screen.y,
    };
    // SAFETY: `point` is a local the call converts in place.
    let is_converted = unsafe { ScreenToClient(window, &raw mut point) }.as_bool();
    is_converted.then_some(Point {
        x: point.x,
        y: point.y,
    })
}

fn apply_frame(handle: HWND, appearance: &Appearance) {
    let theme = appearance.theme;
    let is_dark = BOOL::from(theme.mode == ThemeMode::Dark && theme.contrast == Contrast::Standard);
    let (backdrop, frame_extent) = match appearance.backdrop {
        Backdrop::Mica => (DWMSBT_MAINWINDOW, WHOLE_WINDOW),
        Backdrop::Solid => (DWMSBT_NONE, 0),
    };
    let margins = MARGINS {
        cxLeftWidth: frame_extent,
        cxRightWidth: frame_extent,
        cyTopHeight: frame_extent,
        cyBottomHeight: frame_extent,
    };
    // SAFETY: `margins` is a local that outlives the call.
    if let Err(error) = unsafe { DwmExtendFrameIntoClientArea(handle, &raw const margins) } {
        tracing::debug!(%error, "window frame extension unavailable");
    }
    set_frame_attribute(
        handle,
        DWMWA_USE_IMMERSIVE_DARK_MODE
            .0
            .try_into()
            .expect("attribute fits"),
        &is_dark,
    );
    set_frame_attribute::<DWM_SYSTEMBACKDROP_TYPE>(
        handle,
        DWMWA_SYSTEMBACKDROP_TYPE.0.try_into().expect("attribute fits"),
        &backdrop,
    );
    set_frame_attribute::<DWM_WINDOW_CORNER_PREFERENCE>(
        handle,
        DWMWA_WINDOW_CORNER_PREFERENCE
            .0
            .try_into()
            .expect("attribute fits"),
        &DWMWCP_ROUND,
    );
}

fn set_frame_attribute<Value>(handle: HWND, attribute: u32, value: &Value) {
    let size = size_of::<Value>().try_into().expect("attribute size fits in u32");
    // SAFETY: `value` is a live value of the size passed, of the type the attribute expects.
    let result = unsafe {
        DwmSetWindowAttribute(
            handle,
            windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE(attribute.try_into().expect("attribute fits")),
            std::ptr::from_ref(value).cast(),
            size,
        )
    };
    if let Err(error) = result {
        tracing::debug!(%error, attribute, "window frame attribute unavailable");
    }
}

/// Gives the window the GhostType glyph in the accent shade, for the title bar and the taskbar.
fn set_icons(handle: HWND, appearance: &Appearance) -> Result<WindowIcons> {
    let color = appearance.palette.accent.rest.rgb;
    let dpi = appearance.dpi.0;
    // SAFETY: plain calls.
    let (small_side, large_side) = unsafe {
        (
            GetSystemMetricsForDpi(SM_CXSMICON, dpi),
            GetSystemMetricsForDpi(SM_CXICON, dpi),
        )
    };
    let small = glyph_icon(APP_GLYPH, small_side, color)?;
    let large = glyph_icon(APP_GLYPH, large_side, color)?;
    for (kind, icon) in [(ICON_SMALL, &small), (ICON_BIG, &large)] {
        // SAFETY: the icon stays alive in the returned value while the window shows it.
        unsafe {
            SendMessageW(
                handle,
                WM_SETICON,
                Some(WPARAM(usize::try_from(kind).unwrap_or_default())),
                Some(LPARAM(icon.0.expose_provenance().cast_signed())),
            )
        };
    }
    Ok(WindowIcons {
        _small: small,
        _large: large,
    })
}

/// The glyph of Segoe Fluent Icons that shows `icon`.
fn glyph(icon: PageIcon) -> char {
    match icon {
        PageIcon::Gear => GEAR_GLYPH,
        PageIcon::Cube => CUBE_GLYPH,
        PageIcon::TextCursor => TEXT_CURSOR_GLYPH,
    }
}

fn window_dpi(handle: HWND) -> Dpi {
    // SAFETY: plain call on a live window.
    Dpi(unsafe { GetDpiForWindow(handle) })
}

fn client_bounds(handle: HWND) -> Bounds {
    let mut client = RECT::default();
    // SAFETY: `client` is a local the call fills in.
    if let Err(error) = unsafe { GetClientRect(handle, &raw mut client) } {
        tracing::warn!(%error, "failed to read window size");
    }
    Bounds {
        x: 0,
        y: 0,
        width: client.right,
        height: client.bottom,
    }
}

fn bounds_in(control: HWND, parent: HWND) -> Bounds {
    let mut area = RECT::default();
    // SAFETY: `area` is a local the call fills in.
    if unsafe { GetWindowRect(control, &raw mut area) }.is_err() {
        return Bounds::default();
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
    unsafe { MapWindowPoints(None, Some(parent), &mut corners) };
    Bounds {
        x: corners[0].x,
        y: corners[0].y,
        width: corners[1].x - corners[0].x,
        height: corners[1].y - corners[0].y,
    }
}

fn move_window(handle: HWND, bounds: Bounds) {
    // SAFETY: plain call on a window of this thread.
    let moved = unsafe {
        SetWindowPos(
            handle,
            None,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
    if let Err(error) = moved {
        tracing::warn!(%error, "failed to move window");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_position_reads_signed_words() {
        let lparam = LPARAM(0x0028_FFFB);

        assert_eq!(pointer_position(lparam), Point { x: -5, y: 40 });
    }

    #[test]
    fn pointer_lparam_writes_signed_words() {
        let point = Point { x: -9, y: 40 };

        assert_eq!(pointer_lparam(point), LPARAM(0x0028_FFF7));
    }
}
