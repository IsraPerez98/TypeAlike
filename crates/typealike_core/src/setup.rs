//! The setup: a window that shows when TypeAlike starts for the first time and after a reset of
//! the settings, until the user finishes it. It tells what TypeAlike does, downloads the model,
//! asks for the permission of the OS, and lets the user pick what suggestions use. Its rows are the
//! rows of the settings window, so each platform renders them as in the settings window and sends
//! back each [`SettingsInput`].

use std::iter;

use crate::desktop::{Permission, PermissionState};
use crate::engine::split_next_part;
use crate::hub::ModelFile;
use crate::localization::{MessageArgument, Translations, UiLanguage};
use crate::settings::{
    ClipboardAccess, HistoryAccess, PersonalizationSettings, ScreenTextAccess, ScreenTextSettings,
};
use crate::settings_window::general::{CLIPBOARD, GeneralAction, GeneralPane};
use crate::settings_window::models::ModelsPane;
use crate::settings_window::permissions::permission_row;
use crate::settings_window::personalization::HISTORY;
use crate::settings_window::playground::PlaygroundPane;
use crate::settings_window::screen_text::SCREEN_TEXT;
use crate::settings_window::view::{Button, ControlId, Row, RowIcon, Section, SettingsInput, Switch};

const BACK: &str = "setup-back";
const NEXT: &str = "setup-next";
const AUTOSTART: &str = "setup-autostart";

/// Everything that the setup window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupView {
    /// The title of the window.
    pub window_title: String,
    /// The step that the window shows. Each platform shows the picture of the step above its title.
    pub step: SetupStep,
    /// Which step of how many the window shows.
    pub progress: StepProgress,
    /// The title of the step, in sentence case.
    pub title: String,
    /// One to three sentences below the title.
    pub body: String,
    /// The demo of a suggestion below the body.
    pub demo: Option<Demo>,
    /// The sections, top to bottom, as in the settings window.
    pub sections: Vec<Section>,
    /// A short line at the bottom of the step, such as the promise that the text stays on the
    /// computer.
    pub footnote: Option<String>,
    /// The button that goes back to the step before. The first step has none.
    pub back: Option<Button>,
    /// The button that goes to the next step, or that finishes the setup on the last step. It is
    /// the default button of the window.
    pub next: Button,
    /// The language of the texts.
    pub language: UiLanguage,
}

/// A step of the setup, in the order of the setup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    /// What TypeAlike does.
    Welcome,
    /// The download of the model.
    Model,
    /// The permission of the OS that suggestions need. Only an OS that asks the user for a
    /// permission has this step.
    Permission,
    /// The name and the notes of the user, and what suggestions can use.
    About,
    /// Where to try TypeAlike, where its icon is, and the switch that starts it at login.
    Ready,
}

/// Which step of how many the setup window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepProgress {
    /// The number of the step, from 1.
    pub number: usize,
    /// The number of steps.
    pub count: usize,
    /// The same as a text, for screen readers.
    pub text: String,
}

/// A suggestion as it shows while the user types: the typed text, the suggestion after it in
/// gray, and the key that types the next part of the suggestion. The suggestion is the next part
/// and then the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Demo {
    /// The text that the user types.
    pub typed: String,
    /// The part of the suggestion that one press of the key types: its next word, with the space
    /// before it.
    pub next_part: String,
    /// The rest of the suggestion, which stays gray after the key types the next part.
    pub rest: String,
    /// The name of the key that types the next part, on a key cap.
    pub key: String,
}

/// Where the platform shows the icon of TypeAlike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayPlace {
    /// The notification area of the taskbar (Windows).
    NotificationArea,
    /// The menu bar (macOS).
    MenuBar,
}

/// Something that the user did with a control of the setup that no pane owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetupAction {
    Back,
    Next,
    SwitchAutostart(bool),
}

