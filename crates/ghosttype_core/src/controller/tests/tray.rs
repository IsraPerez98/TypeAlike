//! Tests of the commands of the tray menu.

use super::harness::{Call, Harness, ghost_at_caret, model_file, suggestion_settings};
use crate::app::TrayCommand;
use crate::hub::HubRequest;
use crate::inference::{Backend, InferenceRequest};
use crate::key::Key;
use crate::settings::Settings;
use crate::test_support::PAUSED_BADGE;

#[test]
fn pause_hides_ghost() {
    let mut harness = Harness::new(Settings::default());
    harness.show("est");

    harness.toggle_pause();

    assert!(harness.take_calls().contains(&Call::HideGhost));
}

#[test]
fn pause_unloads_suggestion_model() {
    let mut harness = Harness::new(suggestion_settings());

    harness.toggle_pause();

    assert_eq!(harness.last_request(), Some(InferenceRequest::Unload));
}

#[test]
fn pause_renders_paused_tray() {
    let mut harness = Harness::new(Settings::default());

    harness.toggle_pause();

    let paused_tray = harness.controller.tray_view();
    assert!(harness.take_calls().contains(&Call::RenderTray(paused_tray)));
}

#[test]
fn pause_shows_paused_badge_at_caret() {
    let mut harness = Harness::new(Settings::default());
    harness.press(Key::Char('B'));

    harness.toggle_pause();

    assert!(
        harness
            .take_calls()
            .contains(&ghost_at_caret(None, Some(PAUSED_BADGE)))
    );
}

#[test]
fn resume_loads_suggestion_model() {
    let mut harness = Harness::new(suggestion_settings());
    harness.toggle_pause();

    harness.toggle_pause();

    assert_eq!(
        harness.last_request(),
        Some(InferenceRequest::Load {
            file: model_file(),
            backend: Backend::Cpu,
        })
    );
}

#[test]
fn open_settings_lists_installed_models() {
    let mut harness = Harness::new(Settings::default());

    harness
        .controller
        .on_tray_command(TrayCommand::OpenSettings, &mut harness.shell);

    assert_eq!(harness.hub_requests(), vec![HubRequest::ListInstalled]);
}

#[test]
fn open_settings_shows_settings_window() {
    let mut harness = Harness::new(Settings::default());

    harness
        .controller
        .on_tray_command(TrayCommand::OpenSettings, &mut harness.shell);

    assert!(matches!(harness.take_calls().as_slice(), [Call::ShowSettings(_)]));
}

#[test]
fn quit_command_quits() {
    let mut harness = Harness::new(Settings::default());

    harness
        .controller
        .on_tray_command(TrayCommand::Quit, &mut harness.shell);

    assert_eq!(harness.take_calls(), vec![Call::Quit]);
}
