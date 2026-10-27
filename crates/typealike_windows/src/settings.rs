//! The settings window, in WinUI 3 through windows-reactor, on a thread of its own. It shows each
//! page of the core settings view, with a navigation item for each page. This module is also the
//! one place reactor callbacks on that thread reach the window's state.

mod app_icon;
mod icons;
mod layout;
mod page;
mod text_area_box;

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use typealike_core::settings_window::view::{ControlId, Row, SettingsInput, SettingsView};
use windows::Win32::Foundation::E_UNEXPECTED;
use windows::core::{Error, HRESULT, Result};
use windows_reactor::{
    App, AppContext, AppProxy, Component, ComponentContext, EncodedImage, Grid, LocalSender, NavigationView,
    NavigationViewBackButtonVisible, NavigationViewItem, NavigationViewPaneDisplayMode, View, ViewContext,
    WindowBackdrop, WindowPolicy, WindowVisuals, keyed,
};

use crate::mailbox::Mailbox;
use crate::settings::icons::page_icon;
use crate::settings::page::Renderer;
use crate::system_theme::{Contrast, SystemTheme};

const THREAD_NAME: &str = "settings window";
const WINDOW_TITLE: &str = "TypeAlike";
const WINDOW_WIDTH: f64 = 1000.0;
const WINDOW_HEIGHT: f64 = 720.0;
const MIN_WINDOW_WIDTH: f64 = 640.0;
const MIN_WINDOW_HEIGHT: f64 = 480.0;
const NAVIGATION_WIDTH: f64 = 280.0;

/// The state that reactor callbacks on the settings thread can reach.
struct Host {
    inputs: Mailbox<SettingsInput>,
    /// The last view from the controller. A window that opens starts with it.
    view: SettingsView,
    page: PageState,
}

/// Whether the settings window shows.
enum PageState {
    Closed,
    /// The window opens, and its page does not exist yet.
    Opening,
    /// The window shows its page, which takes messages here.
    Open(LocalSender<PageMessage>),
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

fn with_host<Output>(action: impl FnOnce(&mut Host) -> Output) -> Option<Output> {
    HOST.with(|host| host.try_borrow_mut().ok()?.as_mut().map(action))
}

/// The settings window on a thread of its own, from the side of the controller. Drop ends the
/// thread and closes the window.
pub(crate) struct SettingsThread {
    proxy: AppProxy,
    thread: Option<JoinHandle<()>>,
}

impl fmt::Debug for SettingsThread {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SettingsThread")
            .field("thread", &self.thread)
            .finish_non_exhaustive()
    }
}

impl SettingsThread {
    /// Starts the thread and the WinUI app on it. The window sends each input of the user to
    /// `inputs`.
    pub(crate) fn start(inputs: Mailbox<SettingsInput>) -> Result<Self> {
        let (started, start) = mpsc::channel();
        let thread = thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || run(inputs, &started))?;
        let proxy = start
            .recv()
            .map_err(|error| Error::new(E_UNEXPECTED, error.to_string()))??;
        Ok(Self {
            proxy,
            thread: Some(thread),
        })
    }

    /// Shows `view` in the settings window, when the window shows.
    pub(crate) fn render(&self, view: SettingsView) {
        self.dispatch(move |_| {
            with_host(|host| host.render(view));
        });
    }

    /// Shows the settings window with `view` and brings it to the front. Opens the window when it
    /// is closed.
    pub(crate) fn show(&self, view: SettingsView) {
        self.dispatch(move |context| open_or_show(view, context));
    }

    /// Shows the page icons for `contrast`, when the window shows. WinUI makes the rest of the
    /// window follow the contrast.
    pub(crate) fn set_contrast(&self, contrast: Contrast) {
        self.dispatch(move |_| {
            with_host(|host| host.send_to_page(PageMessage::Contrast(contrast)));
        });
    }

    fn dispatch(&self, work: impl FnOnce(&AppContext) + Send + 'static) {
        let dispatched = self.proxy.dispatch(move |context| {
            work(context);
            Ok(())
        });
        if let Err(error) = dispatched {
            tracing::warn!(%error, "failed to reach the settings window");
        }
    }
}

