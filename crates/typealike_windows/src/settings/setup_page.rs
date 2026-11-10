//! The content of the setup window: the picture, the title and the body of the step, the demo of
//! a suggestion, the sections of the step as on a settings page, a footnote, and a
//! footer with the progress and the buttons. A new step slides in and the demo types its text,
//! unless the animation effects of Windows are off. The window has a fixed size in the center of the
//! main monitor, so the content of each step fits in it.

use std::collections::HashMap;
use std::time::Duration;

use typealike_core::settings_window::view::SettingsInput;
use typealike_core::setup::{Demo, SetupView};
use windows_reactor::{
    AutomationHeadingLevel, Border, ButtonStyle, Component, ComponentContext, ComponentTimer, EncodedImage,
    FontIcon, FontWeight, Grid, GridLength, HorizontalAlignment, Image, KeyedView, ScrollBarVisibility,
    ScrollViewer, StackPanel, TextBlock, TextWrapping, ThemeBrush, Thickness, VerticalAlignment, View,
    ViewContext, WindowBackdrop, WindowVisuals, keyed,
};

use super::fixed_window::{Size, WindowPlace, fix_size};
use super::icons::{SetupPicture, setup_picture};
use super::layout::secondary_text;
use super::page::Renderer;
use super::typed_texts::TypedTexts;
use super::window_icon::show_app_icon;
use super::{RowMessage, on_row_message, setup_dropped, setup_opened};
use crate::system_theme::{Contrast, SystemTheme, are_animations_on};

const WINDOW_SIZE: Size = Size {
    width: 720,
    height: 720,
};
const PICTURE_SIZE: f64 = 56.0;
const GLYPH_SIZE: f64 = 48.0;
const TITLE_SIZE: f64 = 28.0;
const BODY_SIZE: f64 = 14.0;
const BODY_OPACITY: f64 = 0.8;
const CONTENT_MAX_WIDTH: f64 = 560.0;
const CONTENT_MARGIN: Thickness = Thickness::new(40.0, 32.0, 40.0, 32.0);
const TITLE_MARGIN: Thickness = Thickness::new(0.0, 20.0, 0.0, 8.0);
const BLOCK_MARGIN: Thickness = Thickness::new(0.0, 16.0, 0.0, 0.0);
const SECTION_MARGIN: Thickness = Thickness::new(0.0, 12.0, 0.0, 0.0);
const CARD_GAP: f64 = 4.0;
const DEMO_PADDING: Thickness = Thickness::xy(20.0, 16.0);
const DEMO_RADIUS: f64 = 8.0;
const DEMO_TEXT_SIZE: f64 = 16.0;
const GHOST_OPACITY: f64 = 0.5;
const KEY_PADDING: Thickness = Thickness::xy(10.0, 3.0);
const KEY_RADIUS: f64 = 4.0;
const KEY_STROKE: f64 = 1.0;
const PRESSED_KEY_STROKE: f64 = 2.0;
const KEY_TEXT_SIZE: f64 = 12.0;
const STROKE: f64 = 1.0;
const DEMO_COLUMN_GAP: f64 = 12.0;
const FOOTER_PADDING: Thickness = Thickness::xy(24.0, 16.0);
const FOOTER_GAP: f64 = 8.0;
const DOT_SIZE: f64 = 6.0;
const DOT_GAP: f64 = 10.0;
const PILL_WIDTH: f64 = 18.0;
const DOT_OPACITY: f64 = 0.35;
const SLIDE_DISTANCE: f64 = 40.0;
const SLIDE_TIME: Duration = Duration::from_millis(300);
const FADE_TIME: Duration = Duration::from_millis(200);
/// The wait before a new step slides in. The step must exist on the screen first, because WinUI
/// animates only a change of a shown element.
const ENTER_DELAY: Duration = Duration::from_millis(40);
const DEMO_START_WAIT: Duration = Duration::from_millis(600);
const LETTER_WAIT: Duration = Duration::from_millis(35);
const TYPED_WAIT: Duration = Duration::from_millis(400);
const SUGGESTED_WAIT: Duration = Duration::from_millis(1000);
const PRESSED_WAIT: Duration = Duration::from_millis(200);
/// The time after a change of the step in which a press of a button does not count, so that a
/// double click on the next button goes one step and does not skip a step.
const TURN_TIME: Duration = Duration::from_millis(350);