/// What the controller must do after a setup action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SetupOutcome {
    /// The setup shows its new state.
    Shown,
    /// The user finished the setup, and the setup window closes. The action applies the choice of
    /// the autostart switch.
    Finished(GeneralAction),
}

/// What the setup shows: the settings and the state of the panes, and the platform.
pub(crate) struct SetupContext<'context> {
    pub(crate) general: &'context GeneralPane,
    pub(crate) models: &'context ModelsPane,
    pub(crate) playground: &'context PlaygroundPane,
    pub(crate) installed: &'context [ModelFile],
    pub(crate) personalization: &'context PersonalizationSettings,
    pub(crate) screen_text: &'context ScreenTextSettings,
    pub(crate) permissions: &'context [(Permission, PermissionState)],
    pub(crate) tray: TrayPlace,
    pub(crate) translations: &'context Translations,
}

/// The state of an open setup window: the step that it shows, and the choice of the autostart
/// switch, which applies when the user finishes the setup. The switch is on by default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SetupFlow {
    step: SetupStep,
    is_autostart_on: bool,
}

impl SetupFlow {
    /// A setup at its first step.
    pub(crate) fn new() -> Self {
        Self {
            step: SetupStep::Welcome,
            is_autostart_on: true,
        }
    }

    /// The action of `input`, when `input` is for a control of the setup that no pane owns.
    pub(crate) fn action_for(input: &SettingsInput) -> Option<SetupAction> {
        match input {
            SettingsInput::Pressed(id) if id.as_str() == BACK => Some(SetupAction::Back),
            SettingsInput::Pressed(id) if id.as_str() == NEXT => Some(SetupAction::Next),
            SettingsInput::Toggled { id, is_on } if id.as_str() == AUTOSTART => {
                Some(SetupAction::SwitchAutostart(*is_on))
            }
            _ => None,
        }
    }

    /// Handles `action`. The next button goes on only while the step allows it.
    pub(crate) fn on_action(&mut self, action: SetupAction, context: &SetupContext<'_>) -> SetupOutcome {
        let steps = steps(context.permissions);
        let index = step_index(&steps, self.step);
        match action {
            SetupAction::Back => {
                self.step = steps[index.saturating_sub(1)];
            }
            SetupAction::Next if !can_continue(self.step, context) => {}
            SetupAction::Next => match steps.get(index + 1) {
                Some(&next) => self.step = next,
                None if self.is_autostart_on => {
                    return SetupOutcome::Finished(GeneralAction::EnableAutostart);
                }
                None => return SetupOutcome::Finished(GeneralAction::DisableAutostart),
            },
            SetupAction::SwitchAutostart(is_on) => self.is_autostart_on = is_on,
        }
        SetupOutcome::Shown
    }

    /// The view of the step that the setup shows.
    pub(crate) fn view(&self, context: &SetupContext<'_>) -> SetupView {
        let translations = context.translations;
        let steps = steps(context.permissions);
        let number = step_index(&steps, self.step) + 1;
        let count = steps.len();
        let step_content = match self.step {
            SetupStep::Welcome => welcome(translations),
            SetupStep::Model => model(context),
            SetupStep::Permission => permission(context),
            SetupStep::About => about(context),
            SetupStep::Ready => self.ready(context),
        };
        let next_label = match self.step {
            SetupStep::Welcome => "setup-start",
            SetupStep::Ready => "setup-finish",
            SetupStep::Model | SetupStep::Permission | SetupStep::About => "setup-continue",
        };
        SetupView {
            window_title: translations.text("setup-window-title"),
            step: self.step,
            progress: StepProgress {
                number,
                count,
                text: translations.text_with(
                    "setup-progress",
                    &[
                        ("number", count_argument(number)),
                        ("count", count_argument(count)),
                    ],
                ),
            },
            title: translations.text(step_content.title),
            body: translations.text(step_content.body),
            demo: step_content.demo,
            sections: step_content.sections,
            footnote: step_content.footnote,
            back: (self.step != SetupStep::Welcome).then(|| button(BACK, "setup-back", true, translations)),
            next: button(NEXT, next_label, can_continue(self.step, context), translations),
            language: translations.language(),
        }
    }