impl Drop for SettingsThread {
    fn drop(&mut self) {
        if let Err(error) = self.proxy.exit() {
            tracing::warn!(%error, "failed to end the settings window");
        }
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            tracing::warn!("the settings window thread panicked");
        }
    }
}

/// Runs the WinUI app of the thread: gives its dispatcher to `started`, then runs until the
/// controller ends it.
fn run(inputs: Mailbox<SettingsInput>, started: &mpsc::Sender<Result<AppProxy>>) {
    let startup_sender = started.clone();
    let ran = App::run_with(move |context| {
        HOST.with(|host| {
            host.replace(Some(Host {
                inputs,
                view: SettingsView { pages: Vec::new() },
                page: PageState::Closed,
            }))
        });
        // Fails only when the controller stopped to wait, which needs no handling.
        let _ = startup_sender.send(Ok(context.proxy()));
        Ok(())
    });
    if let Err(error) = ran {
        tracing::warn!(%error, "the settings window stopped");
        // Fails when the controller does not wait for the start any more, which needs no handling.
        let _ = started.send(Err(Error::new(HRESULT(error.code().0), error.message())));
    }
    HOST.with(RefCell::take);
}

impl Host {
    fn render(&mut self, view: SettingsView) {
        self.view = view.clone();
        self.send_to_page(PageMessage::Show(view));
    }

    /// Sends `message` to the page of the window, when the window shows.
    fn send_to_page(&self, message: PageMessage) {
        if let PageState::Open(page) = &self.page {
            send(page, message);
        }
    }
}

/// Shows `view` in the open window and brings it to the front, or opens the window with it. The
/// window opens outside any borrow of the host, because its page reads the host when it starts.
fn open_or_show(view: SettingsView, context: &AppContext) {
    let should_open = with_host(|host| {
        host.view = view.clone();
        match &host.page {
            PageState::Open(page) => {
                send(page, PageMessage::Show(view));
                send(page, PageMessage::Activate);
                false
            }
            PageState::Opening => false,
            PageState::Closed => {
                host.page = PageState::Opening;
                true
            }
        }
    });
    if should_open != Some(true) {
        return;
    }
    let policy = WindowPolicy::new()
        .title(WINDOW_TITLE)
        .client_size(WINDOW_WIDTH, WINDOW_HEIGHT)
        .minimum_client_size(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);
    if let Err(error) = context.open_component_window_with_policy::<SettingsPage>((), policy) {
        tracing::warn!(%error, "failed to open the settings window");
        with_host(|host| host.page = PageState::Closed);
    }
}

fn send(page: &LocalSender<PageMessage>, message: PageMessage) {
    if !page.send(message) {
        tracing::warn!("the settings page did not take a message");
    }
}

/// What the settings page reacts to.
#[derive(Clone)]
enum PageMessage {
    /// The controller sends a new view.
    Show(SettingsView),
    /// The controller asks to bring the window to the front.
    Activate,
    /// High contrast went on or off.
    Contrast(Contrast),
    /// The user picked the page with this tag in the navigation.
    Select(Option<Rc<str>>),
    /// The user changed the text of a field that submits its text.
    Typed { id: ControlId, text: String },
    /// The user submitted the text of a field, with the return key or with its button.
    Submit(ControlId),
    /// The user did something with a control.
    Input(SettingsInput),
    /// A control changed in a way that the controller does not need, such as a menu that lost
    /// its selection.
    Ignore,
}

/// The content of the settings window: a navigation item for each page of the last view, and the
/// page that the user selected.
struct SettingsPage {
    view: SettingsView,
    selected: usize,
    /// The text that the user typed in each field. A field shows it in place of the text of the
    /// view, because a view from the controller can come after newer keys. A view that sets its
    /// older text then moves the caret and removes the newer keys.
    typed: HashMap<ControlId, String>,
    /// The icon of each app of the view, by the path of its program, or `None` when the program
    /// has no icon.
    app_icons: HashMap<String, Option<EncodedImage>>,
    /// Whether high contrast is on, which gives the pages icons in the text color.
    contrast: Contrast,
}

