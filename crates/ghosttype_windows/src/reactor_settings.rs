//! A prototype of the settings window in WinUI 3, through windows-reactor, on a thread of its
//! own. It shows the first page of the core settings view. This module is also the one place
//! reactor callbacks on that thread reach the window's state.

use std::cell::RefCell;
use std::fmt;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use ghosttype_core::settings_view::{
    Note, NoteTone, Page, Picker, Row, Section, SettingsInput, SettingsView, Switch, TextArea,
};
use windows::Win32::Foundation::E_UNEXPECTED;
use windows::core::{Error, HRESULT, Result};
use windows_reactor::{
    App, AppContext, AppProxy, Border, Button, ComboBox, Component, ComponentContext, FontWeight, Grid,
    GridLength, KeyedView, LocalSender, ScrollViewer, StackPanel, TextBlock, TextBox, TextWrapping,
    ThemeBrush, Thickness, ToggleSwitch, VerticalAlignment, View, ViewContext, WindowBackdrop, WindowPolicy,
    WindowVisuals, keyed,
};

use crate::mailbox::Mailbox;

const THREAD_NAME: &str = "settings window";
const WINDOW_TITLE: &str = "GhostType";
const WINDOW_WIDTH: f64 = 1000.0;
const WINDOW_HEIGHT: f64 = 720.0;
const MIN_WINDOW_WIDTH: f64 = 640.0;
const MIN_WINDOW_HEIGHT: f64 = 480.0;
const PAGE_MARGIN: Thickness = Thickness::new(56.0, 32.0, 56.0, 32.0);
const PAGE_MAX_WIDTH: f64 = 1000.0;
const TITLE_SIZE: f64 = 28.0;
const CAPTION_SIZE: f64 = 12.0;
const SECONDARY_OPACITY: f64 = 0.75;
const SECTION_GAP: f64 = 28.0;
const HEADING_GAP: f64 = 8.0;
const CARD_GAP: f64 = 4.0;
const CARD_PADDING: Thickness = Thickness::xy(16.0, 12.0);
const CARD_MIN_HEIGHT: f64 = 68.0;
const CARD_RADIUS: f64 = 4.0;
const CARD_STROKE: f64 = 1.0;
const CONTROL_GAP: f64 = 16.0;
const PICKER_MIN_WIDTH: f64 = 200.0;
const TEXT_AREA_MIN_HEIGHT: f64 = 160.0;

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
        if let PageState::Open(page) = &self.page {
            send(page, PageMessage::Show(view));
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
    /// The user did something with a control.
    Input(SettingsInput),
    /// A control changed in a way that the controller does not need, such as a menu that lost
    /// its selection.
    Ignore,
}

/// The content of the settings window: the first page of the last view.
struct SettingsPage {
    view: SettingsView,
}

impl Component for SettingsPage {
    type Input = ();
    type Message = PageMessage;

    fn create(_input: &(), context: &ComponentContext<Self>) -> Self {
        let view = with_host(|host| {
            host.page = PageState::Open(context.sender());
            host.view.clone()
        });
        Self {
            view: view.unwrap_or(SettingsView { pages: Vec::new() }),
        }
    }

    fn update(&mut self, message: PageMessage, context: &ComponentContext<Self>) {
        match message {
            PageMessage::Show(view) => self.view = view,
            PageMessage::Activate => {
                if !context.activate_window() {
                    tracing::warn!("failed to bring the settings window to the front");
                }
            }
            PageMessage::Input(input) => {
                with_host(|host| host.inputs.send(input));
            }
            PageMessage::Ignore => {}
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<'_, Self>) -> View {
        context.window_visuals(WindowVisuals::new().backdrop(WindowBackdrop::Mica));
        let body = self
            .view
            .pages
            .first()
            .map_or_else(|| StackPanel::new().into(), |page| page_view(page, context));
        context.window_frame(WINDOW_TITLE, body)
    }
}

impl Drop for SettingsPage {
    fn drop(&mut self) {
        with_host(|host| host.page = PageState::Closed);
    }
}

fn page_view(page: &Page, context: &ViewContext<'_, SettingsPage>) -> View {
    let header = [
        keyed(
            "title",
            TextBlock::new()
                .text(page.title)
                .font_size(TITLE_SIZE)
                .font_weight(FontWeight::SEMI_BOLD),
        ),
        keyed(
            "summary",
            secondary_text(page.summary).margin(Thickness::new(0.0, 0.0, 0.0, HEADING_GAP)),
        ),
    ];
    let sections = page
        .sections
        .iter()
        .enumerate()
        .flat_map(|(index, section)| section_views(index, section, context));
    let column = StackPanel::new()
        .spacing(CARD_GAP)
        .max_width(PAGE_MAX_WIDTH)
        .margin(PAGE_MARGIN)
        .keyed_children(header.into_iter().chain(sections));
    ScrollViewer::new().content(column).into()
}

fn section_views(index: usize, section: &Section, context: &ViewContext<'_, SettingsPage>) -> Vec<KeyedView> {
    let heading = section.heading.as_ref().map(|heading| {
        keyed(
            format!("section {index}"),
            TextBlock::new()
                .text(heading)
                .font_weight(FontWeight::SEMI_BOLD)
                .margin(Thickness::new(0.0, SECTION_GAP, 0.0, HEADING_GAP)),
        )
    });
    let rows = section.rows.iter().enumerate().map(|(row_index, row)| {
        let key = row.id().map_or_else(
            || format!("section {index} row {row_index}"),
            |id| id.as_str().to_owned(),
        );
        keyed(key, row_view(row, context))
    });
    heading.into_iter().chain(rows).collect()
}

fn row_view(row: &Row, context: &ViewContext<'_, SettingsPage>) -> View {
    match row {
        Row::Switch(switch) => switch_card(switch, context),
        Row::Picker(picker) => picker_card(picker, context),
        Row::TextArea(area) => text_area_card(area, context),
        Row::Note(note) => card(note_text(note).into()),
        Row::Loading(text) => card(secondary_text(text).into()),
        Row::Message(message) => card(secondary_text(&message.text).into()),
        Row::TextField(_) | Row::Output(_) | Row::Item(_) => {
            card(secondary_text("The prototype does not show this row yet.").into())
        }
    }
}

fn card(content: View) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(CARD_STROKE)
        .corner_radius(CARD_RADIUS)
        .padding(CARD_PADDING)
        .min_height(CARD_MIN_HEIGHT)
        .content(content)
        .into()
}

