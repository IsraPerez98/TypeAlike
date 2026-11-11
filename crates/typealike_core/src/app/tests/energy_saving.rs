//! Tests of the stops that save energy: the energy saver of the computer and the idle timeout.

use super::fixtures::{MAIL, app_preparing_on_focus};
use crate::app::{App, BadgeView, Status, StopReason, TrayCommand, TrayGlyph, TrayItem, TrayView};
use crate::desktop::EnergySaver;
use crate::engine::Reaction;
use crate::key::{Key, Press};
use crate::localization::UiLanguage;
use crate::settings::{EnergySaverRule, EnergySavingSettings, Settings};
use crate::test_support::english;

fn energy_saver_badge() -> BadgeView {
    BadgeView {
        status: Status::Paused,
        label: "TypeAlike (off while the computer saves energy)".to_owned(),
        frame: None,
    }
}

fn app_keeping_suggestions_in_energy_saver() -> App {
    let mut app = App::new();
    app.use_settings(&settings_with(EnergySaverRule::KeepSuggestions));
    app
}

fn settings_with(rule: EnergySaverRule) -> Settings {
    Settings {
        energy_saving: EnergySavingSettings {
            energy_saver: rule,
            ..EnergySavingSettings::default()
        },
        ..Settings::default()
    }
}

fn app_in_energy_saver() -> App {
    let mut app = App::new();
    app.use_energy_saver(EnergySaver::On);
    app
}

#[test]
fn key_in_energy_saver_asks_for_no_suggestion() {
    let mut app = app_in_energy_saver();

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert_eq!((reaction, app.has_offer()), (Reaction::Hide, false));
}

#[test]
fn key_in_energy_saver_asks_for_a_suggestion_when_the_switch_is_off() {
    let mut app = app_keeping_suggestions_in_energy_saver();
    app.use_energy_saver(EnergySaver::On);

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn key_after_the_energy_saver_goes_off_asks_for_a_suggestion() {
    let mut app = app_in_energy_saver();
    app.use_energy_saver(EnergySaver::Off);

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn energy_saver_ends_the_session() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    app.use_energy_saver(EnergySaver::On);

    assert_eq!((app.text_before_caret(), app.has_offer()), ("", false));
}

#[test]
fn turning_on_the_switch_in_energy_saver_ends_the_session() {
    let mut app = app_keeping_suggestions_in_energy_saver();
    app.use_energy_saver(EnergySaver::On);
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    app.use_settings(&settings_with(EnergySaverRule::StopSuggestions));

    assert_eq!((app.text_before_caret(), app.has_offer()), ("", false));
}

#[test]
fn settled_input_in_energy_saver_prepares_nothing() {
    let mut app = app_preparing_on_focus();
    app.use_energy_saver(EnergySaver::On);

    let work = app.on_input_settled(&MAIL);

    assert_eq!(work, None);
}

#[test]
fn typing_in_energy_saver_shows_the_energy_saver_badge() {
    let mut app = app_in_energy_saver();

    app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert_eq!(app.badge_view(&english()), Some(energy_saver_badge()));
}

#[test]
fn paused_badge_shows_while_suggestions_are_paused_in_energy_saver() {
    let mut app = app_in_energy_saver();
    app.on_tray_command(TrayCommand::TogglePause);

    app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert_eq!(
        app.badge_view(&english()).map(|badge| badge.label),
        Some("TypeAlike (paused)".to_owned())
    );
}

#[test]
fn tray_in_energy_saver_shows_the_stop_and_offers_the_pause() {
    let app = app_in_energy_saver();

    let view = app.tray_view(&english());

    assert_eq!(
        view,
        TrayView {
            glyph: TrayGlyph::Stopped,
            tooltip: "TypeAlike (off while the computer saves energy)".to_owned(),
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
fn pause_in_energy_saver_shows_the_paused_glyph_in_the_tray() {
    let mut app = app_in_energy_saver();
    app.on_tray_command(TrayCommand::TogglePause);

    let view = app.tray_view(&english());

    assert_eq!(
        (view.glyph, view.tooltip.as_str()),
        (TrayGlyph::Paused, "TypeAlike (paused)")
    );
}

#[test]
fn new_app_has_no_stop_reason() {
    let app = App::new();

    assert_eq!(app.stop_reason(), None);
}

#[test]
fn energy_saver_is_the_stop_reason() {
    let app = app_in_energy_saver();

    assert_eq!(app.stop_reason(), Some(StopReason::EnergySaver));
}

#[test]
fn energy_saver_is_no_stop_reason_when_the_switch_is_off() {
    let mut app = app_keeping_suggestions_in_energy_saver();

    app.use_energy_saver(EnergySaver::On);

    assert_eq!(app.stop_reason(), None);
}

#[test]
fn pause_is_the_stop_reason_also_in_energy_saver() {
    let mut app = app_in_energy_saver();

    app.on_tray_command(TrayCommand::TogglePause);

    assert_eq!(app.stop_reason(), Some(StopReason::Paused));
}

#[test]
fn idle_timeout_is_the_stop_reason() {
    let mut app = App::new();

    app.on_idle();

    assert_eq!(app.stop_reason(), Some(StopReason::Idle));
}

#[test]
fn energy_saver_is_the_stop_reason_also_after_the_idle_timeout() {
    let mut app = app_in_energy_saver();

    app.on_idle();

    assert_eq!(app.stop_reason(), Some(StopReason::EnergySaver));
}

#[test]
fn idle_timeout_ends_the_session_and_hides_the_badge() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    let reaction = app.on_idle();

    assert_eq!(
        (reaction, app.text_before_caret(), app.badge_view(&english())),
        (Reaction::Hide, "", None)
    );
}

#[test]
fn key_after_the_idle_timeout_asks_for_a_suggestion() {
    let mut app = App::new();
    app.on_idle();

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn end_of_the_idle_stop_clears_the_stop_reason() {
    let mut app = App::new();
    app.on_idle();

    let was_idle = app.end_idle();

    assert_eq!((was_idle, app.stop_reason()), (true, None));
}

#[test]
fn end_of_the_idle_stop_tells_when_the_app_was_not_idle() {
    let mut app = App::new();

    let was_idle = app.end_idle();

    assert!(!was_idle);
}

#[test]
fn resume_ends_the_idle_stop() {
    let mut app = App::new();
    app.on_tray_command(TrayCommand::TogglePause);
    app.on_idle();

    app.on_tray_command(TrayCommand::TogglePause);

    assert_eq!(app.stop_reason(), None);
}