/// What the setup page reacts to.
#[derive(Clone)]
pub(super) enum SetupMessage {
    /// The controller sends a new view.
    Show(Box<SetupView>),
    /// The controller asks to bring the window to the front.
    Activate,
    /// The controller closes the window.
    Close,
    /// High contrast went on or off.
    Contrast(Contrast),
    /// A control of a row did something.
    Row(RowMessage),
    /// The new step slides in.
    Enter,
    /// The demo shows its next frame.
    DemoTick,
    /// The time of a change of the step is over, and the buttons count again.
    Turned,
    /// The window got its fixed size, or the change of its frame failed.
    Fixed(windows::core::Result<()>),
    /// The window got the app icon, or the load of the icon failed.
    IconShown(windows::core::Result<()>),
}

impl From<RowMessage> for SetupMessage {
    fn from(message: RowMessage) -> Self {
        Self::Row(message)
    }
}

/// Whether the setup window animates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Motion {
    Animated,
    Still,
}

impl Motion {
    fn of_system() -> Self {
        if are_animations_on() {
            Self::Animated
        } else {
            Self::Still
        }
    }
}

/// Where the step that shows comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entrance {
    /// The next step, which slides in from the trailing edge.
    FromEnd,
    /// The step before, which slides in from the leading edge.
    FromStart,
    /// The step is in its place.
    Entered,
}

/// The entrance of the step of `new` after the step of `old`, or `None` for the same step.
fn entrance(old: Option<&SetupView>, new: &SetupView, motion: Motion) -> Option<Entrance> {
    match old {
        Some(old) if old.step == new.step => None,
        Some(old) if motion == Motion::Animated && new.progress.number > old.progress.number => {
            Some(Entrance::FromEnd)
        }
        Some(_) if motion == Motion::Animated => Some(Entrance::FromStart),
        Some(_) | None => Some(Entrance::Entered),
    }
}

/// A frame of the demo of a suggestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DemoFrame {
    /// Before the first letter.
    Waiting,
    /// This many letters of the typed text show.
    Typing(usize),
    /// The suggestion shows in gray after the typed text.
    Suggested,
    /// The key that types the next part of the suggestion shows as pressed.
    Pressed,
    /// The next part of the suggestion shows as typed text, and the rest stays gray. The demo
    /// stops here.
    Accepted,
}

impl DemoFrame {
    fn first(motion: Motion) -> Self {
        match motion {
            Motion::Animated => Self::Waiting,
            Motion::Still => Self::Accepted,
        }
    }

    /// How long the frame shows before the next frame of a typed text of `letters` letters, or
    /// `None` for the last frame.
    fn wait(self, letters: usize) -> Option<Duration> {
        match self {
            Self::Waiting => Some(DEMO_START_WAIT),
            Self::Typing(count) if count < letters => Some(LETTER_WAIT),
            Self::Typing(_) => Some(TYPED_WAIT),
            Self::Suggested => Some(SUGGESTED_WAIT),
            Self::Pressed => Some(PRESSED_WAIT),
            Self::Accepted => None,
        }
    }

    /// The frame after this one, for a typed text of `letters` letters.
    fn next(self, letters: usize) -> Self {
        match self {
            Self::Waiting => Self::Typing(letters.min(1)),
            Self::Typing(count) if count < letters => Self::Typing(count + 1),
            Self::Typing(_) => Self::Suggested,
            Self::Suggested => Self::Pressed,
            Self::Pressed | Self::Accepted => Self::Accepted,
        }
    }

