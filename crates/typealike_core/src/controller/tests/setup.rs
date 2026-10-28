//! Tests of the setup window: when it shows, how it ends, and the reset of the settings that
//! opens it again.

use super::harness::{Call, Harness, first_start_settings, loaded, model_file};
use crate::app::TrayCommand;
use crate::controller::Delivery;
use crate::desktop::{Permission, PermissionState};
use crate::hub::HubRequest;
use crate::inference::{Backend, InferenceError, InferenceEvent};
use crate::localization::UiLanguage;
use crate::settings::{HistoryAccess, Settings, Setup, WriterNotes};
use crate::settings_window::general::UI_LANGUAGE;
use crate::settings_window::personalization::HISTORY;
use crate::settings_window::view::{ControlId, PageIcon, Row, SettingsInput};
use crate::setup::SetupStep;

const NEXT: &str = "setup-next";
const RESET: &str = "reset-settings";

fn needed() -> Settings {
    Settings {
        setup: Setup::Needed,
        ..Settings::default()
    }
}

fn shown_step(harness: &mut Harness) -> Option<SetupStep> {
    harness.last_setup().map(|view| view.step)
}

#[test]
fn launch_shows_the_setup_until_the_user_finishes_it() {
    let mut harness = Harness::new(needed());

    harness.controller.on_launch(&mut harness.shell);

    assert_eq!(shown_step(&mut harness), Some(SetupStep::Welcome));
}

#[test]
fn launch_after_the_setup_shows_nothing() {
    let mut harness = Harness::new(Settings::default());
    harness.take_calls();

    harness.controller.on_launch(&mut harness.shell);

    assert_eq!(harness.take_calls(), Vec::new());
}

#[test]
fn settings_of_the_tray_open_the_setup_until_the_user_finishes_it() {
    let mut harness = Harness::new(needed());

    harness
        .controller
        .on_tray_command(TrayCommand::OpenSettings, &mut harness.shell);

    let calls = harness.take_calls();
    assert_eq!(
        (
            calls.iter().any(|call| matches!(call, Call::ShowSetup(_))),
            calls.iter().any(|call| matches!(call, Call::ShowSettings(..)))
        ),
        (true, false)
    );
}

#[test]
fn missing_permission_opens_the_setup_until_the_user_finishes_it() {
    let mut harness = Harness::new(needed());

    harness.controller.on_missing_permission(&mut harness.shell);

    assert_eq!(shown_step(&mut harness), Some(SetupStep::Welcome));
}

#[test]
fn missing_permission_after_the_setup_opens_the_permissions_pane() {
    let mut harness = Harness::new(Settings::default());
    harness.shell.permissions = vec![(Permission::Accessibility, PermissionState::Missing)];

    harness.controller.on_missing_permission(&mut harness.shell);

    let shown = harness.take_calls().into_iter().find_map(|call| match call {
        Call::ShowSettings(view, page) => page.map(|index| view.pages[index].icon),
        _ => None,
    });
    assert_eq!(shown, Some(PageIcon::Hand));
}

#[test]
fn next_shows_the_next_step() {
    let mut harness = Harness::new(needed());
    harness.controller.on_launch(&mut harness.shell);

    harness.press_button(NEXT);

    assert_eq!(shown_step(&mut harness), Some(SetupStep::Model));
}

#[test]
fn closed_setup_starts_again_at_its_first_step() {
    let mut harness = Harness::new(needed());
    harness.controller.on_launch(&mut harness.shell);
    harness.press_button(NEXT);

    harness.controller.on_setup_closed();
    harness.controller.on_launch(&mut harness.shell);

    assert_eq!(shown_step(&mut harness), Some(SetupStep::Welcome));
}

#[test]
fn closed_setup_stays_needed() {
    let mut harness = Harness::new(needed());
    harness.controller.on_launch(&mut harness.shell);

    harness.controller.on_setup_closed();

    assert_eq!(harness.saved(), Vec::new());
}

#[test]
fn switch_of_the_setup_changes_its_setting_and_shows_it() {
    let mut harness = Harness::new(needed());
    harness.controller.on_launch(&mut harness.shell);

    harness.toggle(HISTORY, false);

    let history = harness.saved().last().map(|saved| saved.personalization.history);
    assert_eq!(
        (history, harness.last_setup().is_some()),
        (Some(HistoryAccess::Off), true)
    );
}

