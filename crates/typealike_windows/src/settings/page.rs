//! The controls of one settings page, built from the sections and rows of the core view.

use std::collections::HashMap;
use std::iter;
use std::rc::Rc;

use typealike_core::settings_window::view::{
    self as view, Access, Accessory, AppPicker, AppSwitch, ControlId, Item, Note, Output, Page, Permille,
    PermissionRow, Picker, Row, RowIcon, SearchField, Section, SettingsInput, SiteSwitch, Switch, TextArea,
    TextField, TextLine,
};
use windows_reactor::{
    AutoSuggestBox, Border, Button, ButtonStyle, Callback, ComboBox, Component, EncodedImage, FontWeight,
    Grid, GridLength, HorizontalAlignment, HyperlinkButton, Image, KeyEventInfo, KeyedView, Orientation,
    ProgressBar, ProgressRing, RoutedCallback, StackPanel, Symbol, SymbolIcon, TextBlock, TextBox,
    TextWrapping, Thickness, ToggleSwitch, VerticalAlignment, View, ViewContext, VirtualKey, keyed,
};

use super::RowMessage;
use super::icons::row_glyph;
use super::layout::{
    CONTROL_GAP, card, card_header, control_card, glyph_icon, leading_and_text, note_view, scrolling_page,
    secondary_text, stacked_card, two_column_card,
};
use super::text_area_box::{TextAreaBox, TextAreaBoxInput};
use super::typed_texts::TypedTexts;

const TITLE_SIZE: f64 = 28.0;
const SECTION_GAP: f64 = 28.0;
const HEADING_GAP: f64 = 8.0;
const CARD_GAP: f64 = 4.0;
const PICKER_MIN_WIDTH: f64 = 200.0;
const PICKER_MAX_WIDTH: f64 = 320.0;
const TEXT_LINE_MIN_WIDTH: f64 = 200.0;
const PROGRESS_WIDTH: f64 = 120.0;
const RING_SIZE: f64 = 16.0;
const APP_ICON_SIZE: f64 = 24.0;
// WinUI makes a switch 154 px wide, which leaves a gap after its "On" at the trailing edge of a card.
const SWITCH_MIN_WIDTH: f64 = 0.0;

/// Builds the controls of a page from its core sections and rows, in the window of the component
/// `Window`, which takes the message of each control.
pub(super) struct Renderer<'page, 'view, Window: Component> {
    pub(super) context: &'page ViewContext<'view, Window>,
    pub(super) typed: &'page TypedTexts,
    pub(super) app_icons: &'page HashMap<String, Option<EncodedImage>>,
}