    /// How many letters of a typed text of `letters` letters show.
    fn typed_letters(self, letters: usize) -> usize {
        match self {
            Self::Waiting => 0,
            Self::Typing(count) => count,
            Self::Suggested | Self::Pressed | Self::Accepted => letters,
        }
    }

    fn shows_gray_suggestion(self) -> bool {
        matches!(self, Self::Suggested | Self::Pressed | Self::Accepted)
    }
}

/// The content of the setup window.
pub(super) struct SetupPage {
    view: Option<SetupView>,
    place: WindowPlace,
    /// The text that the user typed in each field, as on a settings page.
    typed: TypedTexts,
    /// No row of the setup shows an app, so this stays empty.
    app_icons: HashMap<String, Option<EncodedImage>>,
    contrast: Contrast,
    motion: Motion,
    entrance: Entrance,
    demo: DemoFrame,
    entrance_timer: Option<ComponentTimer>,
    demo_timer: Option<ComponentTimer>,
    /// Runs while the step changes. Until it ends, the presses of buttons do not count.
    turn_timer: Option<ComponentTimer>,
}

impl SetupPage {
    fn show(&mut self, view: SetupView, context: &ComponentContext<Self>) {
        let is_new_step = self.view.as_ref().is_none_or(|old| old.step != view.step);
        if is_new_step {
            self.motion = Motion::of_system();
        }
        if is_new_step && self.view.is_some() {
            self.turn_timer = Some(context.set_timeout(TURN_TIME, SetupMessage::Turned));
        }
        if let Some(next) = entrance(self.view.as_ref(), &view, self.motion) {
            self.entrance = next;
            self.entrance_timer =
                (next != Entrance::Entered).then(|| context.set_timeout(ENTER_DELAY, SetupMessage::Enter));
        }
        let has_demo = view.demo.is_some();
        let is_new_demo = self.view.as_ref().is_none_or(|old| old.demo != view.demo);
        self.typed.on_view(&view.sections);
        self.view = Some(view);
        if is_new_step || is_new_demo {
            self.demo = DemoFrame::first(self.motion);
            self.demo_timer = None;
            if has_demo {
                self.schedule_demo(context);
            }
        }
    }

    fn demo_letters(&self) -> usize {
        self.view
            .as_ref()
            .and_then(|view| view.demo.as_ref())
            .map_or(0, |demo| demo.typed.chars().count())
    }

    fn schedule_demo(&mut self, context: &ComponentContext<Self>) {
        self.demo_timer = self
            .demo
            .wait(self.demo_letters())
            .map(|wait| context.set_timeout(wait, SetupMessage::DemoTick));
    }

