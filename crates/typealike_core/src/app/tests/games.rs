//! Tests of the stop while a game is in front.

use super::fixtures::{MAIL, app_preparing_on_focus};
use crate::app::{App, BadgeView, Status, StopReason, TrayCommand, TrayGlyph};
use crate::desktop::{EnergySaver, GameActivity};
use crate::engine::Reaction;
use crate::key::{Key, Press};
use crate::settings::{EnergySavingSettings, GameRule, Settings};
use crate::test_support::english;

fn game_badge() -> BadgeView {
    BadgeView {
        status: Status::Paused,
        label: "TypeAlike (off while you play a game)".to_owned(),
        frame: None,
    }
}

fn settings_with(rule: GameRule) -> Settings {
    Settings {
        energy_saving: EnergySavingSettings {
            game: rule,
            ..EnergySavingSettings::default()
        },
        ..Settings::default()
    }
}

fn app_keeping_suggestions_during_games() -> App {
    let mut app = App::new();
    app.use_settings(&settings_with(GameRule::KeepSuggestions));
    app
}

fn app_during_a_game() -> App {
    let mut app = App::new();
    app.use_game(GameActivity::InFront);
    app
}

fn app_after_a_game() -> App {
    let mut app = app_during_a_game();
    app.use_game(GameActivity::None);
    app
}

#[test]
fn app_keeps_the_game_activity() {
    let app = app_during_a_game();

    assert_eq!(app.game(), GameActivity::InFront);
}

#[test]
fn key_during_a_game_asks_for_no_suggestion() {
    let mut app = app_during_a_game();

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert_eq!((reaction, app.has_offer()), (Reaction::Hide, false));
}

#[test]
fn key_during_a_game_asks_for_a_suggestion_when_the_switch_is_off() {
    let mut app = app_keeping_suggestions_during_games();
    app.use_game(GameActivity::InFront);

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn key_after_the_game_asks_for_a_suggestion() {
    let mut app = app_after_a_game();

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn game_ends_the_session() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    app.use_game(GameActivity::InFront);

    assert_eq!((app.text_before_caret(), app.has_offer()), ("", false));
}

#[test]
fn turning_on_the_switch_during_a_game_ends_the_session() {
    let mut app = app_keeping_suggestions_during_games();
    app.use_game(GameActivity::InFront);
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    app.use_settings(&settings_with(GameRule::StopSuggestions));

    assert_eq!((app.text_before_caret(), app.has_offer()), ("", false));
}

#[test]
fn settled_input_during_a_game_prepares_nothing() {
    let mut app = app_preparing_on_focus();
    app.use_game(GameActivity::InFront);

    let work = app.on_input_settled(&MAIL);

    assert_eq!(work, None);
}

#[test]
fn typing_during_a_game_shows_the_game_badge() {
    let mut app = app_during_a_game();

    app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert_eq!(app.badge_view(&english()), Some(game_badge()));
}

#[test]
fn tray_during_a_game_shows_the_stop() {
    let app = app_during_a_game();

    let view = app.tray_view(&english());

    assert_eq!(
        (view.glyph, view.tooltip.as_str()),
        (TrayGlyph::Stopped, "TypeAlike (off while you play a game)")
    );
}

#[test]
fn tray_after_the_game_shows_suggestions_as_active() {
    let app = app_after_a_game();

    let view = app.tray_view(&english());

    assert_eq!(view.glyph, TrayGlyph::Active);
}

#[test]
fn game_is_the_stop_reason() {
    let app = app_during_a_game();

    assert_eq!(app.stop_reason(), Some(StopReason::Game));
}

#[test]
fn game_stays_the_stop_reason_after_it_leaves_the_front() {
    let app = app_after_a_game();

    assert_eq!(app.stop_reason(), Some(StopReason::Game));
}

#[test]
fn game_is_no_stop_reason_when_the_switch_is_off() {
    let mut app = app_keeping_suggestions_during_games();

    app.use_game(GameActivity::InFront);

    assert_eq!(app.stop_reason(), None);
}

#[test]
fn turning_off_the_switch_after_a_game_ends_the_game_stop() {
    let mut app = app_after_a_game();

    app.use_settings(&settings_with(GameRule::KeepSuggestions));

    assert_eq!(app.stop_reason(), None);
}

#[test]
fn energy_saver_comes_before_the_game_as_stop_reason() {
    let mut app = app_during_a_game();

    app.use_energy_saver(EnergySaver::On);

    assert_eq!(app.stop_reason(), Some(StopReason::EnergySaver));
}

#[test]
fn game_comes_before_the_idle_timeout_as_stop_reason() {
    let mut app = app_during_a_game();

    app.on_idle();

    assert_eq!(app.stop_reason(), Some(StopReason::Game));
}

#[test]
fn request_after_the_game_ends_the_game_stop() {
    let mut app = app_after_a_game();

    let was_stopped = app.end_stops_before_request();

    assert_eq!((was_stopped, app.stop_reason()), (true, None));
}

#[test]
fn request_after_the_idle_timeout_ends_the_idle_stop() {
    let mut app = App::new();
    app.on_idle();

    let was_stopped = app.end_stops_before_request();

    assert_eq!((was_stopped, app.stop_reason()), (true, None));
}

#[test]
fn request_without_a_stop_tells_that_no_stop_applied() {
    let mut app = App::new();

    let was_stopped = app.end_stops_before_request();

    assert!(!was_stopped);
}

#[test]
fn resume_ends_the_game_stop() {
    let mut app = app_after_a_game();
    app.on_tray_command(TrayCommand::TogglePause);

    app.on_tray_command(TrayCommand::TogglePause);

    assert_eq!(app.stop_reason(), None);
}