impl<Window> Renderer<'_, '_, Window>
where
    Window: Component,
    Window::Message: From<RowMessage> + Clone,
{
    /// The page at `index` of the view, with its header, its search field and its sections.
    pub(super) fn page(&self, index: usize, page: &Page) -> View {
        let header = [
            keyed(
                format!("{index} title"),
                TextBlock::new()
                    .text(&page.title)
                    .font_size(TITLE_SIZE)
                    .font_weight(FontWeight::SEMI_BOLD),
            ),
            keyed(
                format!("{index} summary"),
                secondary_text(&page.summary).margin(Thickness::new(0.0, 0.0, 0.0, HEADING_GAP)),
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
            .placeholder_text(&field.placeholder)
            .query_icon(Symbol::Find)
            .automation_name(&field.placeholder)
            .on_text_changed(self.typed_callback(&field.id));
        self.submit_on_enter(&field.id, box_view.into())
    }

    /// The heading, the rows and the footer of `section`, each with a key that starts with `key`.
    pub(super) fn section(&self, key: &str, section: &Section) -> Vec<KeyedView> {
        let heading = (section.heading.is_some() || section.back.is_some()).then(|| {
            let back = section
                .back
                .as_ref()
                .map(|back| self.button(back, ButtonStyle::Subtle));
            let title = TextBlock::new()
                .text(section.heading.as_deref().unwrap_or_default())
                .font_weight(FontWeight::SEMI_BOLD)
                .vertical_alignment(VerticalAlignment::Center)
                .text_wrapping(TextWrapping::Wrap);
            keyed(
                format!("{key} heading"),
                leading_and_text(back, title, HEADING_GAP).margin(Thickness::new(
                    0.0,
                    SECTION_GAP,
                    0.0,
                    HEADING_GAP,
                )),
            )
        });
        let rows = section.rows.iter().enumerate().map(|(row_index, row)| {
            let id = row.id().map_or("", ControlId::as_str);
            keyed(format!("{key} {row_index} {id}"), self.row(row))
        });
        let footer = section.footer.as_ref().map(|footer| {
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
            Row::SiteSwitch(switch) => self.site_switch_card(switch),
            Row::Picker(picker) => self.picker_card(picker),
            Row::AppPicker(picker) => self.app_picker_card(picker),
            Row::TextArea(area) => self.text_area_card(area),
            Row::TextLine(line) => self.text_line_card(line),
            Row::TextField(field) => self.text_field_card(field),
            Row::Note(note) => card(note_view(note)),
            Row::Output(output) => output_card(output),
            Row::Item(item) => self.item_card(item),
            Row::Permission(permission) => self.permission_card(permission),
            Row::Loading(text) => loading_card(text),
            Row::Message(message) => self.message_card(message),
            Row::Link(button) => self.link(button),
        }
    }

    /// A quiet button that looks like a link, with no card, at the leading edge.
    fn link(&self, button: &view::Button) -> View {
        HyperlinkButton::new()
            .content(button.label.as_str())
            .is_enabled(button.is_enabled)
            .horizontal_alignment(HorizontalAlignment::Left)
            .on_click(
                self.context
                    .message(RowMessage::Input(SettingsInput::Pressed(button.id.clone())).into()),
            )
            .into()
    }

    fn switch_card(&self, switch: &Switch) -> View {
        let toggle = self.toggle_switch(&switch.id, &switch.label).is_on(switch.is_on);
        control_card(
            setting_header(
                switch.icon,
                &switch.label,
                &switch.description,
                switch.note.as_ref(),
            ),
            toggle.into(),
        )
    }

    /// A card with the icon and the name of an app at the leading edge and its switch at the
    /// trailing edge.
    fn app_switch_card(&self, switch: &AppSwitch) -> View {
        let toggle = self
            .toggle_switch(&switch.id, &switch.app_name)
            .is_on(switch.is_on)
            .is_enabled(switch.is_enabled);
        self.app_card(switch.app_path.as_deref(), &switch.app_name, toggle.into())
    }

    /// A card with a globe and the host of a website at the leading edge and its switch at the
    /// trailing edge.
    fn site_switch_card(&self, switch: &SiteSwitch) -> View {
        let toggle = self.toggle_switch(&switch.id, &switch.host).is_on(switch.is_on);
        let icon = SymbolIcon::new().symbol(Symbol::Globe).into();
        control_card(card_header(icon, &switch.host, "", None), toggle.into())
    }

    /// A card with the icon and the name of an app at the leading edge and its menu at the
    /// trailing edge.
    fn app_picker_card(&self, picker: &AppPicker) -> View {
        let menu = self
            .picker_menu(&picker.id, &picker.choices, picker.selected)
            .is_enabled(picker.is_enabled)
            .automation_name(&picker.app_name);
        self.app_card(picker.app_path.as_deref(), &picker.app_name, menu.into())
    }

    /// A card with the icon of the program at `app_path` and `app_name` at the leading edge, and
    /// `control` at the trailing edge.
    fn app_card(&self, app_path: Option<&str>, app_name: &str, control: View) -> View {
        let icon = app_path.and_then(|path| self.app_icons.get(path).cloned().flatten());
        let icon: View = match icon {
            Some(icon) => Image::new()
                .source_data(icon)
                .width(APP_ICON_SIZE)
                .height(APP_ICON_SIZE)
                .into(),
            None => SymbolIcon::new().symbol(Symbol::AllApps).into(),
        };
        control_card(card_header(icon, app_name, "", None), control)
    }

    fn picker_card(&self, picker: &Picker) -> View {
        let menu = self
            .picker_menu(&picker.id, &picker.choices, picker.selected)
            .is_enabled(picker.is_enabled)
            .automation_name(&picker.label);
        control_card(
            setting_header(
                picker.icon,
                &picker.label,
                &picker.description,
                picker.note.as_ref(),
            ),
            menu.into(),
        )
    }

    fn text_area_card(&self, area: &TextArea) -> View {
        let field = View::component::<TextAreaBox>(TextAreaBoxInput {
            text: self.typed_text(&area.id, &area.text).to_owned(),
            label: area.label.clone(),
            on_text_changed: self.edited_callback(&area.id),
        });
        let button = area
            .button
            .as_ref()
            .map(|button| self.button(button, ButtonStyle::Default));
        stacked_card(
            setting_header(area.icon, &area.label, &area.description, None),
            iter::once(field).chain(button).collect(),
        )
    }

    fn text_line_card(&self, line: &TextLine) -> View {
        let field = TextBox::new()
            .text(self.typed_text(&line.id, &line.text))
            .min_width(TEXT_LINE_MIN_WIDTH)
            .automation_name(&line.label)
            .on_text_changed(self.edited_callback(&line.id));
        control_card(
            setting_header(line.icon, &line.label, &line.description, None),
            field.into(),
        )
    }

    fn text_field_card(&self, field: &TextField) -> View {
        let text_box = TextBox::new()
            .text(self.typed_text(&field.id, ""))
            .placeholder_text(&field.placeholder)
            .automation_name(&field.label)
            .on_text_changed(self.typed_callback(&field.id));
        let input: View = if field.is_submit_enabled {
            self.submit_on_enter(&field.id, text_box.into())
        } else {
            text_box.into()
        };
        let submit = Button::new()
            .content(field.submit_label.as_str())
            .is_enabled(field.is_submit_enabled)
            .style(ButtonStyle::Accent)
            .on_click(self.context.message(RowMessage::Submit(field.id.clone()).into()));
        let line = Grid::new()
            .columns([GridLength::STAR, GridLength::Auto])
            .column_spacing(HEADING_GAP)
            .children((Border::new().grid_column(0).content(input), submit.grid_column(1)));
        stacked_card(
            setting_header(field.icon, &field.label, "", None),
            vec![line.into()],
        )
    }

    /// A card with the label and the reason of a permission, and at the trailing edge its state or
    /// the button that asks for it.
    fn permission_card(&self, permission: &PermissionRow) -> View {
        let state = match &permission.access {
            Access::Granted(text) => secondary_text(text)
                .vertical_alignment(VerticalAlignment::Center)
                .into(),
            Access::Missing(button) => self.button(button, ButtonStyle::Accent),
        };
        control_card(
            setting_header(permission.icon, &permission.label, &permission.description, None),
            state,
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
        let leading = StackPanel::new()
            .vertical_alignment(VerticalAlignment::Center)
            .children(texts)
            .grid_column(0);
        two_column_card(leading.into(), Some(trailing.into()))
    }

    fn accessory(&self, accessory: &Accessory) -> Option<View> {
        match accessory {
            Accessory::Action(button)
            | Accessory::Download(button)
            | Accessory::Open(button)
            | Accessory::Installed { delete: button }
            | Accessory::Failed { retry: button } => Some(self.button(button, ButtonStyle::Default)),
            Accessory::Downloading { progress, cancel } => Some(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(CONTROL_GAP)
                    .children((
                        ProgressBar::new()
                            .maximum(f64::from(Permille::FULL.0))
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
        let text = secondary_text(&message.text)
            .vertical_alignment(VerticalAlignment::Center)
            .grid_column(0);
        two_column_card(text.into(), button)
    }

    /// A menu of `choices`, at least as wide as a picker, that shows the choice at `selected` and
    /// sends each pick of the user for the control `id`.
    fn picker_menu(&self, id: &ControlId, choices: &[String], selected: usize) -> ComboBox {
        let id = id.clone();
        ComboBox::new()
            .items_source(choices.iter().map(String::as_str))
            .selected_index(selected)
            .min_width(PICKER_MIN_WIDTH)
            .max_width(PICKER_MAX_WIDTH)
            .on_selection_changed(self.context.callback(move |selection: Option<usize>| {
                selection
                    .map_or(RowMessage::Ignore, |index| {
                        RowMessage::Input(SettingsInput::Picked {
                            id: id.clone(),
                            index,
                        })
                    })
                    .into()
            }))
    }

    /// A push button with the label of `button` in `style`, which sends its press.
    pub(super) fn button(&self, button: &view::Button, style: ButtonStyle) -> View {
        let control = Button::new()
            .content(button.label.as_str())
            .is_enabled(button.is_enabled)
            .on_click(
                self.context
                    .message(RowMessage::Input(SettingsInput::Pressed(button.id.clone())).into()),
            );
        // Reactor gives `ButtonStyle::Default` a null style, which drops the Fluent style of WinUI.
        match style {
            ButtonStyle::Default => control,
            style => control.style(style),
        }
        .into()
    }

    /// Wraps `field` so that the return key in it submits the text of the field `id`.
    fn submit_on_enter(&self, id: &ControlId, field: View) -> View {
        let sender = self.context.sender();
        let id = id.clone();
        Border::new()
            .on_key_up(RoutedCallback::new(move |event: KeyEventInfo| {
                event.key == VirtualKey::ENTER && sender.send(RowMessage::Submit(id.clone()).into())
            }))
            .content(field)
            .into()
    }

    /// The text that the user typed in the field `id`, else `shown`, the text of the view.
    fn typed_text<'text>(&'text self, id: &ControlId, shown: &'text str) -> &'text str {
        self.typed.text(id, shown)
    }

    fn typed_callback(&self, id: &ControlId) -> Callback<Rc<str>> {
        let id = id.clone();
        self.context.callback(move |text: Rc<str>| {
            RowMessage::Typed {
                id: id.clone(),
                text: text.to_string(),
            }
            .into()
        })
    }

    fn edited_callback(&self, id: &ControlId) -> Callback<Rc<str>> {
        let id = id.clone();
        self.context.callback(move |text: Rc<str>| {
            RowMessage::Input(SettingsInput::Edited {
                id: id.clone(),
                text: text.to_string(),
            })
            .into()
        })
    }

    /// A switch at its own width for the control `id`, named `name` for assistive technology.
    fn toggle_switch(&self, id: &ControlId, name: &str) -> ToggleSwitch {
        ToggleSwitch::new()
            .min_width(SWITCH_MIN_WIDTH)
            .automation_name(name)
            .on_toggled(self.toggled_callback(id))
    }

    fn toggled_callback(&self, id: &ControlId) -> Callback<bool> {
        let id = id.clone();
        self.context.callback(move |is_on| {
            RowMessage::Input(SettingsInput::Toggled {
                id: id.clone(),
                is_on,
            })
            .into()
        })
    }
}

/// The header of the card of a setting: the glyph of `icon`, then the label, the description and
/// the note.
fn setting_header(icon: RowIcon, label: &str, description: &str, note: Option<&Note>) -> Grid {
    card_header(glyph_icon(row_glyph(icon)), label, description, note)
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
    let ring = ProgressRing::new()
        .is_active(true)
        .is_indeterminate(true)
        .width(RING_SIZE)
        .height(RING_SIZE)
        .vertical_alignment(VerticalAlignment::Center)
        .into();
    let text = secondary_text(text).vertical_alignment(VerticalAlignment::Center);
    card(
        leading_and_text(Some(ring), text, HEADING_GAP)
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
    )
}