    /// The scrolling content of the step, which slides in from where its entrance says.
    fn step(&self, renderer: &Renderer<'_, '_, Self>, view: &SetupView) -> View {
        let picture = keyed("picture", self.picture(view));
        let title = keyed(
            "title",
            TextBlock::new()
                .text(&view.title)
                .font_size(TITLE_SIZE)
                .font_weight(FontWeight::SEMI_BOLD)
                .text_wrapping(TextWrapping::Wrap)
                .automation_heading_level(AutomationHeadingLevel::Level1)
                .margin(TITLE_MARGIN),
        );
        let body = view.body.as_ref().map(|body| {
            keyed(
                "body",
                TextBlock::new()
                    .text(body)
                    .font_size(BODY_SIZE)
                    .opacity(BODY_OPACITY)
                    .text_wrapping(TextWrapping::Wrap),
            )
        });
        let demo = view.demo.as_ref().map(|demo| {
            keyed(
                "demo",
                Border::new().margin(BLOCK_MARGIN).content(self.demo_view(demo)),
            )
        });
        let sections = view
            .sections
            .iter()
            .enumerate()
            .flat_map(|(index, section)| spaced_section(renderer, index, section));
        let footnote = view
            .footnote
            .as_ref()
            .map(|footnote| keyed("footnote", secondary_text(footnote).margin(BLOCK_MARGIN)));
        let column = StackPanel::new()
            .spacing(CARD_GAP)
            .max_width(CONTENT_MAX_WIDTH)
            .keyed_children(
                [picture, title]
                    .into_iter()
                    .chain(body)
                    .chain(demo)
                    .chain(sections)
                    .chain(footnote),
            );
        let scroll = ScrollViewer::new()
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Disabled)
            .content(Border::new().padding(CONTENT_MARGIN).content(column));
        let (opacity, offset) = match self.entrance {
            Entrance::FromEnd => (0.0, SLIDE_DISTANCE),
            Entrance::FromStart => (0.0, -SLIDE_DISTANCE),
            Entrance::Entered => (1.0, 0.0),
        };
        let slide = Border::new()
            .opacity(opacity)
            .translation(offset, 0.0)
            .content(scroll);
        match self.motion {
            Motion::Animated => slide
                .opacity_transition(SLIDE_TIME)
                .translation_transition(SLIDE_TIME)
                .exit_fade(FADE_TIME)
                .into(),
            Motion::Still => slide.into(),
        }
    }

    /// The picture of the step for the contrast of the window.
    fn picture(&self, view: &SetupView) -> View {
        match setup_picture(view.step, self.contrast) {
            SetupPicture::Bitmap(bitmap) => Image::new()
                .source_data(EncodedImage::from_static(bitmap))
                .width(PICTURE_SIZE)
                .height(PICTURE_SIZE)
                .horizontal_alignment(HorizontalAlignment::Left)
                .into(),
            SetupPicture::Glyph(glyph) => FontIcon::new()
                .glyph(glyph)
                .font_size(GLYPH_SIZE)
                .horizontal_alignment(HorizontalAlignment::Left)
                .into(),
        }
    }

    /// The demo: a field with the typed text, the suggestion after it in gray, and the key that
    /// types the next part of the suggestion. The gray suggestion is the whole text at half
    /// opacity under the typed text, so the typed text covers its own part.
    fn demo_view(&self, demo: &Demo) -> View {
        let letters = demo.typed.chars().count();
        let whole = format!("{}{}{}", demo.typed, demo.next_part, demo.rest);
        let shown = if self.demo == DemoFrame::Accepted {
            format!("{}{}", demo.typed, demo.next_part)
        } else {
            demo.typed
                .chars()
                .take(self.demo.typed_letters(letters))
                .collect()
        };
        let gray_opacity = if self.demo.shows_gray_suggestion() {
            GHOST_OPACITY
        } else {
            0.0
        };
        let gray = Border::new().opacity(gray_opacity).content(demo_text(&whole));
        let gray = match self.motion {
            Motion::Animated => gray.opacity_transition(FADE_TIME),
            Motion::Still => gray,
        };
        let text = Grid::new()
            .grid_column(0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((gray, demo_text(&shown)));
        let is_pressed = self.demo == DemoFrame::Pressed;
        let key = Border::new()
            .grid_column(1)
            .vertical_alignment(VerticalAlignment::Center)
            .background(ThemeBrush::CardBackground)
            .border_brush(if is_pressed {
                ThemeBrush::Accent
            } else {
                ThemeBrush::CardStroke
            })
            .border_thickness(if is_pressed {
                PRESSED_KEY_STROKE
            } else {
                KEY_STROKE
            })
            .corner_radius(KEY_RADIUS)
            .padding(KEY_PADDING)
            .content(
                TextBlock::new()
                    .text(&demo.key)
                    .font_size(KEY_TEXT_SIZE)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .foreground(if is_pressed {
                        ThemeBrush::AccentText
                    } else {
                        ThemeBrush::PrimaryText
                    }),
            );
        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(STROKE)
            .corner_radius(DEMO_RADIUS)
            .padding(DEMO_PADDING)
            .automation_name(format!("{whole} {}", demo.key))
            .content(
                Grid::new()
                    .columns([GridLength::STAR, GridLength::Auto])
                    .column_spacing(DEMO_COLUMN_GAP)
                    .children((text, key)),
            )
            .into()
    }

    /// The bar at the bottom: the progress, then the back and the next buttons.
    fn footer(&self, renderer: &Renderer<'_, '_, Self>, view: &SetupView) -> View {
        let back = view.back.as_ref().map(|back| {
            keyed(
                "back",
                Border::new()
                    .grid_column(2)
                    .content(renderer.button(back, ButtonStyle::Default)),
            )
        });
        let next = keyed(
            "next",
            Border::new()
                .grid_column(3)
                .content(renderer.button(&view.next, ButtonStyle::Accent)),
        );
        let progress = self.progress(view).grid_column(0);
        let bar = Grid::new()
            .columns([
                GridLength::Auto,
                GridLength::STAR,
                GridLength::Auto,
                GridLength::Auto,
            ])
            .column_spacing(FOOTER_GAP)
            .keyed_children(
                [keyed("progress", progress)]
                    .into_iter()
                    .chain(back)
                    .chain([next]),
            );
        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::new(0.0, STROKE, 0.0, 0.0))
            .padding(FOOTER_PADDING)
            .content(bar)
            .into()
    }
}