    fn ready(&self, context: &SetupContext<'_>) -> StepContent {
        let translations = context.translations;
        let (title, body) = match model_state(context) {
            ModelState::Ready => ("setup-ready-title", "setup-ready-body"),
            ModelState::Downloading => ("setup-downloading-title", "setup-downloading-body"),
            ModelState::Missing => ("setup-no-model-title", "setup-no-model-body"),
        };
        let download = recommended_rows(context);
        let download = (!download.is_empty()).then(|| Section::new(download));
        let autostart = context.general.has_autostart().then(|| {
            Section::new(vec![Row::Switch(Switch {
                id: ControlId::new(AUTOSTART),
                icon: RowIcon::Power,
                label: translations.text("general-autostart"),
                description: translations.text("general-autostart-description"),
                is_on: self.is_autostart_on,
                note: None,
            })])
        });
        let tray = match context.tray {
            TrayPlace::NotificationArea => "setup-tray-notification-area",
            TrayPlace::MenuBar => "setup-tray-menu-bar",
        };
        StepContent {
            sections: download.into_iter().chain(autostart).collect(),
            footnote: Some(translations.text(tray)),
            ..StepContent::new(title, body)
        }
    }
}

/// The parts of a step that differ from step to step.
struct StepContent {
    title: &'static str,
    body: &'static str,
    demo: Option<Demo>,
    sections: Vec<Section>,
    footnote: Option<String>,
}

impl StepContent {
    fn new(title: &'static str, body: &'static str) -> Self {
        Self {
            title,
            body,
            demo: None,
            sections: Vec::new(),
            footnote: None,
        }
    }
}

/// Whether the suggestion model is on the computer, downloads, or is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelState {
    Ready,
    Downloading,
    Missing,
}

fn model_state(context: &SetupContext<'_>) -> ModelState {
    if context.general.settings().suggestion_model.is_some() || !context.installed.is_empty() {
        ModelState::Ready
    } else if context.models.has_running_download() {
        ModelState::Downloading
    } else {
        ModelState::Missing
    }
}

/// The steps of the setup: the permission step only on an OS that asks for the Accessibility
/// permission.
fn steps(permissions: &[(Permission, PermissionState)]) -> Vec<SetupStep> {
    let permission = permissions
        .iter()
        .any(|&(permission, _)| permission == Permission::Accessibility)
        .then_some(SetupStep::Permission);
    [SetupStep::Welcome, SetupStep::Model]
        .into_iter()
        .chain(permission)
        .chain([SetupStep::About, SetupStep::Ready])
        .collect()
}

/// The index of `step` in `steps`. A step that the OS no longer has, such as the permission step,
/// counts as the step before it.
fn step_index(steps: &[SetupStep], step: SetupStep) -> usize {
    steps.iter().position(|&each| each == step).unwrap_or(1)
}

/// Whether the user can go on from `step`: the model must download or be on the computer, and
/// the Accessibility permission must be given.
fn can_continue(step: SetupStep, context: &SetupContext<'_>) -> bool {
    match step {
        SetupStep::Model => model_state(context) != ModelState::Missing,
        SetupStep::Permission => context.permissions.iter().all(|&(permission, state)| {
            permission != Permission::Accessibility || state == PermissionState::Granted
        }),
        SetupStep::Welcome | SetupStep::About | SetupStep::Ready => true,
    }
}

/// The step of the welcome, with a demo whose key types the part of the suggestion that Tab types.
fn welcome(translations: &Translations) -> StepContent {
    let suggestion = format!(" {}", translations.text("setup-demo-suggestion"));
    let (next_part, rest) = split_next_part(&suggestion);
    StepContent {
        demo: Some(Demo {
            typed: translations.text("setup-demo-typed"),
            next_part: next_part.to_owned(),
            rest: rest.to_owned(),
            key: translations.text("setup-demo-key"),
        }),
        footnote: Some(translations.text("setup-welcome-privacy")),
        ..StepContent::new("setup-welcome-title", "setup-welcome-body")
    }
}

