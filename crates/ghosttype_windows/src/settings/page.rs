//! The controls of one settings page, built from the sections and rows of the core view.

use std::collections::HashMap;
use std::rc::Rc;

use ghosttype_core::settings_view::{
    self as view, Accessory, AppSwitch, ControlId, Item, Note, NoteTone, Output, Page, Picker, Row,
    SearchField, Section, SettingsInput, Switch, TextArea, TextField, TextLine,
};
use windows_reactor::{
    AutoSuggestBox, Border, Button, ButtonStyle, Callback, ComboBox, Component, ComponentContext,
    EncodedImage, FontWeight, Grid, GridLength, Image, KeyEventInfo, KeyedView, Orientation, ProgressBar,
    ProgressRing, RoutedCallback, ScrollBarVisibility, ScrollViewer, StackPanel, Symbol, SymbolIcon,
    TextBlock, TextBox, TextWrapping, ThemeBrush, Thickness, ToggleSwitch, VerticalAlignment, View,
    ViewContext, VirtualKey, keyed,
};

use super::{PageMessage, SettingsPage};

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
const TEXT_LINE_MIN_WIDTH: f64 = 200.0;
const PROGRESS_WIDTH: f64 = 120.0;
const PERMILLE_MAX: f64 = 1000.0;
const RING_SIZE: f64 = 16.0;
const APP_ICON_SIZE: f64 = 24.0;

/// Builds the controls of a page from its core sections and rows.
pub(super) struct Renderer<'page, 'view> {
    pub(super) context: &'page ViewContext<'view, SettingsPage>,
    pub(super) typed: &'page HashMap<ControlId, String>,
    pub(super) app_icons: &'page HashMap<String, Option<EncodedImage>>,
}

impl Renderer<'_, '_> {
    /// The page at `index` of the view, with its header, its search field and its sections.
    pub(super) fn page(&self, index: usize, page: &Page) -> View {
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
            .keyed_children(header.into_iter().chain(search).chain(sections));
        scrolling_page(column.into())
    }

