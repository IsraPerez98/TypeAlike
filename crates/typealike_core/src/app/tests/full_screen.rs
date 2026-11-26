//! Tests of the stop while an app in full screen is in front.

use super::fixtures::{FakeDesktop, MAIL, app_preparing_on_focus};
use crate::app::{
    App, BadgeView, FULL_SCREEN_UNLOAD_DELAY, GAME_UNLOAD_DELAY, Status, StopReason, TrayCommand, TrayGlyph,
    UnloadWait,
};
use crate::desktop::{Field, FullScreenActivity, GameActivity};
use crate::engine::Reaction;
use crate::key::{Key, Press};
use crate::settings::{EnergySavingSettings, FullScreenRule, Settings};
use crate::test_support::english;

fn settings_with(rule: FullScreenRule) -> Settings {
    Settings {
        energy_saving: EnergySavingSettings {
            full_screen: rule,
            ..EnergySavingSettings::default()
        },
        ..Settings::default()
    }
}

fn app_keeping_suggestions_in_full_screen() -> App {
    let mut app = App::new();
    app.use_settings(&settings_with(FullScreenRule::KeepSuggestions));
    app
}

fn app_in_full_screen() -> App {
    let mut app = App::new();
    app.use_full_screen(FullScreenActivity::InFront);
    app
}

/// An app in full screen that stayed in front past its unload delay.
fn app_after_the_full_screen_delay() -> App {
    let mut app = app_in_full_screen();
    app.end_unload_wait();
    app
}

/// An app after full screen that stayed in front past its unload delay.
fn app_after_full_screen() -> App {
    let mut app = app_after_the_full_screen_delay();
    app.use_full_screen(FullScreenActivity::None);
    app
}

#[test]
fn app_keeps_the_full_screen_activity() {
    let app = app_in_full_screen();

    assert_eq!(app.full_screen(), FullScreenActivity::InFront);
}

#[test]
fn key_in_full_screen_asks_for_no_suggestion() {
    let mut app = app_in_full_screen();

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert_eq!((reaction, app.has_offer()), (Reaction::Hide, false));
}