#[test]
fn picked_language_shows_the_setup_in_it() {
    let mut harness = Harness::new(needed());
    harness.controller.on_launch(&mut harness.shell);

    harness.pick(UI_LANGUAGE, 3);

    let setup = harness.last_setup();
    assert_eq!(
        setup.map(|view| (view.step, view.language)),
        Some((SetupStep::Welcome, UiLanguage::Spanish))
    );
}

#[test]
fn reset_shows_the_setup_in_the_language_of_the_system() {
    let mut harness = Harness::with_ui_language(
        Settings {
            ui_language: Some(UiLanguage::Spanish),
            ..Settings::default()
        },
        UiLanguage::English,
    );
    harness.press_button(RESET);

    harness.press_button(RESET);

    let setup = harness.last_setup();
    assert_eq!(setup.map(|view| view.language), Some(UiLanguage::English));
}

#[test]
fn finish_closes_the_setup_and_keeps_it_finished() {
    let mut harness = Harness::new(needed());
    harness.install(model_file());
    harness.controller.on_launch(&mut harness.shell);

    let steps = [(); 3].map(|()| {
        harness.press_button(NEXT);
        shown_step(&mut harness)
    });
    harness.press_button(NEXT);

    let calls = harness.take_calls();
    assert_eq!(
        (
            steps,
            calls.contains(&Call::CloseSetup),
            harness.saved().last().map(|saved| saved.setup)
        ),
        (
            [
                Some(SetupStep::Model),
                Some(SetupStep::About),
                Some(SetupStep::Ready)
            ],
            true,
            Some(Setup::Finished)
        )
    );
}

#[test]
fn first_press_of_reset_changes_no_setting() {
    let mut harness = Harness::new(Settings::default());

    harness.press_button(RESET);

    assert_eq!(harness.saved(), Vec::new());
}

#[test]
fn second_press_of_reset_gives_the_first_start_settings_and_opens_the_setup() {
    let mut harness = Harness::new(Settings {
        suggestion_model: Some(model_file()),
        writer_notes: WriterNotes("I am a nurse.".to_owned()),
        ..Settings::default()
    });
    harness.answer(loaded());
    harness.press_button(RESET);

    harness.press_button(RESET);

    let expected = Settings {
        suggestion_model: Some(model_file()),
        ..first_start_settings()
    };
    assert_eq!(
        (harness.saved().first().cloned(), shown_step(&mut harness)),
        (Some(expected), Some(SetupStep::Welcome))
    );
}

#[test]
fn other_input_cancels_the_first_press_of_reset() {
    let mut harness = Harness::new(Settings::default());
    harness.press_button(RESET);
    harness.toggle(HISTORY, false);

    harness.press_button(RESET);

    assert_eq!(
        harness.saved().last().map(|saved| saved.setup),
        Some(Setup::Finished)
    );
}

#[test]
fn failed_read_of_the_graphics_card_warns_on_the_general_page() {
    let mut harness = Harness::new(Settings {
        backend: Backend::Vulkan,
        ..Settings::default()
    });
    let event = InferenceEvent::GpuMemory {
        backend: Backend::Vulkan,
        result: Err(InferenceError::BackendUnavailable),
    };

    harness
        .controller
        .on_delivery(Delivery::Recommendation(event), &mut harness.shell);

    let view = harness.shown_settings();
    let has_warning = view.pages[0].sections[0]
        .rows
        .iter()
        .any(|row| matches!(row, Row::Note(note) if note.text.starts_with("TypeAlike cannot read the memory of the graphics card")));
    assert!(has_warning);
}

#[test]
fn search_of_the_setup_searches_hugging_face() {
    let mut harness = Harness::new(needed());
    harness.controller.on_launch(&mut harness.shell);
    harness.press_button(NEXT);
    harness.press_button("setup-other-models");

    let search = SettingsInput::Submitted {
        id: ControlId::new("models-search"),
        text: "qwen".to_owned(),
    };
    harness.controller.on_settings_input(&search, &mut harness.shell);

    assert!(harness.hub_requests().contains(&HubRequest::Search {
        query: "qwen".to_owned()
    }));
}