fn is_press(message: &RowMessage) -> bool {
    matches!(message, RowMessage::Input(SettingsInput::Pressed(_)))
}

impl SetupPage {
    /// A gray dot for each step, and an accent pill over the dot of the step that shows. The dots
    /// after that step move away by the width that the pill adds, so the gaps between the dots
    /// stay the same. When the step changes, the pill and the dots slide to their new places,
    /// unless the animation effects are off.
    fn progress(&self, view: &SetupView) -> Border {
        let shown = view.progress.number;
        let dots = (1..=view.progress.count).map(|number| {
            keyed(
                number,
                self.slide(
                    Border::new()
                        .width(DOT_SIZE)
                        .background(ThemeBrush::PrimaryText)
                        .opacity(DOT_OPACITY)
                        .translation(slot_offset(number, shown), 0.0),
                ),
            )
        });
        let pill = keyed(
            "pill",
            self.slide(
                Border::new()
                    .width(PILL_WIDTH)
                    .background(ThemeBrush::Accent)
                    .translation(slot_offset(shown, shown), 0.0),
            ),
        );
        Border::new()
            .width(progress_width(view.progress.count))
            .vertical_alignment(VerticalAlignment::Center)
            .automation_name(&view.progress.text)
            .content(Grid::new().keyed_children(dots.chain([pill])))
    }

    /// `shape`, a dot or the pill, at the leading edge, which slides to each new place.
    fn slide(&self, shape: Border) -> Border {
        let shape = shape
            .height(DOT_SIZE)
            .corner_radius(DOT_SIZE / 2.0)
            .horizontal_alignment(HorizontalAlignment::Left)
            .vertical_alignment(VerticalAlignment::Center);
        match self.motion {
            Motion::Animated => shape.translation_transition(SLIDE_TIME),
            Motion::Still => shape,
        }
    }
}

/// The distance from the leading edge of the progress to the dot of the step with the `number`,
/// from 1, while the step with the `shown` number shows. The pill covers the dot of that step, and
/// the dots after it move away by the width that the pill adds.
fn slot_offset(number: usize, shown: usize) -> f64 {
    let steps_before = f64::from(u32::try_from(number.saturating_sub(1)).unwrap_or(u32::MAX));
    let room_for_the_pill = if number > shown {
        PILL_WIDTH - DOT_SIZE
    } else {
        0.0
    };
    steps_before * (DOT_SIZE + DOT_GAP) + room_for_the_pill
}

/// The width of the progress of `count` steps: the last dot when its step shows, under the pill.
fn progress_width(count: usize) -> f64 {
    slot_offset(count, count) + PILL_WIDTH
}