impl SettingsPage {
    /// Shows `view`, and loads the icons of its apps that the page does not have yet.
    fn show(&mut self, view: SettingsView) {
        self.view = view;
        let app_paths = self
            .view
            .pages
            .iter()
            .flat_map(|page| &page.sections)
            .flat_map(|section| &section.rows)
            .filter_map(|row| match row {
                Row::AppSwitch(switch) => switch.app_path.as_deref(),
                Row::AppPicker(picker) => picker.app_path.as_deref(),
                _ => None,
            });
        for path in app_paths {
            if !self.app_icons.contains_key(path) {
                let icon = app_icon::app_icon_file(path).map(EncodedImage::new);
                self.app_icons.insert(path.to_owned(), icon);
            }
        }
    }
}

impl Component for SettingsPage {
    type Input = ();
    type Message = PageMessage;

    fn create(_input: &(), context: &ComponentContext<Self>) -> Self {
        let view = with_host(|host| {
            host.page = PageState::Open(context.sender());
            host.view.clone()
        });
        let mut page = Self {
            view: SettingsView { pages: Vec::new() },
            selected: 0,
            typed: HashMap::new(),
            app_icons: HashMap::new(),
            contrast: SystemTheme::read().contrast,
        };
        if let Some(view) = view {
            page.show(view);
        }
        page
    }

    fn update(&mut self, message: PageMessage, context: &ComponentContext<Self>) {
        match message {
            PageMessage::Show(view) => self.show(view),
            PageMessage::Activate => {
                if !context.activate_window() {
                    tracing::warn!("failed to bring the settings window to the front");
                }
            }
            PageMessage::Contrast(contrast) => self.contrast = contrast,
            PageMessage::Select(tag) => {
                if let Some(index) = tag.and_then(|tag| tag.parse().ok()) {
                    self.selected = index;
                }
            }
            PageMessage::Typed { id, text } => {
                self.typed.insert(id, text);
            }
            PageMessage::Submit(id) => {
                let text = self.typed.get(&id).cloned().unwrap_or_default();
                send_input(SettingsInput::Submitted { id, text });
            }
            PageMessage::Input(input) => {
                if let SettingsInput::Edited { id, text } = &input {
                    self.typed.insert(id.clone(), text.clone());
                }
                send_input(input);
            }
            PageMessage::Ignore => {}
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<'_, Self>) -> View {
        context.window_visuals(WindowVisuals::new().backdrop(WindowBackdrop::Mica));
        let selected = self.selected.min(self.view.pages.len().saturating_sub(1));
        let renderer = Renderer {
            context,
            typed: &self.typed,
            app_icons: &self.app_icons,
        };
        let items = self.view.pages.iter().enumerate().map(|(index, page)| {
            keyed(
                index,
                NavigationViewItem::new()
                    .tag(index.to_string())
                    .is_selected(index == selected)
                    .selects_on_invoked(true)
                    .icon(page_icon(page.icon, self.contrast))
                    .content(page.title),
            )
        });
        // The key of the page makes a new scroll viewer for each page, so each page starts at its top.
        let body = self
            .view
            .pages
            .get(selected)
            .map(|page| keyed(selected, renderer.page(selected, page)));
        let navigation = NavigationView::new()
            .pane_display_mode(NavigationViewPaneDisplayMode::Left)
            .open_pane_length(NAVIGATION_WIDTH)
            .is_pane_toggle_button_visible(false)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .is_settings_visible(false)
            .always_show_header(false)
            .on_selected_tag_changed(context.callback(PageMessage::Select))
            .keyed_menu_items(items)
            .content(Grid::new().keyed_children(body));
        context.window_frame(WINDOW_TITLE, navigation)
    }
}

impl Drop for SettingsPage {
    fn drop(&mut self) {
        with_host(|host| host.page = PageState::Closed);
    }
}

fn send_input(input: SettingsInput) {
    with_host(|host| host.inputs.send(input));
}
