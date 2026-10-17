//! A prototype of the settings window in WinUI 3, through windows-reactor, on a thread of its
//! own. It shows each page of the core settings view, with a navigation item for each page. This
//! module is also the one place reactor callbacks on that thread reach the window's state.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use ghosttype_core::settings_view::{
    self as view, Accessory, ControlId, Item, Note, NoteTone, Output, Page, PageIcon, Picker, Row,
    SearchField, Section, SettingsInput, SettingsView, Switch, TextArea, TextField,
};
use windows::Win32::Foundation::E_UNEXPECTED;
use windows::core::{Error, HRESULT, Result};
use windows_reactor::{
    App, AppContext, AppProxy, AutoSuggestBox, Border, Button, ButtonStyle, Callback, ComboBox, Component,
    ComponentContext, FontWeight, Grid, GridLength, KeyEventInfo, KeyedView, LocalSender, NavigationView,
    NavigationViewBackButtonVisible, NavigationViewItem, NavigationViewPaneDisplayMode, Orientation,
    ProgressBar, ProgressRing, RoutedCallback, ScrollViewer, StackPanel, Symbol, TextBlock, TextBox,
    TextWrapping, ThemeBrush, Thickness, ToggleSwitch, VerticalAlignment, View, ViewContext, VirtualKey,
    WindowBackdrop, WindowPolicy, WindowVisuals, keyed,
};

use crate::mailbox::Mailbox;

const THREAD_NAME: &str = "settings window";
const WINDOW_TITLE: &str = "GhostType";
const WINDOW_WIDTH: f64 = 1000.0;
const WINDOW_HEIGHT: f64 = 720.0;
const MIN_WINDOW_WIDTH: f64 = 640.0;
const MIN_WINDOW_HEIGHT: f64 = 480.0;
const NAVIGATION_WIDTH: f64 = 280.0;
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
const PROGRESS_WIDTH: f64 = 120.0;
const PERMILLE_MAX: f64 = 1000.0;
const RING_SIZE: f64 = 16.0;

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
    /// The text in each field that submits its text. The view does not hold it.
    typed: HashMap<ControlId, String>,
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
            selected: 0,
            typed: HashMap::new(),
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
            PageMessage::Input(input) => send_input(input),
            PageMessage::Ignore => {}
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<'_, Self>) -> View {
        context.window_visuals(WindowVisuals::new().backdrop(WindowBackdrop::Mica));
        let selected = self.selected.min(self.view.pages.len().saturating_sub(1));
        let renderer = Renderer {
            context,
            typed: &self.typed,
        };
        let items = self.view.pages.iter().enumerate().map(|(index, page)| {
            keyed(
                index,
                NavigationViewItem::new()
                    .tag(index.to_string())
                    .is_selected(index == selected)
                    .selects_on_invoked(true)
                    .icon(page_symbol(page.icon))
                    .content(page.title),
            )
        });
        let body = self
            .view
            .pages
            .get(selected)
            .map_or_else(|| StackPanel::new().into(), |page| renderer.page(selected, page));
        let navigation = NavigationView::new()
            .pane_display_mode(NavigationViewPaneDisplayMode::Left)
            .open_pane_length(NAVIGATION_WIDTH)
            .is_pane_toggle_button_visible(false)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .is_settings_visible(false)
            .always_show_header(false)
            .on_selected_tag_changed(context.callback(PageMessage::Select))
            .keyed_menu_items(items)
            .content(body);
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

fn page_symbol(icon: PageIcon) -> Symbol {
    match icon {
        PageIcon::Gear => Symbol::Setting,
        PageIcon::Cube => Symbol::Download,
        PageIcon::TextCursor => Symbol::Edit,
    }
}

/// Builds the controls of a page from its core sections and rows.
struct Renderer<'page, 'view> {
    context: &'page ViewContext<'view, SettingsPage>,
    typed: &'page HashMap<ControlId, String>,
}

impl Renderer<'_, '_> {
    fn page(&self, index: usize, page: &Page) -> View {
        let header = [
            keyed(
                format!("{index} title"),
                TextBlock::new()
                    .text(page.title)
                    .font_size(TITLE_SIZE)
                    .font_weight(FontWeight::SEMI_BOLD),
            ),
            keyed(
                format!("{index} summary"),
                secondary_text(page.summary).margin(Thickness::new(0.0, 0.0, 0.0, HEADING_GAP)),
            ),
        ];
        let search = page
            .search
            .as_ref()
            .map(|field| keyed(format!("{index} search"), self.search(field)));
        let sections = page
            .sections
            .iter()
            .enumerate()
            .flat_map(|(section_index, section)| self.section(&format!("{index} {section_index}"), section));
        let column = StackPanel::new()
            .spacing(CARD_GAP)
            .max_width(PAGE_MAX_WIDTH)
            .margin(PAGE_MARGIN)
            .keyed_children(header.into_iter().chain(search).chain(sections));
        ScrollViewer::new().content(column).into()
    }

