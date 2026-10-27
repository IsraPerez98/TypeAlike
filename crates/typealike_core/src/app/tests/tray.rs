//! Tests of the tray view and the commands of the tray menu.

use crate::app::{App, Status, TrayCommand, TrayItem, TrayOutcome, TrayView};

#[test]
fn new_app_is_active_with_pause_entry() {
    let app = App::new();

    let view = app.tray_view();

    assert_eq!(
        view,
        TrayView {
            status: Status::Active,
            tooltip: "TypeAlike",
            items: vec![
                TrayItem {
                    label: "Pause suggestions",
                    command: TrayCommand::TogglePause
                },
                TrayItem {
                    label: "Settings…",
                    command: TrayCommand::OpenSettings
                },
                TrayItem {
                    label: "Quit TypeAlike",
                    command: TrayCommand::Quit
                },
            ],
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

    let view = app.tray_view();

    assert_eq!(
        view,
        TrayView {
            status: Status::Paused,
            tooltip: "TypeAlike (paused)",
            items: vec![
                TrayItem {
                    label: "Resume suggestions",
                    command: TrayCommand::TogglePause
                },
                TrayItem {
                    label: "Settings…",
                    command: TrayCommand::OpenSettings
                },
                TrayItem {
                    label: "Quit TypeAlike",
                    command: TrayCommand::Quit
                },
            ],
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