    fn search(&self, field: &SearchField) -> View {
        let box_view = AutoSuggestBox::new()
            .text(self.typed_text(&field.id, ""))
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
            Row::AppSwitch(switch) => self.app_switch_card(switch),
            Row::Picker(picker) => self.picker_card(picker),
            Row::TextArea(area) => self.text_area_card(area),
            Row::TextLine(line) => self.text_line_card(line),
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

    /// A card with the icon and the name of an app at the leading edge and its switch at the
    /// trailing edge.
    fn app_switch_card(&self, switch: &AppSwitch) -> View {
        let id = switch.id.clone();
        let toggle = ToggleSwitch::new()
            .is_on(switch.is_on)
            .is_enabled(switch.is_enabled)
            .automation_name(&switch.app_name)
            .on_toggled(self.context.callback(move |is_on| {
                PageMessage::Input(SettingsInput::Toggled {
                    id: id.clone(),
                    is_on,
                })
            }));
        let icon = switch
            .app_path
            .as_ref()
            .and_then(|path| self.app_icons.get(path).cloned().flatten());
        let icon: View = match icon {
            Some(icon) => Image::new()
                .source_data(icon)
                .width(APP_ICON_SIZE)
                .height(APP_ICON_SIZE)
                .into(),
            None => SymbolIcon::new().symbol(Symbol::AllApps).into(),
        };
        let name = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(CONTROL_GAP)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                Border::new()
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(icon),
                TextBlock::new()
                    .text(&switch.app_name)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text_wrapping(TextWrapping::Wrap),
            ));
        card(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(CONTROL_GAP)
                .children((name.grid_column(0), control_with_column(toggle.into())))
                .into(),
        )
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
        let field = View::component::<TextAreaBox>(TextAreaBoxInput {
            text: self.typed_text(&area.id, &area.text).to_owned(),
            label: area.label,
            on_text_changed: self.context.callback(move |text: Rc<str>| {
                PageMessage::Input(SettingsInput::Edited {
                    id: id.clone(),
                    text: text.to_string(),
                })
            }),
        });
        let button = area
            .button
            .as_ref()
            .map(|button| self.button(button, ButtonStyle::Default));
        let rows: Vec<View> = [
            Some(TextBlock::new().text(area.label).into()),
            (!area.description.is_empty()).then(|| secondary_text(area.description).into()),
            Some(field),
            button,
        ]
        .into_iter()
        .flatten()
        .collect();
        card(StackPanel::new().spacing(HEADING_GAP).children(rows).into())
    }

    fn text_line_card(&self, line: &TextLine) -> View {
        let id = line.id.clone();
        let field = TextBox::new()
            .text(self.typed_text(&line.id, &line.text))
            .min_width(TEXT_LINE_MIN_WIDTH)
            .automation_name(line.label)
            .on_text_changed(self.context.callback(move |text: Rc<str>| {
                PageMessage::Input(SettingsInput::Edited {
                    id: id.clone(),
                    text: text.to_string(),
                })
            }));
        control_card(line.label, line.description, None, field.into())
    }

    fn text_field_card(&self, field: &TextField) -> View {
        let text_box = TextBox::new()
            .text(self.typed_text(&field.id, ""))
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
        let button = message
            .button
            .as_ref()
            .map(|button| self.button(button, ButtonStyle::Default));
        let text = secondary_text(&message.text).vertical_alignment(VerticalAlignment::Center);
        card(
            Grid::new()
                .columns([GridLength::STAR, GridLength::Auto])
                .column_spacing(CONTROL_GAP)
                .children(
                    [Some(text.grid_column(0).into()), button.map(control_with_column)]
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

    /// The text that the user typed in the field `id`, else `shown`, the text of the view.
    fn typed_text<'text>(&'text self, id: &ControlId, shown: &'text str) -> &'text str {
        self.typed.get(id).map_or(shown, String::as_str)
    }

    fn typed_callback(&self, id: &ControlId) -> Callback<Rc<str>> {
        let id = id.clone();
        self.context.callback(move |text: Rc<str>| PageMessage::Typed {
            id: id.clone(),
            text: text.to_string(),
        })
    }
}

/// The multi-line text box of a text area. It gets its text one render after it exists, because
/// reactor sets the text of a new `TextBox` before `AcceptsReturn`, and a single-line WinUI text
/// box keeps only the first line of the text that it gets.
struct TextAreaBox {
    shows_text: bool,
}

#[derive(Clone, PartialEq)]
struct TextAreaBoxInput {
    text: String,
    label: &'static str,
    on_text_changed: Callback<Rc<str>>,
}

impl Component for TextAreaBox {
    type Input = TextAreaBoxInput;
    type Message = ();

    fn create(_input: &TextAreaBoxInput, context: &ComponentContext<Self>) -> Self {
        if !context.sender().send(()) {
            tracing::warn!("the text area did not take a message");
        }
        Self { shows_text: false }
    }

    fn update(&mut self, (): (), _context: &ComponentContext<Self>) {
        self.shows_text = true;
    }

    fn view(&self, input: &TextAreaBoxInput, _context: &mut ViewContext<'_, Self>) -> View {
        let field = TextBox::new()
            .accepts_return(true)
            .text_wrapping(TextWrapping::Wrap)
            .min_height(TEXT_AREA_MIN_HEIGHT)
            .automation_name(input.label)
            .on_text_changed(input.on_text_changed.clone());
        if self.shows_text {
            field.text(&input.text).into()
        } else {
            field.into()
        }
    }
}

/// Puts `column` at the leading edge of a page that scrolls up and down, as wide as the page up to
/// [`PAGE_MAX_WIDTH`]. The width limit is on a grid column, because WinUI places the direct child
/// of a `ScrollViewer` past its right edge when that child has a `MaxWidth` smaller than the page.
fn scrolling_page(column: View) -> View {
    let limited = Grid::new()
        .columns([GridLength::STAR.max(PAGE_MAX_WIDTH)])
        .children((column,));
    ScrollViewer::new()
        .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
        .content(Border::new().padding(PAGE_MARGIN).content(limited))
        .into()
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