/// A card with the label and the description at the leading edge and `control` at the trailing
/// edge.
fn control_card(label: &str, description: &str, note: Option<&Note>, control: View) -> View {
    let texts = StackPanel::new()
        .vertical_alignment(VerticalAlignment::Center)
        .children(
            [
                Some(
                    TextBlock::new()
                        .text(label)
                        .text_wrapping(TextWrapping::Wrap)
                        .into(),
                ),
                (!description.is_empty()).then(|| secondary_text(description).into()),
                note.map(|note| note_text(note).into()),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<View>>(),
        );
    card(
        Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(CONTROL_GAP)
            .children((texts.grid_column(0), control_with_column(control)))
            .into(),
    )
}

fn control_with_column(control: View) -> View {
    Border::new()
        .grid_column(1)
        .vertical_alignment(VerticalAlignment::Center)
        .content(control)
        .into()
}

fn switch_card(switch: &Switch, context: &ViewContext<'_, SettingsPage>) -> View {
    let id = switch.id.clone();
    let toggle = ToggleSwitch::new()
        .is_on(switch.is_on)
        .automation_name(switch.label)
        .on_toggled(context.callback(move |is_on| {
            PageMessage::Input(SettingsInput::Toggled {
                id: id.clone(),
                is_on,
            })
        }));
    control_card(switch.label, switch.description, None, toggle.into())
}

fn picker_card(picker: &Picker, context: &ViewContext<'_, SettingsPage>) -> View {
    let id = picker.id.clone();
    let menu = ComboBox::new()
        .items_source(picker.choices.iter().map(String::as_str))
        .selected_index(picker.selected)
        .is_enabled(picker.is_enabled)
        .min_width(PICKER_MIN_WIDTH)
        .automation_name(picker.label)
        .on_selection_changed(context.callback(move |selection: Option<usize>| match selection {
            Some(index) => PageMessage::Input(SettingsInput::Picked {
                id: id.clone(),
                index,
            }),
            None => PageMessage::Ignore,
        }));
    control_card(
        picker.label,
        picker.description,
        picker.note.as_ref(),
        menu.into(),
    )
}

fn text_area_card(area: &TextArea, context: &ViewContext<'_, SettingsPage>) -> View {
    let id = area.id.clone();
    let field = TextBox::new()
        .text(&area.text)
        .accepts_return(true)
        .text_wrapping(TextWrapping::Wrap)
        .min_height(TEXT_AREA_MIN_HEIGHT)
        .automation_name(area.label)
        .on_text_changed(context.callback(move |text: std::rc::Rc<str>| {
            PageMessage::Input(SettingsInput::Edited {
                id: id.clone(),
                text: text.to_string(),
            })
        }));
    let button = area.button.as_ref().map(|button| {
        Button::new()
            .content(button.label)
            .is_enabled(button.is_enabled)
            .on_click(context.message(PageMessage::Input(SettingsInput::Pressed(button.id.clone()))))
            .into()
    });
    let rows: Vec<View> = [
        Some(TextBlock::new().text(area.label).into()),
        (!area.description.is_empty()).then(|| secondary_text(area.description).into()),
        Some(field.into()),
        button,
    ]
    .into_iter()
    .flatten()
    .collect();
    card(StackPanel::new().spacing(HEADING_GAP).children(rows).into())
}

fn secondary_text(text: &str) -> TextBlock {
    TextBlock::new()
        .text(text)
        .font_size(CAPTION_SIZE)
        .opacity(SECONDARY_OPACITY)
        .text_wrapping(TextWrapping::Wrap)
}

fn note_text(note: &Note) -> TextBlock {
    let text = secondary_text(note.text);
    match note.tone {
        NoteTone::Plain | NoteTone::Busy => text,
        NoteTone::Error => text.foreground(ThemeBrush::SystemCritical).opacity(1.0),
    }
}