/// The views of `section`, below a gap.
fn spaced_section(
    renderer: &Renderer<'_, '_, SetupPage>,
    index: usize,
    section: &typealike_core::settings_window::view::Section,
) -> Vec<KeyedView> {
    let gap = keyed(format!("{index} gap"), Border::new().margin(SECTION_MARGIN));
    std::iter::once(gap)
        .chain(renderer.section(&index.to_string(), section))
        .collect()
}

fn demo_text(text: &str) -> TextBlock {
    TextBlock::new()
        .text(text)
        .font_size(DEMO_TEXT_SIZE)
        .text_wrapping(TextWrapping::Wrap)
}

impl Component for SetupPage {
    type Input = ();
    type Message = SetupMessage;

    fn create(_input: &(), context: &ComponentContext<Self>) -> Self {
        let motion = Motion::of_system();
        let mut page = Self {
            view: None,
            place: WindowPlace::centered(WINDOW_SIZE),
            typed: TypedTexts::default(),
            app_icons: HashMap::new(),
            contrast: SystemTheme::read().contrast,
            motion,
            entrance: Entrance::Entered,
            demo: DemoFrame::first(motion),
            entrance_timer: None,
            demo_timer: None,
            turn_timer: None,
        };
        if let Some(view) = setup_opened(context.sender()) {
            page.show(view, context);
        }
        if !context.run_window(|window| SetupMessage::Fixed(fix_size(&window))) {
            tracing::warn!("failed to ask for the fixed size of the setup window");
        }
        if !context.run_window(|window| SetupMessage::IconShown(show_app_icon(&window))) {
            tracing::warn!("failed to ask for the icon of the setup window");
        }
        page
    }