/// The step of the model: the model that TypeAlike recommends, with the menu of the backend,
/// while no model is on the computer. Else the menus of the suggestion model and the backend.
fn model(context: &SetupContext<'_>) -> StepContent {
    let translations = context.translations;
    let general = context.general;
    let recommended = recommended_rows(context);
    if recommended.is_empty() {
        return StepContent {
            sections: vec![Section {
                footer: Some(translations.text("setup-model-ready")),
                ..general.model_section(context.playground, context.installed, translations)
            }],
            ..StepContent::new("setup-model-select-title", "setup-model-body")
        };
    }
    let sections = {
        let model = if context.models.is_reading_gpu_memory(general.settings().backend) {
            Section::new(vec![Row::Loading(translations.text("setup-model-checking"))])
        } else {
            Section {
                footer: Some(translations.text("setup-model-footer")),
                ..Section::new(recommended)
            }
        };
        iter::once(model)
            .chain(general.backend_section(context.playground, translations))
            .collect()
    };
    StepContent {
        sections,
        ..StepContent::new("setup-model-title", "setup-model-body")
    }
}

/// The row of the model that TypeAlike recommends, below its warning, while no model is on the
/// computer.
fn recommended_rows(context: &SetupContext<'_>) -> Vec<Row> {
    let backend = context.general.settings().backend;
    let translations = context.translations;
    context
        .models
        .recommendation_warning(backend, translations)
        .into_iter()
        .chain(context.models.recommended_row(backend, translations))
        .collect()
}

fn permission(context: &SetupContext<'_>) -> StepContent {
    let rows = context
        .permissions
        .iter()
        .filter(|&&(permission, _)| permission == Permission::Accessibility)
        .map(|&(permission, state)| permission_row(permission, state, context.translations))
        .collect();
    StepContent {
        sections: vec![Section::new(rows)],
        ..StepContent::new("setup-permission-title", "setup-permission-body")
    }
}

fn about(context: &SetupContext<'_>) -> StepContent {
    let translations = context.translations;
    let switch = |id: &'static str, icon, (label, description): (&str, &str), is_on| {
        Row::Switch(Switch {
            id: ControlId::new(id),
            icon,
            label: translations.text(label),
            description: translations.text(description),
            is_on,
            note: None,
        })
    };
    let sources = Section {
        heading: Some(translations.text("setup-sources-heading")),
        back: None,
        rows: vec![
            switch(
                HISTORY,
                RowIcon::ClockArrow,
                ("personalization-history", "setup-history-description"),
                context.personalization.history == HistoryAccess::On,
            ),
            switch(
                CLIPBOARD,
                RowIcon::Clipboard,
                ("screen-text-clipboard", "screen-text-clipboard-description"),
                context.general.settings().clipboard_access == ClipboardAccess::Allowed,
            ),
            switch(
                SCREEN_TEXT,
                RowIcon::Monitor,
                ("screen-text-switch", "setup-screen-text-description"),
                context.screen_text.access == ScreenTextAccess::Allowed,
            ),
        ],
        footer: Some(translations.text("setup-sources-footer")),
    };
    StepContent {
        sections: vec![context.general.writer_section(translations), sources],
        ..StepContent::new("setup-about-title", "setup-about-body")
    }
}

fn button(id: &'static str, label: &str, is_enabled: bool, translations: &Translations) -> Button {
    Button {
        id: ControlId::new(id),
        label: translations.text(label),
        is_enabled,
    }
}

fn count_argument(count: usize) -> MessageArgument<'static> {
    MessageArgument::Number(u32::try_from(count).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests;