    fn search(&self, field: &SearchField) -> View {
        let box_view = AutoSuggestBox::new()
            .text(self.typed_text(&field.id))
            .placeholder_text(field.placeholder)
            .query_icon(Symbol::Find)
            .automation_name(field.placeholder)
            .on_text_changed(self.typed_callback(&field.id));
        self.submit_on_enter(&field.id, box_view.into())
    }

    fn section(&self, key: &str, section: &Section) -> Vec<KeyedView> {
        let heading = (section.heading.is_some() || section.back.is_some()).then(|| {
            let back = section
                .back
                .as_ref()
                .map(|back| self.button(back, ButtonStyle::Subtle));
            let title = section.heading.as_ref().map(|heading| {
                TextBlock::new()
                    .text(heading)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text_wrapping(TextWrapping::Wrap)
                    .into()
            });
            keyed(
                format!("{key} heading"),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(HEADING_GAP)
                    .margin(Thickness::new(0.0, SECTION_GAP, 0.0, HEADING_GAP))
                    .children(back.into_iter().chain(title).collect::<Vec<View>>()),
            )
        });
        let rows = section.rows.iter().enumerate().map(|(row_index, row)| {
            let id = row.id().map_or("", ControlId::as_str);
            keyed(format!("{key} {row_index} {id}"), self.row(row))
        });
        let footer = section.footer.map(|footer| {
            keyed(
                format!("{key} footer"),
                secondary_text(footer).margin(Thickness::new(0.0, CARD_GAP, 0.0, 0.0)),
            )
        });
        heading.into_iter().chain(rows).chain(footer).collect()
    }

    fn row(&self, row: &Row) -> View {
        match row {
            Row::Switch(switch) => self.switch_card(switch),
            Row::Picker(picker) => self.picker_card(picker),
            Row::TextArea(area) => self.text_area_card(area),
            Row::TextField(field) => self.text_field_card(field),
            Row::Note(note) => card(note_text(note).into()),
            Row::Output(output) => output_card(output),
            Row::Item(item) => self.item_card(item),
            Row::Loading(text) => loading_card(text),
            Row::Message(message) => self.message_card(message),
        }
    }

    fn switch_card(&self, switch: &Switch) -> View {
        let id = switch.id.clone();
        let toggle = ToggleSwitch::new()
            .is_on(switch.is_on)
            .automation_name(switch.label)
            .on_toggled(self.context.callback(move |is_on| {
                PageMessage::Input(SettingsInput::Toggled {
                    id: id.clone(),
                    is_on,
                })
            }));
        control_card(switch.label, switch.description, None, toggle.into())
    }

    fn picker_card(&self, picker: &Picker) -> View {
        let id = picker.id.clone();
        let menu = ComboBox::new()
            .items_source(picker.choices.iter().map(String::as_str))
            .selected_index(picker.selected)
            .is_enabled(picker.is_enabled)
            .min_width(PICKER_MIN_WIDTH)
            .automation_name(picker.label)
            .on_selection_changed(self.context.callback(move |selection: Option<usize>| {
                selection.map_or(PageMessage::Ignore, |index| {
                    PageMessage::Input(SettingsInput::Picked {
                        id: id.clone(),
                        index,
                    })
                })
            }));
        control_card(
            picker.label,
            picker.description,
            picker.note.as_ref(),
            menu.into(),
        )
    }