#[test]
fn key_in_full_screen_asks_for_a_suggestion_when_the_switch_is_off() {
    let mut app = app_keeping_suggestions_in_full_screen();
    app.use_full_screen(FullScreenActivity::InFront);

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn key_after_full_screen_asks_for_a_suggestion() {
    let mut app = app_after_full_screen();

    let reaction = app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert!(matches!(reaction, Reaction::Request(_)));
}

#[test]
fn full_screen_ends_the_session() {
    let mut app = App::new();
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    app.use_full_screen(FullScreenActivity::InFront);

    assert_eq!((app.text_before_caret(), app.has_offer()), ("", false));
}

#[test]
fn turning_on_the_switch_in_full_screen_ends_the_session() {
    let mut app = app_keeping_suggestions_in_full_screen();
    app.use_full_screen(FullScreenActivity::InFront);
    app.on_key(Key::Char('H'), Press::First, &MAIL);

    app.use_settings(&settings_with(FullScreenRule::StopSuggestions));

    assert_eq!((app.text_before_caret(), app.has_offer()), ("", false));
}

#[test]
fn settled_input_in_full_screen_prepares_nothing() {
    let mut app = app_preparing_on_focus();
    app.use_full_screen(FullScreenActivity::InFront);

    let work = app.on_input_settled(&MAIL);

    assert_eq!(work, None);
}

#[test]
fn typing_in_full_screen_shows_the_full_screen_badge() {
    let mut app = app_in_full_screen();

    app.on_key(Key::Char('H'), Press::First, &MAIL);

    assert_eq!(
        app.badge_view(&english()),
        Some(BadgeView {
            status: Status::Paused,
            label: "TypeAlike (off while an app fills the screen)".to_owned(),
            frame: None,
        })
    );
}

#[test]
fn tray_in_full_screen_shows_the_stop() {
    let app = app_in_full_screen();

    let view = app.tray_view(&english());

    assert_eq!(
        (view.glyph, view.tooltip.as_str()),
        (
            TrayGlyph::Stopped,
            "TypeAlike (off while an app fills the screen)"
        )
    );
}

#[test]
fn tray_after_full_screen_shows_suggestions_as_active() {
    let app = app_after_full_screen();

    let view = app.tray_view(&english());

    assert_eq!(view.glyph, TrayGlyph::Active);
}

#[test]
fn game_in_full_screen_shows_the_game_in_the_tray() {
    let mut app = app_in_full_screen();

    app.use_game(GameActivity::InFront);

    assert_eq!(
        app.tray_view(&english()).tooltip,
        "TypeAlike (off while you play a game)"
    );
}

#[test]
fn full_screen_is_no_stop_reason_during_its_unload_delay() {
    let app = app_in_full_screen();

    assert_eq!(app.stop_reason(), None);
}

#[test]
fn full_screen_is_the_stop_reason_after_its_unload_delay() {
    let app = app_after_the_full_screen_delay();

    assert_eq!(app.stop_reason(), Some(StopReason::FullScreen));
}

#[test]
fn full_screen_waits_its_delay_before_the_unload() {
    let app = app_in_full_screen();

    assert_eq!(
        app.unload_wait(),
        Some(UnloadWait {
            reason: StopReason::FullScreen,
            delay: FULL_SCREEN_UNLOAD_DELAY,
        })
    );
}

#[test]
fn game_in_full_screen_waits_the_delay_of_the_game() {
    let mut app = app_in_full_screen();

    app.use_game(GameActivity::InFront);

    assert_eq!(
        app.unload_wait(),
        Some(UnloadWait {
            reason: StopReason::Game,
            delay: GAME_UNLOAD_DELAY,
        })
    );
}

#[test]
fn full_screen_that_leaves_during_its_unload_delay_is_no_stop_reason() {
    let mut app = app_in_full_screen();

    app.use_full_screen(FullScreenActivity::None);

    assert_eq!((app.stop_reason(), app.unload_wait()), (None, None));
}

#[test]
fn full_screen_stays_the_stop_reason_after_it_leaves_the_front() {
    let app = app_after_full_screen();

    assert_eq!(app.stop_reason(), Some(StopReason::FullScreen));
}

#[test]
fn full_screen_is_no_stop_reason_when_the_switch_is_off() {
    let mut app = app_keeping_suggestions_in_full_screen();

    app.use_full_screen(FullScreenActivity::InFront);

    assert_eq!(app.stop_reason(), None);
}

#[test]
fn turning_off_the_switch_after_full_screen_ends_the_full_screen_stop() {
    let mut app = app_after_full_screen();

    app.use_settings(&settings_with(FullScreenRule::KeepSuggestions));

    assert_eq!(app.stop_reason(), None);
}

#[test]
fn game_comes_before_full_screen_as_stop_reason() {
    let mut app = app_in_full_screen();
    app.use_game(GameActivity::InFront);

    app.end_unload_wait();

    assert_eq!(app.stop_reason(), Some(StopReason::Game));
}

#[test]
fn full_screen_comes_before_the_idle_timeout_as_stop_reason() {
    let mut app = app_after_the_full_screen_delay();

    app.on_idle();

    assert_eq!(app.stop_reason(), Some(StopReason::FullScreen));
}

#[test]
fn request_after_full_screen_ends_the_full_screen_stop() {
    let mut app = app_after_full_screen();

    let was_stopped = app.end_stops_before_request();

    assert_eq!((was_stopped, app.stop_reason()), (true, None));
}

#[test]
fn resume_ends_the_full_screen_stop() {
    let mut app = app_after_full_screen();
    app.on_tray_command(TrayCommand::TogglePause);

    app.on_tray_command(TrayCommand::TogglePause);

    assert_eq!(app.stop_reason(), None);
}

#[test]
fn settled_input_after_full_screen_prepares_nothing() {
    let mut app = app_preparing_on_focus();
    app.use_full_screen(FullScreenActivity::InFront);
    app.end_unload_wait();
    app.use_full_screen(FullScreenActivity::None);

    let work = app.on_input_settled(&FakeDesktop(Field::Open));

    assert_eq!(work, None);
}

#[test]
fn resume_in_full_screen_after_its_delay_keeps_the_full_screen_stop() {
    let mut app = app_after_the_full_screen_delay();
    app.on_tray_command(TrayCommand::TogglePause);

    app.on_tray_command(TrayCommand::TogglePause);

    assert_eq!(app.stop_reason(), Some(StopReason::FullScreen));
}
