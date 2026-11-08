//! Tests of the tray view and the commands of the tray menu.

use crate::app::{App, TrayCommand, TrayGlyph, TrayItem, TrayOutcome, TrayView};
use crate::localization::UiLanguage;
use crate::test_support::english;

#[test]
fn new_app_is_active_with_pause_entry() {
    let app = App::new();

    let view = app.tray_view(&english());

    assert_eq!(
        view,
        TrayView {
            glyph: TrayGlyph::Active,
            tooltip: "TypeAlike".to_owned(),
            items: vec![
                TrayItem {
                    label: "Pause suggestions".to_owned(),
                    command: TrayCommand::TogglePause
                },
                TrayItem {
                    label: "Settings…".to_owned(),
                    command: TrayCommand::OpenSettings
                },
                TrayItem {
                    label: "Quit TypeAlike".to_owned(),
                    command: TrayCommand::Quit
                },
            ],
            language: UiLanguage::English,
        }
    );
}

#[test]
fn pausing_reports_paused() {
    let mut app = App::new();

    let outcome = app.on_tray_command(TrayCommand::TogglePause);

    assert_eq!(outcome, TrayOutcome::Paused);
}

#[test]
fn resuming_reports_resumed() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);

    let outcome = app.on_tray_command(TrayCommand::TogglePause);

    assert_eq!(outcome, TrayOutcome::Resumed);
}

#[test]
fn paused_app_offers_resume_entry() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);

    let view = app.tray_view(&english());

    assert_eq!(
        view,
        TrayView {
            glyph: TrayGlyph::Paused,
            tooltip: "TypeAlike (paused)".to_owned(),
            items: vec![
                TrayItem {
                    label: "Resume suggestions".to_owned(),
                    command: TrayCommand::TogglePause
                },
                TrayItem {
                    label: "Settings…".to_owned(),
                    command: TrayCommand::OpenSettings
                },
                TrayItem {
                    label: "Quit TypeAlike".to_owned(),
                    command: TrayCommand::Quit
                },
            ],
            language: UiLanguage::English,
        }
    );
}

#[test]
fn settings_command_opens_settings() {
    let mut app = App::new();

    let outcome = app.on_tray_command(TrayCommand::OpenSettings);

    assert_eq!(outcome, TrayOutcome::OpenSettings);
}

#[test]
fn quit_command_quits() {
    let mut app = App::new();

    let outcome = app.on_tray_command(TrayCommand::Quit);

    assert_eq!(outcome, TrayOutcome::Quit);
}