    fn text_area_card(&self, area: &TextArea) -> View {
        let id = area.id.clone();
        let field = TextBox::new()
            .text(&area.text)
            .accepts_return(true)
            .text_wrapping(TextWrapping::Wrap)
            .min_height(TEXT_AREA_MIN_HEIGHT)
            .automation_name(area.label)
            .on_text_changed(self.context.callback(move |text: Rc<str>| {
                PageMessage::Input(SettingsInput::Edited {
                    id: id.clone(),
                    text: text.to_string(),
                })
            }));
        let button = area
            .button
            .as_ref()
            .map(|button| self.button(button, ButtonStyle::Default));
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

    fn text_field_card(&self, field: &TextField) -> View {
        let text_box = TextBox::new()
            .text(self.typed_text(&field.id))
            .placeholder_text(field.placeholder)
            .automation_name(field.label)
            .on_text_changed(self.typed_callback(&field.id));
        let input: View = if field.is_submit_enabled {
            self.submit_on_enter(&field.id, text_box.into())
        } else {
            text_box.into()
        };
        let submit = Button::new()
            .content(field.submit_label)
            .is_enabled(field.is_submit_enabled)
            .style(ButtonStyle::Accent)
            .on_click(self.context.message(PageMessage::Submit(field.id.clone())));
        let line = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(HEADING_GAP)
            .children((Border::new().grid_column(0).content(input), submit.grid_column(1)));
        card(
            StackPanel::new()
                .spacing(HEADING_GAP)
                .children((TextBlock::new().text(field.label), line))
                .into(),
        )
    }

    fn item_card(&self, item: &Item) -> View {
        let texts: Vec<View> = [
            Some(
                TextBlock::new()
                    .text(&item.title)
                    .text_wrapping(TextWrapping::Wrap)
                    .into(),
            ),
            (!item.subtitle.is_empty()).then(|| secondary_text(&item.subtitle).into()),
        ]
        .into_iter()
        .flatten()
        .collect();
        let trailing: Vec<View> = [
            (!item.detail.is_empty()).then(|| {
                secondary_text(&item.detail)
                    .vertical_alignment(VerticalAlignment::Center)
                    .into()
            }),
            self.accessory(&item.accessory),
        ]
        .into_iter()
        .flatten()
        .collect();
        let trailing = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(CONTROL_GAP)
            .children(trailing);
        card(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(CONTROL_GAP)
                .children((
                    StackPanel::new()
                        .vertical_alignment(VerticalAlignment::Center)
                        .children(texts)
                        .grid_column(0),
                    control_with_column(trailing.into()),
                ))
                .into(),
        )
    }

    fn accessory(&self, accessory: &Accessory) -> Option<View> {
        match accessory {
            Accessory::Download(button)
            | Accessory::Open(button)
            | Accessory::Installed { delete: button }
            | Accessory::Failed { retry: button } => Some(self.button(button, ButtonStyle::Default)),
            Accessory::Downloading { progress, cancel } => Some(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(CONTROL_GAP)
                    .children((
                        ProgressBar::new()
                            .maximum(PERMILLE_MAX)
                            .value(f64::from(progress.0))
                            .width(PROGRESS_WIDTH)
                            .vertical_alignment(VerticalAlignment::Center),
                        self.button(cancel, ButtonStyle::Default),
                    ))
                    .into(),
            ),
            Accessory::Canceling => Some(
                ProgressBar::new()
                    .is_indeterminate(true)
                    .width(PROGRESS_WIDTH)
                    .vertical_alignment(VerticalAlignment::Center)
                    .into(),
            ),
            Accessory::Unavailable => None,
        }
    }

    fn message_card(&self, message: &view::Message) -> View {
        let retry = message
            .retry
            .as_ref()
            .map(|retry| self.button(retry, ButtonStyle::Default));
        let text = secondary_text(&message.text).vertical_alignment(VerticalAlignment::Center);
        card(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(CONTROL_GAP)
                .children(
                    [Some(text.grid_column(0).into()), retry.map(control_with_column)]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<View>>(),
                )
                .into(),
        )
    }

    fn button(&self, button: &view::Button, style: ButtonStyle) -> View {
        Button::new()
            .content(button.label)
            .is_enabled(button.is_enabled)
            .style(style)
            .on_click(
                self.context
                    .message(PageMessage::Input(SettingsInput::Pressed(button.id.clone()))),
            )
            .into()
    }

    /// Wraps `field` so that the return key in it submits the text of the field `id`.
    fn submit_on_enter(&self, id: &ControlId, field: View) -> View {
        let sender = self.context.sender();
        let id = id.clone();
        Border::new()
            .on_key_up(RoutedCallback::new(move |event: KeyEventInfo| {
                event.key == VirtualKey::ENTER && sender.send(PageMessage::Submit(id.clone()))
            }))
            .content(field)
            .into()
    }

    fn typed_text(&self, id: &ControlId) -> &str {
        self.typed.get(id).map_or("", String::as_str)
    }

    fn typed_callback(&self, id: &ControlId) -> Callback<Rc<str>> {
        let id = id.clone();
        self.context.callback(move |text: Rc<str>| PageMessage::Typed {
            id: id.clone(),
            text: text.to_string(),
        })
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

fn output_card(output: &Output) -> View {
    card(
        StackPanel::new()
            .spacing(CARD_GAP)
            .children((
                TextBlock::new()
                    .text(&output.text)
                    .text_wrapping(TextWrapping::Wrap)
                    .is_text_selection_enabled(true),
                secondary_text(&output.detail),
            ))
            .into(),
    )
}

fn loading_card(text: &str) -> View {
    card(
        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(HEADING_GAP)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                ProgressRing::new()
                    .is_active(true)
                    .is_indeterminate(true)
                    .width(RING_SIZE)
                    .height(RING_SIZE),
                secondary_text(text).vertical_alignment(VerticalAlignment::Center),
            ))
            .into(),
    )
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