    fn update(&mut self, message: SetupMessage, context: &ComponentContext<Self>) {
        match message {
            SetupMessage::Show(view) => self.show(*view, context),
            SetupMessage::Activate => {
                if !context.activate_window() {
                    tracing::warn!("failed to bring the setup window to the front");
                }
            }
            SetupMessage::Close => {
                if !context.close_window() {
                    tracing::warn!("failed to close the setup window");
                }
            }
            SetupMessage::Contrast(contrast) => self.contrast = contrast,
            SetupMessage::Row(message) if self.turn_timer.is_some() && is_press(&message) => {}
            SetupMessage::Row(message) => on_row_message(&mut self.typed, message),
            SetupMessage::Turned => self.turn_timer = None,
            SetupMessage::Fixed(Ok(())) | SetupMessage::IconShown(Ok(())) => {}
            SetupMessage::Fixed(Err(error)) => {
                tracing::warn!(%error, "failed to fix the size of the setup window");
            }
            SetupMessage::IconShown(Err(error)) => {
                tracing::warn!(%error, "failed to show the app icon on the setup window");
            }
            SetupMessage::Enter => {
                self.entrance = Entrance::Entered;
                self.entrance_timer = None;
            }
            SetupMessage::DemoTick => {
                self.demo = self.demo.next(self.demo_letters());
                self.schedule_demo(context);
            }
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<'_, Self>) -> View {
        context.window_visuals(
            self.place
                .applied_to(WindowVisuals::new().backdrop(WindowBackdrop::Mica)),
        );
        let Some(view) = &self.view else {
            return context.window_frame("", Grid::new());
        };
        let renderer = Renderer {
            context,
            typed: &self.typed,
            app_icons: &self.app_icons,
        };
        // The key of the step makes a new scroll viewer for each step, so each step starts at its
        // top, and the step before fades out while the new one slides in.
        let step = keyed(view.progress.number, self.step(&renderer, view));
        let footer = keyed(
            "footer",
            Border::new().grid_row(1).content(self.footer(&renderer, view)),
        );
        let root = Grid::new()
            .rows([GridLength::STAR, GridLength::Auto])
            .keyed_children([
                keyed("step", Grid::new().grid_row(0).keyed_children([step])),
                footer,
            ]);
        context.window_frame(view.window_title.as_str(), root)
    }
}

impl Drop for SetupPage {
    fn drop(&mut self) {
        setup_dropped();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use typealike_core::settings_window::view::ControlId;
    use typealike_core::setup::{SetupStep, StepProgress};

    fn view_at(step: SetupStep, number: usize) -> SetupView {
        SetupView {
            window_title: String::new(),
            step,
            progress: StepProgress {
                number,
                count: 4,
                text: String::new(),
            },
            title: String::new(),
            body: None,
            demo: None,
            sections: Vec::new(),
            footnote: None,
            back: None,
            next: typealike_core::settings_window::view::Button {
                id: ControlId::new("next"),
                label: String::new(),
                is_enabled: true,
            },
            language: typealike_core::localization::UiLanguage::English,
        }
    }

    #[test]
    fn next_step_slides_in_from_the_end_and_the_step_before_from_the_start() {
        let welcome = view_at(SetupStep::Welcome, 1);
        let model = view_at(SetupStep::Model, 2);

        let entrances = [
            entrance(Some(&welcome), &model, Motion::Animated),
            entrance(Some(&model), &welcome, Motion::Animated),
        ];

        assert_eq!(entrances, [Some(Entrance::FromEnd), Some(Entrance::FromStart)]);
    }

    #[test]
    fn same_step_keeps_its_place() {
        let model = view_at(SetupStep::Model, 2);

        assert_eq!(entrance(Some(&model), &model, Motion::Animated), None);
    }

    #[test]
    fn first_step_and_still_windows_show_the_step_in_its_place() {
        let welcome = view_at(SetupStep::Welcome, 1);
        let model = view_at(SetupStep::Model, 2);

        let entrances = [
            entrance(None, &welcome, Motion::Animated),
            entrance(Some(&welcome), &model, Motion::Still),
        ];

        assert_eq!(entrances, [Some(Entrance::Entered), Some(Entrance::Entered)]);
    }

    #[test]
    fn demo_types_each_letter_then_shows_and_takes_the_suggestion() {
        let mut frame = DemoFrame::first(Motion::Animated);

        let frames = [(); 7].map(|()| {
            frame = frame.next(2);
            frame
        });

        assert_eq!(
            frames,
            [
                DemoFrame::Typing(1),
                DemoFrame::Typing(2),
                DemoFrame::Suggested,
                DemoFrame::Pressed,
                DemoFrame::Accepted,
                DemoFrame::Accepted,
                DemoFrame::Accepted,
            ]
        );
    }

    #[test]
    fn only_a_button_press_waits_for_the_change_of_the_step() {
        let messages = [
            RowMessage::Input(SettingsInput::Pressed(ControlId::new("next"))),
            RowMessage::Input(SettingsInput::Toggled {
                id: ControlId::new("switch"),
                is_on: true,
            }),
            RowMessage::Ignore,
        ];

        let presses = messages.map(|message| is_press(&message));

        assert_eq!(presses, [true, false, false]);
    }

    #[test]
    fn dots_after_the_shown_step_make_room_for_the_pill() {
        let offsets = [1, 2, 3, 4].map(|number| slot_offset(number, 2));

        assert_eq!(offsets, [0.0, 16.0, 44.0, 60.0]);
    }

    #[test]
    fn progress_is_as_wide_as_its_dots_its_gaps_and_the_pill() {
        assert!((progress_width(5) - 82.0).abs() < f64::EPSILON);
    }

    #[test]
    fn demo_stops_when_the_key_typed_the_next_part() {
        assert_eq!(DemoFrame::Accepted.wait(2), None);
    }

    #[test]
    fn still_demo_shows_its_last_frame_at_once() {
        let frame = DemoFrame::first(Motion::Still);

        assert_eq!(
            (frame, frame.typed_letters(5), frame.shows_gray_suggestion()),
            (DemoFrame::Accepted, 5, true)
        );
    }
}
